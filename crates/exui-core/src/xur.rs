//! XUR version 5 scene reader.
//!
//! Follows `XuiLoadBinaryScene` (0x9216f180) and the object, property and
//! timeline readers it calls; see doc/xui-format.md.
//!
//! [`Document`] borrows the file: strings, vectors, quaternions and custom
//! blobs are resolved from the input on access. The object tree is the one
//! part that allocates; it is decoded into a few flat arenas (objects,
//! properties, values, timelines, keyframes) whose sizes are bounded by the
//! input length, because the format can only be walked front to back.

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::num::NonZeroU16;
use core::ptr;

use crate::cursor::Cursor;
use crate::cursor::Short;
use crate::error::Region;
use crate::error::XurError;
use crate::schema::ClassId;
use crate::schema::ClassRef;
use crate::schema::PropDef;
use crate::schema::PropKind;
use crate::schema::PropType;
use crate::schema::Registry;
use crate::schema::ScalarType;
use crate::value::Color;
use crate::value::Quaternion;
use crate::value::Vector3;
use crate::wide::WideStr;

const VECTOR_SIZE: usize = 12;
const QUATERNION_SIZE: usize = 16;

/// Four-character section tag.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectionTag([u8; 4]);

impl SectionTag {
    pub const STRN: Self = Self(*b"STRN");
    pub const VECT: Self = Self(*b"VECT");
    pub const QUAT: Self = Self(*b"QUAT");
    pub const CUST: Self = Self(*b"CUST");
    pub const DATA: Self = Self(*b"DATA");

    pub const fn bytes(self) -> [u8; 4] {
        self.0
    }
}

impl fmt::Display for SectionTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            if byte.is_ascii_graphic() {
                fmt::Write::write_char(f, char::from(byte))?;
            } else {
                write!(f, "\\x{byte:02X}")?;
            }
        }

        Ok(())
    }
}

impl fmt::Debug for SectionTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SectionTag({self})")
    }
}

/// XUR header flag word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderFlags(u32);

impl HeaderFlags {
    /// Ten allocation hint words follow the header.
    pub const ALLOCATION_HINTS: Self = Self(0x1);

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Object record flag byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectFlags(u8);

impl ObjectFlags {
    pub const PROPERTIES: Self = Self(0x1);
    pub const CHILDREN: Self = Self(0x2);
    pub const TIMELINES: Self = Self(0x4);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// One-based index into the string table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StringId(NonZeroU16);

impl StringId {
    /// Index 0 is the null string.
    pub fn new(index: u16) -> Option<Self> {
        NonZeroU16::new(index).map(Self)
    }

    pub fn get(self) -> u16 {
        self.0.get()
    }
}

/// Zero-based index into the VECT table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VectorIndex(pub u32);

/// Zero-based index into the QUAT table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QuaternionIndex(pub u32);

/// Byte offset of a length-prefixed blob in the CUST section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CustomOffset(pub u32);

/// Position of a definition in its class level or compound table; also its
/// bit in the property mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PropIndex(pub u8);

/// Base class steps above a timeline target's class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LevelsUp(pub u8);

/// Element of an indexed property selected by a timeline property path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ElementIndex(pub u32);

/// Index of an object in its [`Document`], in file order; the root is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(u32);

impl ObjectId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Timeline frame number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameTime(pub u32);

/// Named frame command byte. The loader stores it without interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameCommand(pub u8);

/// Keyframe interpolation byte. The loader stores it without interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Interpolation(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EaseIn(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EaseOut(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EaseScale(pub u8);

/// Ten words that follow the header when [`HeaderFlags::ALLOCATION_HINTS`]
/// is set. The loader only sums them to size its allocator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationHints(pub [u32; 10]);

/// Range in one of the document arenas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: u32,
    len: u32,
}

impl Span {
    const EMPTY: Self = Self { start: 0, len: 0 };

    /// Arena lengths stay below the input length, which the header limits
    /// to u32.
    fn new(start: usize, end: usize) -> Self {
        Self {
            start: start as u32,
            len: (end - start) as u32,
        }
    }

    fn of<T>(self, items: &[T]) -> &[T] {
        // Spans are created by the reader for these arenas only.
        &items[self.start as usize..][..self.len as usize]
    }
}

/// Sub-properties of one compound value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompoundRef(Span);

/// A stored property value. Table references are kept as stored; the
/// original resolves them on access, and so do the [`Document`] accessors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    /// `None` is the null string.
    String(Option<StringId>),
    Color(Color),
    Vector(VectorIndex),
    Quaternion(QuaternionIndex),
    Custom(CustomOffset),
    Compound(CompoundRef),
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Self::Bool(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i32> {
        match *self {
            Self::Integer(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_unsigned(&self) -> Option<u32> {
        match *self {
            Self::Unsigned(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match *self {
            Self::Float(value) => Some(value),
            _ => None,
        }
    }

    /// The outer `Option` is `None` for other types; the inner one is the
    /// null string.
    pub fn as_string(&self) -> Option<Option<StringId>> {
        match *self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match *self {
            Self::Color(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_vector(&self) -> Option<VectorIndex> {
        match *self {
            Self::Vector(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_quaternion(&self) -> Option<QuaternionIndex> {
        match *self {
            Self::Quaternion(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_custom(&self) -> Option<CustomOffset> {
        match *self {
            Self::Custom(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<CompoundRef> {
        match *self {
            Self::Compound(value) => Some(value),
            _ => None,
        }
    }
}

/// A loaded property. Values are read through [`Document::values`].
#[derive(Clone, Copy)]
pub struct Property {
    def: &'static PropDef,
    index: PropIndex,
    values: Span,
}

impl Property {
    pub fn def(&self) -> &'static PropDef {
        self.def
    }

    pub fn index(&self) -> PropIndex {
        self.index
    }

    /// Whether this property was loaded for exactly `def`.
    pub fn is(&self, def: &PropDef) -> bool {
        ptr::eq(self.def, def)
    }
}

impl fmt::Debug for Property {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Property")
            .field("name", &self.def.name)
            .field("index", &self.index)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NamedFrame {
    pub name: Option<StringId>,
    pub time: FrameTime,
    pub command: FrameCommand,
    pub target: Option<StringId>,
}

/// Where a timeline property path points. Read the definitions through
/// [`Document::path_defs`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedPath {
    /// Class level that declares the first definition.
    pub class: ClassId,
    pub levels_up: LevelsUp,
    /// Position of the first definition in `class`.
    pub index: PropIndex,
    defs: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineProperty {
    /// `None` when the stored path depth is 0.
    pub path: Option<ResolvedPath>,
    pub element: Option<ElementIndex>,
    has_value: bool,
}

impl TimelineProperty {
    /// Whether keyframes carry a value for this property. Compound and
    /// custom properties, and empty paths, have none.
    pub fn has_value(&self) -> bool {
        self.has_value
    }
}

/// One keyframe. Values are read through [`Document::keyframe_values`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    pub time: FrameTime,
    pub interpolation: Interpolation,
    pub ease_in: EaseIn,
    pub ease_out: EaseOut,
    pub ease_scale: EaseScale,
    values: Span,
}

/// Keyframe value. Vectors and quaternions are copied from their tables at
/// load time, as the original does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyframeValue {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    String(Option<StringId>),
    Color(Color),
    Vector(Vector3),
    Quaternion(Quaternion),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timeline {
    pub element: Option<StringId>,
    /// Child whose `Id` equals `element`. `None` only when the timeline has
    /// no properties, since the loader searches once per property.
    pub target: Option<ObjectId>,
    properties: Span,
    keyframes: Span,
}

#[derive(Clone, Copy, Debug)]
struct ObjectRecord {
    class: ClassId,
    parent: Option<ObjectId>,
    first_child: Option<ObjectId>,
    next_sibling: Option<ObjectId>,
    properties: Span,
    named_frames: Span,
    timelines: Span,
}

#[derive(Clone, Debug)]
pub struct Document<'a> {
    allocation_hints: Option<AllocationHints>,
    strings: Vec<WideStr<'a>>,
    vectors: &'a [u8],
    quaternions: &'a [u8],
    custom: &'a [u8],

    objects: Vec<ObjectRecord>,
    properties: Vec<Property>,
    values: Vec<Value>,
    named_frames: Vec<NamedFrame>,
    timelines: Vec<Timeline>,
    timeline_properties: Vec<TimelineProperty>,
    path_defs: Vec<&'static PropDef>,
    keyframes: Vec<Keyframe>,
    keyframe_values: Vec<KeyframeValue>,
}

impl<'a> Document<'a> {
    pub fn parse(data: &'a [u8], registry: &Registry) -> Result<Self, XurError> {
        let header = Header::parse(data)?;

        let strings = parse_strings(data, header.strn)?;
        let vectors = match header.vect {
            Some(section) => table_slice(data, section, VECTOR_SIZE)?,
            None => &[],
        };
        let quaternions = match header.quat {
            Some(section) => table_slice(data, section, QUATERNION_SIZE)?,
            None => &[],
        };
        let custom = match header.cust {
            Some(section) => section.slice(data)?,
            None => &[],
        };

        let document = Self {
            allocation_hints: header.allocation_hints,
            strings,
            vectors,
            quaternions,
            custom,
            objects: Vec::new(),
            properties: Vec::new(),
            values: Vec::new(),
            named_frames: Vec::new(),
            timelines: Vec::new(),
            timeline_properties: Vec::new(),
            path_defs: Vec::new(),
            keyframes: Vec::new(),
            keyframe_values: Vec::new(),
        };

        let data_slice = header.data.slice(data)?;
        let mut cursor = Cursor::new(data_slice, header.data.offset as usize);
        let mut reader = Reader {
            registry,
            class_slots: vec![ClassSlot::Unresolved; document.strings.len()],
            doc: document,
            scratch_properties: Vec::new(),
            scratch_values: Vec::new(),
            ids: Vec::new(),
            value_types: Vec::new(),
        };
        reader.read_tree(&mut cursor)?;

        Ok(reader.doc)
    }

    pub fn allocation_hints(&self) -> Option<AllocationHints> {
        self.allocation_hints
    }

    pub fn root(&self) -> ObjectRef<'_, 'a> {
        ObjectRef {
            doc: self,
            id: ObjectId(0),
        }
    }

    /// `None` when `id` belongs to another document.
    pub fn object(&self, id: ObjectId) -> Option<ObjectRef<'_, 'a>> {
        self.objects.get(id.index())?;

        Some(ObjectRef { doc: self, id })
    }

    /// Objects in file order (depth first, parents before children).
    pub fn objects(&self) -> impl Iterator<Item = ObjectRef<'_, 'a>> {
        (0..self.objects.len()).map(|index| ObjectRef {
            doc: self,
            id: ObjectId(index as u32),
        })
    }

    pub fn strings(&self) -> &[WideStr<'a>] {
        &self.strings
    }

    /// `None` for an index past the table, which the original reads as the
    /// null string.
    pub fn string(&self, id: StringId) -> Option<WideStr<'a>> {
        self.strings.get(usize::from(id.get()) - 1).copied()
    }

    pub fn vector_count(&self) -> usize {
        self.vectors.len() / VECTOR_SIZE
    }

    pub fn vector(&self, index: VectorIndex) -> Option<Vector3> {
        let [x, y, z] = floats(self.vectors, index.0)?;

        Some(Vector3::new(x, y, z))
    }

    pub fn quaternion_count(&self) -> usize {
        self.quaternions.len() / QUATERNION_SIZE
    }

    pub fn quaternion(&self, index: QuaternionIndex) -> Option<Quaternion> {
        let [x, y, z, w] = floats(self.quaternions, index.0)?;

        Some(Quaternion::new(x, y, z, w))
    }

    pub fn custom(&self) -> &'a [u8] {
        self.custom
    }

    /// Custom value blob at `offset`: a big-endian u32 length followed by
    /// that many bytes, which the property's custom type handler decodes.
    /// `None` marks a blob outside the section.
    pub fn custom_data(&self, offset: CustomOffset) -> Option<&'a [u8]> {
        let start = offset.0 as usize;
        let prefix = self.custom.get(start..start.checked_add(4)?)?;
        let length = u32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;
        let data_start = start + 4;

        self.custom.get(data_start..data_start.checked_add(length)?)
    }

    /// Values of a property: one for a plain property, the elements of an
    /// indexed one.
    pub fn values(&self, property: &Property) -> &[Value] {
        property.values.of(&self.values)
    }

    /// The value of a property that is not indexed.
    pub fn value(&self, property: &Property) -> Option<&Value> {
        if property.def.flags.is_indexed() {
            return None;
        }

        self.values(property).first()
    }

    /// Sub-properties of a compound value, in definition order.
    pub fn compound(&self, compound: CompoundRef) -> &[Property] {
        compound.0.of(&self.properties)
    }

    pub fn timeline_properties(&self, timeline: &Timeline) -> &[TimelineProperty] {
        timeline.properties.of(&self.timeline_properties)
    }

    pub fn keyframes(&self, timeline: &Timeline) -> &[Keyframe] {
        timeline.keyframes.of(&self.keyframes)
    }

    /// Values for the timeline properties whose
    /// [`TimelineProperty::has_value`] is true, in property order.
    pub fn keyframe_values(&self, keyframe: &Keyframe) -> &[KeyframeValue] {
        keyframe.values.of(&self.keyframe_values)
    }

    /// Definitions from the class-level property down through compounds.
    pub fn path_defs(&self, path: &ResolvedPath) -> &[&'static PropDef] {
        path.defs.of(&self.path_defs)
    }

    fn record(&self, id: ObjectId) -> &ObjectRecord {
        // ObjectRef ids come from this document.
        &self.objects[id.index()]
    }
}

/// An object of a [`Document`].
#[derive(Clone, Copy)]
pub struct ObjectRef<'d, 'a> {
    doc: &'d Document<'a>,
    id: ObjectId,
}

impl<'d, 'a> ObjectRef<'d, 'a> {
    pub fn id(self) -> ObjectId {
        self.id
    }

    pub fn class(self) -> ClassId {
        self.doc.record(self.id).class
    }

    pub fn parent(self) -> Option<ObjectRef<'d, 'a>> {
        let parent = self.doc.record(self.id).parent?;

        Some(ObjectRef {
            doc: self.doc,
            id: parent,
        })
    }

    pub fn children(self) -> Children<'d, 'a> {
        Children {
            doc: self.doc,
            next: self.doc.record(self.id).first_child,
        }
    }

    /// Loaded properties of every class level, root base class first.
    pub fn properties(self) -> &'d [Property] {
        self.doc.record(self.id).properties.of(&self.doc.properties)
    }

    /// The property loaded for `def`, compared by address.
    pub fn property(self, def: &PropDef) -> Option<&'d Property> {
        self.properties().iter().find(|property| property.is(def))
    }

    pub fn named_frames(self) -> &'d [NamedFrame] {
        self.doc
            .record(self.id)
            .named_frames
            .of(&self.doc.named_frames)
    }

    /// First named frame called `name`.
    pub fn named_frame(self, name: &str) -> Option<&'d NamedFrame> {
        self.named_frames().iter().find(|frame| {
            frame
                .name
                .and_then(|id| self.doc.string(id))
                .is_some_and(|text| text.eq_str(name))
        })
    }

    pub fn timelines(self) -> &'d [Timeline] {
        self.doc.record(self.id).timelines.of(&self.doc.timelines)
    }
}

impl fmt::Debug for ObjectRef<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ObjectRef").field(&self.id).finish()
    }
}

/// Children of an object, in file order.
#[derive(Clone)]
pub struct Children<'d, 'a> {
    doc: &'d Document<'a>,
    next: Option<ObjectId>,
}

impl<'d, 'a> Iterator for Children<'d, 'a> {
    type Item = ObjectRef<'d, 'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        self.next = self.doc.record(id).next_sibling;

        Some(ObjectRef { doc: self.doc, id })
    }
}

#[derive(Clone, Copy, Debug)]
struct Section {
    tag: SectionTag,
    offset: u32,
    size: u32,
}

impl Section {
    fn slice(self, data: &[u8]) -> Result<&[u8], XurError> {
        let start = self.offset as usize;
        let range = start
            .checked_add(self.size as usize)
            .and_then(|end| data.get(start..end));

        range.ok_or(XurError::SectionOutOfBounds {
            tag: self.tag,
            offset: self.offset,
            size: self.size,
        })
    }
}

struct Header {
    allocation_hints: Option<AllocationHints>,
    strn: Section,
    vect: Option<Section>,
    quat: Option<Section>,
    cust: Option<Section>,
    data: Section,
}

impl Header {
    fn parse(data: &[u8]) -> Result<Self, XurError> {
        let header_error = |short: Short| truncated(Region::Header, short);
        let mut cursor = Cursor::new(data, 0);

        let magic = cursor.bytes(4).map_err(header_error)?;
        if magic != b"XUIB" {
            let mut found = [0; 4];
            found.copy_from_slice(magic);
            return Err(XurError::BadMagic { found });
        }

        let version = cursor.u32().map_err(header_error)?;
        if version != 5 {
            return Err(XurError::UnsupportedVersion { version });
        }

        let flags = HeaderFlags(cursor.u32().map_err(header_error)?);
        let schema = cursor.u16().map_err(header_error)?;
        if schema != 0xC {
            return Err(XurError::UnsupportedSchema { schema });
        }

        let file_size = cursor.u32().map_err(header_error)?;
        let section_count = cursor.u16().map_err(header_error)?;

        let allocation_hints = if flags.contains(HeaderFlags::ALLOCATION_HINTS) {
            let mut words = [0; 10];
            for word in &mut words {
                *word = cursor.u32().map_err(header_error)?;
            }
            Some(AllocationHints(words))
        } else {
            None
        };

        if usize::try_from(file_size).ok() != Some(data.len()) {
            return Err(XurError::FileSizeMismatch {
                header: file_size,
                actual: data.len(),
            });
        }

        if section_count < 2 {
            return Err(XurError::TooFewSections {
                count: section_count,
            });
        }

        // Later entries with the same tag replace earlier ones; unknown tags
        // are skipped.
        let mut strn = None;
        let mut vect = None;
        let mut quat = None;
        let mut cust = None;
        let mut data_section = None;
        for _ in 0..section_count {
            let mut tag = [0; 4];
            tag.copy_from_slice(cursor.bytes(4).map_err(header_error)?);
            let section = Section {
                tag: SectionTag(tag),
                offset: cursor.u32().map_err(header_error)?,
                size: cursor.u32().map_err(header_error)?,
            };

            match section.tag {
                SectionTag::STRN => strn = Some(section),
                SectionTag::VECT => vect = Some(section),
                SectionTag::QUAT => quat = Some(section),
                SectionTag::CUST => cust = Some(section),
                SectionTag::DATA => data_section = Some(section),
                _ => {
                    // Not consumed by the version 5 loader.
                }
            }
        }

        let strn = strn.ok_or(XurError::MissingSection {
            tag: SectionTag::STRN,
        })?;
        let data_section = data_section.ok_or(XurError::MissingSection {
            tag: SectionTag::DATA,
        })?;

        Ok(Self {
            allocation_hints,
            strn,
            vect,
            quat,
            cust,
            data: data_section,
        })
    }
}

/// Cached class lookup for one string table slot.
#[derive(Clone, Copy)]
enum ClassSlot {
    Unresolved,
    Unknown,
    Known(ClassId),
}

/// Keyframe value types: every scalar except custom.
#[derive(Clone, Copy)]
enum KeyframeType {
    Bool,
    Integer,
    Unsigned,
    Float,
    String,
    Color,
    Vector,
    Quaternion,
}

impl KeyframeType {
    fn from_prop(ty: PropType) -> Option<Self> {
        let keyframe = match ty.scalar()? {
            ScalarType::Bool => Self::Bool,
            ScalarType::Integer => Self::Integer,
            ScalarType::Unsigned => Self::Unsigned,
            ScalarType::Float => Self::Float,
            ScalarType::String => Self::String,
            ScalarType::Color => Self::Color,
            ScalarType::Vector => Self::Vector,
            ScalarType::Quaternion => Self::Quaternion,
            ScalarType::Custom => return None,
        };

        Some(keyframe)
    }
}

/// An object whose children and timelines are still being read.
#[derive(Clone, Copy)]
struct ObjectFrame {
    id: ObjectId,
    has_timelines: bool,
    children_left: u32,
    last_child: Option<ObjectId>,
}

/// A property whose values are being read onto the value scratch stack.
struct PendingProperty {
    def: &'static PropDef,
    index: PropIndex,
    left: u32,
    value_start: usize,
}

struct CompoundBlock {
    property: &'static str,
    declared: u16,
    offset: usize,
}

/// A property block (one class level or one compound value) being read.
/// Its finished properties sit on the property scratch stack from
/// `property_start`.
struct BlockFrame {
    defs: &'static [PropDef],
    mask: u32,
    next_def: usize,
    pending: Option<PendingProperty>,
    property_start: usize,
    entries: u32,
    compound: Option<CompoundBlock>,
}

impl BlockFrame {
    fn next_set_def(&mut self) -> Option<(PropIndex, &'static PropDef)> {
        while self.next_def < self.defs.len() {
            let index = self.next_def;
            self.next_def += 1;

            // Shifts of 32 or more yield 0 on the original hardware, so later
            // definitions can never be present.
            let present = index < 32 && (self.mask >> index) & 1 != 0;
            if present {
                return Some((PropIndex(index as u8), &self.defs[index]));
            }
        }

        None
    }
}

struct Reader<'r, 'a> {
    registry: &'r Registry,
    class_slots: Vec<ClassSlot>,
    doc: Document<'a>,

    // Scratch stacks keep each block's properties and each property's values
    // contiguous when they move into the arenas.
    scratch_properties: Vec<Property>,
    scratch_values: Vec<Value>,

    /// Child `Id` strings of the object whose timelines are being read,
    /// stably sorted by string so the first match is the earliest child.
    ids: Vec<(WideStr<'a>, ObjectId)>,
    value_types: Vec<KeyframeType>,
}

impl<'r, 'a> Reader<'r, 'a> {
    fn read_tree(&mut self, cursor: &mut Cursor<'_>) -> Result<(), XurError> {
        let root = self.read_object(cursor, None)?;
        let mut stack = vec![root];

        while let Some(top) = stack.last_mut() {
            if top.children_left > 0 {
                top.children_left -= 1;
                let parent = top.id;
                let previous = top.last_child;

                let child = self.read_object(cursor, Some(parent))?;
                match previous {
                    Some(previous) => {
                        self.doc.objects[previous.index()].next_sibling = Some(child.id)
                    }
                    None => self.doc.objects[parent.index()].first_child = Some(child.id),
                }
                top.last_child = Some(child.id);
                stack.push(child);
                continue;
            }

            let Some(done) = stack.pop() else {
                break;
            };

            if done.has_timelines {
                self.read_timelines(cursor, done.id)?;
            }
        }

        Ok(())
    }

    /// Reads the class, flags, properties and child count of one object.
    fn read_object(
        &mut self,
        cursor: &mut Cursor<'_>,
        parent: Option<ObjectId>,
    ) -> Result<ObjectFrame, XurError> {
        let offset = cursor.offset();
        let class_index = cursor.u16().map_err(data_error)?;
        let flags = ObjectFlags(cursor.u8().map_err(data_error)?);
        let class = self.resolve_class(offset, class_index)?;

        let properties = if flags.contains(ObjectFlags::PROPERTIES) {
            let declared = cursor.u16().map_err(data_error)?;
            let start = self.scratch_properties.len();

            let mut actual = 0u32;
            for level in class.levels() {
                actual += self.read_block(cursor, level.def().props)?;
            }

            if actual != u32::from(declared) {
                return Err(XurError::PropertyCountMismatch {
                    offset,
                    class: class.name(),
                    declared,
                    actual,
                });
            }

            self.drain_properties(start)
        } else {
            Span::EMPTY
        };

        let children_left = if flags.contains(ObjectFlags::CHILDREN) {
            cursor.u32().map_err(data_error)?
        } else {
            0
        };

        // Each object takes at least three input bytes, so the count fits.
        let id = ObjectId(self.doc.objects.len() as u32);
        self.doc.objects.push(ObjectRecord {
            class: class.id(),
            parent,
            first_child: None,
            next_sibling: None,
            properties,
            named_frames: Span::EMPTY,
            timelines: Span::EMPTY,
        });

        Ok(ObjectFrame {
            id,
            has_timelines: flags.contains(ObjectFlags::TIMELINES),
            children_left,
            last_child: None,
        })
    }

    fn resolve_class(&mut self, offset: usize, index: u16) -> Result<ClassRef<'r>, XurError> {
        let invalid = XurError::InvalidClassString { offset, index };
        let Some(slot) = usize::from(index).checked_sub(1) else {
            return Err(invalid);
        };
        let Some(&name) = self.doc.strings.get(slot) else {
            return Err(invalid);
        };

        let id = match self.class_slots[slot] {
            ClassSlot::Known(id) => Some(id),
            ClassSlot::Unknown => None,
            ClassSlot::Unresolved => {
                let found = self.registry.find_wide(name);
                self.class_slots[slot] = match found {
                    Some(id) => ClassSlot::Known(id),
                    None => ClassSlot::Unknown,
                };
                found
            }
        };

        let registry = self.registry;
        id.and_then(|id| registry.class(id))
            .ok_or_else(|| XurError::UnknownClass {
                offset,
                name: name.to_wide_string(),
            })
    }

    /// Reads one class-level property block, leaving its properties on the
    /// scratch stack, and returns the number of value entries it holds (each
    /// indexed element and each compound counts once).
    fn read_block(
        &mut self,
        cursor: &mut Cursor<'_>,
        defs: &'static [PropDef],
    ) -> Result<u32, XurError> {
        let mut stack = vec![self.open_block(cursor, defs, None)?];

        loop {
            let Some(top) = stack.last_mut() else {
                unreachable!("the outermost block returns before the stack empties");
            };

            if let Some(pending) = &mut top.pending {
                if pending.left > 0 {
                    pending.left -= 1;
                    top.entries += 1;
                    let def = pending.def;

                    match def.ty.kind() {
                        PropKind::Compound(sub_defs) => {
                            let offset = cursor.offset();
                            let declared = cursor.u16().map_err(data_error)?;
                            let compound = CompoundBlock {
                                property: def.name,
                                declared,
                                offset,
                            };
                            let block = self.open_block(cursor, sub_defs, Some(compound))?;
                            stack.push(block);
                        }
                        PropKind::Scalar(scalar) => {
                            let value = read_block_value(cursor, scalar)?;
                            self.scratch_values.push(value);
                        }
                    }
                    continue;
                }

                if let Some(done) = top.pending.take() {
                    let values = self.drain_values(done.value_start);
                    self.scratch_properties.push(Property {
                        def: done.def,
                        index: done.index,
                        values,
                    });
                }
                continue;
            }

            if let Some((index, def)) = top.next_set_def() {
                let count = if def.flags.is_indexed() {
                    read_element_count(cursor)?
                } else {
                    1
                };

                // A zero count records nothing, as in the original.
                if count > 0 {
                    top.pending = Some(PendingProperty {
                        def,
                        index,
                        left: count,
                        value_start: self.scratch_values.len(),
                    });
                }
                continue;
            }

            let Some(done) = stack.pop() else {
                unreachable!("stack holds the block that just finished");
            };

            let Some(compound) = done.compound else {
                return Ok(done.entries);
            };

            if done.entries > u32::from(compound.declared) {
                return Err(XurError::CompoundOverflow {
                    offset: compound.offset,
                    property: compound.property,
                    declared: compound.declared,
                    actual: done.entries,
                });
            }

            let properties = self.drain_properties(done.property_start);
            self.scratch_values
                .push(Value::Compound(CompoundRef(properties)));
        }
    }

    fn open_block(
        &self,
        cursor: &mut Cursor<'_>,
        defs: &'static [PropDef],
        compound: Option<CompoundBlock>,
    ) -> Result<BlockFrame, XurError> {
        let header = cursor.u8().map_err(data_error)?;

        // The low three bits give the mask size; the upper five are ignored.
        let mask = match header & 7 {
            1 => u32::from(cursor.u8().map_err(data_error)?),
            2 => u32::from(cursor.u16().map_err(data_error)?),
            4 => cursor.u32().map_err(data_error)?,
            skipped => {
                // Other sizes are consumed but test as an empty mask.
                cursor.bytes(usize::from(skipped)).map_err(data_error)?;
                0
            }
        };

        Ok(BlockFrame {
            defs,
            mask,
            next_def: 0,
            pending: None,
            property_start: self.scratch_properties.len(),
            entries: 0,
            compound,
        })
    }

    fn drain_properties(&mut self, start: usize) -> Span {
        let arena_start = self.doc.properties.len();
        self.doc
            .properties
            .extend(self.scratch_properties.drain(start..));

        Span::new(arena_start, self.doc.properties.len())
    }

    fn drain_values(&mut self, start: usize) -> Span {
        let arena_start = self.doc.values.len();
        self.doc.values.extend(self.scratch_values.drain(start..));

        Span::new(arena_start, self.doc.values.len())
    }

    fn read_timelines(&mut self, cursor: &mut Cursor<'_>, id: ObjectId) -> Result<(), XurError> {
        // The original silently skips the timeline data when fewer than four
        // bytes remain.
        if cursor.remaining() < 4 {
            return Ok(());
        }

        let frame_count = cursor.u32().map_err(data_error)?;
        let frames_start = self.doc.named_frames.len();
        for _ in 0..frame_count {
            if cursor.remaining() < 9 {
                return Err(truncated(
                    Region::Section(SectionTag::DATA),
                    Short {
                        offset: cursor.offset(),
                        needed: 9,
                    },
                ));
            }

            let name = self.checked_string(cursor)?;
            let time = FrameTime(cursor.u32().map_err(data_error)?);
            let command = FrameCommand(cursor.u8().map_err(data_error)?);
            let target = self.checked_string(cursor)?;
            self.doc.named_frames.push(NamedFrame {
                name,
                time,
                command,
                target,
            });
        }
        self.doc.objects[id.index()].named_frames =
            Span::new(frames_start, self.doc.named_frames.len());

        // Timelines animate children, so they are present only with children.
        if self.doc.objects[id.index()].first_child.is_none() {
            return Ok(());
        }

        self.collect_child_ids(id);

        let timeline_count = cursor.u32().map_err(data_error)?;
        let timelines_start = self.doc.timelines.len();
        for _ in 0..timeline_count {
            let timeline = self.read_timeline(cursor)?;
            self.doc.timelines.push(timeline);
        }
        self.doc.objects[id.index()].timelines =
            Span::new(timelines_start, self.doc.timelines.len());

        Ok(())
    }

    /// Fills `ids` with the string of every loaded `Id` property of the
    /// children of `parent`. Null and out-of-range strings never match.
    fn collect_child_ids(&mut self, parent: ObjectId) {
        self.ids.clear();

        let mut child = self.doc.objects[parent.index()].first_child;
        while let Some(child_id) = child {
            let record = &self.doc.objects[child_id.index()];
            for property in record.properties.of(&self.doc.properties) {
                if property.def.name != "Id" {
                    continue;
                }

                let Some(Value::String(Some(string))) =
                    property.values.of(&self.doc.values).first()
                else {
                    continue;
                };

                if let Some(text) = self.doc.string(*string) {
                    self.ids.push((text, child_id));
                }
            }
            child = record.next_sibling;
        }

        // Stable, so equal strings keep child order.
        self.ids.sort_by(|left, right| left.0.cmp(&right.0));
    }

    fn find_child_by_id(&self, element: Option<StringId>) -> Option<ObjectId> {
        let wanted = self.doc.string(element?)?;
        let position = self.ids.partition_point(|(text, _)| *text < wanted);

        self.ids
            .get(position)
            .filter(|(text, _)| *text == wanted)
            .map(|(_, child)| *child)
    }

    fn read_timeline(&mut self, cursor: &mut Cursor<'_>) -> Result<Timeline, XurError> {
        let offset = cursor.offset();
        let element = self.checked_string(cursor)?;
        let property_count = cursor.u32().map_err(data_error)?;

        let target = if property_count > 0 {
            let Some(target) = self.find_child_by_id(element) else {
                return Err(XurError::TimelineTargetNotFound { offset, element });
            };
            Some(target)
        } else {
            None
        };

        let properties_start = self.doc.timeline_properties.len();
        if let Some(target) = target {
            let class = self.doc.objects[target.index()].class;
            for _ in 0..property_count {
                let property = self.read_property_path(cursor, class)?;
                self.doc.timeline_properties.push(property);
            }
        }
        let properties = Span::new(properties_start, self.doc.timeline_properties.len());

        let keyframes_offset = cursor.offset();
        let keyframe_count = cursor.u32().map_err(data_error)?;

        // Value types are fixed per timeline; computing them once keeps
        // keyframe reading proportional to the bytes consumed.
        self.value_types.clear();
        for property in properties.of(&self.doc.timeline_properties) {
            let Some(path) = &property.path else {
                if keyframe_count > 0 {
                    return Err(XurError::EmptyPropertyPath {
                        offset: keyframes_offset,
                    });
                }
                continue;
            };

            let defs = path.defs.of(&self.doc.path_defs);
            let ty = defs.last().and_then(|def| KeyframeType::from_prop(def.ty));
            if let Some(ty) = ty {
                self.value_types.push(ty);
            }
        }

        let keyframes_start = self.doc.keyframes.len();
        for _ in 0..keyframe_count {
            let keyframe = self.read_keyframe(cursor)?;
            self.doc.keyframes.push(keyframe);
        }

        Ok(Timeline {
            element,
            target,
            properties,
            keyframes: Span::new(keyframes_start, self.doc.keyframes.len()),
        })
    }

    fn read_property_path(
        &mut self,
        cursor: &mut Cursor<'_>,
        target: ClassId,
    ) -> Result<TimelineProperty, XurError> {
        let offset = cursor.offset();
        let header = cursor.u8().map_err(data_error)?;
        let depth = header & 0x7F;

        let path = if depth > 0 {
            Some(self.resolve_path(cursor, offset, target, depth)?)
        } else {
            None
        };

        let element = if header & 0x80 != 0 {
            Some(ElementIndex(cursor.u32().map_err(data_error)?))
        } else {
            None
        };

        let has_value = path
            .and_then(|path| path.defs.of(&self.doc.path_defs).last().copied())
            .is_some_and(|def| KeyframeType::from_prop(def.ty).is_some());

        Ok(TimelineProperty {
            path,
            element,
            has_value,
        })
    }

    fn resolve_path(
        &mut self,
        cursor: &mut Cursor<'_>,
        offset: usize,
        target: ClassId,
        depth: u8,
    ) -> Result<ResolvedPath, XurError> {
        let levels_up = LevelsUp(cursor.u8().map_err(data_error)?);
        let index = PropIndex(cursor.u8().map_err(data_error)?);

        let Some(target_class) = self.registry.class(target) else {
            unreachable!("object classes come from this registry");
        };
        let Some(level) = target_class.ancestor(levels_up.0) else {
            return Err(XurError::PropertyPathLevel {
                offset,
                class: target_class.name(),
                levels_up: levels_up.0,
            });
        };

        let Some(mut def) = level.def().props.get(usize::from(index.0)) else {
            return Err(XurError::PropertyPathIndex {
                offset,
                class: level.name(),
                index: index.0,
            });
        };

        let defs_start = self.doc.path_defs.len();
        self.doc.path_defs.push(def);
        for _ in 1..depth {
            let sub_index = cursor.u8().map_err(data_error)?;
            let Some(sub_defs) = def.ty.compound() else {
                return Err(XurError::PropertyPathNotCompound {
                    offset,
                    property: def.name,
                });
            };

            let Some(sub) = sub_defs.get(usize::from(sub_index)) else {
                return Err(XurError::PropertyPathSubIndex {
                    offset,
                    property: def.name,
                    index: sub_index,
                });
            };
            def = sub;
            self.doc.path_defs.push(def);
        }

        Ok(ResolvedPath {
            class: level.id(),
            levels_up,
            index,
            defs: Span::new(defs_start, self.doc.path_defs.len()),
        })
    }

    fn read_keyframe(&mut self, cursor: &mut Cursor<'_>) -> Result<Keyframe, XurError> {
        let time = FrameTime(cursor.u32().map_err(data_error)?);
        let interpolation = Interpolation(cursor.u8().map_err(data_error)?);
        let ease_in = EaseIn(cursor.u8().map_err(data_error)?);
        let ease_out = EaseOut(cursor.u8().map_err(data_error)?);
        let ease_scale = EaseScale(cursor.u8().map_err(data_error)?);

        let values_start = self.doc.keyframe_values.len();
        for index in 0..self.value_types.len() {
            let value = self.read_keyframe_value(cursor, self.value_types[index])?;
            self.doc.keyframe_values.push(value);
        }

        Ok(Keyframe {
            time,
            interpolation,
            ease_in,
            ease_out,
            ease_scale,
            values: Span::new(values_start, self.doc.keyframe_values.len()),
        })
    }

    fn read_keyframe_value(
        &self,
        cursor: &mut Cursor<'_>,
        ty: KeyframeType,
    ) -> Result<KeyframeValue, XurError> {
        let offset = cursor.offset();

        let value = match ty {
            KeyframeType::Bool => KeyframeValue::Bool(cursor.u8().map_err(data_error)? != 0),
            KeyframeType::Integer => {
                KeyframeValue::Integer(cursor.u32().map_err(data_error)? as i32)
            }
            KeyframeType::Unsigned => KeyframeValue::Unsigned(cursor.u32().map_err(data_error)?),
            KeyframeType::Float => KeyframeValue::Float(cursor.f32().map_err(data_error)?),
            KeyframeType::String => KeyframeValue::String(self.checked_string(cursor)?),
            KeyframeType::Color => {
                KeyframeValue::Color(Color::from_argb(cursor.u32().map_err(data_error)?))
            }
            KeyframeType::Vector => {
                let index = cursor.u32().map_err(data_error)?;
                let Some(vector) = self.doc.vector(VectorIndex(index)) else {
                    return Err(XurError::InvalidVectorIndex { offset, index });
                };
                KeyframeValue::Vector(vector)
            }
            KeyframeType::Quaternion => {
                let index = cursor.u32().map_err(data_error)?;
                let Some(quaternion) = self.doc.quaternion(QuaternionIndex(index)) else {
                    return Err(XurError::InvalidQuaternionIndex { offset, index });
                };
                KeyframeValue::Quaternion(quaternion)
            }
        };

        Ok(value)
    }

    /// String index that must be 0 or within the table.
    fn checked_string(&self, cursor: &mut Cursor<'_>) -> Result<Option<StringId>, XurError> {
        let offset = cursor.offset();
        let index = cursor.u16().map_err(data_error)?;
        if usize::from(index) > self.doc.strings.len() {
            return Err(XurError::InvalidStringIndex { offset, index });
        }

        Ok(StringId::new(index))
    }
}

fn read_element_count(cursor: &mut Cursor<'_>) -> Result<u32, XurError> {
    let first = cursor.u8().map_err(data_error)?;
    if first & 0x80 == 0 {
        return Ok(u32::from(first));
    }

    let count = match first & 0x7F {
        1 => u32::from(cursor.u8().map_err(data_error)?),
        2 => u32::from(cursor.u16().map_err(data_error)?),
        4 => cursor.u32().map_err(data_error)?,
        skipped => {
            cursor.bytes(usize::from(skipped)).map_err(data_error)?;
            0
        }
    };

    Ok(count)
}

/// Reads a value in property block encoding. Table indices are stored
/// unchecked, as the original resolves them on access.
fn read_block_value(cursor: &mut Cursor<'_>, ty: ScalarType) -> Result<Value, XurError> {
    let value = match ty {
        ScalarType::Bool => Value::Bool(cursor.u8().map_err(data_error)? != 0),
        ScalarType::Integer => Value::Integer(cursor.u32().map_err(data_error)? as i32),
        ScalarType::Unsigned => Value::Unsigned(cursor.u32().map_err(data_error)?),
        ScalarType::Float => Value::Float(cursor.f32().map_err(data_error)?),
        ScalarType::String => Value::String(StringId::new(cursor.u16().map_err(data_error)?)),
        ScalarType::Color => Value::Color(Color::from_argb(cursor.u32().map_err(data_error)?)),
        ScalarType::Vector => Value::Vector(VectorIndex(cursor.u32().map_err(data_error)?)),
        ScalarType::Quaternion => {
            Value::Quaternion(QuaternionIndex(cursor.u32().map_err(data_error)?))
        }
        ScalarType::Custom => Value::Custom(CustomOffset(cursor.u32().map_err(data_error)?)),
    };

    Ok(value)
}

/// Big-endian floats of element `index` in a packed table.
fn floats<const N: usize>(table: &[u8], index: u32) -> Option<[f32; N]> {
    let start = (index as usize).checked_mul(N * 4)?;
    let bytes = table.get(start..start.checked_add(N * 4)?)?;

    let mut out = [0.0; N];
    for (value, chunk) in out.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *value = f32::from_be_bytes(*chunk);
    }

    Some(out)
}

fn parse_strings(data: &[u8], section: Section) -> Result<Vec<WideStr<'_>>, XurError> {
    let slice = section.slice(data)?;
    let mut cursor = Cursor::new(slice, section.offset as usize);

    let mut strings = Vec::new();
    while cursor.remaining() > 0 {
        let offset = cursor.offset();
        let invalid = |_| XurError::InvalidStringTable { offset };

        // The counting pass masks bit 15 and the copy pass does not, so a
        // set bit 15 has no consistent meaning.
        let length = cursor.u16().map_err(invalid)?;
        if length & 0x8000 != 0 {
            return Err(XurError::InvalidStringTable { offset });
        }

        let bytes = cursor.bytes(usize::from(length) * 2).map_err(invalid)?;

        // Strings are used as C strings, so each ends at its first NUL.
        strings.push(WideStr::from_be_bytes(bytes));
    }

    Ok(strings)
}

/// A VECT or QUAT section, which must hold whole elements.
fn table_slice(data: &[u8], section: Section, element_size: usize) -> Result<&[u8], XurError> {
    let slice = section.slice(data)?;
    if slice.len() % element_size != 0 {
        return Err(XurError::MalformedFloatTable {
            tag: section.tag,
            size: section.size,
        });
    }

    Ok(slice)
}

fn truncated(region: Region, short: Short) -> XurError {
    XurError::Truncated {
        region,
        offset: short.offset,
        needed: short.needed,
    }
}

fn data_error(short: Short) -> XurError {
    truncated(Region::Section(SectionTag::DATA), short)
}

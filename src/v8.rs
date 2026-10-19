//! XUR version 8 reader.
//!
//! Version 8 comes from later dashboards than the one exui-core follows, and
//! no class registry of its hosts is known. Values are typed with the
//! version 5 class definitions of the [`Registry`]. A class the registry
//! lacks keeps `XuiElement` for its root level, since every class derives
//! from it; its other levels are read as untyped integers, which is exact
//! for every type except compounds and indexed properties. Layout notes are
//! in doc/xur.md.

use std::collections::HashMap;
use std::fmt;
use std::ptr;

use exui_core::Registry;
use exui_core::schema::ClassRef;
use exui_core::schema::PropDef;
use exui_core::schema::PropType;
use exui_core::schema::xui;
use exui_core::value::Color;
use exui_core::value::Quaternion;
use exui_core::value::Vector3;
use exui_core::xur::CustomOffset;
use exui_core::xur::EaseIn;
use exui_core::xur::EaseOut;
use exui_core::xur::EaseScale;
use exui_core::xur::ElementIndex;
use exui_core::xur::FrameCommand;
use exui_core::xur::FrameTime;
use exui_core::xur::Interpolation;
use exui_core::xur::PropIndex;

use crate::error::DecompileError;
use crate::error::V8Error;
use crate::scene;
use crate::scene::Element;
use crate::scene::ElementClass;
use crate::scene::ElementId;
use crate::scene::Keyframe;
use crate::scene::NamedFrame;
use crate::scene::Property;
use crate::scene::Scalar;
use crate::scene::Scene;
use crate::scene::SchemaVersion;
use crate::scene::Timeline;
use crate::scene::TimelineProperty;
use crate::scene::Value;

/// Four-character section tag.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tag(pub [u8; 4]);

impl Tag {
    pub const STRN: Self = Self(*b"STRN");
    pub const VECT: Self = Self(*b"VECT");
    pub const QUAT: Self = Self(*b"QUAT");
    pub const CUST: Self = Self(*b"CUST");
    pub const FLOT: Self = Self(*b"FLOT");
    pub const COLR: Self = Self(*b"COLR");
    pub const NAME: Self = Self(*b"NAME");
    pub const KEYD: Self = Self(*b"KEYD");
    pub const KEYP: Self = Self(*b"KEYP");
    pub const DATA: Self = Self(*b"DATA");
}

impl fmt::Display for Tag {
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

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({self})")
    }
}

/// Object record flag byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObjectFlags(u8);

impl ObjectFlags {
    const PROPERTIES: Self = Self(0x1);
    const CHILDREN: Self = Self(0x2);
    const TIMELINES: Self = Self(0x4);
    const ID: Self = Self(0x8);

    const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A byte window that remembers its absolute file offset.
#[derive(Clone, Copy)]
struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
    origin: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], origin: usize) -> Self {
        Self {
            data,
            position: 0,
            origin,
        }
    }

    fn offset(&self) -> usize {
        self.origin + self.position
    }

    fn is_empty(&self) -> bool {
        self.position == self.data.len()
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.position).copied()
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8], V8Error> {
        let truncated = V8Error::Truncated {
            offset: self.offset(),
        };
        let end = self.position.checked_add(count).ok_or(truncated.clone())?;
        let bytes = self.data.get(self.position..end).ok_or(truncated)?;
        self.position = end;

        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, V8Error> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, V8Error> {
        let bytes = self.bytes(2)?;

        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, V8Error> {
        let bytes = self.bytes(4)?;

        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// A byte below 0xF0 is its own value; 0xF0 to 0xFE carry the high four
    /// bits of a 12-bit value whose low byte follows; 0xFF introduces a u32.
    fn varint(&mut self) -> Result<u32, V8Error> {
        let first = self.u8()?;
        match first {
            0..0xF0 => Ok(u32::from(first)),
            0xF0..0xFF => Ok((u32::from(first & 0x0F) << 8) | u32::from(self.u8()?)),
            0xFF => self.u32(),
        }
    }
}

struct Section<'a> {
    tag: Tag,
    body: Cursor<'a>,
}

struct Sections<'a> {
    schema: SchemaVersion,
    table: Vec<Section<'a>>,
}

impl<'a> Sections<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, V8Error> {
        // Twelve varints follow the fixed header in every observed file. Their
        // meaning is not known.
        const PREAMBLE_VARINTS: usize = 12;

        let mut cursor = Cursor::new(data, 0);
        let magic = cursor.bytes(4)?;
        if magic != b"XUIB" {
            let mut found = [0; 4];
            found.copy_from_slice(magic);
            return Err(V8Error::BadMagic { found });
        }

        let version = cursor.u32()?;
        if version != 8 {
            return Err(V8Error::UnsupportedVersion { version });
        }

        let _flags = cursor.u32()?;
        let schema = SchemaVersion(cursor.u16()?);
        let file_size = cursor.u32()?;
        if usize::try_from(file_size).ok() != Some(data.len()) {
            return Err(V8Error::FileSizeMismatch {
                header: file_size,
                actual: data.len(),
            });
        }

        let section_count = cursor.u16()?;
        for _ in 0..PREAMBLE_VARINTS {
            cursor.varint()?;
        }

        let mut table = Vec::with_capacity(usize::from(section_count));
        for _ in 0..section_count {
            let mut tag = [0; 4];
            tag.copy_from_slice(cursor.bytes(4)?);
            let offset = cursor.u32()?;
            let size = cursor.u32()?;

            let tag = Tag(tag);
            let start = offset as usize;
            let body = start
                .checked_add(size as usize)
                .and_then(|end| data.get(start..end));
            let Some(body) = body else {
                return Err(V8Error::SectionOutOfBounds { tag, offset, size });
            };
            table.push(Section {
                tag,
                body: Cursor::new(body, start),
            });
        }

        Ok(Self { schema, table })
    }

    /// Last section with `tag`; an absent section reads as empty.
    fn get(&self, tag: Tag) -> Cursor<'a> {
        self.find(tag).unwrap_or(Cursor::new(&[], 0))
    }

    fn require(&self, tag: Tag) -> Result<Cursor<'a>, V8Error> {
        self.find(tag).ok_or(V8Error::MissingSection { tag })
    }

    fn find(&self, tag: Tag) -> Option<Cursor<'a>> {
        let section = self.table.iter().rev().find(|section| section.tag == tag)?;

        Some(section.body)
    }
}

#[derive(Clone, Copy)]
struct RawNamedFrame {
    name: u32,
    time: FrameTime,
    command: FrameCommand,
    target: Option<u32>,
}

#[derive(Clone, Copy)]
struct RawKeyframe {
    time: FrameTime,
    interpolation: Interpolation,
    ease: [u8; 3],
    values_start: usize,
}

struct Tables<'a> {
    strings: Vec<String>,
    floats: Cursor<'a>,
    colors: Cursor<'a>,
    vectors: Cursor<'a>,
    quaternions: Cursor<'a>,
    custom: Cursor<'a>,
    named_frames: Vec<RawNamedFrame>,
    keyframes: Vec<RawKeyframe>,
    keyframe_values: Vec<u32>,
}

impl Tables<'_> {
    /// One-based string reference; 0 and indices past the table are null.
    fn string(&self, index: u32) -> Option<String> {
        let slot = (index as usize).checked_sub(1)?;

        self.strings.get(slot).cloned()
    }

    fn element<const N: usize>(
        table: Cursor<'_>,
        tag: Tag,
        offset: usize,
        index: u32,
    ) -> Result<[u8; N], V8Error> {
        let invalid = V8Error::InvalidTableIndex { tag, offset, index };
        let start = (index as usize).checked_mul(N).ok_or(invalid.clone())?;
        let bytes = table.data.get(start..start + N).ok_or(invalid)?;

        let mut out = [0; N];
        out.copy_from_slice(bytes);

        Ok(out)
    }

    fn float(&self, offset: usize, index: u32) -> Result<f32, V8Error> {
        let bytes = Self::element::<4>(self.floats, Tag::FLOT, offset, index)?;

        Ok(f32::from_be_bytes(bytes))
    }

    fn color(&self, offset: usize, index: u32) -> Result<Color, V8Error> {
        let bytes = Self::element::<4>(self.colors, Tag::COLR, offset, index)?;

        Ok(Color::from_argb(u32::from_be_bytes(bytes)))
    }

    fn vector(&self, offset: usize, index: u32) -> Result<Vector3, V8Error> {
        let bytes = Self::element::<12>(self.vectors, Tag::VECT, offset, index)?;
        let [x, y, z] = floats(&bytes);

        Ok(Vector3::new(x, y, z))
    }

    fn quaternion(&self, offset: usize, index: u32) -> Result<Quaternion, V8Error> {
        let bytes = Self::element::<16>(self.quaternions, Tag::QUAT, offset, index)?;
        let [x, y, z, w] = floats(&bytes);

        Ok(Quaternion::new(x, y, z, w))
    }

    /// Blob at `offset` in CUST: a u32 length and that many bytes.
    fn custom_blob(&self, offset: CustomOffset) -> Option<&[u8]> {
        let start = offset.0 as usize;
        let prefix = self.custom.data.get(start..start.checked_add(4)?)?;
        let length = u32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;

        self.custom
            .data
            .get(start + 4..(start + 4).checked_add(length)?)
    }
}

/// Where the property definitions of an element come from.
#[derive(Clone, Copy)]
enum Levels<'r> {
    Known(ClassRef<'r>),
    /// Class the registry lacks, with the number of property blocks read.
    Unknown {
        count: usize,
    },
}

#[derive(Clone, Copy)]
struct ObjectFrame {
    id: ElementId,
    flags: ObjectFlags,
    children_left: u32,
    follower_may_be_zero: bool,
}

struct Reader<'r, 't, 'a> {
    registry: &'r Registry,
    tables: &'t Tables<'a>,
    elements: Vec<Element>,
    /// Definition source of each element, by element index.
    levels: Vec<Levels<'r>>,
    /// Compounds by slot. A slot read again has no inline body and reuses
    /// the first value.
    compounds: HashMap<u32, Vec<Property>>,
}

impl<'r> Reader<'r, '_, '_> {
    fn read_tree(&mut self, cursor: &mut Cursor<'_>) -> Result<(), V8Error> {
        // Nothing follows the root.
        let root = self.read_object(cursor, false)?;
        let mut stack = vec![root];

        while let Some(top) = stack.last_mut() {
            if top.children_left > 0 {
                top.children_left -= 1;
                let parent = top.id;

                // A sibling starts with a class index, which is never 0. The
                // last child is followed by the parent's named frame count
                // or whatever follows the parent.
                let follower_may_be_zero = top.children_left == 0
                    && (top.flags.contains(ObjectFlags::TIMELINES) || top.follower_may_be_zero);

                let child = self.read_object(cursor, follower_may_be_zero)?;
                self.elements[parent.0].children.push(child.id);
                stack.push(child);
                continue;
            }

            let Some(done) = stack.pop() else {
                break;
            };

            if done.flags.contains(ObjectFlags::TIMELINES) {
                self.read_timelines(cursor, done)?;
            }
        }

        Ok(())
    }

    /// `follower_may_be_zero` tells whether the field after the object's
    /// record, children and timelines may be 0.
    fn read_object(
        &mut self,
        cursor: &mut Cursor<'_>,
        follower_may_be_zero: bool,
    ) -> Result<ObjectFrame, V8Error> {
        let offset = cursor.offset();
        let class_index = cursor.varint()?;
        let flags = ObjectFlags(cursor.u8()?);

        let Some(name) = self.tables.string(class_index) else {
            return Err(V8Error::InvalidClassString {
                offset,
                index: class_index,
            });
        };
        let known = self
            .registry
            .find(&name)
            .and_then(|id| self.registry.class(id));

        let mut properties = Vec::new();
        let mut level_count = 0;
        if flags.contains(ObjectFlags::PROPERTIES) {
            // The count of set mask bits over all levels.
            let declared = cursor.varint()?;
            let mut actual = 0u32;
            match known {
                Some(class) => {
                    for level in class.levels() {
                        actual += self.read_block(cursor, level.def().props, &mut properties)?;
                        level_count += 1;
                    }
                }
                None => {
                    while actual < declared {
                        let defs = unknown_level_defs(level_count);
                        actual += self.read_block(cursor, defs, &mut properties)?;
                        level_count += 1;
                    }
                }
            }

            if actual != declared {
                return Err(V8Error::PropertyCountMismatch {
                    offset,
                    class: name,
                    declared,
                    actual,
                });
            }

            // The empty levels after the last non-empty one of an unknown
            // class are single 0 bytes. They can be told apart from the next
            // field only when that field is never 0.
            if known.is_none() && cursor.peek() == Some(0) {
                let next_may_be_zero = flags.contains(ObjectFlags::ID)
                    || flags.contains(ObjectFlags::CHILDREN)
                    || flags.contains(ObjectFlags::TIMELINES)
                    || follower_may_be_zero;
                if next_may_be_zero {
                    return Err(V8Error::AmbiguousLevelCount {
                        offset: cursor.offset(),
                    });
                }

                while cursor.peek() == Some(0) {
                    cursor.position += 1;
                    level_count += 1;
                }
            }
        }

        if flags.contains(ObjectFlags::ID) {
            let _id = cursor.varint()?;
        }

        let children_left = if flags.contains(ObjectFlags::CHILDREN) {
            cursor.varint()?
        } else {
            0
        };

        let levels = match known {
            Some(class) => Levels::Known(class),
            None => Levels::Unknown { count: level_count },
        };
        let class = match known {
            Some(class) => ElementClass::Known(class.def()),
            None => ElementClass::Unknown(name),
        };

        let element = ElementId(self.elements.len());
        self.elements.push(Element {
            class,
            properties,
            children: Vec::new(),
            named_frames: Vec::new(),
            timelines: Vec::new(),
        });
        self.levels.push(levels);

        Ok(ObjectFrame {
            id: element,
            flags,
            children_left,
            follower_may_be_zero,
        })
    }

    /// Reads a varint mask and a value per set bit, appending the
    /// properties to `out`. Returns the number of set bits.
    fn read_block(
        &mut self,
        cursor: &mut Cursor<'_>,
        defs: &'static [PropDef],
        out: &mut Vec<Property>,
    ) -> Result<u32, V8Error> {
        let mask = cursor.varint()?;

        for bit in 0..32u8 {
            if mask & (1 << bit) == 0 {
                continue;
            }

            let def = defs.get(usize::from(bit));
            let count = if def.is_some_and(|def| def.flags.is_indexed()) {
                cursor.varint()?
            } else {
                1
            };

            let mut values = Vec::new();
            for _ in 0..count {
                values.push(self.read_value(cursor, def)?);
            }
            out.push(Property {
                def,
                index: PropIndex(bit),
                values,
            });
        }

        Ok(mask.count_ones())
    }

    /// Booleans are a raw byte, compounds are nested blocks and every
    /// other value is a varint, either the value itself or a table index.
    fn read_value(
        &mut self,
        cursor: &mut Cursor<'_>,
        def: Option<&'static PropDef>,
    ) -> Result<Value, V8Error> {
        let offset = cursor.offset();
        let Some(def) = def else {
            return Ok(Value::Scalar(Scalar::Raw(cursor.varint()?)));
        };

        let scalar = match def.ty {
            PropType::Bool => Scalar::Bool(cursor.u8()? != 0),
            PropType::Integer => Scalar::Integer(cursor.varint()? as i32),
            PropType::Unsigned => Scalar::Unsigned(cursor.varint()?),
            PropType::Float => Scalar::Float(self.tables.float(offset, cursor.varint()?)?),
            PropType::String => Scalar::String(self.tables.string(cursor.varint()?)),
            PropType::Color => Scalar::Color(self.tables.color(offset, cursor.varint()?)?),
            PropType::Vector => Scalar::Vector(self.tables.vector(offset, cursor.varint()?)?),
            PropType::Quaternion => {
                Scalar::Quaternion(self.tables.quaternion(offset, cursor.varint()?)?)
            }
            PropType::Custom => {
                let custom = CustomOffset(cursor.varint()?);
                let value = scene::custom_value(def, custom, self.tables.custom_blob(custom))?;
                return Ok(value);
            }
            PropType::Compound(sub_defs) => {
                return self.read_compound(cursor, sub_defs);
            }
        };

        Ok(Value::Scalar(scalar))
    }

    /// Compound: a slot, then on the slot's first use a value count and a
    /// block over the sub-definitions. The definition tables bound the
    /// nesting depth.
    fn read_compound(
        &mut self,
        cursor: &mut Cursor<'_>,
        sub_defs: &'static [PropDef],
    ) -> Result<Value, V8Error> {
        let slot = cursor.varint()?;
        if let Some(properties) = self.compounds.get(&slot) {
            return Ok(Value::Compound(properties.clone()));
        }

        let _count = cursor.varint()?;
        let mut properties = Vec::new();
        self.read_block(cursor, sub_defs, &mut properties)?;
        self.compounds.insert(slot, properties.clone());

        Ok(Value::Compound(properties))
    }

    /// Named frame count, and when non-zero their start in NAME. With
    /// children, the timelines follow.
    fn read_timelines(
        &mut self,
        cursor: &mut Cursor<'_>,
        frame: ObjectFrame,
    ) -> Result<(), V8Error> {
        let offset = cursor.offset();
        let frame_count = cursor.varint()? as usize;
        let named_frames = if frame_count > 0 {
            let start = cursor.varint()? as usize;
            let frames = start
                .checked_add(frame_count)
                .and_then(|end| self.tables.named_frames.get(start..end));
            let Some(frames) = frames else {
                return Err(V8Error::InvalidTableIndex {
                    tag: Tag::NAME,
                    offset,
                    index: start as u32,
                });
            };
            frames
                .iter()
                .map(|frame| NamedFrame {
                    name: self.tables.string(frame.name),
                    time: frame.time,
                    command: frame.command,
                    target: frame.target.and_then(|target| self.tables.string(target)),
                })
                .collect()
        } else {
            Vec::new()
        };
        self.elements[frame.id.0].named_frames = named_frames;

        if !frame.flags.contains(ObjectFlags::CHILDREN) {
            return Ok(());
        }

        let timeline_count = cursor.varint()?;
        let mut timelines = Vec::new();
        for _ in 0..timeline_count {
            timelines.push(self.read_timeline(cursor, frame.id)?);
        }
        self.elements[frame.id.0].timelines = timelines;

        Ok(())
    }

    /// Target string, paths, keyframe count and the keyframes' start in KEYD.
    fn read_timeline(
        &self,
        cursor: &mut Cursor<'_>,
        parent: ElementId,
    ) -> Result<Timeline, V8Error> {
        let offset = cursor.offset();
        let element = cursor.varint()?;
        let path_count = cursor.varint()?;

        // As in version 5, a timeline without properties names no child.
        let mut properties = Vec::new();
        if path_count > 0 {
            let wanted = self.tables.string(element);
            let target = self.elements[parent.0]
                .children
                .iter()
                .copied()
                .find(|child| wanted.is_some() && self.element_id(*child) == wanted.as_deref());
            let Some(target) = target else {
                return Err(V8Error::TimelineTargetNotFound { offset });
            };

            for _ in 0..path_count {
                properties.push(self.read_path(cursor, target)?);
            }
        }

        let keyframes_offset = cursor.offset();
        let keyframe_count = cursor.varint()? as usize;
        let keyframe_start = cursor.varint()? as usize;
        let raw_keyframes = keyframe_start
            .checked_add(keyframe_count)
            .and_then(|end| self.tables.keyframes.get(keyframe_start..end));
        let Some(raw_keyframes) = raw_keyframes else {
            return Err(V8Error::InvalidTableIndex {
                tag: Tag::KEYD,
                offset: keyframes_offset,
                index: keyframe_start as u32,
            });
        };

        let mut keyframes = Vec::with_capacity(keyframe_count);
        for raw in raw_keyframes {
            let mut values = Vec::with_capacity(properties.len());
            for (position, property) in properties.iter().enumerate() {
                let index = raw.values_start + position;
                let Some(&value) = self.tables.keyframe_values.get(index) else {
                    return Err(V8Error::InvalidTableIndex {
                        tag: Tag::KEYP,
                        offset: keyframes_offset,
                        index: index as u32,
                    });
                };
                let def = property.path.last().copied().flatten();
                values.push(self.keyframe_value(keyframes_offset, def, value)?);
            }

            keyframes.push(Keyframe {
                time: raw.time,
                interpolation: raw.interpolation,
                ease_in: EaseIn(raw.ease[0]),
                ease_out: EaseOut(raw.ease[1]),
                ease_scale: EaseScale(raw.ease[2]),
                values,
            });
        }

        Ok(Timeline {
            element: self.tables.string(element),
            properties,
            keyframes,
        })
    }

    /// Depth and element flag byte, then for a non-zero depth the levels up,
    /// the property index and one sub-index per compound step, then the
    /// element index when flagged.
    fn read_path(
        &self,
        cursor: &mut Cursor<'_>,
        target: ElementId,
    ) -> Result<TimelineProperty, V8Error> {
        let header = cursor.u8()?;
        let depth = header & 0x7F;
        if depth == 0 {
            return Err(DecompileError::EmptyPropertyPath.into());
        }

        let levels_up = cursor.u8()?;
        let index = cursor.u8()?;
        let mut def = self.level_def(target, levels_up, index);
        let mut path = vec![def];
        for _ in 1..depth {
            let sub_index = cursor.u8()?;
            def = def
                .and_then(|def| def.ty.compound())
                .and_then(|sub_defs| sub_defs.get(usize::from(sub_index)));
            path.push(def);
        }

        let element = if header & 0x80 != 0 {
            Some(ElementIndex(cursor.varint()?))
        } else {
            None
        };

        Ok(TimelineProperty { path, element })
    }

    /// The loaded `XuiElement.Id` of an element, the string timelines
    /// match against.
    fn element_id(&self, element: ElementId) -> Option<&str> {
        let id_def = &xui::XUI_ELEMENT.props[0];
        let property = self.elements[element.0]
            .properties
            .iter()
            .find(|property| property.def.is_some_and(|def| ptr::eq(def, id_def)))?;

        match property.values.first()? {
            Value::Scalar(Scalar::String(Some(text))) => Some(text),
            _ => None,
        }
    }

    /// Definition `index` of the class level `levels_up` above `target`.
    fn level_def(&self, target: ElementId, levels_up: u8, index: u8) -> Option<&'static PropDef> {
        let defs = match self.levels[target.0] {
            Levels::Known(class) => class.ancestor(levels_up)?.def().props,
            Levels::Unknown { count } => {
                let level = count.checked_sub(usize::from(levels_up) + 1)?;
                unknown_level_defs(level)
            }
        };

        defs.get(usize::from(index))
    }

    fn keyframe_value(
        &self,
        offset: usize,
        def: Option<&'static PropDef>,
        value: u32,
    ) -> Result<Scalar, V8Error> {
        let Some(def) = def else {
            return Ok(Scalar::Raw(value));
        };

        let scalar = match def.ty {
            PropType::Bool => Scalar::Bool(value != 0),
            PropType::Integer => Scalar::Integer(value as i32),
            PropType::Unsigned => Scalar::Unsigned(value),
            PropType::Float => Scalar::Float(self.tables.float(offset, value)?),
            PropType::String => Scalar::String(self.tables.string(value)),
            PropType::Color => Scalar::Color(self.tables.color(offset, value)?),
            PropType::Vector => Scalar::Vector(self.tables.vector(offset, value)?),
            PropType::Quaternion => Scalar::Quaternion(self.tables.quaternion(offset, value)?),
            PropType::Compound(_) | PropType::Custom => Scalar::Raw(value),
        };

        Ok(scalar)
    }
}

/// Reads a version 8 file into a scene.
pub fn parse(data: &[u8], registry: &Registry) -> Result<Scene, V8Error> {
    let sections = Sections::parse(data)?;
    let strings = parse_strings(sections.require(Tag::STRN)?)?;

    let tables = Tables {
        strings,
        floats: sections.get(Tag::FLOT),
        colors: sections.get(Tag::COLR),
        vectors: sections.get(Tag::VECT),
        quaternions: sections.get(Tag::QUAT),
        custom: sections.get(Tag::CUST),
        named_frames: parse_named_frames(sections.get(Tag::NAME))?,
        keyframes: parse_keyframes(sections.get(Tag::KEYD))?,
        keyframe_values: parse_varints(sections.get(Tag::KEYP))?,
    };

    let mut reader = Reader {
        registry,
        tables: &tables,
        elements: Vec::new(),
        levels: Vec::new(),
        compounds: HashMap::new(),
    };
    reader.read_tree(&mut sections.require(Tag::DATA)?)?;

    Ok(Scene {
        schema: sections.schema,
        allocation_hints: false,
        elements: reader.elements,
    })
}

/// STRN: u32 byte length, u16 count, then that many NUL-terminated strings
/// of one byte per character.
fn parse_strings(mut cursor: Cursor<'_>) -> Result<Vec<String>, V8Error> {
    let _length = cursor.u32()?;
    let count = cursor.u16()?;

    let mut strings = Vec::with_capacity(usize::from(count));
    for index in 0..count {
        let rest = &cursor.data[cursor.position..];
        let Some(end) = rest.iter().position(|byte| *byte == 0) else {
            return Err(V8Error::UnterminatedString { index });
        };
        strings.push(rest[..end].iter().map(|byte| char::from(*byte)).collect());
        cursor.position += end + 1;
    }

    Ok(strings)
}

/// NAME: named frames, each a name, a time, a command byte and, for
/// commands 2 and above, a target name.
fn parse_named_frames(mut cursor: Cursor<'_>) -> Result<Vec<RawNamedFrame>, V8Error> {
    let mut frames = Vec::new();
    while !cursor.is_empty() {
        let name = cursor.varint()?;
        let time = FrameTime(cursor.varint()?);
        let command = FrameCommand(cursor.u8()?);
        let target = if command.0 >= 2 {
            Some(cursor.varint()?)
        } else {
            None
        };
        frames.push(RawNamedFrame {
            name,
            time,
            command,
            target,
        });
    }

    Ok(frames)
}

/// KEYD: keyframes, each a frame, a flag byte whose low six bits select the
/// interpolation data that follows, and the start of its values in KEYP.
fn parse_keyframes(mut cursor: Cursor<'_>) -> Result<Vec<RawKeyframe>, V8Error> {
    let mut keyframes = Vec::new();
    while !cursor.is_empty() {
        let time = FrameTime(cursor.varint()?);
        let mode = cursor.u8()? & 0x3F;

        let mut ease = [0; 3];
        match mode {
            2 => ease.copy_from_slice(cursor.bytes(3)?),
            0xA => {
                cursor.varint()?;
            }
            0xB => {
                cursor.u8()?;
            }
            _ => {
                // No interpolation data.
            }
        }
        let values_start = cursor.varint()? as usize;

        // XUI text knows linear (0), none (1) and ease (2); the other modes
        // are written as linear.
        let interpolation = match mode {
            1 | 2 => Interpolation(mode),
            _ => Interpolation(0),
        };
        keyframes.push(RawKeyframe {
            time,
            interpolation,
            ease,
            values_start,
        });
    }

    Ok(keyframes)
}

/// KEYP: keyframe value references.
fn parse_varints(mut cursor: Cursor<'_>) -> Result<Vec<u32>, V8Error> {
    let mut values = Vec::new();
    while !cursor.is_empty() {
        values.push(cursor.varint()?);
    }

    Ok(values)
}

fn floats<const N: usize, const M: usize>(bytes: &[u8; M]) -> [f32; N] {
    let mut out = [0.0; N];
    for (value, chunk) in out.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *value = f32::from_be_bytes(*chunk);
    }

    out
}

/// Definitions of property block `level`, root first, of a class the
/// registry lacks. Every class derives from `XuiElement`.
fn unknown_level_defs(level: usize) -> &'static [PropDef] {
    if level == 0 {
        xui::XUI_ELEMENT.props
    } else {
        &[]
    }
}

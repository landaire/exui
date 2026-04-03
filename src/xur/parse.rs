//! XUR v5 binary parser.
//!
//! Encoding rules (from xam_1888.exe reverse engineering):
//!
//! Object: u16(class_name) + u8(flags) + [properties] + [children]
//!
//! Properties: u16(total_value_count) + LoadObjectPropsFromBinary
//!   LoadObjectPropsFromBinary walks class hierarchy via recursion:
//!     1. Recurse for base class
//!     2. LoadPropertiesFromBinary for this class's own props
//!
//! LoadPropertiesFromBinary:
//!   Read bitmask byte: low 3 bits = N (extra bytes: 0, 1, 2, or 4)
//!     N==0: no properties (bitmask empty)
//!     N==1: bitmask = u8 BE at offset+1
//!     N==2: bitmask = u16 BE at offset+1
//!     N==4: bitmask = u32 BE at offset+1
//!   For each set bit: read typed value
//!
//! Compound (type 9): u16(value_count) + recursive LoadPropertiesFromBinary
//!
//! Array properties: first byte: if < 0x80 -> count = byte; else consume (byte & 0x7f)+1 bytes
//!
//! Children: u32(child_count) + child_count * Object

use std::io::Cursor;

use byteorder::BigEndian;
use byteorder::ByteOrder;
use byteorder::ReadBytesExt;
use tracing::debug;
use tracing::trace;

use super::Header;
use super::Object;
use super::PropType;
use super::PropertyGroup;
use super::PropertyValue;
use super::SectionHeader;
use super::Xur;
use super::XUIB_MAGIC;
use super::object_flags;
use super::section_tag;

// -- Error type --

#[derive(Debug)]
pub enum ParseError {
    TooShort { needed: usize, available: usize },
    BadMagic([u8; 4]),
    MissingSection(&'static str),
    BadString { offset: usize },
    BadObject(String),
    Io(std::io::Error),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { needed, available } => {
                write!(f, "need {needed} bytes, only {available} available")
            }
            Self::BadMagic(m) => write!(f, "bad magic: {m:02x?}"),
            Self::MissingSection(s) => write!(f, "missing required section: {s}"),
            Self::BadString { offset } => write!(f, "bad string at offset {offset}"),
            Self::BadObject(msg) => write!(f, "bad object: {msg}"),
            Self::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// -- Encoding primitives --

fn pos(c: &Cursor<&[u8]>) -> usize {
    c.position() as usize
}

/// Read a v5 length-prefixed bitmask.
/// First byte: low 3 bits = N (number of bitmask data bytes after this byte).
/// N==0: empty (no bits set). N==1: u8. N==2: u16 BE. N==4: u32 BE.
/// Returns the bitmask value. High 5 bits of first byte are metadata (ignored here).
fn read_v5_bitmask(c: &mut Cursor<&[u8]>) -> Result<u32, ParseError> {
    let first = c.read_u8()?;
    let n = first & 7;
    match n {
        0 => Ok(0),
        1 => Ok(u32::from(c.read_u8()?)),
        2 => Ok(u32::from(c.read_u16::<BigEndian>()?)),
        4 => Ok(c.read_u32::<BigEndian>()?),
        _ => {
            // Sizes 3, 5, 6, 7 are not used by the v5 runtime
            for _ in 0..n {
                c.read_u8()?;
            }
            Ok(0)
        }
    }
}

/// Read a v5 array count prefix.
/// First byte: if < 0x80, count = byte value (1 byte consumed).
/// If >= 0x80, (byte & 0x7f) + 1 bytes consumed total for the count encoding.
fn read_v5_array_count(c: &mut Cursor<&[u8]>) -> Result<u32, ParseError> {
    let first = c.read_u8()?;
    if first < 0x80 {
        Ok(u32::from(first))
    } else {
        // Multi-byte count: (first & 0x7f) additional bytes
        let extra = (first & 0x7f) as usize;
        let mut val = 0u32;
        for _ in 0..extra {
            val = (val << 8) | u32::from(c.read_u8()?);
        }
        Ok(val)
    }
}

// -- v5 property type tables --
//
// IMPORTANT: These tables are v5-specific. v8 (xamd.dll) has different orderings:
//   - XuiElement: v5 has Show=7/Anchor=9; v8 has Anchor=7/Show=9. v8 has 27 props vs ~14.
//   - Fill: v5 has index 2 as TextureFileName(string); v8 same. Indices 4-9 differ.
//   - Gradient: v5 has Radial=0/NumStops=1; v8 same ordering confirmed.
//   - XuiControl: v5 indices 14-15 may differ from v8 (SizeToText vs ClipChildren).
//   - XuiHtmlElement: v5 only, does not exist in v8.
//
// The v5 tables below are validated against simple.xur and controls_demo1.xur test data.
// The v8 tables would need separate definitions when v8 parsing is implemented.

use PropType::Bool;
use PropType::Color;
use PropType::Compound;
use PropType::Custom;
use PropType::Float;
use PropType::Integer;
use PropType::Quaternion;
use PropType::Unsigned;
use PropType::Vector3;
// Note: PropType::String conflicts with std String, use qualified below

// v5 XuiElement: 14 properties (v8 has 27 with different ordering at indices 7+)
const XUIELEMENT_TYPES_V5: &[PropType] = &[
    PropType::String, // 0: Id
    Float,            // 1: Width
    Float,            // 2: Height
    Vector3,          // 3: Position
    Vector3,          // 4: Scale
    Quaternion,       // 5: Rotation
    Float,            // 6: Opacity
    Unsigned,         // 7: Show           (v8: Anchor at 7)
    Vector3,          // 8: Pivot
    Bool,             // 9: Anchor         (v8: Show at 9)
    Unsigned,         // 10: BlendMode
    Bool,             // 11: DisableTimelineRecursion
    Bool,             // 12: ColorWriteFlags
    Color,            // 13: ColorFactor
];

// XuiFigure: same between v5 and v8
const XUIFIGURE_TYPES: &[PropType] = &[
    Compound, // 0: Stroke
    Compound, // 1: Fill
    Bool,     // 2: Closed
    Custom,   // 3: Points (u32 CUST offset)
];

// Stroke: same between v5 and v8
const STROKE_TYPES: &[PropType] = &[
    Float, // 0: StrokeWidth
    Color, // 1: StrokeColor
];

// Fill: 11 properties. Same ordering between v5 and v8.
// bitmask 0x0408 = bits{3,10} gives Gradient + TransformVersion
const FILL_TYPES: &[PropType] = &[
    Unsigned,         // 0: FillType
    Color,            // 1: FillColor
    PropType::String, // 2: TextureFileName (confirmed from v8 _GetPropDef)
    Compound,         // 3: Gradient
    Vector3,          // 4: Translation
    Vector3,          // 5: Scale
    Float,            // 6: Angle
    Unsigned,         // 7: WrapX
    Unsigned,         // 8: WrapY
    Unsigned,         // 9: BrushFlags
    Unsigned,         // 10: TransformVersion
];

// Gradient: same ordering between v5 and v8.
const GRADIENT_TYPES: &[PropType] = &[
    Bool,  // 0: Radial          (confirmed from v8)
    Unsigned, // 1: NumStops      (confirmed from v5 test data: bit 1 = uint value 1)
    Color,    // 2: StopColor     (array property, confirmed from v5 test data)
    Float,    // 3: StopPos       (array property, confirmed from v5 test data)
];

// Gradient properties that are indexed arrays (have array count prefix).
fn is_gradient_array_prop(bit: u32) -> bool {
    matches!(bit, 2 | 3) // StopColor and StopPos
}

// v5 XuiControl: 19 properties (indices 14-15 may differ from v8)
const XUICONTROL_TYPES_V5: &[PropType] = &[
    PropType::String, // 0: ClassOverride
    PropType::String, // 1: Visual
    Bool,             // 2: Enabled
    Bool,             // 3: UnfocusedInput
    PropType::String, // 4: NavLeft
    PropType::String, // 5: NavRight
    PropType::String, // 6: NavUp
    PropType::String, // 7: NavDown
    PropType::String, // 8: NavTabForward
    PropType::String, // 9: NavTabBackward
    PropType::String, // 10: Text
    Float,            // 11: PointSize
    PropType::String, // 12: ImagePath
    Bool,             // 13: HasContextMenu
    Bool,             // 14: SizeToText     (v8: ClipChildren)
    Bool,             // 15: UseNuiAsMouse  (v8: EnableEffects)
    PropType::String, // 16: AutoId
    Float,            // 17: HoverSelectTimer
    Bool,             // 18: QuickInput
];

const XUISCENE_TYPES: &[PropType] = &[
    PropType::String, // 0: DefaultFocus
    PropType::String, // 1: TransFrom
    PropType::String, // 2: TransTo
    PropType::String, // 3: TransBackFrom
    PropType::String, // 4: TransBackTo
    Unsigned,         // 5: InterruptTransitions
    Bool,             // 6: IgnorePresses
    Bool,             // 7: RecurseTransitions
];

const XUIBUTTON_TYPES: &[PropType] = &[
    Unsigned,         // 0: PressKey
    PropType::String, // 1: PressPath
    PropType::String, // 2: PressAction
    PropType::String, // 3: BackPath
    PropType::String, // 4: BackAction
    PropType::String, // 5: OverPath
    PropType::String, // 6: OverAction
];

const XUINAVBUTTON_TYPES: &[PropType] = &[
    PropType::String, // 0: PressPath
    Bool,             // 1: StayVisible
    Unsigned,         // 2: SrcTransIndex
    Unsigned,         // 3: DestTransIndex
];

const XUITABSCENE_TYPES: &[PropType] = &[
    Unsigned, // 0: TabCount
    Bool,     // 1: Wrap
    Bool,     // 2: UserInterrupt
    Bool,     // 3: VerticalTabs
    Bool,     // 4: NoAutoHide
    Unsigned, // 5: DefaultTab
];

const XUITEXT_TYPES: &[PropType] = &[
    PropType::String, // 0: Text
    Color,            // 1: TextColor
    Color,            // 2: DropShadowColor
    Float,            // 3: PointSize
    PropType::String, // 4: Font
    Unsigned,         // 5: TextStyle
    Integer,          // 6: LineSpacingAdjust
    Float,            // 7: TextScale
];

const XUIIMAGE_TYPES: &[PropType] = &[
    Unsigned,         // 0: SizeMode
    PropType::String, // 1: ImagePath
    Unsigned,         // 2: BrushFlags
    PropType::String, // 3: TextureSurfaceElement
    Unsigned,         // 4: LoadType
];

// -- Class hierarchy --
// Returns the full chain of type tables from XuiElement outward.
// XuiElement is always read first (handled separately), so this returns
// only the DERIVED levels in order (innermost derived first).
// Confirmed from CXuiClassBase<T>::Register() in xamd.dll.

fn get_hierarchy(class_name: &str) -> &'static [&'static [PropType]] {
    match class_name {
        // Direct children of XuiElement (1 derived level)
        "XuiCanvas" => &[&[]],
        "XuiFigure" => &[XUIFIGURE_TYPES],
        "XuiText" => &[XUITEXT_TYPES],
        "XuiImage" => &[XUIIMAGE_TYPES],
        "XuiGroup" => &[&[]],
        "XuiNineGrid" => &[&[]],
        "XuiSound" => &[&[]],
        "XuiVisual" => &[&[]],
        "XuiTransition" => &[&[]],
        "XuiImagePresenter" => &[&[]],
        "XuiTextPresenter" => &[&[]],
        "XuiGridPanel" => &[&[]],
        "XuiShader" => &[&[]],
        "XuiVariable" => &[&[]],
        // XuiElement -> XuiControl (2 derived levels)
        "XuiControl" => &[XUICONTROL_TYPES_V5],
        "XuiLabel" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiCheckbox" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiRadioButton" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiRadioGroup" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiScrollEnd" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiScrollBar" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiList" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiProgressBar" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiSlider" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiEdit" => &[XUICONTROL_TYPES_V5, &[]],
        "XuiCaret" => &[XUICONTROL_TYPES_V5, &[]],
        // XuiElement -> XuiControl -> XuiButton/XuiScene (2 derived levels)
        "XuiButton" => &[XUICONTROL_TYPES_V5, XUIBUTTON_TYPES],
        "XuiScene" => &[XUICONTROL_TYPES_V5, XUISCENE_TYPES],
        // XuiElement -> XuiControl -> XuiButton -> XuiNavButton/XuiBackButton (3 derived)
        "XuiNavButton" => &[XUICONTROL_TYPES_V5, XUIBUTTON_TYPES, XUINAVBUTTON_TYPES],
        "XuiBackButton" => &[XUICONTROL_TYPES_V5, XUIBUTTON_TYPES, &[]],
        // XuiElement -> XuiControl -> XuiScene -> XuiTabScene/etc (3 derived)
        "XuiTabScene" => &[XUICONTROL_TYPES_V5, XUISCENE_TYPES, XUITABSCENE_TYPES],
        "XuiMessageBox" => &[XUICONTROL_TYPES_V5, XUISCENE_TYPES, &[]],
        "XuiPerspectiveScene" => &[XUICONTROL_TYPES_V5, XUISCENE_TYPES, &[]],
        // XuiElement -> XuiControl -> XuiCheckbox -> XuiListItem (3 derived)
        "XuiListItem" => &[XUICONTROL_TYPES_V5, &[], &[]],
        // XuiElement -> XuiControl -> XuiList -> XuiCommonList (3 derived)
        "XuiCommonList" => &[XUICONTROL_TYPES_V5, &[], &[]],
        // XuiElement -> XuiGroup -> XuiTextureSurface (2 derived)
        "XuiTextureSurface" => &[&[], &[]],
        // XuiElement -> XuiSound -> XuiSoundXAudio (2 derived)
        "XuiSoundXAudio" => &[&[], &[]],
        // XuiHtmlElement - v5-only class, extends XuiElement directly
        "XuiHtmlElement" => &[&[PropType::String]],
        // Unknown classes: assume 1 derived level
        _ => &[&[]],
    }
}

fn get_compound_sub_types(parent_types: &[PropType], bit: u32) -> Option<&'static [PropType]> {
    let ptr = parent_types.as_ptr();
    if std::ptr::eq(ptr, XUIFIGURE_TYPES.as_ptr()) {
        match bit {
            0 => Some(STROKE_TYPES),
            1 => Some(FILL_TYPES),
            _ => None,
        }
    } else if std::ptr::eq(ptr, FILL_TYPES.as_ptr()) {
        match bit {
            3 => Some(GRADIENT_TYPES),
            _ => None,
        }
    } else {
        None
    }
}

/// Check if a property is an indexed array (has array count prefix).
fn is_array_property(types: &[PropType], bit: u32) -> bool {
    let ptr = types.as_ptr();
    if std::ptr::eq(ptr, GRADIENT_TYPES.as_ptr()) {
        return is_gradient_array_prop(bit);
    }
    false
}

// -- File parsing --

const HEADER_SIZE: usize = 0x14;
const SECTION_HEADER_SIZE: usize = 12;

pub fn parse(data: &[u8]) -> Result<Xur<'_>, ParseError> {
    if data.len() < HEADER_SIZE {
        return Err(ParseError::TooShort {
            needed: HEADER_SIZE,
            available: data.len(),
        });
    }

    let header = parse_header(data)?;
    let section_count = header.section_count as usize;

    // reserved=1 adds 10 u32 pre-computed values (40 bytes) before the section table
    let extra_header = if header.reserved != 0 { 40 } else { 0 };
    let sect_table_offset = HEADER_SIZE + extra_header;
    let needed = sect_table_offset + section_count * SECTION_HEADER_SIZE;
    if data.len() < needed {
        return Err(ParseError::TooShort { needed, available: data.len() });
    }

    let sections = parse_section_headers(data, section_count, sect_table_offset);
    let strn = find_section(&sections, section_tag::STRN)
        .ok_or(ParseError::MissingSection("STRN"))?;
    let data_sect = find_section(&sections, section_tag::DATA)
        .ok_or(ParseError::MissingSection("DATA"))?;
    let vect = find_section(&sections, section_tag::VECT);
    let cust = find_section(&sections, section_tag::CUST);

    let strn_data = section_bytes(data, &strn)?;
    let data_data = section_bytes(data, &data_sect)?;
    let vect_data = vect.map(|v| section_bytes(data, &v)).transpose()?.unwrap_or(&[]);
    let cust_data = cust.map(|c| section_bytes(data, &c)).transpose()?.unwrap_or(&[]);

    let strings = parse_string_table(strn_data)?;
    let mut cursor = Cursor::new(data_data);
    let root = parse_object(&mut cursor, data_data, &strings)?;

    Ok(Xur { header, strings, vectors: vect_data, custom: cust_data, root })
}

fn parse_header(data: &[u8]) -> Result<Header, ParseError> {
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&data[0..4]);
    if &magic != XUIB_MAGIC {
        return Err(ParseError::BadMagic(magic));
    }
    let raw_version = BigEndian::read_u32(&data[4..8]);
    Ok(Header {
        magic,
        format_version: super::FormatVersion::from_u32(raw_version),
        raw_version,
        reserved: BigEndian::read_u32(&data[8..12]),
        xui_version: BigEndian::read_u16(&data[0x0C..0x0E]),
        total_size: BigEndian::read_u32(&data[0x0E..0x12]),
        section_count: BigEndian::read_u16(&data[0x12..0x14]),
    })
}

fn parse_section_headers(data: &[u8], count: usize, table_offset: usize) -> Vec<SectionHeader> {
    (0..count).map(|i| {
        let base = table_offset + i * SECTION_HEADER_SIZE;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&data[base..base + 4]);
        SectionHeader {
            tag,
            offset: BigEndian::read_u32(&data[base + 4..base + 8]),
            size: BigEndian::read_u32(&data[base + 8..base + 12]),
        }
    }).collect()
}

fn find_section(sections: &[SectionHeader], tag: &[u8; 4]) -> Option<SectionHeader> {
    sections.iter().find(|s| &s.tag == tag).copied()
}

fn section_bytes<'a>(data: &'a [u8], section: &SectionHeader) -> Result<&'a [u8], ParseError> {
    let start = section.offset as usize;
    let end = start + section.size as usize;
    if end > data.len() {
        return Err(ParseError::TooShort { needed: end, available: data.len() });
    }
    Ok(&data[start..end])
}

fn parse_string_table(data: &[u8]) -> Result<Vec<std::string::String>, ParseError> {
    let mut strings = Vec::new();
    let mut cursor = Cursor::new(data);
    while pos(&cursor) < data.len() {
        let char_count = cursor.read_u16::<BigEndian>()
            .map_err(|_| ParseError::BadString { offset: pos(&cursor) })? as usize;
        let mut s = std::string::String::with_capacity(char_count);
        for _ in 0..char_count {
            let cu = cursor.read_u16::<BigEndian>()
                .map_err(|_| ParseError::BadString { offset: pos(&cursor) })?;
            s.push(char::from_u32(u32::from(cu)).unwrap_or('\u{FFFD}'));
        }
        strings.push(s);
    }
    Ok(strings)
}

// -- Object parsing --

fn parse_object<'a>(
    c: &mut Cursor<&'a [u8]>,
    full_data: &'a [u8],
    strings: &[std::string::String],
) -> Result<Object<'a>, ParseError> {
    let obj_start = pos(c);

    let class_name = u32::from(c.read_u16::<BigEndian>()?);
    if class_name == 0 {
        return Err(ParseError::BadObject(format!("null class name @{obj_start}")));
    }

    let class_str = strings
        .get((class_name - 1) as usize)
        .map(|s| s.as_str())
        .unwrap_or("");

    debug!("@{obj_start} object class={class_str:?} (id={class_name})");

    let flags = c.read_u8()?;
    let has_props = flags & object_flags::HAS_PROPERTIES != 0;
    let has_children = flags & object_flags::HAS_CHILDREN != 0;
    let has_timelines = flags & 0x04 != 0;

    let props_start = pos(c);
    let mut properties = Vec::new();

    if has_props {
        // u16 total value count (metadata, not used for parsing)
        let _total_count = c.read_u16::<BigEndian>()?;

        // XuiElement base level: read bitmask + values
        let base_bitmask = read_v5_bitmask(c)?;
        debug!("  @{} XuiElement bitmask=0x{base_bitmask:08x}", pos(c));
        if base_bitmask != 0 {
            let mut group = read_property_values(c, base_bitmask, XUIELEMENT_TYPES_V5)?;
            group.level = 0;
            properties.push(group);
        }

        // Walk the derived class hierarchy (one bitmask per level)
        let hierarchy = get_hierarchy(class_str);
        for (level, types) in hierarchy.iter().enumerate() {
            let bitmask = read_v5_bitmask(c)?;
            if bitmask == 0 {
                trace!("  @{} derived[{level}] empty", pos(c));
                continue;
            }
            debug!("  @{} derived[{level}] bitmask=0x{bitmask:08x}", pos(c));
            let mut group = if types.is_empty() {
                read_unknown_values(c, bitmask)?
            } else {
                read_property_values(c, bitmask, types)?
            };
            group.level = level + 1; // 0 = XuiElement, 1+ = derived
            properties.push(group);
        }
    }

    let props_end = pos(c);
    let raw_props = &full_data[props_start..props_end.min(full_data.len())];

    let mut children = Vec::new();
    if has_children {
        // v5: child count is u32 BE
        let child_count = c.read_u32::<BigEndian>()?;
        debug!("  @{} children={child_count}", pos(c));
        for i in 0..child_count {
            match parse_object(c, full_data, strings) {
                Ok(child) => children.push(child),
                Err(e) => {
                    return Err(ParseError::BadObject(format!(
                        "child {i}/{child_count} @{}: {e}", pos(c)
                    )));
                }
            }
        }
    }

    if has_timelines {
        skip_timelines(c)?;
    }

    Ok(Object { class_name, properties, children, _raw_props: raw_props })
}

/// Skip over timeline data in a v5 object.
/// Format (from sub_100cb730 in xam_1888.exe):
///
/// Phase 1 - Named timelines:
///   u32(count), for each: u16(name) + u32(duration) + u8(type) + u16(from_name)
///
/// Phase 2 - Named frames:
///   u32(frame_count), for each:
///     u16(name) + u32(keyframe_count)
///     Keyframe paths: each is 1 byte (depth|flag) + 2 bytes (if depth>0)
///       + (depth-1) bytes + optional u32 (if flag bit 7)
///     u32(subtimeline_count)
///     Subtimelines: each is 8 bytes fixed + N*4 bytes for animated property values
///       where N = number of keyframe paths (keyframe_count)
fn skip_timelines(c: &mut Cursor<&[u8]>) -> Result<(), ParseError> {
    let start = pos(c);

    // Phase 1: named timelines - u32(count), each 9 bytes
    let timeline_count = c.read_u32::<BigEndian>()?;
    debug!("  @{start} timelines: {timeline_count} named timelines");
    for _ in 0..timeline_count {
        c.read_u16::<BigEndian>()?; // name string ref
        c.read_u32::<BigEndian>()?; // duration
        c.read_u8()?;               // type
        c.read_u16::<BigEndian>()?; // from_name string ref
    }

    // Phase 2: named frames
    let named_frame_count = c.read_u32::<BigEndian>()?;
    debug!("  @{} timelines: {named_frame_count} named frames", pos(c));
    for _ in 0..named_frame_count {
        let _name = c.read_u16::<BigEndian>()?;
        let keyframe_count = c.read_u32::<BigEndian>()?;

        // Keyframe paths (sub_100cb2d0):
        // Each: u8(depth_and_flag), if depth>0: 2 bytes + (depth-1) bytes, if flag: u32
        for _ in 0..keyframe_count {
            let first = c.read_u8()?;
            let depth = (first & 0x7f) as usize;
            let has_value = first & 0x80 != 0;
            if depth > 0 {
                c.read_u8()?; // hierarchy level
                c.read_u8()?; // property index
                for _ in 1..depth {
                    c.read_u8()?; // compound sub-index
                }
            }
            if has_value {
                c.read_u32::<BigEndian>()?;
            }
        }

        // Subtimelines (sub_100cb558):
        // Each: u32 + 4*u8 (8 bytes fixed) + keyframe_count * 4 bytes (property values)
        let subtimeline_count = c.read_u32::<BigEndian>()?;
        let sub_size = 8 + (keyframe_count as usize) * 4;
        trace!(
            "  @{} {subtimeline_count} subtimelines, {sub_size} bytes each",
            pos(c)
        );
        for _ in 0..subtimeline_count {
            // 8 bytes fixed header
            c.read_u32::<BigEndian>()?; // time/index
            c.read_u8()?;              // flags
            c.read_u8()?;              // ease type
            c.read_u8()?;              // ease data
            c.read_u8()?;              // ease data
            // One value per keyframe path (4 bytes each)
            for _ in 0..keyframe_count {
                c.read_u32::<BigEndian>()?;
            }
        }
    }

    debug!("  @{} timelines done", pos(c));
    Ok(())
}

/// Read property values given a bitmask and type table.
fn read_property_values(
    c: &mut Cursor<&[u8]>,
    bitmask: u32,
    types: &[PropType],
) -> Result<PropertyGroup, ParseError> {
    let mut values = Vec::new();
    for bit in 0..32u32 {
        if bitmask & (1 << bit) == 0 {
            continue;
        }
        let prop_type = types.get(bit as usize).copied().unwrap_or(Unsigned);

        if is_array_property(types, bit) {
            let count = read_v5_array_count(c)? as usize;
            trace!("    @{} bit{bit} array count={count}", pos(c));
            for _ in 0..count {
                values.push(read_single_value(c, prop_type, types, bit)?);
            }
        } else {
            values.push(read_single_value(c, prop_type, types, bit)?);
        }
    }
    Ok(PropertyGroup { bitmask, values, level: 0 })
}

/// Read values for an unknown derived class level (all as u32).
fn read_unknown_values(
    c: &mut Cursor<&[u8]>,
    bitmask: u32,
) -> Result<PropertyGroup, ParseError> {
    let mut values = Vec::new();
    for bit in 0..32u32 {
        if bitmask & (1 << bit) == 0 {
            continue;
        }
        values.push(PropertyValue::Unsigned(c.read_u32::<BigEndian>()?));
    }
    Ok(PropertyGroup { bitmask, values, level: 0 })
}

/// Read a single typed property value.
fn read_single_value(
    c: &mut Cursor<&[u8]>,
    prop_type: PropType,
    parent_types: &[PropType],
    bit: u32,
) -> Result<PropertyValue, ParseError> {
    let val_start = pos(c);
    let val = match prop_type {
        Bool => PropertyValue::Bool(c.read_u8()? != 0),
        Integer | Unsigned => PropertyValue::Unsigned(c.read_u32::<BigEndian>()?),
        Float => PropertyValue::Float(c.read_f32::<BigEndian>()?),
        PropType::String => PropertyValue::String(u32::from(c.read_u16::<BigEndian>()?)),
        Color => PropertyValue::Color(c.read_u32::<BigEndian>()?),
        Vector3 => PropertyValue::Vector(c.read_u32::<BigEndian>()?),
        Quaternion => PropertyValue::Quaternion(c.read_u32::<BigEndian>()?),
        Custom => PropertyValue::Custom(c.read_u32::<BigEndian>()?),
        Compound => {
            let sub_types = get_compound_sub_types(parent_types, bit);
            read_compound_value(c, sub_types)?
        }
    };
    trace!("    @{val_start} bit{bit} {prop_type:?} -> @{}", pos(c));
    Ok(val)
}

/// Read a compound property (type 9).
/// v5 format: u16(value_count) + recursive LoadPropertiesFromBinary(sub_defs).
/// No compound_idx in the stream - sub-definitions come from the runtime class hierarchy.
fn read_compound_value(
    c: &mut Cursor<&[u8]>,
    sub_types: Option<&'static [PropType]>,
) -> Result<PropertyValue, ParseError> {
    let compound_start = pos(c);
    let value_count = c.read_u16::<BigEndian>()?;
    debug!("    @{compound_start} compound value_count={value_count}");

    // Read one bitmask + values (same as a class level)
    let bitmask = read_v5_bitmask(c)?;
    debug!("    @{} compound bitmask=0x{bitmask:08x}", pos(c));

    if bitmask == 0 {
        return Ok(PropertyValue::Object(vec![]));
    }

    let types = sub_types.unwrap_or(&[]);
    let group = read_property_values(c, bitmask, types)?;
    debug!("    @{} compound done", pos(c));
    Ok(PropertyValue::Object(vec![group]))
}

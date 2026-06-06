pub mod archive;
pub mod parse;
pub mod parse_v8;

use std::fmt;

pub use parse::ParseError;

/// Known file magic values.
pub const XUIB_MAGIC: &[u8; 4] = b"XUIB";
pub const XUIZ_MAGIC: &[u8; 4] = b"XUIZ";

/// Section tag four-character codes.
pub mod section_tag {
    pub const STRN: &[u8; 4] = b"STRN";
    pub const VECT: &[u8; 4] = b"VECT";
    pub const QUAT: &[u8; 4] = b"QUAT";
    pub const CUST: &[u8; 4] = b"CUST";
    pub const DATA: &[u8; 4] = b"DATA";
    pub const FLOT: &[u8; 4] = b"FLOT";
    pub const COLR: &[u8; 4] = b"COLR";
}

/// Object flags (bitmask in the flags byte after string_id).
pub mod object_flags {
    pub const HAS_PROPERTIES: u8 = 0x01;
    pub const HAS_CHILDREN: u8 = 0x02;
    pub const HAS_TIMELINE: u8 = 0x04;
    /// v8 only: object has an Id registered for timeline/lookup.
    pub const HAS_ID: u8 = 0x08;
}

/// Property value type IDs used in class property definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PropType {
    Bool = 1,
    Integer = 2,
    Unsigned = 3,
    Float = 4,
    String = 5,
    Color = 6,
    Vector3 = 7,
    Quaternion = 8,
    Compound = 9,
    Custom = 10,
}

impl PropType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Bool),
            2 => Some(Self::Integer),
            3 => Some(Self::Unsigned),
            4 => Some(Self::Float),
            5 => Some(Self::String),
            6 => Some(Self::Color),
            7 => Some(Self::Vector3),
            8 => Some(Self::Quaternion),
            9 => Some(Self::Compound),
            10 => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Binary format version, detected from the file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVersion {
    /// XUIB version 5 (early Xbox 360 SDK). Values inline, no lookup tables.
    V5,
    /// XUIB version 8 (later Xbox 360 SDK). Uses FLOT/COLR/QUAT lookup tables.
    V8,
    /// Unknown version.
    Unknown(u32),
}

impl FormatVersion {
    pub fn from_u32(v: u32) -> Self {
        match v {
            5 => Self::V5,
            8 => Self::V8,
            other => Self::Unknown(other),
        }
    }
}

/// A parsed XUR file. Version-independent output structure.
#[derive(Debug)]
pub struct Xur<'a> {
    pub header: Header,
    pub strings: Vec<String>,
    pub vectors: &'a [u8],
    pub quaternions: &'a [u8],
    pub custom: &'a [u8],
    pub root: Object<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub magic: [u8; 4],
    pub format_version: FormatVersion,
    pub raw_version: u32,
    pub reserved: u32,
    pub xui_version: u16,
    pub total_size: u32,
    pub section_count: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct SectionHeader {
    pub tag: [u8; 4],
    pub offset: u32,
    pub size: u32,
}

/// A parsed element in the tree.
#[derive(Debug)]
pub struct Object<'a> {
    pub class_name: u32,
    pub properties: Vec<PropertyGroup>,
    pub children: Vec<Object<'a>>,
    pub timelines: Option<TimelineData>,
    pub _raw_props: &'a [u8],
}

/// All timeline/animation data for an object.
#[derive(Debug, Clone)]
pub struct TimelineData {
    pub named_frames: Vec<NamedFrame>,
    pub timelines: Vec<Timeline>,
}

/// A named frame marker (stop point, transition label).
#[derive(Debug, Clone)]
pub struct NamedFrame {
    pub name: u32, // 1-based string table index
    pub time: u32,
    pub command: u8,    // 0=none, 1=stop
    pub from_name: u32, // 1-based string table index, 0=none
}

/// An animated property timeline targeting a child object.
#[derive(Debug, Clone)]
pub struct Timeline {
    pub target_name: u32, // 1-based string table index of target object Id
    pub target_class: Option<String>, // resolved class name of target (for property name lookup)
    pub paths: Vec<KeyframePath>, // which properties are animated
    pub keyframes: Vec<Keyframe>,
}

/// Identifies which property is being animated.
#[derive(Debug, Clone)]
pub struct KeyframePath {
    pub hier_level: u8,
    pub prop_idx: u8,
    pub prop_type: Option<PropType>,
    pub prop_name: String, // resolved property name (e.g. "Opacity")
    pub depth: u8,
    pub extra_bytes: Vec<u8>,
    pub default_value: Option<u32>,
}

/// A single keyframe (one point in time with values for all animated properties).
#[derive(Debug, Clone)]
pub struct Keyframe {
    pub time: u32,
    pub interpolation: u8,
    pub ease: [u8; 3],
    pub values: Vec<TimelineValue>,
}

/// A typed timeline value.
#[derive(Debug, Clone)]
pub enum TimelineValue {
    Bool(bool),
    Float(f32),
    Unsigned(u32),
    String(u32), // 1-based string index (stored as u32 in timelines)
    Color(u32),
    Vector(u32),     // VECT table index
    Quaternion(u32), // QUAT table index
}

/// A group of properties at one class hierarchy level.
#[derive(Debug, Clone)]
pub struct PropertyGroup {
    pub bitmask: u32,
    pub values: Vec<PropertyValue>,
    /// Hierarchy level: 0 = XuiElement base, 1+ = derived levels (innermost first)
    pub level: usize,
}

#[derive(Debug, Clone)]
pub enum PropertyValue {
    Bool(bool),
    Integer(u32),
    Unsigned(u32),
    Float(f32),
    String(u32),
    Color(u32),
    Vector(u32),
    Quaternion(u32),
    Object(Vec<PropertyGroup>),
    Custom(u32),
    Unknown(Vec<u8>),
}

impl<'a> Xur<'a> {
    /// Parse a XUIB binary. For XUIZ archives, extract the XUIB first
    /// using `xur::archive::extract_xuib`.
    pub fn parse(data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() >= 8 {
            let raw_version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
            if let FormatVersion::V8 = FormatVersion::from_u32(raw_version) {
                return parse_v8::parse(data);
            }
        }
        parse::parse(data)
    }

    /// Get a string by 1-based index.
    pub fn get_string(&self, index: u32) -> Option<&str> {
        if index == 0 {
            return None;
        }
        self.strings.get((index - 1) as usize).map(|s| s.as_str())
    }

    /// Get a quaternion by 0-based index.
    pub fn get_quaternion(&self, index: u32) -> Option<(f32, f32, f32, f32)> {
        let offset = (index as usize) * 16;
        if offset + 16 > self.quaternions.len() {
            return None;
        }
        let x = f32::from_be_bytes(self.quaternions[offset..offset + 4].try_into().unwrap());
        let y = f32::from_be_bytes(self.quaternions[offset + 4..offset + 8].try_into().unwrap());
        let z = f32::from_be_bytes(
            self.quaternions[offset + 8..offset + 12]
                .try_into()
                .unwrap(),
        );
        let w = f32::from_be_bytes(
            self.quaternions[offset + 12..offset + 16]
                .try_into()
                .unwrap(),
        );
        Some((x, y, z, w))
    }

    /// Get a vector3 by 0-based index.
    pub fn get_vector(&self, index: u32) -> Option<(f32, f32, f32)> {
        let offset = (index as usize) * 12;
        if offset + 12 > self.vectors.len() {
            return None;
        }
        let x = f32::from_be_bytes(self.vectors[offset..offset + 4].try_into().unwrap());
        let y = f32::from_be_bytes(self.vectors[offset + 4..offset + 8].try_into().unwrap());
        let z = f32::from_be_bytes(self.vectors[offset + 8..offset + 12].try_into().unwrap());
        Some((x, y, z))
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = std::str::from_utf8(&self.magic).unwrap_or("????");
        write!(
            f,
            "{tag} {:?} (xui={:#06x}, size={}, sections={})",
            self.format_version, self.xui_version, self.total_size, self.section_count
        )
    }
}

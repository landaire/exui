use core::fmt;

use crate::archive::EntryOffset;
use crate::archive::EntrySize;
use crate::wide::WideString;
use crate::xur::SectionTag;
use crate::xur::StringId;

/// Part of a XUR file that a read failed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// File header and section table.
    Header,
    Section(SectionTag),
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Header => f.write_str("header"),
            Self::Section(tag) => write!(f, "{tag} section"),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegisterError {
    #[error("class {name} is already registered")]
    AlreadyRegistered { name: &'static str },

    #[error("class {name} names base class {base}, which is not registered")]
    BaseNotRegistered {
        name: &'static str,
        base: &'static str,
    },

    #[error("registry has no room for class {name}")]
    RegistryFull { name: &'static str },
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArchiveError {
    #[error("bad XUIZ magic {found:02X?}")]
    BadMagic { found: [u8; 4] },

    #[error("unsupported XUIZ version {version}")]
    UnsupportedVersion { version: u32 },

    #[error("XUIZ header file size {header} does not match data size {actual}")]
    FileSizeMismatch { header: u32, actual: usize },

    #[error("XUIZ data truncated at offset {offset:#x}, {needed} bytes needed")]
    Truncated { offset: usize, needed: usize },

    #[error("XUIZ entry at offset {offset:#x} overruns the entry table")]
    EntryTableOverrun { offset: usize },

    #[error("XUIZ entry at offset {offset:#x} has a name that is not UTF-8")]
    InvalidName { offset: usize },

    #[error("empty archive path")]
    EmptyPath,

    #[error("archive entry not found")]
    NotFound,

    #[error("archive entry data at offset {offset:?} size {size:?} is out of bounds")]
    EntryOutOfBounds {
        offset: EntryOffset,
        size: EntrySize,
    },

    #[error("trailer of archive entry at offset {offset:?} size {size:?} is out of bounds")]
    TrailerOutOfBounds {
        offset: EntryOffset,
        size: EntrySize,
    },
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum XurError {
    #[error("bad XUR magic {found:02X?}")]
    BadMagic { found: [u8; 4] },

    #[error("unsupported XUR version {version}")]
    UnsupportedVersion { version: u32 },

    #[error("unsupported XUI schema version {schema:#x}")]
    UnsupportedSchema { schema: u16 },

    #[error("XUR header file size {header} does not match data size {actual}")]
    FileSizeMismatch { header: u32, actual: usize },

    #[error("XUR has {count} sections, at least 2 are required")]
    TooFewSections { count: u16 },

    #[error("XUR lacks the {tag} section")]
    MissingSection { tag: SectionTag },

    #[error("{tag} section at offset {offset:#x} size {size:#x} is out of bounds")]
    SectionOutOfBounds {
        tag: SectionTag,
        offset: u32,
        size: u32,
    },

    #[error("XUR data truncated at offset {offset:#x} in the {region}, {needed} bytes needed")]
    Truncated {
        region: Region,
        offset: usize,
        needed: usize,
    },

    #[error("malformed string table entry at offset {offset:#x}")]
    InvalidStringTable { offset: usize },

    #[error("{tag} section size {size:#x} is not a whole number of elements")]
    MalformedFloatTable { tag: SectionTag, size: u32 },

    #[error("invalid class name string index {index} at offset {offset:#x}")]
    InvalidClassString { offset: usize, index: u16 },

    #[error("unknown class {name} at offset {offset:#x}")]
    UnknownClass { offset: usize, name: WideString },

    #[error(
        "object of class {class} at offset {offset:#x} declares {declared} property values but holds {actual}"
    )]
    PropertyCountMismatch {
        offset: usize,
        class: &'static str,
        declared: u16,
        actual: u32,
    },

    #[error(
        "compound property {property} at offset {offset:#x} declares {declared} values but holds {actual}"
    )]
    CompoundOverflow {
        offset: usize,
        property: &'static str,
        declared: u16,
        actual: u32,
    },

    #[error("invalid string index {index} at offset {offset:#x}")]
    InvalidStringIndex { offset: usize, index: u16 },

    #[error("invalid vector index {index} at offset {offset:#x}")]
    InvalidVectorIndex { offset: usize, index: u32 },

    #[error("invalid quaternion index {index} at offset {offset:#x}")]
    InvalidQuaternionIndex { offset: usize, index: u32 },

    #[error("timeline at offset {offset:#x} names no child element ({element:?})")]
    TimelineTargetNotFound {
        offset: usize,
        element: Option<StringId>,
    },

    #[error("property path at offset {offset:#x} climbs {levels_up} levels above class {class}")]
    PropertyPathLevel {
        offset: usize,
        class: &'static str,
        levels_up: u8,
    },

    #[error(
        "property path at offset {offset:#x} selects property {index} of class {class}, which does not exist"
    )]
    PropertyPathIndex {
        offset: usize,
        class: &'static str,
        index: u8,
    },

    #[error(
        "property path at offset {offset:#x} selects sub-property {index} of {property}, which does not exist"
    )]
    PropertyPathSubIndex {
        offset: usize,
        property: &'static str,
        index: u8,
    },

    #[error(
        "property path at offset {offset:#x} descends into {property}, which is not a compound"
    )]
    PropertyPathNotCompound {
        offset: usize,
        property: &'static str,
    },

    #[error("keyframe at offset {offset:#x} needs a value for an empty property path")]
    EmptyPropertyPath { offset: usize },
}

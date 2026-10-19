use exui_core::XurError;
use exui_core::schema::ClassId;
use exui_core::xur::CustomOffset;

use crate::v8::Tag;

/// A loaded value that cannot be written as XUI text.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecompileError {
    #[error("document class {class:?} is not in the registry")]
    ClassNotInRegistry { class: ClassId },

    #[error("{property} references vector {index}, which is not in the VECT table")]
    InvalidVectorIndex { property: &'static str, index: u32 },

    #[error("{property} references quaternion {index}, which is not in the QUAT table")]
    InvalidQuaternionIndex { property: &'static str, index: u32 },

    #[error("{property} references a custom blob at {offset:?} outside the CUST section")]
    CustomOutOfBounds {
        property: &'static str,
        offset: CustomOffset,
    },

    #[error("figure points blob at {offset:?} holds no complete point list")]
    MalformedFigurePoints { offset: CustomOffset },

    #[error("custom property {property} has no text form")]
    UnsupportedCustom { property: &'static str },

    #[error("timeline property has an empty path")]
    EmptyPropertyPath,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum V8Error {
    #[error("bad XUR magic {found:02X?}")]
    BadMagic { found: [u8; 4] },

    #[error("unsupported XUR version {version}")]
    UnsupportedVersion { version: u32 },

    #[error("XUR header file size {header} does not match data size {actual}")]
    FileSizeMismatch { header: u32, actual: usize },

    #[error("XUR data truncated at offset {offset:#x}")]
    Truncated { offset: usize },

    #[error("XUR lacks the {tag} section")]
    MissingSection { tag: Tag },

    #[error("{tag} section at offset {offset:#x} size {size:#x} is out of bounds")]
    SectionOutOfBounds { tag: Tag, offset: u32, size: u32 },

    #[error("string {index} of the STRN section is not terminated")]
    UnterminatedString { index: u16 },

    #[error("invalid class name string index {index} at offset {offset:#x}")]
    InvalidClassString { offset: usize, index: u32 },

    #[error(
        "object of class {class} at offset {offset:#x} declares {declared} property values but holds {actual}"
    )]
    PropertyCountMismatch {
        offset: usize,
        class: String,
        declared: u32,
        actual: u32,
    },

    #[error(
        "empty property levels at offset {offset:#x} cannot be told apart from the field that follows"
    )]
    AmbiguousLevelCount { offset: usize },

    #[error("index {index} at offset {offset:#x} is outside the {tag} section")]
    InvalidTableIndex { tag: Tag, offset: usize, index: u32 },

    #[error("timeline at offset {offset:#x} names no child element")]
    TimelineTargetNotFound { offset: usize },

    #[error(transparent)]
    Decompile(#[from] DecompileError),
}

/// Failure to read a XUR file into a scene.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SceneError {
    #[error(transparent)]
    Version5(#[from] XurError),

    #[error(transparent)]
    Version8(#[from] V8Error),

    #[error(transparent)]
    Decompile(#[from] DecompileError),
}

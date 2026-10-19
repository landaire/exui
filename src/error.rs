use std::fmt;
use std::path::PathBuf;

use exui_core::ArchiveWriteError;
use exui_core::WriteError;
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

    #[error("figure points blob at {offset:?} is not exactly a size and a complete point list")]
    MalformedFigurePoints { offset: CustomOffset },

    #[error("custom property {property} has no text form")]
    UnsupportedCustom { property: &'static str },

    #[error("timeline property has an empty path")]
    EmptyPropertyPath,

    #[error("string {index} is not in the STRN table")]
    InvalidStringIndex { index: u16 },

    #[error("string {index} is empty, which the text form reads as the null string")]
    EmptyString { index: u16 },

    #[error("string {index} holds an unpaired UTF-16 surrogate, which XML text cannot hold")]
    InvalidUtf16 { index: u16 },
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

/// Why an archive entry with a text form is kept as data.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TextFormError {
    #[error(transparent)]
    Scene(#[from] SceneError),

    #[error(transparent)]
    StringTable(#[from] StringTableError),
}

/// One-based line of an XML source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLine(pub u32);

impl fmt::Display for SourceLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}", self.0)
    }
}

/// XUI or string table text that does not compile.
#[derive(Clone, Debug, thiserror::Error)]
pub enum CompileError {
    #[error("{line}: {source}")]
    Xml {
        line: SourceLine,
        source: quick_xml::Error,
    },

    #[error("the document has no root element")]
    NoRootElement,

    #[error("{line}: a second root element")]
    MultipleRootElements { line: SourceLine },

    #[error("{line}: element {element} is not closed")]
    UnclosedElement { line: SourceLine, element: String },

    #[error("{line}: text outside the root element")]
    TextOutsideRoot { line: SourceLine },

    #[error("{line}: element {element} has no attribute {attribute}")]
    UnexpectedAttribute {
        line: SourceLine,
        element: String,
        attribute: String,
    },

    #[error("{line}: element {element} is not allowed in {parent}")]
    UnexpectedElement {
        line: SourceLine,
        element: String,
        parent: String,
    },

    #[error("{line}: element {element} holds text")]
    UnexpectedText { line: SourceLine, element: String },

    #[error("{line}: element {element} appears twice")]
    DuplicateElement { line: SourceLine, element: String },

    #[error("{line}: {parent} lacks element {element}")]
    MissingElement {
        line: SourceLine,
        element: &'static str,
        parent: String,
    },

    #[error("{line}: {element} lacks attribute {attribute}")]
    MissingAttribute {
        line: SourceLine,
        element: String,
        attribute: &'static str,
    },

    #[error("{line}: attribute {attribute} of {element} has bad value {text:?}")]
    BadAttribute {
        line: SourceLine,
        element: String,
        attribute: &'static str,
        text: String,
    },

    #[error("{line}: XUI version {version:?} is not 000c")]
    UnsupportedVersion { line: SourceLine, version: String },

    #[error("{line}: class {name} is not in the registry")]
    UnknownClass { line: SourceLine, name: String },

    #[error("{line}: {owner} has no property {name}")]
    UnknownProperty {
        line: SourceLine,
        owner: &'static str,
        name: String,
    },

    #[error("{line}: property {property} is given more often than it is declared")]
    DuplicateProperty { line: SourceLine, property: String },

    #[error("{line}: indexed property {property} lacks an index attribute")]
    MissingIndex {
        line: SourceLine,
        property: &'static str,
    },

    #[error("{line}: property {property} is not indexed")]
    UnexpectedIndex {
        line: SourceLine,
        property: &'static str,
    },

    #[error("{line}: element {found} of {property} where element {expected} comes next")]
    IndexOutOfOrder {
        line: SourceLine,
        property: String,
        expected: usize,
        found: u32,
    },

    #[error("{line}: custom property {property} has no text form")]
    UnsupportedCustom {
        line: SourceLine,
        property: &'static str,
    },

    #[error("{line}: {text:?} is not a value of property {property}")]
    BadValue {
        line: SourceLine,
        property: &'static str,
        text: String,
    },

    #[error("{line}: field {field} of the point list is malformed")]
    BadPoints { line: SourceLine, field: usize },

    #[error("{line}: point list declares {declared} points but holds {fields} fields")]
    PointCount {
        line: SourceLine,
        declared: u32,
        fields: usize,
    },

    #[error("{line}: {text:?} is not a number for {element}")]
    BadNumber {
        line: SourceLine,
        element: String,
        text: String,
    },

    #[error("{line}: unknown named frame command {text:?}")]
    UnknownCommand { line: SourceLine, text: String },

    #[error(
        "{line}: no child has the timeline Id {}",
        id.as_deref().map_or("(none)".to_string(), |id| format!("{id:?}"))
    )]
    UnknownTimelineTarget {
        line: SourceLine,
        id: Option<String>,
    },

    #[error("{line}: {class} has no property path {path}")]
    UnknownTimelineProperty {
        line: SourceLine,
        class: &'static str,
        path: String,
    },

    #[error("{line}: keyframe holds {found} values for {expected} animated properties")]
    KeyframeValueCount {
        line: SourceLine,
        expected: usize,
        found: usize,
    },

    #[error(transparent)]
    Write(#[from] WriteError),

    #[error(transparent)]
    StringTable(#[from] StringTableWriteError),
}

/// A `.xus` string table that cannot be read or has no exact text form.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum StringTableError {
    #[error("bad string table magic {found:02X?}")]
    BadMagic { found: [u8; 4] },

    #[error("string table format {format} is not 1")]
    UnsupportedFormat { format: u8 },

    #[error("string table flags {flags:#04x} select no single key kind")]
    UnsupportedFlags { flags: u8 },

    #[error("string table header size {header} does not match data size {actual}")]
    FileSizeMismatch { header: u32, actual: usize },

    #[error("string table truncated in entry {entry}")]
    Truncated { entry: u16 },

    #[error("string table has {count} bytes after its entries")]
    TrailingData { count: usize },

    #[error("entry {entry} holds an unpaired UTF-16 surrogate, which XML text cannot hold")]
    InvalidUtf16 { entry: u16 },
}

/// A string table too large for the `.xus` layout.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum StringTableWriteError {
    #[error("{count} entries do not fit the 16-bit entry count")]
    TooManyEntries { count: usize },

    #[error("a string of entry {entry} is longer than 65535 units")]
    StringTooLong { entry: usize },

    #[error("string table is larger than 4 GiB")]
    TooLarge,
}

/// Failure to build an archive from a directory.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{}: file name is not UTF-8", path.display())]
    InvalidFileName { path: PathBuf },

    #[error("{} and {} both store entry {name}", first.display(), second.display())]
    DuplicateEntry {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },

    #[error("entry {name} has no source file")]
    MissingSource { name: String },

    #[error("entry {name} is not in the order file")]
    Unlisted { name: String },

    #[error("{}: {source}", path.display())]
    Compile { path: PathBuf, source: CompileError },

    #[error(transparent)]
    Archive(#[from] ArchiveWriteError),
}

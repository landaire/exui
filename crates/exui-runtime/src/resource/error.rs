use exui_core::ArchiveError;
use xbox360_draw::error::TextureError;
use xbox360_image::error::ImageError;

use crate::extension::ExtensionName;
use crate::extension::SharedPatchError;
use crate::resource::MemoryBlockId;
use crate::resource::ModuleHandle;
use crate::resource::SectionName;
use crate::resource::WidePath;

/// Failure to resolve a resource locator or read the resource it names.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("locator {path} uses the unsupported scheme {scheme:?}")]
    UnsupportedScheme {
        scheme: UnsupportedScheme,
        path: WidePath,
    },

    #[error("locator {path} has an unknown scheme")]
    UnknownScheme { path: WidePath },

    #[error("relative path {path} has no base path to resolve against")]
    NoBasePath { path: WidePath },

    #[error("section locator {path} is not `section://<module>,<name>#<file>`")]
    MalformedSection { path: WidePath },

    #[error("image section {section} of module {module:X?} is not available")]
    SectionNotFound {
        module: ModuleHandle,
        section: SectionName,
    },

    #[error("image section {section} is not a XUIZ archive")]
    Archive {
        section: SectionName,
        #[source]
        source: ArchiveError,
    },

    #[error("{path} is not in section {section}")]
    NotInArchive {
        section: SectionName,
        path: WidePath,
        #[source]
        source: ArchiveError,
    },

    #[error("file {path} is not available")]
    FileNotFound { path: WidePath },

    #[error("memory locator {path} is not `memory://<block>,<size>`")]
    MalformedMemory { path: WidePath },

    #[error("memory block {block:X?} of {size} bytes is not registered")]
    MemoryBlockNotFound { block: MemoryBlockId, size: u32 },

    #[error("extension locator {path} is not `ext://<name>/<file>`")]
    MalformedExtension { path: WidePath },

    #[error("extension {name} is not enabled")]
    ExtensionNotEnabled { name: ExtensionName },

    #[error("{path} is not in extension {name}")]
    NotInExtension {
        name: ExtensionName,
        path: WidePath,
        #[source]
        source: ArchiveError,
    },

    #[error("{path} of section {section} could not be patched")]
    Patch {
        section: SectionName,
        path: WidePath,
        #[source]
        source: SharedPatchError,
    },
}

/// Locator schemes XUI handles that this runtime does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedScheme {
    /// `memory://`, in-memory archives.
    Memory,
}

/// Failure of a [`crate::resource::TextureLoader`].
#[derive(Debug, thiserror::Error)]
pub enum TextureLoadError {
    #[error(transparent)]
    Resource(#[from] ResourceError),

    #[error("the texture loader does not handle {path}")]
    UnsupportedPath { path: WidePath },

    #[error("image of {len} bytes is too short for its signature")]
    Truncated { len: usize },

    #[error("image signature {magic:02X?} selects no supported decoder")]
    UnsupportedFormat { magic: [u8; 4] },

    #[error("image decoding failed")]
    Decode(#[from] ImageError),

    #[error("texture upload failed")]
    Store(#[from] TextureError),
}

/// Failure to load a `.xus` string table.
#[derive(Debug, thiserror::Error)]
pub enum StringTableError {
    #[error(transparent)]
    Resource(#[from] ResourceError),

    #[error("string table does not start with XUIS")]
    Magic,

    #[error("string table format byte is {format}, not 1")]
    Format { format: u8 },

    #[error("string table ends at byte {len} inside entry {entry}")]
    Truncated { entry: u16, len: usize },

    /// The localized table and its base differ in entry count
    /// (0x8030002F).
    #[error("string overlay of extension {name} does not fit the table")]
    Overlay {
        name: ExtensionName,
        #[source]
        source: OverlayError,
    },

    #[error("localized string table has {localized} entries, its base {base}")]
    FallbackCount { localized: usize, base: usize },
}

/// A string overlay entry that its table cannot take.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OverlayError {
    #[error("entry key kind does not match the table's {table:?} keys")]
    KeyKind { table: crate::resource::EntryKeys },

    #[error("index {index} is past the end of the table ({len} entries)")]
    IndexPastEnd { index: u32, len: usize },
}

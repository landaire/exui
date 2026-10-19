use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use exui_core::ArchiveError;
use exui_core::TreeError;
use exui_core::WriteError;
use exui_core::XurError;
use exui_core::schema::ClassId;
use exui_core::wide::WideString;
use exui_extension::DESCRIPTOR_PATH;
use exui_extension::DecodeError;
use exui_extension::ElementPath;
use exui_extension::ExtensionName;

use crate::extension::PatchIndex;

/// Failure to load an extension archive.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("archive of extension {name} is not a XUIZ archive")]
    Archive {
        name: ExtensionName,
        #[source]
        source: ArchiveError,
    },

    #[error("extension {name} has no {DESCRIPTOR_PATH}")]
    NoDescriptor { name: ExtensionName },

    #[error("{DESCRIPTOR_PATH} of extension {name} cannot be read")]
    DescriptorEntry {
        name: ExtensionName,
        #[source]
        source: ArchiveError,
    },

    #[error("descriptor of extension {name} is malformed")]
    Descriptor {
        name: ExtensionName,
        #[source]
        source: DecodeError,
    },

    #[error("archive loaded as extension {name} describes extension {described}")]
    NameMismatch {
        name: ExtensionName,
        described: ExtensionName,
    },

    #[error("extension {name} is loaded twice")]
    Duplicate { name: ExtensionName },

    #[error("extension {name} targets section {section:?}, which is not a section name")]
    InvalidSection {
        name: ExtensionName,
        section: String,
    },

    #[error("extension {name} targets file {path:?}, which is not an archive path")]
    InvalidPath { name: ExtensionName, path: String },
}

/// Failure to patch a scene file.
#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    #[error("scene file does not parse")]
    Parse(#[source] XurError),

    #[error("scene file cannot be converted to a tree")]
    Tree(#[source] TreeError),

    #[error("patched scene cannot be written")]
    Write(#[source] WriteError),

    #[error("patch {} of extension {name} does not apply", .patch.0)]
    Extension {
        name: ExtensionName,
        patch: PatchIndex,
        #[source]
        source: ScenePatchError,
    },
}

/// Failure of one patch of an extension.
#[derive(Debug, thiserror::Error)]
pub enum ScenePatchError {
    #[error("overlay {path} is not in the extension's archive")]
    OverlayNotFound {
        path: String,
        #[source]
        source: ArchiveError,
    },

    #[error("overlay {path} does not parse")]
    OverlayParse {
        path: String,
        #[source]
        source: XurError,
    },

    #[error("overlay {path} cannot be converted to a tree")]
    OverlayTree {
        path: String,
        #[source]
        source: TreeError,
    },

    #[error("element {path} is not in the scene")]
    ElementNotFound { path: ElementPath },

    #[error("element {element} has no named frame {frame}")]
    FrameNotFound { element: ElementPath, frame: String },

    #[error("the root element cannot be removed")]
    RemoveRoot,

    #[error("overlay element {id:?} of class {overlay:?} stands for one of class {target:?}")]
    ClassMismatch {
        id: Option<WideString>,
        target: ClassId,
        overlay: ClassId,
    },
}

/// A [`PatchError`] that every read of the failed file reports.
#[derive(Clone, Debug)]
pub struct SharedPatchError(pub(crate) Arc<PatchError>);

impl SharedPatchError {
    pub fn error(&self) -> &PatchError {
        &self.0
    }
}

impl fmt::Display for SharedPatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl core::error::Error for SharedPatchError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.0.source()
    }
}

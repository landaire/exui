use alloc::boxed::Box;

use exui_core::XurError;
use xbox360_draw::error::DrawListError;
use xbox360_draw::error::TextureError;

use crate::scene::LoadError;
use crate::scene::PropertyError;
use crate::text::TextError;

/// Failure to produce a frame.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("texture operation failed")]
    Texture {
        #[from]
        source: TextureError,
    },

    #[error("draw list rejected geometry")]
    DrawList {
        #[from]
        source: DrawListError,
    },

    #[error("text rendering failed")]
    Text {
        #[from]
        source: TextError,
    },
}

/// Failure of `XuiScene_StartTransition` 0x92152730 to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    /// E_INVALIDARG.
    #[error("scene has no parent")]
    NoParent,

    /// E_FAIL: the scene is already inside a XuiTransition.
    #[error("scene is already in a transition")]
    InTransition,
}

#[derive(Debug, thiserror::Error)]
pub enum SkinError {
    #[error("skin file does not parse")]
    Parse {
        #[from]
        source: XurError,
    },

    /// 0x80004005: the key is already registered; earlier keys of the file
    /// stay registered.
    #[error("visual is already registered")]
    Duplicate { key: Box<[u16]> },

    #[error("skin bytes must stay mapped for the runtime's lifetime")]
    NotMapped,

    #[error("skin resource could not be read")]
    Resource {
        #[from]
        source: crate::resource::ResourceError,
    },
}

/// Failure to create a visual.
#[derive(Debug, thiserror::Error)]
pub enum VisualError {
    /// 0x80300026.
    #[error("no visual found")]
    NotFound,

    /// 0x80300025: the visual is already being instantiated.
    #[error("visual instantiation is recursive")]
    Recursive,

    #[error("visual instantiation failed")]
    Load {
        #[from]
        source: LoadError,
    },

    #[error("visual could not be attached")]
    Tree {
        #[from]
        source: crate::scene::TreeError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("class {name} is not registered")]
    UnknownClass { name: &'static str },

    #[error("class {name} already has a behavior")]
    DuplicateBehavior { name: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FocusError {
    #[error("element is focused by another user")]
    InvalidUser,

    /// 0x80300014.
    #[error("element is not focusable")]
    NotFocusable,

    /// The element did not handle set focus, or no descendant took focus.
    #[error("element did not take focus")]
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MessageBoxError {
    #[error("message box needs 1 to 9 buttons and a valid default")]
    InvalidArgument,

    #[error("message box visual is missing a required part")]
    MissingPart,

    #[error("XuiMessageBox class is not registered")]
    NoClass,
}

/// No state frame pair of the visual could be played (0x80300026).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("no state frames found")]
pub struct NoStateFrames;

#[derive(Debug, thiserror::Error)]
pub enum NavError {
    #[error("scene could not be loaded")]
    Load {
        #[from]
        source: LoadError,
    },

    #[error("scene resource could not be read")]
    Resource {
        #[from]
        source: crate::resource::ResourceError,
    },

    #[error("scene file does not parse")]
    Parse {
        #[from]
        source: exui_core::XurError,
    },

    /// E_FAIL: the next scene already had a back scene.
    #[error("scene already has a back scene")]
    HasBackScene,

    /// E_FAIL: the target is not on the back chain.
    #[error("target is not a back scene of the current scene")]
    NotOnBackChain,

    /// 0x8030000b.
    #[error("scene has no parent")]
    NoParent,

    /// 0x8030000a: the file's root has no child with a XuiScene level.
    #[error("scene file holds no scene")]
    NoScene,

    #[error("scene could not be attached")]
    Tree {
        #[from]
        source: crate::scene::TreeError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum SetPropertyError {
    /// 0x80300007: no class level of the element declares the property.
    #[error("element class has no property {name}")]
    NotOwned { name: &'static str },

    #[error("property store rejected the value")]
    Store {
        #[from]
        source: PropertyError,
    },
}

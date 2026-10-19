use exui_core::XurError;
use exui_core::schema::ClassId;
use exui_core::schema::ScalarType;

use crate::resource::ResourceError;
use crate::scene::ElementId;

/// Invalid tree operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TreeError {
    #[error("element {element:?} does not exist")]
    UnknownElement { element: ElementId },

    #[error("element {child:?} already has a parent")]
    AlreadyAttached { child: ElementId },

    #[error("element {child:?} is {parent:?} or one of its ancestors")]
    Cycle { parent: ElementId, child: ElementId },

    #[error("element {before:?} is not a child of {parent:?}")]
    NotAChild {
        parent: ElementId,
        before: ElementId,
    },
}

/// Invalid property access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PropertyError {
    #[error("element {element:?} does not exist")]
    UnknownElement { element: ElementId },

    #[error("property path is empty")]
    EmptyPath,

    #[error("property {name} holds {expected:?} values, not {found:?}")]
    TypeMismatch {
        name: &'static str,
        expected: Option<ScalarType>,
        found: ScalarType,
    },

    #[error("property {name} is a compound; set its members")]
    Compound { name: &'static str },

    #[error("property {name} is not a property of the element's class")]
    NotOnClass { name: &'static str },

    #[error("indexed property {name} needs an element index")]
    IndexRequired { name: &'static str },

    #[error("property {name} is not indexed")]
    NotIndexed { name: &'static str },

    #[error("index {index} of property {name} is not below its count {count}")]
    IndexOutOfRange {
        name: &'static str,
        index: u32,
        count: usize,
    },
}

/// Failure to instantiate a scene.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Resource(#[from] ResourceError),

    #[error("scene file is not a valid XUR")]
    Xur(#[from] XurError),

    #[error("class override {override_name} does not derive from the authored class {authored}")]
    ClassOverrideNotDerived {
        authored: &'static str,
        override_name: &'static str,
    },

    #[error("class {class:?} is not in the registry")]
    UnknownClass { class: ClassId },

    #[error("property {name} refers to a missing vector or quaternion table entry")]
    MissingTableEntry { name: &'static str },

    #[error("property {name} could not be applied")]
    Property {
        name: &'static str,
        #[source]
        source: PropertyError,
    },

    #[error(transparent)]
    Tree(#[from] TreeError),
}

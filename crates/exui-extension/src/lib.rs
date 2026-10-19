//! Descriptor of an XUI extension: the file [`DESCRIPTOR_PATH`] at the root
//! of an extension archive. Extension build tools write it; the runtime
//! reads it.

#![no_std]
extern crate alloc;
#[cfg(test)]
extern crate std;

mod descriptor;
mod error;
mod name;

#[cfg(test)]
mod tests;

pub use self::descriptor::BladeTab;
pub use self::descriptor::DESCRIPTOR_PATH;
pub use self::descriptor::Descriptor;
pub use self::descriptor::ElementPath;
pub use self::descriptor::EntryTarget;
pub use self::descriptor::PoseHold;
pub use self::descriptor::ScenePatch;
pub use self::descriptor::StringEntry;
pub use self::descriptor::StringKey;
pub use self::descriptor::StringOverlay;
pub use self::error::DecodeError;
pub use self::error::EncodeError;
pub use self::name::ExtensionName;

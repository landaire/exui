//! XUI element tree: instantiation from XUR documents, properties, layout
//! and rendering into a draw list (doc/xui-render.md).

mod content;
mod error;
pub mod figure;
pub mod image;
mod layout;
mod load;
mod paint;
mod presenter;
mod property;
mod render;
mod tree;

#[cfg(test)]
mod tests;

pub(crate) use self::content::is_scene_path;
pub use self::error::LoadError;
pub use self::error::PropertyError;
pub use self::error::TreeError;
pub use self::load::instantiate;
pub use self::presenter::PlainControlSource;
pub use self::presenter::PresenterSource;
pub use self::presenter::SourceImage;
pub use self::presenter::SourceText;
pub use self::presenter::owner_control;
pub use self::property::PropValue;
pub use self::render::RenderContext;
pub use self::render::RenderScratch;
pub(crate) use self::render::measure_text;
pub use self::render::render;
pub(crate) use self::render::text_line_starts;
pub use self::tree::Anchor;
pub use self::tree::BasePathId;
pub use self::tree::Children;
pub use self::tree::ClassTraits;
pub use self::tree::ContentKind;
pub use self::tree::Element;
pub use self::tree::ElementFlags;
pub use self::tree::ElementId;
pub use self::tree::ElementTree;
pub use self::tree::Size;
pub use self::tree::SizeChange;
pub use self::tree::XuiBlendMode;

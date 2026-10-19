//! Presenter data binding: text and image presenters pull their content
//! from the owner control on every render (xui-controls.md section 6).

use alloc::borrow::Cow;

use crate::resource::TextureObject;
use crate::scene::BasePathId;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::content::Content;

/// Answer to message 0x7df (get source text).
#[derive(Clone, Debug, PartialEq)]
pub struct SourceText<'a> {
    pub text: Cow<'a, [u16]>,
    /// `None` uses the presenter's PointSize (-1.0 in the original).
    pub point_size: Option<f32>,
    /// `None` keeps the presenter's TextStyle.
    pub style: Option<u32>,
    /// Data +0x18: pixels the text moves up (XuiEdit smooth scrolling).
    pub y_offset: f32,
}

/// Answer to message 0x7e1 (get source image). The brush kind follows the
/// answer: a texture object (+0x14) before a path (+0x10).
#[derive(Clone, Debug, PartialEq)]
pub enum SourceImage<'a> {
    Path {
        path: Cow<'a, [u16]>,
        /// Base path relative paths resolve against: the source control's.
        base: Option<BasePathId>,
    },
    Texture(TextureObject),
}

/// Supplies presenter content during drawing.
pub trait PresenterSource {
    fn source_text<'a>(
        &'a mut self,
        tree: &'a ElementTree,
        presenter: ElementId,
    ) -> Option<SourceText<'a>>;

    fn source_image<'a>(
        &'a mut self,
        tree: &'a ElementTree,
        presenter: ElementId,
    ) -> Option<SourceImage<'a>>;
}

/// The answers of a plain XuiControl: Text, PointSize and ImagePath of the
/// owner control for presenters with DataAssociation 0.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainControlSource;

impl PresenterSource for PlainControlSource {
    fn source_text<'a>(
        &'a mut self,
        tree: &'a ElementTree,
        presenter: ElementId,
    ) -> Option<SourceText<'a>> {
        if data_association(tree, presenter)? != 0 {
            return None;
        }
        let control = tree
            .get(owner_control(tree, presenter)?)?
            .control
            .as_ref()?;
        let text = control.text.as_deref()?;

        Some(SourceText {
            text: Cow::Borrowed(text),
            point_size: control.point_size,
            style: None,
            y_offset: 0.0,
        })
    }

    fn source_image<'a>(
        &'a mut self,
        tree: &'a ElementTree,
        presenter: ElementId,
    ) -> Option<SourceImage<'a>> {
        if data_association(tree, presenter)? != 0 {
            return None;
        }
        let owner = owner_control(tree, presenter)?;
        let element = tree.get(owner)?;
        let path = element.control.as_ref()?.image_path.as_deref()?;

        Some(SourceImage::Path {
            path: Cow::Borrowed(path),
            base: element.base_path,
        })
    }
}

/// `XuiFindOwnerControl`: the nearest ancestor with a XuiControl level,
/// starting at the parent.
pub fn owner_control(tree: &ElementTree, presenter: ElementId) -> Option<ElementId> {
    let mut ancestor = tree.parent(presenter);
    while let Some(current) = ancestor {
        if tree.get(current)?.traits.is_control {
            return Some(current);
        }
        ancestor = tree.parent(current);
    }

    None
}

fn data_association(tree: &ElementTree, presenter: ElementId) -> Option<u32> {
    match &tree.get(presenter)?.content {
        Content::Text(text) => Some(text.data_association),
        Content::Image(image) => Some(image.data_association),
        _ => None,
    }
}

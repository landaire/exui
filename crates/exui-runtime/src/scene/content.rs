//! Per-class element data.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;

use exui_core::value::Color;
use xbox360_draw::draw::TextureId;

use crate::message::ResourceMask;
use crate::resource::CachedTexture;
use crate::resource::TextureObject;
use crate::scene::ContentKind;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::figure::Figure;
use crate::scene::image::SizeMode;
use crate::scene::paint::TextureFlags;

/// XuiControl fields used by rendering and presenters.
#[derive(Debug)]
pub(crate) struct ControlProps {
    pub(crate) visual: Option<Box<[u16]>>,
    pub(crate) text: Option<Box<[u16]>>,
    /// `None` uses the presenter's PointSize; the property reads -1.0.
    pub(crate) point_size: Option<f32>,
    pub(crate) image_path: Option<Box<[u16]>>,
}

impl ControlProps {
    pub(crate) fn new() -> Self {
        Self {
            visual: None,
            text: None,
            point_size: None,
            image_path: None,
        }
    }
}

/// Text property block shared by XuiText and XuiTextPresenter.
#[derive(Debug)]
pub(crate) struct TextContent {
    /// XuiText only; presenters pull their text from the owner control.
    pub(crate) text: Option<Box<[u16]>>,
    pub(crate) color: Color,
    pub(crate) shadow_color: Color,
    pub(crate) point_size: f32,
    pub(crate) font: Option<Box<[u16]>>,
    pub(crate) style: u32,
    pub(crate) line_spacing_adjust: i32,
    pub(crate) data_association: u32,
}

/// Image brush of XuiImage and XuiImagePresenter.
#[derive(Debug)]
pub(crate) struct ImageContent {
    /// XuiImage ImagePath.
    pub(crate) path: Option<Box<[u16]>>,
    pub(crate) size_mode: SizeMode,
    pub(crate) brush_flags: TextureFlags,
    pub(crate) data_association: u32,
    pub(crate) texture: Option<CachedTexture>,
    /// Path the presenter's texture was loaded from.
    pub(crate) loaded_path: Option<Box<[u16]>>,
    /// Texture object the presenter shows; holds its texture alive.
    pub(crate) object: Option<TextureObject>,

    /// XuiImage scene content (`XuiImageBrush_AttachElement`): the loaded
    /// file root, a child of this element.
    pub(crate) scene: Option<ElementId>,
    /// Scene content detached from the brush by an ImagePath store that had
    /// no message context; destroyed by the next load.
    pub(crate) released_scene: Option<ElementId>,
    /// XuiImagePresenter: source path that `scene` was loaded from.
    pub(crate) scene_path: Option<Rc<[u16]>>,
}

impl ImageContent {
    /// `XuiImage_LoadIfNeeded` has work: a released scene to destroy or a
    /// scene path without attached content. A failed load leaves `scene`
    /// empty, so it is retried.
    pub(crate) fn needs_scene_load(&self) -> bool {
        self.released_scene.is_some()
            || (self.scene.is_none() && self.path.as_deref().is_some_and(is_scene_path))
    }
}

/// XuiNineGrid fields.
#[derive(Debug)]
pub(crate) struct NineGridContent {
    pub(crate) path: Option<Box<[u16]>>,
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) right: u32,
    pub(crate) bottom: u32,
    pub(crate) no_center: bool,
    pub(crate) texture: Option<CachedTexture>,
}

#[derive(Debug)]
pub(crate) enum Content {
    None,
    Figure(Box<Figure>),
    Image(Box<ImageContent>),
    NineGrid(Box<NineGridContent>),
    Text(Box<TextContent>),
}

impl Content {
    /// Zeroed fields; the element constructor then applies the schema
    /// defaults. Fields without a default stay zero, as the original
    /// constructors clear them [INFERENCE].
    pub(crate) fn new(kind: ContentKind) -> Self {
        match kind {
            ContentKind::None => Self::None,
            ContentKind::Figure => Self::Figure(Box::new(Figure::new())),
            ContentKind::Image | ContentKind::ImagePresenter => {
                Self::Image(Box::new(ImageContent {
                    path: None,
                    size_mode: SizeMode::NONE,
                    brush_flags: TextureFlags::default(),
                    data_association: 0,
                    texture: None,
                    loaded_path: None,
                    object: None,
                    scene: None,
                    released_scene: None,
                    scene_path: None,
                }))
            }
            ContentKind::NineGrid => Self::NineGrid(Box::new(NineGridContent {
                path: None,
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
                no_center: false,
                texture: None,
            })),
            ContentKind::Text
            | ContentKind::TextPresenter
            | ContentKind::HtmlElement
            | ContentKind::HtmlPresenter => {
                let html = matches!(kind, ContentKind::HtmlElement | ContentKind::HtmlPresenter);
                Self::Text(Box::new(TextContent {
                    text: None,
                    color: Color::from_argb(if html { 0xff00_0000 } else { 0 }),
                    shadow_color: Color::from_argb(0),
                    point_size: if html { 14.0 } else { 0.0 },
                    font: None,
                    style: 0,
                    line_spacing_adjust: 0,
                    data_association: 0,
                }))
            }
        }
    }

    /// Moves the textures this element owns to `out` and forgets the cached
    /// resource textures it draws, so the next draw loads them again.
    pub(crate) fn release_textures(&mut self, out: &mut Vec<TextureId>) {
        match self {
            Self::Figure(figure) => {
                out.extend(figure.gradient_texture.take());
                figure.fill_texture = None;
            }
            Self::Image(image) => image.texture = None,
            Self::NineGrid(grid) => grid.texture = None,
            // Text draws from the glyph cache of the text renderer.
            Self::None | Self::Text(_) => {}
        }
    }

    /// Message 0x28 at the content's class level. XuiImage and
    /// XuiImagePresenter drop their texture on the texture bit; XuiNineGrid
    /// drops its texture on every 0x28. The next draw loads it again.
    pub(crate) fn release_resources(&mut self, mask: ResourceMask) {
        match self {
            Self::Image(image) if mask.contains(ResourceMask::TEXTURES) => image.texture = None,
            Self::NineGrid(grid) => grid.texture = None,
            // The XuiFigure and XuiText handlers of 0x28 are not decoded.
            Self::None | Self::Figure(_) | Self::Image(_) | Self::Text(_) => {}
        }
    }

    /// The texture cache entry this element draws.
    pub(crate) fn cached_texture(&self) -> Option<CachedTexture> {
        match self {
            Self::Figure(figure) => figure.fill_texture,
            Self::Image(image) => image.texture,
            Self::NineGrid(grid) => grid.texture,
            Self::None | Self::Text(_) => None,
        }
    }
}

impl ElementTree {
    /// Image brush of an XuiImage or XuiImagePresenter.
    pub(crate) fn image_content(&self, id: ElementId) -> Option<&ImageContent> {
        match &self.get(id)?.content {
            Content::Image(image) => Some(image),
            _ => None,
        }
    }

    pub(crate) fn image_content_mut(&mut self, id: ElementId) -> Option<&mut ImageContent> {
        match &mut self.get_mut(id)?.content {
            Content::Image(image) => Some(image),
            _ => None,
        }
    }
}

/// A path ending in `.xu` plus one character loads as scene content
/// instead of a texture (`XuiImageBrush_LoadPath`).
pub(crate) fn is_scene_path(path: &[u16]) -> bool {
    let Some(start) = path.len().checked_sub(4) else {
        return false;
    };

    path[start..start + 3]
        .iter()
        .zip(".xu".bytes())
        .all(|(&unit, byte)| u8::try_from(unit).is_ok_and(|unit| unit.eq_ignore_ascii_case(&byte)))
}

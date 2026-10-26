//! Presenter data binding (XuiTextPresenter 0x9215c9f0, XuiImagePresenter
//! 0x92163090) and XuiMessageBox (0x9215f5e8).

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;

use crate::control::Builtin;
use crate::control::ClassState;
use crate::control::props;
use crate::error::MessageBoxError;
use crate::message::Body;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::Message;
use crate::message::NotifyKind;
use crate::message::SourceImageQuery;
use crate::message::SourceTextQuery;
use crate::resource::TextureObject;
use crate::scene::BasePathId;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::PresenterSource;
use crate::scene::PropValue;
use crate::scene::SourceImage;
use crate::scene::SourceText;
use crate::scene::is_scene_path;
use crate::scene::measure_text;
use crate::text::TextExtent;

pub(super) fn on_text_presenter(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    message: &mut Message,
) {
    if let Body::Init { .. } = message.body
        && let Some(ClassState::TextPresenter {
            owner,
            html_version,
            html_association,
            html_text,
        }) = cx.class_state(element)
    {
        *owner = None;
        *html_version = None;
        *html_association = None;
        *html_text = None;
    }
}

/// Text a presenter pulled from its owner control this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PulledText {
    pub text: Option<Rc<[u16]>>,
    /// `None` keeps the presenter's PointSize.
    pub point_size: Option<f32>,
    pub style: u32,
    pub y_offset: f32,
}

/// Image a presenter pulled from its owner control this frame: a texture
/// object, or a path with the owner's base path for relative paths.
#[derive(Clone, Debug, PartialEq)]
pub enum PulledImage {
    Path {
        path: Rc<[u16]>,
        base: Option<BasePathId>,
    },
    Texture(TextureObject),
}

/// Presenter answers gathered once per frame before drawing, since
/// presenters poll their source on every render.
#[derive(Debug, Default)]
pub struct RuntimePresenterSource {
    pub texts: BTreeMap<ElementId, PulledText>,
    pub images: BTreeMap<ElementId, PulledImage>,
    /// Image presenter owner cache (+0x8c), never invalidated.
    image_owners: BTreeMap<ElementId, ElementId>,
}

impl RuntimePresenterSource {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PresenterSource for RuntimePresenterSource {
    fn source_text<'a>(
        &'a mut self,
        _tree: &'a ElementTree,
        presenter: ElementId,
    ) -> Option<SourceText<'a>> {
        let pulled = self.texts.get(&presenter)?;
        let text = pulled.text.as_deref()?;

        Some(SourceText {
            text: alloc::borrow::Cow::Borrowed(text),
            point_size: pulled.point_size,
            style: Some(pulled.style),
            y_offset: pulled.y_offset,
        })
    }

    fn source_image<'a>(
        &'a mut self,
        _tree: &'a ElementTree,
        presenter: ElementId,
    ) -> Option<SourceImage<'a>> {
        let pulled = self.images.get(&presenter)?;

        Some(match pulled {
            PulledImage::Path { path, base } => SourceImage::Path {
                path: alloc::borrow::Cow::Borrowed(path),
                base: *base,
            },
            PulledImage::Texture(texture) => SourceImage::Texture(texture.clone()),
        })
    }
}

impl Context<'_, '_> {
    /// `XuiFindOwnerControl` 0x92157c60: nearest ancestor with a XuiControl
    /// level, starting at the parent.
    pub fn find_owner_control(&self, element: ElementId) -> Option<ElementId> {
        let mut current = self.tree.parent(element);
        while let Some(node) = current {
            if self.is_builtin_a(node, Builtin::Control) {
                return Some(node);
            }
            current = self.tree.parent(node);
        }

        None
    }

    /// Extent of `text` drawn by the text element or presenter `element`
    /// in its font, point size and style, wrapped at its width.
    pub fn measure_text(&mut self, element: ElementId, text: &[u16]) -> Option<TextExtent> {
        let runtime = &mut *self.runtime;
        measure_text(self.tree, element, text, &runtime.fonts, &mut runtime.text)
    }

    /// `XuiTextPresenter_GetSourceText` 0x9215c820.
    pub fn presenter_source_text(&mut self, presenter: ElementId) -> PulledText {
        let html = self.is_builtin_a(presenter, Builtin::HtmlPresenter);
        let style = if html {
            0
        } else {
            match self
                .tree
                .property(presenter, &[props::text_presenter_style()], None)
            {
                Some(PropValue::Unsigned(style)) => style,
                _ => 0,
            }
        };

        let cached = match self.class_state(presenter) {
            Some(ClassState::TextPresenter { owner, .. }) => *owner,
            _ => None,
        };
        let owner = match cached {
            Some(owner) if self.tree.contains(owner) => Some(owner),
            _ => {
                let owner = self.find_owner_control(presenter);
                if let Some(ClassState::TextPresenter { owner: slot, .. }) =
                    self.class_state(presenter)
                {
                    *slot = owner;
                }
                owner
            }
        };

        // Without an owner control no text is pulled.
        let Some(owner) = owner else {
            return PulledText {
                text: None,
                point_size: None,
                style,
                y_offset: 0.0,
            };
        };

        let association_def = if html {
            props::html_presenter_data_association()
        } else {
            props::text_presenter_data_association()
        };
        let association = match self.tree.property(presenter, &[association_def], None) {
            Some(PropValue::Unsigned(association)) => association,
            _ => 0,
        };

        if html {
            let message = self.send_body(
                owner,
                Body::GetTextVersion {
                    version: 0,
                    item: 0,
                    association,
                    text: None,
                },
            );
            let Body::GetTextVersion { version, text, .. } = message.body else {
                return PulledText {
                    text: None,
                    point_size: None,
                    style,
                    y_offset: 0.0,
                };
            };
            let Some(ClassState::TextPresenter {
                html_version,
                html_association,
                html_text,
                ..
            }) = self.class_state(presenter)
            else {
                return PulledText {
                    text,
                    point_size: None,
                    style,
                    y_offset: 0.0,
                };
            };
            if *html_version != Some(version) || *html_association != Some(association) {
                *html_version = Some(version);
                *html_association = Some(association);
                *html_text = text;
            }
            return PulledText {
                text: html_text.clone(),
                point_size: None,
                style,
                y_offset: 0.0,
            };
        }

        let message = self.send_body(
            owner,
            Body::GetSourceText(SourceTextQuery {
                item: 0,
                association,
                item_data: false,
                text: None,
                point_size: None,
                extended: true,
                y_offset: 0.0,
                style,
            }),
        );

        match message.body {
            Body::GetSourceText(query) => PulledText {
                text: query.text,
                point_size: query.point_size,
                style: query.style,
                y_offset: query.y_offset,
            },
            _ => PulledText {
                text: None,
                point_size: None,
                style,
                y_offset: 0.0,
            },
        }
    }

    /// `XuiImagePresenter_UpdateFromSource` 0x9215f348: texture object and
    /// path sources.
    pub fn presenter_source_image(
        &mut self,
        source: &mut RuntimePresenterSource,
        presenter: ElementId,
    ) -> Option<PulledImage> {
        let owner = match source.image_owners.get(&presenter) {
            Some(owner) => *owner,
            None => {
                let owner = self.find_owner_control(presenter)?;
                source.image_owners.insert(presenter, owner);
                owner
            }
        };

        let association = match self.tree.property(
            presenter,
            &[props::image_presenter_data_association()],
            None,
        ) {
            Some(PropValue::Unsigned(association)) => association,
            _ => 0,
        };

        let message = self.send_body(
            owner,
            Body::GetSourceImage(SourceImageQuery {
                item: 0,
                association,
                item_data: false,
                force_reload: false,
                path: None,
                texture: None,
            }),
        );

        let Body::GetSourceImage(query) = message.body else {
            return None;
        };

        if let Some(texture) = query.texture {
            return Some(PulledImage::Texture(texture));
        }

        Some(PulledImage::Path {
            path: query.path?,
            base: self.scene_base_path(owner),
        })
    }

    /// `XuiElementGetBasePath` 0x921EC178: a bubbling query that only
    /// scenes answer (`XuiSceneCreateInternal` is the one caller of
    /// `XuiElementSetBasePath`), so a list item built from a skin visual
    /// resolves data image paths against its scene.
    fn scene_base_path(&self, element: ElementId) -> Option<BasePathId> {
        let mut current = Some(element);
        while let Some(node) = current {
            if self.is_builtin_a(node, Builtin::Scene) {
                return self.tree.base_path(node);
            }
            current = self.tree.parent(node);
        }

        None
    }

    /// Pulls every shown presenter's text and image for this frame.
    pub fn pull_presenters(&mut self, root: ElementId, source: &mut RuntimePresenterSource) {
        source.texts.clear();
        source.images.clear();
        source
            .image_owners
            .retain(|presenter, _| self.tree.contains(*presenter));

        let mut stack = Vec::new();
        stack.push(root);
        while let Some(element) = stack.pop() {
            if !self.is_shown(element) {
                continue;
            }

            if self.is_builtin_a(element, Builtin::TextPresenter)
                || self.is_builtin_a(element, Builtin::HtmlPresenter)
            {
                let pulled = self.presenter_source_text(element);
                source.texts.insert(element, pulled);
            } else if self.is_builtin_a(element, Builtin::ImagePresenter) {
                let pulled = self.presenter_source_image(source, element);
                let scene_source = match &pulled {
                    Some(PulledImage::Path { path, base }) if is_scene_path(path) => {
                        Some((Rc::clone(path), *base))
                    }
                    _ => None,
                };
                let is_scene = scene_source.is_some();
                self.update_presenter_scene(element, scene_source);
                if let Some(pulled) = pulled.filter(|_| !is_scene) {
                    source.images.insert(element, pulled);
                }
            }

            stack.extend(self.tree.children(element));
        }
    }

    /// `XuiImage_LoadPath` for a presenter source path: a scene path loads
    /// the file and attaches its root to the brush (`XuiImageBrush_AttachElement`),
    /// here a child of the presenter. The scene is reloaded only when the
    /// path changes.
    fn update_presenter_scene(
        &mut self,
        presenter: ElementId,
        source: Option<(Rc<[u16]>, Option<BasePathId>)>,
    ) {
        let Some(image) = self.tree.image_content(presenter) else {
            return;
        };
        let unchanged = match (&source, &image.scene_path) {
            (Some((path, _)), Some(loaded)) => image.scene.is_some() && **path == **loaded,
            (None, None) => image.scene.is_none(),
            _ => false,
        };
        if unchanged {
            return;
        }

        let previous = self.tree.image_content_mut(presenter).and_then(|image| {
            image.scene_path = None;
            image.scene.take()
        });
        if let Some(previous) = previous {
            self.destroy(previous);
        }

        let Some((path, base)) = source else {
            return;
        };

        // A failed load leaves the brush empty; the next frame retries it.
        let Ok(scene) = self.load_document(base, &path) else {
            return;
        };
        if let Some(image) = self.tree.image_content_mut(presenter) {
            image.scene = Some(scene);
            image.scene_path = Some(path);
        }
        if self.insert_child(presenter, scene, None).is_err() {
            if let Some(image) = self.tree.image_content_mut(presenter) {
                image.scene = None;
                image.scene_path = None;
            }
            self.destroy(scene);
            return;
        }
        self.layout_brush_scene(presenter, scene);
    }
}

/// XuiMessageBox instance (+4 owner, +8 previous focus, +0xc user).
#[derive(Clone, Copy, Debug, Default)]
pub struct MessageBoxState {
    pub owner: Option<ElementId>,
    pub previous_focus: Option<ElementId>,
    /// User of the previous focus, or the shared slot when none.
    pub user: Option<FocusUser>,
}

impl MessageBoxState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Digit of an Id that is exactly `Button` plus one digit.
fn button_digit(id: &[u16]) -> Option<u32> {
    const PREFIX: &[u8] = b"Button";

    if id.len() != PREFIX.len() + 1 {
        return None;
    }
    if !id
        .iter()
        .zip(PREFIX)
        .all(|(unit, byte)| *unit == u16::from(*byte))
    {
        return None;
    }

    let digit = id[PREFIX.len()];
    (u16::from(b'0')..=u16::from(b'9'))
        .contains(&digit)
        .then(|| u32::from(digit - u16::from(b'0')))
}

pub(super) fn on_message_box(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    let Body::Notify(notify) = &message.body else {
        return;
    };
    if !matches!(notify.kind, NotifyKind::Press { .. }) {
        return;
    }

    let source = notify.source;
    let Some(digit) = cx
        .tree
        .get(source)
        .and_then(|source| source.id())
        .and_then(button_digit)
    else {
        return;
    };
    message.handled = true;

    let state = match cx.class_state(element) {
        Some(ClassState::MessageBox(state)) => *state,
        _ => MessageBoxState::new(),
    };

    // The box is destroyed before the result is sent.
    cx.destroy(element);
    let user = state.user.unwrap_or(FocusUser::Shared);
    let _ = cx.set_focus(state.previous_focus, user);
    if let Some(owner) = state.owner {
        cx.send_body(
            owner,
            Body::MessageBoxResult {
                button: Some(digit),
            },
        );
    }
}

/// Arguments of `XuiShowMessageBoxEx`.
#[derive(Clone, Debug)]
pub struct MessageBoxRequest<'t> {
    /// Visual name; `None` selects `XuiMessageBox<N>`.
    pub visual: Option<&'t [u16]>,
    pub parent: Option<ElementId>,
    pub owner: ElementId,
    pub title: Option<&'t [u16]>,
    pub text: Option<&'t [u16]>,
    pub buttons: &'t [&'t [u16]],
    pub default_button: usize,
    /// Flag 1: center the body in the parent.
    pub center: bool,
}

impl Context<'_, '_> {
    /// `XuiShowMessageBoxEx` 0x9215f8e8.
    pub fn show_message_box(
        &mut self,
        request: &MessageBoxRequest<'_>,
    ) -> Result<ElementId, MessageBoxError> {
        // Buttons are named by one decimal digit.
        const MAX_BUTTONS: usize = 9;

        let count = request.buttons.len();
        if !(1..=MAX_BUTTONS).contains(&count) || request.default_button >= count {
            return Err(MessageBoxError::InvalidArgument);
        }

        let parent = match request.parent {
            Some(parent) => parent,
            None => {
                let mut top = request.owner;
                while let Some(parent) = self.tree.parent(top) {
                    top = parent;
                }
                top
            }
        };

        let visual: Vec<u16> = match request.visual {
            Some(name) => name.into(),
            None => alloc::format!("XuiMessageBox{count}")
                .encode_utf16()
                .collect(),
        };

        let class = self
            .registry
            .find("XuiMessageBox")
            .and_then(|class| self.registry.class(class))
            .ok_or(MessageBoxError::NoClass)?;
        let element = self.tree.create(class);
        let _ = self.tree.set_property(
            element,
            &[props::visual()],
            None,
            PropValue::String(Some(visual.as_slice().into())),
        );
        if self.insert_child(parent, element, None).is_err() {
            self.destroy(element);
            return Err(MessageBoxError::MissingPart);
        }
        self.broadcast(element, &Body::CreateResources);

        match self.setup_message_box(element, request) {
            Ok(()) => Ok(element),
            Err(error) => {
                self.destroy(element);
                Err(error)
            }
        }
    }

    /// `XuiMessageBox_Setup` 0x9215f6d8.
    fn setup_message_box(
        &mut self,
        element: ElementId,
        request: &MessageBoxRequest<'_>,
    ) -> Result<(), MessageBoxError> {
        let body = self
            .control_visual(element)
            .and_then(|visual| self.tree.first_child(visual))
            .ok_or(MessageBoxError::MissingPart)?;

        let mut default = None;
        for (index, text) in request.buttons.iter().enumerate() {
            let id: Vec<u16> = alloc::format!("Button{index}").encode_utf16().collect();
            let button = self
                .tree
                .child_by_id(body, &id)
                .ok_or(MessageBoxError::MissingPart)?;
            self.set_control_text(button, Some(text));
            if index == request.default_button {
                default = Some(button);
            }
        }

        let message_text: Vec<u16> = "MessageText".encode_utf16().collect();
        if let Some(text) = self.tree.child_by_id(body, &message_text) {
            self.set_control_text(text, request.text);
        }
        self.set_control_text(body, request.title);

        if request.center
            && let Some(parent) = self.tree.parent(element)
            && let (Some(parent_size), Some(body_size)) = (
                self.tree.get(parent).map(|parent| parent.size()),
                self.tree.get(body).map(|body| body.size()),
            )
        {
            let position = exui_core::value::Vector3::new(
                (parent_size.width - body_size.width) * 0.5,
                (parent_size.height - body_size.height) * 0.5,
                0.0,
            );
            let _ = self.tree.set_property(
                element,
                &[props::position()],
                None,
                PropValue::Vector(position),
            );
        }

        let previous_focus = self.focus_owner(request.owner);
        let user = previous_focus
            .and_then(|focus| self.focus_user(focus))
            .unwrap_or(FocusUser::Shared);
        if let Some(ClassState::MessageBox(state)) = self.class_state(element) {
            state.owner = Some(request.owner);
            state.previous_focus = previous_focus;
            state.user = Some(user);
        }

        if let Some(default) = default {
            let _ = self.init_focus(default, user, false);
        }

        Ok(())
    }
}

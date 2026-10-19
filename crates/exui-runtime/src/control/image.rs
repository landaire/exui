//! XuiImage level (impl 0x921631b8): scene content of the image brush.
//! Texture paths load at draw time (scene/render.rs).

use exui_core::value::Vector3;

use crate::control::props;
use crate::message::Body;
use crate::message::Context;
use crate::message::Message;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::scene::Size;
use crate::scene::image::SizeMode;
use crate::scene::image::fit_size;
use crate::scene::is_scene_path;

pub(super) fn on_image(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match message.body {
        Body::Render => cx.load_image_if_needed(element),
        Body::CreateResources => {
            cx.load_image_if_needed(element);
            message.handled = true;
        }
        _ => {}
    }
}

impl Context<'_, '_> {
    /// `XuiImage_LoadIfNeeded` 0x9215f560 for scene paths: loads the file
    /// relative to the image (`XuiImageBrush_LoadPath`) and attaches its
    /// root as a child of the image.
    pub(crate) fn load_image_if_needed(&mut self, element: ElementId) {
        self.destroy_released_image_scene(element);

        let Some(image) = self.tree.image_content(element) else {
            return;
        };
        if image.scene.is_some() {
            return;
        }
        let Some(path) = image.path.as_deref().filter(|path| is_scene_path(path)) else {
            return;
        };
        let path = path.to_vec();
        let base = self.tree.base_path(element);

        // A failed load leaves the brush empty; the next pass retries it.
        let Ok(scene) = self.load_document(base, &path) else {
            return;
        };

        // Recorded before the insert so that a handler of the insert
        // messages sees the content as attached.
        if let Some(image) = self.tree.image_content_mut(element) {
            image.scene = Some(scene);
        }
        if self.insert_child(element, scene, None).is_err() {
            if let Some(image) = self.tree.image_content_mut(element) {
                image.scene = None;
            }
            self.destroy(scene);
            return;
        }
        self.layout_brush_scene(element, scene);
    }

    /// `sub_9215F198`, the element case of `XuiImage_Draw`: places the
    /// attached scene root by the image's SizeMode, relative to the
    /// position and scale it had when attached. Applied once at attach.
    pub(crate) fn layout_brush_scene(&mut self, element: ElementId, scene: ElementId) {
        let Some(size_mode) = self
            .tree
            .image_content(element)
            .map(|image| image.size_mode)
        else {
            return;
        };
        let (Some(image), Some(content)) = (self.tree.get(element), self.tree.get(scene)) else {
            return;
        };
        let Size { width, height } = image.size();
        let Size {
            width: content_width,
            height: content_height,
        } = content.size();
        let position = content.position();
        let scale = content.scale();

        let (offset, factor) = if size_mode.contains(SizeMode::RESIZE_TO_TEXTURE) {
            self.tree.set_bounds(element, content_width, content_height);
            self.flush_size_changes();
            ((0.0, 0.0), (1.0, 1.0))
        } else if size_mode.contains(SizeMode::CENTER) {
            (
                (
                    (width - content_width) * 0.5,
                    (height - content_height) * 0.5,
                ),
                (1.0, 1.0),
            )
        } else if size_mode.contains(SizeMode::STRETCH) {
            ((0.0, 0.0), (width / content_width, height / content_height))
        } else if size_mode.contains(SizeMode::FIT) || size_mode.contains(SizeMode::FIT_CENTERED) {
            let fitted = fit_size(
                Size { width, height },
                Size {
                    width: content_width,
                    height: content_height,
                },
            );
            let factor = fitted.width / content_width;
            let offset = if size_mode.contains(SizeMode::FIT) {
                (0.0, 0.0)
            } else {
                ((width - fitted.width) * 0.5, (height - fitted.height) * 0.5)
            };
            (offset, (factor, factor))
        } else {
            return;
        };

        let _ = self.tree.set_property(
            scene,
            &[props::scale()],
            None,
            PropValue::Vector(Vector3::new(
                scale.x * factor.0,
                scale.y * factor.1,
                scale.z,
            )),
        );
        let _ = self.tree.set_property(
            scene,
            &[props::position()],
            None,
            PropValue::Vector(Vector3::new(
                position.x + offset.0,
                position.y + offset.1,
                position.z,
            )),
        );
    }

    /// Destroys scene content an ImagePath store detached from the brush.
    pub(crate) fn destroy_released_image_scene(&mut self, element: ElementId) {
        let released = self
            .tree
            .image_content_mut(element)
            .and_then(|image| image.released_scene.take());
        if let Some(scene) = released {
            self.destroy(scene);
        }
    }
}

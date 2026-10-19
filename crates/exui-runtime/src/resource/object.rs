//! Texture objects: images the application hands to the UI, such as the
//! texture object of message 0x7e1 (+0x14) or the image a control draws in
//! its render handler (xui-controls.md). An object uploads its image on the
//! first draw; its texture is destroyed after the last reference drops,
//! like the reference-counted textures of the original.

use alloc::collections::BTreeSet;
use alloc::rc::Rc;
use alloc::rc::Weak;
use alloc::vec::Vec;
use core::cell::Cell;
use core::cell::RefCell;
use core::num::NonZeroU32;

use xbox360_draw::draw::TextureId;
use xbox360_draw::error::TextureError;
use xbox360_draw::image::Image;
use xbox360_draw::texture::TextureStore;

use crate::resource::CachedTexture;

/// Textures of dropped [`TextureObject`]s and released memory blocks,
/// destroyed before the next draw.
#[derive(Clone, Debug, Default)]
pub struct TextureReleases(Rc<RefCell<Shared>>);

impl TextureReleases {
    pub fn new() -> Self {
        Self::default()
    }

    /// A texture object of `image` whose texture is released here.
    pub fn create(&self, image: Image) -> TextureObject {
        TextureObject(Rc::new(Inner {
            width: image.width(),
            height: image.height(),
            pending: RefCell::new(Some(image)),
            texture: Cell::new(None),
            releases: Rc::clone(&self.0),
        }))
    }

    /// Queues `texture` for destruction.
    pub(crate) fn release(&self, texture: TextureId) {
        self.0.borrow_mut().released.push(texture);
    }

    /// Textures to destroy, oldest first.
    pub fn take(&self) -> Vec<TextureId> {
        core::mem::take(&mut self.0.borrow_mut().released)
    }

    /// Moves the queued textures and those of every live object to `out`.
    /// The objects keep no texture afterwards.
    pub(crate) fn take_all(&self, out: &mut Vec<TextureId>) {
        let mut shared = self.0.borrow_mut();
        out.append(&mut shared.released);
        out.extend(core::mem::take(&mut shared.live));
    }
}

#[derive(Debug, Default)]
struct Shared {
    /// Uploaded textures of live objects.
    live: BTreeSet<TextureId>,
    /// Textures to destroy, oldest first.
    released: Vec<TextureId>,
}

/// A shared image; clones refer to the same texture.
#[derive(Clone)]
pub struct TextureObject(Rc<Inner>);

struct Inner {
    width: NonZeroU32,
    height: NonZeroU32,
    /// The image until its upload.
    pending: RefCell<Option<Image>>,
    texture: Cell<Option<CachedTexture>>,
    releases: Rc<RefCell<Shared>>,
}

impl TextureObject {
    pub fn width(&self) -> NonZeroU32 {
        self.0.width
    }

    pub fn height(&self) -> NonZeroU32 {
        self.0.height
    }

    /// The texture, uploading the image on first use. `None` after a
    /// failed upload, which consumed the image, and after the UI released
    /// every texture of its factory.
    pub fn texture(
        &self,
        store: &mut dyn TextureStore,
    ) -> Result<Option<CachedTexture>, TextureError> {
        if let Some(texture) = self.0.texture.get() {
            let live = self.0.releases.borrow().live.contains(&texture.id);
            return Ok(live.then_some(texture));
        }
        let Some(image) = self.0.pending.borrow_mut().take() else {
            return Ok(None);
        };

        let id = store.create_texture(image)?;
        self.0.releases.borrow_mut().live.insert(id);
        let texture = CachedTexture {
            id,
            width: self.0.width,
            height: self.0.height,
        };
        self.0.texture.set(Some(texture));

        Ok(Some(texture))
    }

    /// A reference that does not keep the image alive.
    pub fn downgrade(&self) -> WeakTextureObject {
        WeakTextureObject(Rc::downgrade(&self.0))
    }
}

impl PartialEq for TextureObject {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl core::fmt::Debug for TextureObject {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureObject")
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .field("texture", &self.0.texture.get())
            .finish()
    }
}

/// A [`TextureObject`] reference that does not keep the image alive.
#[derive(Clone)]
pub struct WeakTextureObject(Weak<Inner>);

impl WeakTextureObject {
    /// The object while any [`TextureObject`] of it exists.
    pub fn upgrade(&self) -> Option<TextureObject> {
        self.0.upgrade().map(TextureObject)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let Some(texture) = self.texture.get() else {
            return;
        };

        // A texture taken by `take_all` is already queued elsewhere.
        let mut shared = self.releases.borrow_mut();
        if shared.live.remove(&texture.id) {
            shared.released.push(texture.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use xbox360_draw::draw::Rgba8;
    use xbox360_draw::texture::TextureSlots;

    use super::*;

    struct CountingStore {
        slots: TextureSlots<()>,
        created: u32,
    }

    impl TextureStore for CountingStore {
        fn create_texture(&mut self, _image: Image) -> Result<TextureId, TextureError> {
            self.created += 1;
            self.slots.insert(())
        }

        fn update_texture(&mut self, _id: TextureId, _image: Image) -> Result<(), TextureError> {
            Ok(())
        }

        fn destroy_texture(&mut self, _id: TextureId) -> Result<(), TextureError> {
            Ok(())
        }
    }

    #[test]
    fn uploads_once_and_releases_after_the_last_clone() {
        let one = NonZeroU32::MIN;
        let image = Image::new(one, one, vec![Rgba8::new(1, 2, 3, 4)]).unwrap();
        let releases = TextureReleases::new();
        let object = releases.create(image);
        let clone = object.clone();
        let mut store = CountingStore {
            slots: TextureSlots::new(),
            created: 0,
        };

        let first = object.texture(&mut store).unwrap().unwrap();
        let second = clone.texture(&mut store).unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(store.created, 1);

        drop(object);
        assert!(releases.take().is_empty());
        drop(clone);
        assert_eq!(releases.take(), vec![first.id]);
    }
}

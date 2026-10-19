use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::num::NonZeroU32;

use xbox360_draw::draw::TextureId;
use xbox360_draw::image::Image;
use xbox360_draw::texture::TextureStore;

use crate::resource::Locator;
use crate::resource::Resources;
use crate::resource::TextureLoadError;
use crate::resource::TextureLoader;
use crate::resource::locator;

/// A loaded texture and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedTexture {
    pub id: TextureId,
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

/// Textures loaded from resources, keyed by resolved path. A path loads
/// once; failed loads are not cached, so they are retried like XUI retries
/// an image whose load failed. Entries of released memory blocks are
/// removed with the block.
#[derive(Debug, Default)]
pub struct TextureCache {
    entries: RefCell<BTreeMap<Box<[u16]>, CachedTexture>>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            entries: RefCell::new(BTreeMap::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    fn get(&self, key: &[u16]) -> Option<CachedTexture> {
        self.entries.borrow().get(key).copied()
    }

    fn insert(&self, key: Box<[u16]>, texture: CachedTexture) {
        self.entries.borrow_mut().insert(key, texture);
    }

    pub(crate) fn remove(&self, key: &[u16]) -> Option<CachedTexture> {
        self.entries.borrow_mut().remove(key)
    }

    /// Empties the cache, moving every texture to `out`.
    fn take_all(&self, out: &mut Vec<TextureId>) {
        let entries = core::mem::take(&mut *self.entries.borrow_mut());
        out.extend(entries.into_values().map(|texture| texture.id));
    }

    /// Removes the textures `in_use` does not hold, moving them to `out`.
    fn retain_in_use(&self, in_use: &BTreeSet<TextureId>, out: &mut Vec<TextureId>) {
        self.entries.borrow_mut().retain(|_, texture| {
            let keep = in_use.contains(&texture.id);
            if !keep {
                out.push(texture.id);
            }

            keep
        });
    }
}

/// Default texture loader: reads the path through the resources and
/// decodes PNG or JPEG by file signature.
#[derive(Clone, Copy, Debug, Default)]
pub struct SignatureLoader;

impl TextureLoader for SignatureLoader {
    fn load(&self, path: &[u16], resources: &Resources<'_>) -> Result<Image, TextureLoadError> {
        let image = match resources.resolve(path, None)? {
            Locator::Memory { block, size } => {
                xbox360_image::decode(&resources.read_memory(block, size)?)?
            }
            locator => xbox360_image::decode(&resources.read(&locator)?)?,
        };

        Ok(image)
    }
}

impl Resources<'_> {
    /// Loads the image `path` (an ImagePath or TextureFileName value) with
    /// the texture loader after joining relative paths with `base`.
    pub fn texture(
        &mut self,
        path: &[u16],
        base: Option<&[u16]>,
        store: &mut dyn TextureStore,
    ) -> Result<CachedTexture, TextureLoadError> {
        let full = locator::join(path, base)?;
        let key = match self.resolve(&full, None) {
            Ok(locator) => locator.cache_key(),
            // Paths only the texture loader understands key as written.
            Err(_) => full.clone().into_boxed_slice(),
        };
        if let Some(texture) = self.textures.get(&key) {
            return Ok(texture);
        }

        let image = match self.texture_loader {
            Some(loader) => loader.load(&full, self)?,
            None => SignatureLoader.load(&full, self)?,
        };
        let (width, height) = (image.width(), image.height());
        let id = store.create_texture(image)?;
        let texture = CachedTexture { id, width, height };
        self.textures.insert(key, texture);

        Ok(texture)
    }

    /// Moves every loaded texture and the textures of released memory
    /// blocks to `out`.
    pub(crate) fn release_textures(&mut self, out: &mut Vec<TextureId>) {
        out.extend(self.texture_releases.take());
        self.textures.take_all(out);
    }

    /// `0x9217A400(0)`, the texture cache flush of `XuiFreeResources`: the
    /// cached textures no element draws are forgotten and destroyed before
    /// the next draw, so their paths load again on the next use.
    ///
    /// [INFERENCE] XUI textures are reference counted, so the flush frees
    /// only the textures that no element holds.
    pub(crate) fn flush_textures(&self, in_use: &BTreeSet<TextureId>) {
        let mut released = Vec::new();
        self.textures.retain_in_use(in_use, &mut released);
        for texture in released {
            self.texture_releases.release(texture);
        }
    }
}

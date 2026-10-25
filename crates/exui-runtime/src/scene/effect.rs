//! Render target of an EnableEffects element (doc/xui-render.md section 5).

use core::num::NonZeroU32;

use xbox360_draw::draw::TextureId;
use xbox360_draw::error::TextureError;
use xbox360_draw::texture::MAX_TEXTURE_DIMENSION;
use xbox360_draw::texture::TextureStore;

use crate::error::FrameError;
use crate::scene::Size;
use crate::scene::paint::LocalVertex;

/// Size of a render target in texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TargetSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl TargetSize {
    /// `ceil` of the element size, converted with truncation.
    pub(crate) fn of_element(size: Size) -> Self {
        Self {
            width: libm::ceilf(size.width) as u32,
            height: libm::ceilf(size.height) as u32,
        }
    }

    /// Each side rounded up to a multiple of 32. A side too large to round
    /// is rejected as the texture store rejects an oversized target.
    fn allocation(self) -> Result<Self, TextureError> {
        let too_large = TextureError::TooLarge {
            width: self.width,
            height: self.height,
            max: MAX_TEXTURE_DIMENSION,
        };
        match (
            self.width.checked_next_multiple_of(32),
            self.height.checked_next_multiple_of(32),
        ) {
            (Some(width), Some(height)) => Ok(Self { width, height }),
            _ => Err(too_large),
        }
    }
}

/// The texture an element's children are drawn into in render pass 1
/// (element +0xf4, 0x92185648).
#[derive(Debug)]
pub(crate) struct EffectTarget {
    pub(crate) texture: TextureId,
    /// Allocated size; it only grows.
    allocated: TargetSize,
    /// Size of the last pass-1 draw.
    used: TargetSize,
}

impl EffectTarget {
    /// Records the size of a pass-1 draw and reallocates the texture when
    /// it does not fit. Returns `None` when there is no texture to draw
    /// into: the first allocation of an empty size.
    pub(crate) fn prepare(
        target: &mut Option<EffectTarget>,
        used: TargetSize,
        textures: &mut dyn TextureStore,
    ) -> Result<Option<TextureId>, FrameError> {
        if let Some(current) = target.as_mut() {
            current.used = used;
            if current.allocated.width >= used.width && current.allocated.height >= used.height {
                return Ok(Some(current.texture));
            }
        }

        let allocated = used.allocation()?;
        let (Some(width), Some(height)) = (
            NonZeroU32::new(allocated.width),
            NonZeroU32::new(allocated.height),
        ) else {
            return Ok(None);
        };

        if let Some(previous) = target.take() {
            textures.destroy_texture(previous.texture)?;
        }
        let texture = textures.create_target(width, height)?;
        *target = Some(EffectTarget {
            texture,
            allocated,
            used,
        });

        Ok(Some(texture))
    }

    /// The quad of 0x92185a50 as two triangles of its 4-vertex strip: the
    /// used texels, moved up and left by half a unit.
    pub(crate) fn quad(&self) -> [LocalVertex; 4] {
        const HALF_TEXEL: f32 = 0.5;

        let width = self.used.width as f32;
        let height = self.used.height as f32;
        let u = width / self.allocated.width as f32;
        let v = height / self.allocated.height as f32;
        let vertex = |x: f32, y: f32, u: f32, v: f32| LocalVertex {
            x: x - HALF_TEXEL,
            y: y - HALF_TEXEL,
            u,
            v,
        };

        [
            vertex(0.0, height, 0.0, v),
            vertex(0.0, 0.0, 0.0, 0.0),
            vertex(width, height, u, v),
            vertex(width, 0.0, u, 0.0),
        ]
    }
}

/// Triangle list of a 4-vertex strip.
pub(crate) const STRIP_TRIANGLES: [[u32; 3]; 2] = [[0, 1, 2], [2, 1, 3]];

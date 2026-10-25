//! Font instances: pixel metrics, glyph records and the A8 glyph cache.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::num::NonZeroU32;

use xbox360_draw::draw::Rgba8;
use xbox360_draw::draw::TextureId;
use xbox360_draw::image::Image;
use xbox360_draw::texture::TextureStore;

use crate::text::error::GlyphCacheError;
use crate::text::error::TextError;
use crate::text::library::TypefaceEntry;
use crate::text::library::TypefaceGeneration;
use crate::text::raster::MAX_EDGES;
use crate::text::raster::OVERSAMPLE;
use crate::text::raster::Placement;
use crate::text::raster::Rasterizer;
use crate::text::sfnt::Font;
use crate::text::sfnt::GlyphId;
use crate::text::sfnt::OutlineScratch;
use crate::text::synthetic::SyntheticStyle;

/// Font instances are shared by (typeface, style & 0xe, point size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstanceKey {
    pub generation: TypefaceGeneration,
    pub font_flags: u32,
    /// Bits of the clamped point size.
    pub point_size: u32,
}

/// Which font of a typeface supplies a glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontSlot {
    Primary,
    Fallback,
}

/// Placement of a rasterized glyph in the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub page: u16,
    /// Top left texel of the glyph image.
    pub x: u16,
    pub y: u16,
    /// Image left edge relative to the pen in pixels.
    pub left: i16,
    /// Image width in pixels; the height is the instance's cell height.
    pub width: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellState {
    NotRasterized,
    /// Glyph without contours.
    Blank,
    /// Glyph that cannot be rasterized into the cache; it draws nothing.
    Failed,
    Cached(Cell),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphRecord {
    pub slot: FontSlot,
    pub glyph: GlyphId,
    /// `ceil` of the advance in pixels.
    pub advance: u16,
    pub cell: CellState,
}

/// One size and style of a typeface.
#[derive(Debug)]
pub(crate) struct FontInstance {
    pub key: InstanceKey,
    /// Frame of the latest request that used the instance.
    pub last_used: u32,
    pixel_size: f32,
    /// Line height after the typeface's reduction, the maximum over its fonts.
    line_height: f32,
    /// Distance the glyph image starts above the line top.
    top_offset: f32,
    /// Baseline below the glyph image top.
    ascent: f32,

    page_size: NonZeroU32,
    pages: Vec<Page>,
    /// Page receiving new cells.
    current_page: usize,
    glyphs: BTreeMap<u16, GlyphRecord>,
    /// Frame in which the cache needed a new page; the cache is cleared
    /// before the instance is used in a later frame.
    overflow_frame: Option<u32>,
}

impl FontInstance {
    pub(crate) fn new(key: InstanceKey, typeface: &TypefaceEntry, point_size: f32) -> Self {
        const PAGE_SMALL: NonZeroU32 = NonZeroU32::new(128).unwrap();
        const PAGE_MEDIUM: NonZeroU32 = NonZeroU32::new(256).unwrap();
        const PAGE_LARGE: NonZeroU32 = NonZeroU32::new(512).unwrap();

        let pixel_size = point_size * 96.0 / 72.0;
        let fraction = typeface.line_height_reduction;
        let style_of = |font: &Font| SyntheticStyle::new(key.font_flags, font.units_per_em());

        let font_line_height = |font: &Font| {
            (font.line_height_units() + style_of(font).extra_units()) as f32 * pixel_size
                / font.units_per_em() as f32
        };
        let primary_height = font_line_height(&typeface.primary);
        let reduced = |height: f32| height - height * fraction;
        let line_height = match &typeface.fallback {
            Some(fallback) => reduced(primary_height).max(reduced(font_line_height(fallback))),
            None => reduced(primary_height),
        };

        let primary = &typeface.primary;
        let ascent = (primary.ascender() as i32 + style_of(primary).extra_units()) as f32
            * pixel_size
            / primary.units_per_em() as f32;
        let page_size = if pixel_size < 12.0 {
            PAGE_SMALL
        } else if pixel_size < 64.0 {
            PAGE_MEDIUM
        } else {
            PAGE_LARGE
        };

        Self {
            key,
            last_used: 0,
            pixel_size,
            line_height,
            top_offset: fraction * primary_height,
            ascent,
            page_size,
            pages: Vec::new(),
            current_page: 0,
            glyphs: BTreeMap::new(),
            overflow_frame: None,
        }
    }

    /// `ceil(line height)`, the base of the line pitch.
    pub(crate) fn line_height_ceil(&self) -> i32 {
        libm::ceilf(self.line_height) as i32
    }

    pub(crate) fn top_offset(&self) -> f32 {
        self.top_offset
    }

    /// Height of every glyph image and quad.
    pub(crate) fn cell_height(&self) -> u32 {
        // The float cast saturates a negative line height to 0.
        (libm::ceilf(self.line_height) as u32).saturating_add(1)
    }

    pub(crate) fn page_size(&self) -> u32 {
        self.page_size.get()
    }

    pub(crate) fn page_texture(&self, page: u16) -> Option<TextureId> {
        self.pages.get(page as usize).and_then(|page| page.texture)
    }

    /// Glyph record of a code unit, looked up in the primary font, then the
    /// fallback font, then the primary font's default glyph.
    pub(crate) fn record(&mut self, typeface: &TypefaceEntry, code: u16) -> GlyphRecord {
        if let Some(record) = self.glyphs.get(&code) {
            return *record;
        }

        let primary = typeface
            .primary
            .glyph(code)
            .map(|glyph| (FontSlot::Primary, glyph));
        let fallback = || {
            typeface
                .fallback
                .as_ref()
                .and_then(|font| font.glyph(code))
                .map(|glyph| (FontSlot::Fallback, glyph))
        };
        let (slot, glyph) = primary
            .or_else(fallback)
            .unwrap_or((FontSlot::Primary, GlyphId::DEFAULT));
        let font = typeface.font(slot);
        let style = SyntheticStyle::new(self.key.font_flags, font.units_per_em());

        let advance = libm::ceilf(
            (font.advance_units(glyph) as i32 + style.extra_units()) as f32 * self.pixel_size
                / font.units_per_em() as f32,
        ) as u16;
        let record = GlyphRecord {
            slot,
            glyph,
            advance,
            cell: CellState::NotRasterized,
        };
        self.glyphs.insert(code, record);

        record
    }

    /// Clears an overflowed cache once the frame that overflowed it is
    /// over. The pages stay allocated, so a stable working set settles
    /// without further clears.
    pub(crate) fn begin_use(&mut self, frame: u32) {
        if self.overflow_frame.is_none_or(|overflow| overflow == frame) {
            return;
        }

        self.overflow_frame = None;
        for record in self.glyphs.values_mut() {
            record.cell = CellState::NotRasterized;
        }

        for page in &mut self.pages {
            page.clear();
        }
        self.current_page = 0;
    }

    /// Rasterizes the glyph of `code` into the cache if needed. A glyph
    /// that fails for its own data or size stays failed until the cache is
    /// cleared; one that finds the cache full is retried.
    pub(crate) fn cache(
        &mut self,
        typeface: &TypefaceEntry,
        code: u16,
        frame: u32,
        scratch: &mut Scratch,
    ) -> Result<CellState, GlyphCacheError> {
        let record = self.record(typeface, code);
        if record.cell != CellState::NotRasterized {
            return Ok(record.cell);
        }

        let result = self.rasterize(typeface, record, frame, scratch);
        if matches!(result, Err(GlyphCacheError::Full { .. })) {
            return result;
        }

        let cell = match &result {
            Ok(cell) => *cell,
            Err(_) => CellState::Failed,
        };
        if let Some(record) = self.glyphs.get_mut(&code) {
            record.cell = cell;
        }

        result
    }

    fn rasterize(
        &mut self,
        typeface: &TypefaceEntry,
        record: GlyphRecord,
        frame: u32,
        scratch: &mut Scratch,
    ) -> Result<CellState, GlyphCacheError> {
        let font = typeface.font(record.slot);
        let style = SyntheticStyle::new(self.key.font_flags, font.units_per_em());

        let scale = self.pixel_size * OVERSAMPLE as f32 / font.units_per_em() as f32;
        let placement = Placement {
            scale,
            baseline: self.ascent * OVERSAMPLE as f32,
        };
        let bounds = {
            let mut sink = scratch.rasterizer.outline_sink(placement);
            font.outline(record.glyph, style, &mut sink, &mut scratch.outline)
                .map_err(|source| GlyphCacheError::Outline {
                    glyph: record.glyph.0,
                    source,
                })?
        };
        if scratch.rasterizer.overflowed() {
            return Err(GlyphCacheError::TooManyEdges {
                glyph: record.glyph.0,
                limit: MAX_EDGES,
            });
        }

        let Some(bounds) = bounds else {
            return Ok(CellState::Blank);
        };

        let pixel_scale = scale / OVERSAMPLE as f32;
        let left = libm::floorf(bounds.x_min as f32 * pixel_scale);
        let width =
            (libm::ceilf(bounds.x_max as f32 * pixel_scale - left) as u32).saturating_add(1);
        let height = self.cell_height();
        let page_size = self.page_size.get();
        if width.saturating_add(2) > page_size {
            return Err(GlyphCacheError::TooLarge {
                width,
                height,
                page_size,
            });
        }

        // Cell widths follow XUI's ceil(advance) + 7 rounded to 16,
        // widened for images wider than the advance.
        let slot_width = (record.advance as u32 + 7)
            .next_multiple_of(16)
            .max((width + 2).next_multiple_of(16));
        let slot = self.allocate(slot_width, frame)?;

        let stride = page_size as usize;
        let origin = slot.y as usize * stride + slot.x as usize;
        let page = &mut self.pages[slot.page as usize];
        scratch.rasterizer.fill(
            left as i32,
            width as usize,
            height as usize,
            &mut page.coverage,
            origin,
            stride,
        );
        page.dirty = true;

        Ok(CellState::Cached(Cell {
            page: slot.page,
            x: slot.x as u16,
            y: slot.y as u16,
            left: left as i16,
            width: width as u16,
        }))
    }

    /// Creates textures for pages that have none yet.
    pub(crate) fn create_textures(
        &mut self,
        textures: &mut dyn TextureStore,
    ) -> Result<(), TextError> {
        let size = self.page_size;
        for page in self.pages.iter_mut().filter(|page| page.texture.is_none()) {
            page.texture = Some(textures.create_texture(page.image(size)?)?);
            page.dirty = false;
        }

        Ok(())
    }

    /// Uploads pages changed since their last upload.
    pub(crate) fn upload(&mut self, textures: &mut dyn TextureStore) -> Result<(), TextError> {
        let size = self.page_size;
        for page in self.pages.iter_mut().filter(|page| page.dirty) {
            let image = page.image(size)?;
            match page.texture {
                Some(texture) => textures.update_texture(texture, image)?,
                None => page.texture = Some(textures.create_texture(image)?),
            }
            page.dirty = false;
        }

        Ok(())
    }

    pub(crate) fn textures(&self) -> impl Iterator<Item = TextureId> + '_ {
        self.pages.iter().filter_map(|page| page.texture)
    }

    /// Reserves a cell `width` texels wide. Rows are `ceil(line height) + 5`
    /// texels tall and images start one texel inside their cell.
    ///
    /// Not 1:1: XUI evicts cells within the frame when its one cache
    /// texture is full. Cells here stay valid for the frame's draw list, so
    /// further pages are added for the rest of the frame, up to
    /// `MAX_PAGES`; past that a glyph is not cached in that frame.
    fn allocate(&mut self, width: u32, frame: u32) -> Result<Slot, GlyphCacheError> {
        const MAX_PAGES: usize = 8;

        let page_size = self.page_size.get();
        let row_height = (libm::ceilf(self.line_height) as u32).saturating_add(5);
        if width > page_size || row_height > page_size {
            return Err(GlyphCacheError::TooLarge {
                width,
                height: row_height,
                page_size,
            });
        }

        while let Some(page) = self.pages.get_mut(self.current_page) {
            if page.cursor_x + width > page_size {
                page.cursor_x = 0;
                page.cursor_y += row_height;
            }

            if page.cursor_y + row_height <= page_size {
                let slot = Slot {
                    page: self.current_page as u16,
                    x: page.cursor_x + 1,
                    y: page.cursor_y + 1,
                };
                page.cursor_x += width;
                return Ok(slot);
            }

            self.current_page += 1;
        }

        if !self.pages.is_empty() {
            self.overflow_frame = Some(frame);
        }

        if self.pages.len() >= MAX_PAGES {
            return Err(GlyphCacheError::Full {
                pages: self.pages.len(),
            });
        }

        let mut page = Page::new(self.page_size);
        page.cursor_x = width;
        self.pages.push(page);
        self.current_page = self.pages.len() - 1;

        Ok(Slot {
            page: self.current_page as u16,
            x: 1,
            y: 1,
        })
    }
}

/// Image origin of a reserved cell.
#[derive(Clone, Copy, Debug)]
struct Slot {
    page: u16,
    x: u32,
    y: u32,
}

/// Buffers reused across glyph rasterizations.
#[derive(Debug, Default)]
pub(crate) struct Scratch {
    pub rasterizer: Rasterizer,
    pub outline: OutlineScratch,
}

/// One cache texture: coverage bytes and the row allocator.
#[derive(Debug)]
struct Page {
    texture: Option<TextureId>,
    coverage: Vec<u8>,
    cursor_x: u32,
    cursor_y: u32,
    dirty: bool,
}

impl Page {
    fn new(size: NonZeroU32) -> Self {
        let texels = size.get() as usize * size.get() as usize;

        Self {
            texture: None,
            coverage: vec![0; texels],
            cursor_x: 0,
            cursor_y: 0,
            dirty: true,
        }
    }

    fn clear(&mut self) {
        self.coverage.fill(0);
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.dirty = true;
    }

    /// Premultiplied white with alpha = coverage.
    fn image(&self, size: NonZeroU32) -> Result<Image, TextError> {
        let pixels = self
            .coverage
            .iter()
            .map(|alpha| Rgba8::new(*alpha, *alpha, *alpha, *alpha))
            .collect();

        Ok(Image::new(size, size, pixels)?)
    }
}

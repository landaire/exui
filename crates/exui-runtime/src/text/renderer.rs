use alloc::vec::Vec;

use xbox360_draw::draw::Rgba8;
use xbox360_draw::draw::TextureId;
use xbox360_draw::texture::TextureStore;

use crate::text::GlyphQuad;
use crate::text::TextExtent;
use crate::text::TextRequest;
use crate::text::TextStyle;
use crate::text::cache::CellState;
use crate::text::cache::FontInstance;
use crate::text::cache::InstanceKey;
use crate::text::cache::Scratch;
use crate::text::error::TextError;
use crate::text::layout;
use crate::text::layout::ELLIPSIS;
use crate::text::layout::LinePitch;
use crate::text::layout::PlacedLine;
use crate::text::layout::TextRect;
use crate::text::library::FontLibrary;
use crate::text::library::TypefaceEntry;

/// Lays out text into glyph quads and owns the glyph cache textures.
///
/// Per frame: any number of [`Self::layout`] and [`Self::measure`] calls,
/// then one [`Self::flush`] before the draw list is rendered.
#[derive(Debug)]
pub struct TextRenderer {
    instances: Vec<FontInstance>,
    scratch: Scratch,
    lines: Vec<PlacedLine>,

    frame: u32,
    /// Textures of dropped font instances with the frame that dropped them.
    /// The draw list of that frame may still sample them.
    retired: Vec<(u32, TextureId)>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            instances: Vec::new(),
            scratch: Scratch::default(),
            lines: Vec::new(),
            frame: 0,
            retired: Vec::new(),
        }
    }

    /// Appends the quads of `request` to `out`. With a drop shadow, each
    /// glyph's shadow quad precedes it.
    pub fn layout(
        &mut self,
        fonts: &FontLibrary,
        request: &TextRequest<'_>,
        textures: &mut dyn TextureStore,
        out: &mut Vec<GlyphQuad>,
    ) -> Result<(), TextError> {
        let (index, typeface) = self.instance(fonts, request)?;
        self.destroy_retired(textures)?;

        let frame = self.frame;
        let instance = &mut self.instances[index];

        instance.begin_use(frame);

        let text = request.text;
        let rect = TextRect::from_size(request.box_width, request.box_height);
        let pitch = LinePitch {
            height: instance.line_height_ceil(),
            adjust: request.line_spacing_adjust,
        };
        layout::place_lines(
            text,
            request.style,
            rect,
            pitch,
            request.scroll,
            &mut |code| instance.record(typeface, code).advance as i32,
            &mut self.lines,
        );

        // Not 1:1: a glyph that cannot be cached draws nothing and the rest
        // of the text still draws. The failure path of XuiFont_CacheGlyph
        // is not decoded.
        for line in &self.lines {
            for code in line_codes(text, line) {
                let _ = instance.cache(typeface, code, frame, &mut self.scratch);
            }
        }

        instance.create_textures(textures)?;

        let page_size = instance.page_size() as f32;
        let cell_height = instance.cell_height();
        let rect_height = rect.height as f32;
        for line in &self.lines {
            let top = line.y as f32 - request.scroll - instance.top_offset();
            let mut pen = line.x;
            for code in line_codes(text, line) {
                let record = instance.record(typeface, code);
                let origin = pen;
                pen += record.advance as i32;

                let CellState::Cached(cell) = record.cell else {
                    continue;
                };

                // create_textures gave every page a texture.
                let Some(texture) = instance.page_texture(cell.page) else {
                    continue;
                };

                let left = (origin + cell.left as i32) as f32;
                let quad = GlyphQuad {
                    left,
                    top,
                    right: left + cell.width as f32,
                    bottom: top + cell_height as f32,
                    uv: [
                        cell.x as f32 / page_size,
                        cell.y as f32 / page_size,
                        (cell.x as u32 + cell.width as u32) as f32 / page_size,
                        (cell.y as u32 + cell_height) as f32 / page_size,
                    ],
                    color: request.color,
                    texture,
                };

                // The shadow is the glyph moved by one cache texel, which is
                // one pixel because quads map texels 1:1.
                if let Some(shadow) = request.shadow_color {
                    push_clipped(out, offset(quad, shadow), rect_height);
                }

                push_clipped(out, quad, rect_height);
            }
        }

        Ok(())
    }

    /// `XuiMeasureText` with the request's box width as the wrap width.
    pub fn measure(
        &mut self,
        fonts: &FontLibrary,
        request: &TextRequest<'_>,
    ) -> Result<TextExtent, TextError> {
        let (index, typeface) = self.instance(fonts, request)?;
        let instance = &mut self.instances[index];

        let rect = TextRect::from_size(request.box_width, request.box_height);
        let pitch = LinePitch {
            height: instance.line_height_ceil(),
            adjust: request.line_spacing_adjust,
        };
        let extent = layout::measure(
            request.text,
            request.style,
            rect.width,
            pitch,
            &mut |code| instance.record(typeface, code).advance as i32,
        );

        Ok(TextExtent {
            width: extent.width as f32,
            height: extent.height as f32,
        })
    }

    /// `XuiFontGetLineStarts` 0x92184048 with the request's box width as the
    /// wrap width: fills `out` with the start index of every line. Returns
    /// the font line height rounded up (`XuiFontGetLineHeight` 0x92180208).
    pub fn line_starts(
        &mut self,
        fonts: &FontLibrary,
        request: &TextRequest<'_>,
        out: &mut Vec<usize>,
    ) -> Result<u32, TextError> {
        let (index, typeface) = self.instance(fonts, request)?;
        let instance = &mut self.instances[index];

        let rect = TextRect::from_size(request.box_width, request.box_height);
        layout::line_starts(
            request.text,
            request.style,
            rect.width,
            &mut |code| instance.record(typeface, code).advance as i32,
            out,
        );

        Ok(instance.line_height_ceil().max(0) as u32)
    }

    /// Uploads glyph cache pages changed since the last flush and ends the
    /// frame. Call after the frame's last `layout` and before rendering.
    pub fn flush(&mut self, textures: &mut dyn TextureStore) -> Result<(), TextError> {
        self.destroy_retired(textures)?;

        for instance in &mut self.instances {
            instance.upload(textures)?;
        }

        self.frame = self.frame.wrapping_add(1);

        Ok(())
    }

    /// Moves every glyph cache texture to `out`. The caches start empty.
    pub(crate) fn release_textures(&mut self, out: &mut Vec<TextureId>) {
        out.extend(self.retired.drain(..).map(|(_, texture)| texture));

        for instance in self.instances.drain(..) {
            out.extend(instance.textures());
        }
    }

    /// Drops every font instance (`XuiFreeResources` bit 0x4, the font
    /// cache flush 0x921824D0). Their textures are destroyed once the draw
    /// list of this frame is done.
    pub(crate) fn release_fonts(&mut self) {
        let frame = self.frame;
        for instance in self.instances.drain(..) {
            self.retired
                .extend(instance.textures().map(|texture| (frame, texture)));
        }
    }

    /// Finds or creates the font instance of a request.
    ///
    /// Not 1:1: XUI keeps a font while an element references it. Here at
    /// most `MAX_INSTANCES` stay, and creating another drops the least
    /// recently used one.
    fn instance<'f>(
        &mut self,
        fonts: &'f FontLibrary,
        request: &TextRequest<'_>,
    ) -> Result<(usize, &'f TypefaceEntry), TextError> {
        const MAX_POINT_SIZE: f32 = 150.0;
        const MAX_INSTANCES: usize = 16;

        if request.point_size.is_nan() || request.point_size < 1.0 {
            return Err(TextError::PointSize {
                point_size: request.point_size,
            });
        }

        let point_size = request.point_size.min(MAX_POINT_SIZE);
        let typeface = fonts.resolve(request.typeface)?;
        let key = InstanceKey {
            generation: typeface.generation,
            font_flags: request.style.0 & TextStyle::FONT_FLAGS,
            point_size: point_size.to_bits(),
        };
        let frame = self.frame;

        if let Some(index) = self
            .instances
            .iter()
            .position(|instance| instance.key == key)
        {
            self.instances[index].last_used = frame;
            return Ok((index, typeface));
        }

        // Instances of replaced typeface registrations are unreachable.
        let retired = &mut self.retired;
        self.instances.retain(|instance| {
            let live = fonts.by_generation(instance.key.generation).is_some();
            if !live {
                retired.extend(instance.textures().map(|texture| (frame, texture)));
            }

            live
        });

        if self.instances.len() >= MAX_INSTANCES {
            let oldest = self
                .instances
                .iter()
                .enumerate()
                .max_by_key(|(_, instance)| frame.wrapping_sub(instance.last_used))
                .map(|(index, _)| index);
            // The draw list of this frame may still sample its textures.
            if let Some(oldest) = oldest {
                let instance = self.instances.swap_remove(oldest);
                self.retired
                    .extend(instance.textures().map(|texture| (frame, texture)));
            }
        }

        let mut instance = FontInstance::new(key, typeface, point_size);
        instance.last_used = frame;
        self.instances.push(instance);

        Ok((self.instances.len() - 1, typeface))
    }

    fn destroy_retired(&mut self, textures: &mut dyn TextureStore) -> Result<(), TextError> {
        let frame = self.frame;
        let mut result = Ok(());
        self.retired.retain(|(retired_frame, texture)| {
            if *retired_frame == frame || result.is_err() {
                return true;
            }

            result = textures.destroy_texture(*texture);
            false
        });

        Ok(result?)
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Code units drawn for a placed line, including its ellipsis.
fn line_codes<'a>(text: &'a [u16], line: &PlacedLine) -> impl Iterator<Item = u16> + 'a {
    let ellipsis = if line.ellipsis { Some(ELLIPSIS) } else { None };

    text[line.start..line.end].iter().copied().chain(ellipsis)
}

fn offset(quad: GlyphQuad, color: Rgba8) -> GlyphQuad {
    GlyphQuad {
        left: quad.left + 1.0,
        top: quad.top + 1.0,
        right: quad.right + 1.0,
        bottom: quad.bottom + 1.0,
        color,
        ..quad
    }
}

/// Clips a quad to `0..rect_height` vertically, adjusting v proportionally.
fn push_clipped(out: &mut Vec<GlyphQuad>, quad: GlyphQuad, rect_height: f32) {
    if quad.bottom <= 0.0 || quad.top >= rect_height {
        return;
    }

    let height = quad.bottom - quad.top;
    let v_span = quad.uv[3] - quad.uv[1];
    let mut clipped = quad;
    if quad.top < 0.0 {
        clipped.top = 0.0;
        clipped.uv[1] = quad.uv[1] + -quad.top / height * v_span;
    }

    if quad.bottom > rect_height {
        clipped.bottom = rect_height;
        clipped.uv[3] = quad.uv[3] - (quad.bottom - rect_height) / height * v_span;
    }

    out.push(clipped);
}

//! XUI text: font files, typefaces, the glyph cache and line layout
//! (doc/xui-text.md).

mod cache;
mod error;
mod kinsoku;
mod layout;
mod library;
mod raster;
mod reader;
mod renderer;
mod sfnt;
mod synthetic;
mod xtt;

#[cfg(test)]
pub(crate) mod tests;

use xbox360_draw::draw::Rgba8;
use xbox360_draw::draw::TextureId;

pub use self::error::FontError;
pub use self::error::InflateError;
pub use self::error::TextError;
pub use self::library::FontData;
pub use self::library::FontLibrary;
pub use self::library::LoadedFont;
pub use self::library::Typeface;
pub use self::renderer::TextRenderer;
pub use self::sfnt::Tag;

/// XUI `TextStyle` bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct TextStyle(pub u32);

impl TextStyle {
    /// Draw a drop shadow in `TextRequest::shadow_color`.
    pub const DROP_SHADOW: u32 = 0x1;
    /// Bits passed to font creation; part of the font cache key.
    pub const FONT_FLAGS: u32 = 0xe;
    /// Do not wrap; clip at the rect width.
    pub const NO_WRAP: u32 = 0x10;
    /// Stop after the first line and keep trailing spaces.
    pub const SINGLE_LINE: u32 = 0x20;
    /// Left-alignment marker read only by the Korean break rule.
    pub const KOREAN_LEFT_MARKER: u32 = 0x100;
    pub const ALIGN_RIGHT: u32 = 0x200;
    pub const CENTER_HORIZONTAL: u32 = 0x400;
    pub const CENTER_VERTICAL: u32 = 0x1000;
    /// `measure` uses the multi-line measurer.
    pub const MULTI_LINE_MEASURE: u32 = 0x2000;
    /// Append U+2026 to truncated lines.
    pub const ELLIPSIS: u32 = 0x4000;
    pub const KOREAN_RULE_LEFT: u32 = 0x01_0000;
    pub const KOREAN_RULE_CENTER: u32 = 0x04_0000;
    pub const KOREAN_RULE_RIGHT: u32 = 0x10_0000;

    pub const fn contains(self, bits: u32) -> bool {
        self.0 & bits == bits
    }
}

/// One text draw or measurement of an element.
#[derive(Clone, Copy, Debug)]
pub struct TextRequest<'a> {
    /// UTF-16 code units; a NUL code unit ends the text early.
    pub text: &'a [u16],
    /// Registered typeface name; `None` or empty selects the default typeface.
    pub typeface: Option<&'a [u16]>,
    pub point_size: f32,
    pub style: TextStyle,

    /// Premultiplied glyph color, already multiplied by the element color factor.
    pub color: Rgba8,
    /// Premultiplied drop shadow color; `Some` when `TextStyle::DROP_SHADOW` is set.
    pub shadow_color: Option<Rgba8>,

    /// Pixels added to the line pitch.
    pub line_spacing_adjust: i32,
    /// Element size in pixels; truncated to integers before layout.
    pub box_width: f32,
    pub box_height: f32,
    /// Pixels the lines move up inside the box (the XuiEdit scroll offset).
    pub scroll: f32,
}

/// Textured quad in element-local pixels, sampling `texture` (premultiplied
/// white with alpha = coverage) modulated by `color`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    /// Normalized `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
    pub color: Rgba8,
    pub texture: TextureId,
}

/// Measured size of a text in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextExtent {
    /// Widest line.
    pub width: f32,
    /// Line count times the line pitch.
    pub height: f32,
}

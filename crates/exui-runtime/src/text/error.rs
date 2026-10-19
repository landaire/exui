use alloc::string::String;
use core::fmt;

use xbox360_draw::error::ImageLayoutError;
use xbox360_draw::error::TextureError;
use zune_inflate::errors::DecodeErrorStatus;

use crate::text::sfnt::Tag;

/// Failure to load a font file or register a typeface.
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("xtt file of {len} bytes is shorter than its 0x118-byte header")]
    XttTruncated { len: usize },

    #[error("xtt version {version:#010X} is not 0x00010000")]
    XttVersion { version: u32 },

    #[error("xtt header records file size {recorded} but the file has {actual} bytes")]
    XttFileSize { recorded: u32, actual: usize },

    #[error("xtt signed length {signed_len:#X} exceeds file size {file_len:#X} - 0x100")]
    XttSignedLength { signed_len: u32, file_len: usize },

    #[error(
        "xtt compressed data of {compressed_len:#X} bytes at 0x118 exceeds file size {file_len:#X}"
    )]
    XttCompressedLength {
        compressed_len: u32,
        file_len: usize,
    },

    #[error("xtt payload inflation failed")]
    XttInflate(#[source] InflateError),

    #[error("font data of {len} bytes ends inside the {what}")]
    Truncated { what: &'static str, len: usize },

    #[error("sfnt table count {count} is outside 3..=32")]
    TableCount { count: u16 },

    #[error("required table {tag} is missing")]
    MissingTable { tag: Tag },

    #[error("table {tag} at {offset:#X} with {len:#X} bytes exceeds the font data")]
    TableBounds { tag: Tag, offset: u32, len: u32 },

    #[error("table {tag} is too short for its fields")]
    TableTooShort { tag: Tag },

    #[error("head magic {magic:#010X} is not 0x5F0F3CF5")]
    HeadMagic { magic: u32 },

    #[error("head unitsPerEm is zero")]
    ZeroUnitsPerEm,

    #[error("head indexToLocFormat {format} is neither 0 nor 1")]
    LocaFormat { format: i16 },

    #[error("paged xglf glyph data needs long xloc entries")]
    PagedGlyphsShortLoca,

    #[error("hhea numberOfHMetrics is zero")]
    ZeroHMetrics,

    #[error("cmap has no format 0 or format 4 subtable")]
    NoCmapSubtable,

    #[error("typeface name contains ';'")]
    TypefaceName,
}

/// Failure to lay out or measure text.
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error("typeface {name:?} is not registered")]
    UnknownTypeface { name: String },

    #[error("no default typeface is registered")]
    NoDefaultTypeface,

    #[error("point size {point_size} is below 1")]
    PointSize { point_size: f32 },

    #[error("glyph cache page has an invalid layout")]
    PageLayout(#[from] ImageLayoutError),

    #[error("glyph cache texture operation failed")]
    Texture(#[from] TextureError),
}

/// Failure to cache one glyph. The glyph draws nothing.
#[derive(Debug, thiserror::Error)]
pub enum GlyphCacheError {
    #[error("glyph {glyph} has a malformed outline")]
    Outline {
        glyph: u16,
        #[source]
        source: GlyphError,
    },

    #[error("glyph {glyph} outline has more than {limit} edges")]
    TooManyEdges { glyph: u16, limit: usize },

    #[error("glyph cell of {width}x{height} does not fit a {page_size}x{page_size} cache page")]
    TooLarge {
        width: u32,
        height: u32,
        page_size: u32,
    },

    #[error("glyph cache of a font instance is full at {pages} pages")]
    Full { pages: usize },
}

/// Malformed glyph data.
#[derive(Debug, thiserror::Error)]
pub enum GlyphError {
    #[error("glyph data is truncated")]
    Truncated(#[from] ShortRead),

    #[error("glyph record {start:#X}..{end:#X} lies outside {len:#X} bytes of glyph data")]
    RecordBounds {
        start: usize,
        end: usize,
        len: usize,
    },

    #[error("xglf page {page} lies outside the glyph pages")]
    PageBounds { page: usize },

    #[error("xglf page {page} does not inflate")]
    PageInflate {
        page: usize,
        #[source]
        source: InflateError,
    },

    #[error("contour end point {end} does not follow end point {previous}")]
    ContourOrder { previous: usize, end: usize },

    #[error("outline has more than {limit} points")]
    TooManyPoints { limit: usize },

    #[error("composite glyph has more than {limit} components")]
    TooManyComponents { limit: u32 },

    #[error("composite glyph nests deeper than {limit} levels")]
    TooDeep { limit: u8 },

    #[error("component glyph {glyph} uses point-matched placement")]
    PointMatched { glyph: u16 },
}

/// A read ran past the end of the data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("read of {needed} bytes at offset {offset:#X} runs past the end of {len:#X} bytes")]
pub struct ShortRead {
    pub offset: usize,
    pub needed: usize,
    pub len: usize,
}

/// Error of the zlib decoder.
pub struct InflateError(pub DecodeErrorStatus);

impl fmt::Debug for InflateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.0)
    }
}

impl fmt::Display for InflateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.0)
    }
}

impl core::error::Error for InflateError {}

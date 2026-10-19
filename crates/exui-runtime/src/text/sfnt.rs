//! TrueType tables used by XUI: head, hhea, hmtx, cmap, loca/xloc and
//! glyf/xglf, including the paged compressed `xglf` of `xttf` files.

use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;
use core::ops::Range;

use zune_inflate::DeflateDecoder;
use zune_inflate::DeflateOptions;

use crate::text::error::FontError;
use crate::text::error::GlyphError;
use crate::text::error::InflateError;
use crate::text::error::ShortRead;
use crate::text::library::FontData;
use crate::text::reader::Reader;
use crate::text::reader::u16_at;

/// Most outline points of one glyph over all its components. Not 1:1: it
/// bounds the work hostile font data can cause; the original's limit is not
/// known.
const MAX_OUTLINE_POINTS: usize = 0x4000;

/// Four-byte sfnt table tag.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tag(pub [u8; 4]);

impl fmt::Display for Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            let shown = if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '?'
            };
            write!(formatter, "{shown}")?;
        }

        Ok(())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Tag({self})")
    }
}

/// Glyph index in a font.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GlyphId(pub u16);

impl GlyphId {
    /// The glyph drawn for characters missing from every font.
    pub(crate) const DEFAULT: Self = Self(0);
}

/// Glyph bounding box from the glyf header, in font units, y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphBounds {
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
}

/// Receives a glyph outline in font units, y up. Contours are closed.
pub(crate) trait OutlineSink {
    fn move_to(&mut self, x: f32, y: f32);

    fn line_to(&mut self, x: f32, y: f32);

    fn quad_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32);

    fn close(&mut self);
}

/// Parsed TrueType font. Tables are byte ranges of `tables`.
#[derive(Debug)]
pub(crate) struct Font {
    tables: FontData,

    units_per_em: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,

    cmap: Cmap,
    hmtx: Range<usize>,
    h_metric_count: u16,
    loca: Range<usize>,
    loca_long: bool,
    glyphs: GlyphTable,
}

impl Font {
    /// Parses the sfnt `tables`, taken from `container` when it is an
    /// `xttf` file and the file itself otherwise. Only the `xglf` bytes of
    /// the container are kept.
    pub(crate) fn parse(tables: FontData, container: Option<FontData>) -> Result<Self, FontError> {
        let bytes = tables.bytes();
        let directory = Directory::parse(bytes)?;

        let head_tag = Tag(*b"head");
        let head = Head::parse(&bytes[directory.require(head_tag, bytes.len())?])
            .map_err(|_| FontError::TableTooShort { tag: head_tag })?;
        if head.magic != 0x5F0F_3CF5 {
            return Err(FontError::HeadMagic { magic: head.magic });
        }

        if head.units_per_em == 0 {
            return Err(FontError::ZeroUnitsPerEm);
        }

        let loca_long = match head.index_to_loc_format {
            0 => false,
            1 => true,
            format => return Err(FontError::LocaFormat { format }),
        };

        let hhea_tag = Tag(*b"hhea");
        let hhea = Hhea::parse(&bytes[directory.require(hhea_tag, bytes.len())?])
            .map_err(|_| FontError::TableTooShort { tag: hhea_tag })?;
        if hhea.h_metric_count == 0 {
            return Err(FontError::ZeroHMetrics);
        }

        let hmtx_tag = Tag(*b"hmtx");
        let hmtx = directory.require(hmtx_tag, bytes.len())?;
        if hmtx.len() < hhea.h_metric_count as usize * 4 {
            return Err(FontError::TableTooShort { tag: hmtx_tag });
        }

        let cmap = Cmap::parse(bytes, directory.require(Tag(*b"cmap"), bytes.len())?)?;

        let loca_tag = if directory.find(Tag(*b"loca")).is_some() {
            Tag(*b"loca")
        } else {
            Tag(*b"xloc")
        };
        let loca = directory.require(loca_tag, bytes.len())?;

        // xglf offsets are relative to the xttf file, not to the inflated
        // tables.
        let glyphs = match (directory.find(Tag(*b"glyf")), directory.find(Tag(*b"xglf"))) {
            (Some(glyf), _) => GlyphTable::Plain(glyf.range(bytes.len())?),
            (None, Some(xglf)) => {
                if !loca_long {
                    return Err(FontError::PagedGlyphsShortLoca);
                }

                let (container, pages) = match container {
                    Some(file) => {
                        let range = xglf.range(file.bytes().len())?;
                        let len = range.len();
                        (Some(file.into_range(range)), 0..len)
                    }
                    None => (None, xglf.range(bytes.len())?),
                };

                GlyphTable::Paged {
                    container,
                    pages,
                    decoded: RefCell::new(None),
                }
            }
            (None, None) => return Err(FontError::MissingTable { tag: Tag(*b"glyf") }),
        };

        Ok(Self {
            tables,
            units_per_em: head.units_per_em,
            ascender: hhea.ascender,
            descender: hhea.descender,
            line_gap: hhea.line_gap,
            cmap,
            hmtx,
            h_metric_count: hhea.h_metric_count,
            loca,
            loca_long,
            glyphs,
        })
    }

    pub(crate) fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub(crate) fn ascender(&self) -> i16 {
        self.ascender
    }

    /// `ascender - descender + lineGap` in font units.
    pub(crate) fn line_height_units(&self) -> i32 {
        self.ascender as i32 - self.descender as i32 + self.line_gap as i32
    }

    /// Glyph of a UTF-16 code unit; `None` when the cmap maps it to glyph 0.
    pub(crate) fn glyph(&self, code: u16) -> Option<GlyphId> {
        let bytes = self.tables.bytes();
        let glyph = match &self.cmap {
            Cmap::Format0 { glyphs } => bytes[glyphs.clone()]
                .get(code as usize)
                .map_or(0, |glyph| *glyph as u16),
            Cmap::Format4 {
                subtable,
                segment_count,
            } => format4_lookup(&bytes[subtable.clone()], *segment_count, code).unwrap_or(0),
        };

        if glyph == 0 {
            None
        } else {
            Some(GlyphId(glyph))
        }
    }

    /// Advance width in font units. Glyphs past the long metrics repeat the
    /// last advance.
    pub(crate) fn advance_units(&self, glyph: GlyphId) -> u16 {
        let index = glyph.0.min(self.h_metric_count - 1) as usize;

        // Parse checked that every long metric lies inside hmtx.
        u16_at(self.tables.bytes(), self.hmtx.start + index * 4).unwrap_or(0)
    }

    /// Emits the outline of `glyph` and returns its glyf bounds, or `None`
    /// for a glyph without contours.
    ///
    /// Not 1:1: the point and component limits bound the work hostile font
    /// data can cause; the original's limits are not known.
    pub(crate) fn outline(
        &self,
        glyph: GlyphId,
        sink: &mut dyn OutlineSink,
        scratch: &mut OutlineScratch,
    ) -> Result<Option<GlyphBounds>, GlyphError> {
        const MAX_DEPTH: u8 = 8;
        // Bounds the work of composites that reference glyphs repeatedly.
        const MAX_COMPONENTS: u32 = 1024;

        if !self.glyph_record(glyph, &mut scratch.record)? {
            return Ok(None);
        }

        let bounds = GlyphHeader::parse(&mut Reader::new(&scratch.record))?.bounds;

        let mut pending = Vec::new();
        pending.push(Component {
            glyph,
            transform: Transform::IDENTITY,
            depth: 0,
        });
        let mut visited = 0u32;
        let mut points = 0usize;
        while let Some(component) = pending.pop() {
            visited += 1;
            if visited > MAX_COMPONENTS {
                return Err(GlyphError::TooManyComponents {
                    limit: MAX_COMPONENTS,
                });
            }

            if !self.glyph_record(component.glyph, &mut scratch.record)? {
                continue;
            }

            let mut reader = Reader::new(&scratch.record);
            let header = GlyphHeader::parse(&mut reader)?;
            if header.contour_count >= 0 {
                decode_simple(
                    &mut reader,
                    header.contour_count as u16,
                    points,
                    &mut scratch.contours,
                )?;
                points += scratch.contours.points.len();
                emit_contours(&scratch.contours, &component.transform, sink);
                continue;
            }

            if component.depth >= MAX_DEPTH {
                return Err(GlyphError::TooDeep { limit: MAX_DEPTH });
            }

            parse_composite(&mut reader, &component, &mut pending)?;
        }

        Ok(Some(bounds))
    }

    /// Copies one glyph's glyf record into `record`; `false` when it is empty.
    fn glyph_record(&self, glyph: GlyphId, record: &mut Vec<u8>) -> Result<bool, GlyphError> {
        record.clear();

        let loca = &self.tables.bytes()[self.loca.clone()];
        let index = glyph.0 as usize;
        let (start, end) = if self.loca_long {
            let mut entries = Reader::at(loca, index * 4)?;
            (entries.u32()? as usize, entries.u32()? as usize)
        } else {
            let mut entries = Reader::at(loca, index * 2)?;
            (entries.u16()? as usize * 2, entries.u16()? as usize * 2)
        };

        let (container, pages, decoded) = match &self.glyphs {
            GlyphTable::Plain(glyf) => {
                if start == end {
                    return Ok(false);
                }

                let glyf = &self.tables.bytes()[glyf.clone()];
                let bytes = glyf.get(start..end).ok_or(GlyphError::RecordBounds {
                    start,
                    end,
                    len: glyf.len(),
                })?;
                record.extend_from_slice(bytes);
                return Ok(true);
            }
            GlyphTable::Paged {
                container,
                pages,
                decoded,
            } => (container, pages, decoded),
        };

        let page = start >> 16;
        let mut decoded = decoded.borrow_mut();
        let bytes = match &mut *decoded {
            Some(cached) if cached.index == page => &cached.bytes,
            slot => {
                let source = container.as_ref().unwrap_or(&self.tables).bytes();
                &slot
                    .insert(DecodedPage {
                        index: page,
                        bytes: inflate_page(&source[pages.clone()], page)?,
                    })
                    .bytes
            }
        };

        // Records do not span pages: the last record of a page ends at the
        // end of its inflated data.
        let offset = start & 0xFFFF;
        let end = if end >> 16 == page {
            end & 0xFFFF
        } else {
            bytes.len()
        };
        if offset == end {
            return Ok(false);
        }

        let bytes = bytes.get(offset..end).ok_or(GlyphError::RecordBounds {
            start: offset,
            end,
            len: bytes.len(),
        })?;
        record.extend_from_slice(bytes);

        Ok(true)
    }
}

/// Buffers reused across glyph outline decodes.
#[derive(Debug, Default)]
pub(crate) struct OutlineScratch {
    /// glyf record of the component being decoded.
    record: Vec<u8>,
    contours: Contours,
}

/// Decoded points of one simple glyph.
#[derive(Debug, Default)]
struct Contours {
    points: Vec<OutlinePoint>,
    flags: Vec<u8>,
    /// Exclusive end point index of each contour.
    ends: Vec<usize>,
}

/// Where glyph records are stored.
#[derive(Debug)]
enum GlyphTable {
    /// `glyf`: records at the loca offsets of this range of the tables.
    Plain(Range<usize>),
    /// `xglf`: fixed-size pages, each an independent zlib stream, in
    /// `pages` of the `xttf` file's `xglf` bytes or, without a container,
    /// of the tables.
    /// Long xloc entries are `page << 16 | offset` into the inflated page.
    Paged {
        container: Option<FontData>,
        pages: Range<usize>,
        decoded: RefCell<Option<DecodedPage>>,
    },
}

/// Most recently inflated `xglf` page.
#[derive(Debug)]
struct DecodedPage {
    index: usize,
    bytes: Vec<u8>,
}

/// Decoded glyf point in font units.
#[derive(Clone, Copy, Debug, Default)]
struct OutlinePoint {
    x: i32,
    y: i32,
    on_curve: bool,
}

#[derive(Debug)]
enum Cmap {
    Format0 {
        glyphs: Range<usize>,
    },
    Format4 {
        subtable: Range<usize>,
        segment_count: u16,
    },
}

impl Cmap {
    /// Selects the last encoding subtable of format 0 or 4.
    fn parse(bytes: &[u8], table: Range<usize>) -> Result<Self, FontError> {
        let tag = Tag(*b"cmap");
        let too_short = |_: ShortRead| FontError::TableTooShort { tag };
        let cmap = &bytes[table.clone()];

        let mut header = Reader::new(cmap);
        header.skip(2).map_err(too_short)?;
        let count = header.u16().map_err(too_short)?;

        let mut selected = None;
        for _ in 0..count {
            header.skip(4).map_err(too_short)?;
            let offset = header.u32().map_err(too_short)? as usize;
            let format = u16_at(cmap, offset).map_err(too_short)?;
            if format == 0 || format == 4 {
                selected = Some(Subtable { offset, format });
            }
        }

        let Some(subtable) = selected else {
            return Err(FontError::NoCmapSubtable);
        };

        let mut reader = Reader::at(cmap, subtable.offset).map_err(too_short)?;
        let start = table.start + subtable.offset;
        if subtable.format == 0 {
            reader.skip(6).map_err(too_short)?;
            reader.skip(256).map_err(too_short)?;

            return Ok(Self::Format0 {
                glyphs: start + 6..start + 6 + 256,
            });
        }

        reader.skip(2).map_err(too_short)?;
        let len = reader.u16().map_err(too_short)? as usize;
        reader.skip(2).map_err(too_short)?;
        let segment_count = reader.u16().map_err(too_short)? / 2;
        if len < 16 + segment_count as usize * 8 || cmap.len() < subtable.offset + len {
            return Err(FontError::TableTooShort { tag });
        }

        Ok(Self::Format4 {
            subtable: start..start + len,
            segment_count,
        })
    }
}

/// Encoding subtable chosen from the cmap records.
#[derive(Clone, Copy, Debug)]
struct Subtable {
    offset: usize,
    format: u16,
}

/// Directory entry. The range is checked against the bytes it refers to
/// when the table is used, since `xglf` refers to the container.
#[derive(Debug)]
struct TableRecord {
    tag: Tag,
    offset: u32,
    len: u32,
}

impl TableRecord {
    fn range(&self, bytes_len: usize) -> Result<Range<usize>, FontError> {
        let start = self.offset as usize;
        let end = start
            .checked_add(self.len as usize)
            .filter(|end| *end <= bytes_len);

        end.map(|end| start..end).ok_or(FontError::TableBounds {
            tag: self.tag,
            offset: self.offset,
            len: self.len,
        })
    }
}

#[derive(Debug)]
struct Directory {
    records: Vec<TableRecord>,
}

impl Directory {
    fn parse(bytes: &[u8]) -> Result<Self, FontError> {
        let truncated = |what: &'static str| {
            move |_: ShortRead| FontError::Truncated {
                what,
                len: bytes.len(),
            }
        };

        let mut reader = Reader::new(bytes);
        reader.skip(4).map_err(truncated("offset table"))?;
        let table_count = reader.u16().map_err(truncated("offset table"))?;
        if !(3..=32).contains(&table_count) {
            return Err(FontError::TableCount { count: table_count });
        }
        reader.skip(6).map_err(truncated("offset table"))?;

        let mut records = Vec::with_capacity(table_count as usize);
        for _ in 0..table_count {
            let tag = reader.bytes(4).map_err(truncated("table directory"))?;
            let tag = Tag([tag[0], tag[1], tag[2], tag[3]]);
            reader.skip(4).map_err(truncated("table directory"))?;
            let offset = reader.u32().map_err(truncated("table directory"))?;
            let len = reader.u32().map_err(truncated("table directory"))?;

            records.push(TableRecord { tag, offset, len });
        }

        Ok(Self { records })
    }

    fn find(&self, tag: Tag) -> Option<&TableRecord> {
        self.records.iter().find(|record| record.tag == tag)
    }

    /// Range of a required table inside `bytes_len` bytes.
    fn require(&self, tag: Tag, bytes_len: usize) -> Result<Range<usize>, FontError> {
        self.find(tag)
            .ok_or(FontError::MissingTable { tag })?
            .range(bytes_len)
    }
}

/// Fields of `head` that XUI uses.
#[derive(Clone, Copy, Debug)]
struct Head {
    magic: u32,
    units_per_em: u16,
    index_to_loc_format: i16,
}

impl Head {
    fn parse(table: &[u8]) -> Result<Self, ShortRead> {
        let mut reader = Reader::at(table, 12)?;
        let magic = reader.u32()?;
        reader.skip(2)?;
        let units_per_em = reader.u16()?;
        let index_to_loc_format = Reader::at(table, 50)?.i16()?;

        Ok(Self {
            magic,
            units_per_em,
            index_to_loc_format,
        })
    }
}

/// Fields of `hhea` that XUI uses.
#[derive(Clone, Copy, Debug)]
struct Hhea {
    ascender: i16,
    descender: i16,
    line_gap: i16,
    h_metric_count: u16,
}

impl Hhea {
    fn parse(table: &[u8]) -> Result<Self, ShortRead> {
        let mut reader = Reader::at(table, 4)?;
        let ascender = reader.i16()?;
        let descender = reader.i16()?;
        let line_gap = reader.i16()?;
        let h_metric_count = u16_at(table, 34)?;

        Ok(Self {
            ascender,
            descender,
            line_gap,
            h_metric_count,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct GlyphHeader {
    /// Negative for composite glyphs.
    contour_count: i16,
    bounds: GlyphBounds,
}

impl GlyphHeader {
    fn parse(reader: &mut Reader<'_>) -> Result<Self, ShortRead> {
        let contour_count = reader.i16()?;
        let bounds = GlyphBounds {
            x_min: reader.i16()?,
            y_min: reader.i16()?,
            x_max: reader.i16()?,
            y_max: reader.i16()?,
        };

        Ok(Self {
            contour_count,
            bounds,
        })
    }
}

/// 2x2 matrix and offset applied to component points: `x' = xx*x + yx*y + dx`.
#[derive(Clone, Copy, Debug)]
struct Transform {
    xx: f32,
    xy: f32,
    yx: f32,
    yy: f32,
    dx: f32,
    dy: f32,
}

impl Transform {
    const IDENTITY: Self = Self {
        xx: 1.0,
        xy: 0.0,
        yx: 0.0,
        yy: 1.0,
        dx: 0.0,
        dy: 0.0,
    };

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.xx * x + self.yx * y + self.dx,
            self.xy * x + self.yy * y + self.dy,
        )
    }

    /// `self` applied after `inner`.
    fn compose(&self, inner: &Self) -> Self {
        let (dx, dy) = self.apply(inner.dx, inner.dy);

        Self {
            xx: self.xx * inner.xx + self.yx * inner.xy,
            xy: self.xy * inner.xx + self.yy * inner.xy,
            yx: self.xx * inner.yx + self.yx * inner.yy,
            yy: self.xy * inner.yx + self.yy * inner.yy,
            dx,
            dy,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Component {
    glyph: GlyphId,
    transform: Transform,
    depth: u8,
}

/// Inflates page `index` of an `xglf` table.
fn inflate_page(pages: &[u8], index: usize) -> Result<Vec<u8>, GlyphError> {
    // [INFERENCE] from the file layout of the known xttf fonts; not yet
    // confirmed in XuiFont_ParseTables.
    const PAGE_LEN: usize = 0x1000;
    // xloc offsets are 16 bits, so no record lies past this.
    const MAX_INFLATED: usize = 0x1_0000;

    let out_of_range = || GlyphError::PageBounds { page: index };
    let start = index.checked_mul(PAGE_LEN).ok_or_else(out_of_range)?;
    let page = pages.get(start..).ok_or_else(out_of_range)?;
    let page = &page[..page.len().min(PAGE_LEN)];

    // Each stream is followed by zero padding to the page end.
    let options = DeflateOptions::default().set_limit(MAX_INFLATED);
    DeflateDecoder::new_with_options(page, options)
        .decode_zlib()
        .map_err(|error| GlyphError::PageInflate {
            page: index,
            source: InflateError(error.error),
        })
}

/// Format 4 lookup; `None` for malformed segment arrays.
fn format4_lookup(subtable: &[u8], segment_count: u16, code: u16) -> Option<u16> {
    let segments = segment_count as usize;
    let end_codes = 14;
    let start_codes = end_codes + segments * 2 + 2;
    let deltas = start_codes + segments * 2;
    let range_offsets = deltas + segments * 2;

    for segment in 0..segments {
        if u16_at(subtable, end_codes + segment * 2).ok()? < code {
            continue;
        }

        let start = u16_at(subtable, start_codes + segment * 2).ok()?;
        let delta = u16_at(subtable, deltas + segment * 2).ok()?;
        let range_offset = u16_at(subtable, range_offsets + segment * 2).ok()?;
        if start > code {
            return Some(0);
        }

        if range_offset == 0 {
            return Some(code.wrapping_add(delta));
        }

        let address =
            range_offsets + segment * 2 + range_offset as usize + (code - start) as usize * 2;
        return match u16_at(subtable, address).ok()? {
            0 => Some(0),
            glyph => Some(glyph.wrapping_add(delta)),
        };
    }

    Some(0)
}

/// Decodes the contours of a simple glyph; `reader` is past the header.
/// `used` points of the glyph's other components count toward the limit.
fn decode_simple(
    reader: &mut Reader<'_>,
    contour_count: u16,
    used: usize,
    contours: &mut Contours,
) -> Result<(), GlyphError> {
    const ON_CURVE: u8 = 0x01;
    const X_SHORT: u8 = 0x02;
    const Y_SHORT: u8 = 0x04;
    const REPEAT: u8 = 0x08;
    const X_SAME_OR_POSITIVE: u8 = 0x10;
    const Y_SAME_OR_POSITIVE: u8 = 0x20;

    contours.points.clear();
    contours.flags.clear();
    contours.ends.clear();

    let mut previous_end = 0usize;
    for _ in 0..contour_count {
        let end = reader.u16()? as usize + 1;
        if end <= previous_end {
            return Err(GlyphError::ContourOrder {
                previous: previous_end,
                end,
            });
        }

        contours.ends.push(end);
        previous_end = end;
    }

    let point_count = previous_end;
    if used.saturating_add(point_count) > MAX_OUTLINE_POINTS {
        return Err(GlyphError::TooManyPoints {
            limit: MAX_OUTLINE_POINTS,
        });
    }

    let instructions_len = reader.u16()?;
    reader.skip(instructions_len as usize)?;

    while contours.flags.len() < point_count {
        let flag = reader.u8()?;
        contours.flags.push(flag);

        if flag & REPEAT != 0 {
            let repeat = reader.u8()?;
            for _ in 0..repeat {
                contours.flags.push(flag);
            }
        }
    }
    contours.flags.truncate(point_count);

    let mut x = 0i32;
    for &flag in &contours.flags {
        if flag & X_SHORT != 0 {
            let delta = reader.u8()? as i32;
            x += if flag & X_SAME_OR_POSITIVE != 0 {
                delta
            } else {
                -delta
            };
        } else if flag & X_SAME_OR_POSITIVE == 0 {
            x += reader.i16()? as i32;
        }

        contours.points.push(OutlinePoint {
            x,
            y: 0,
            on_curve: flag & ON_CURVE != 0,
        });
    }

    let mut y = 0i32;
    for (point, &flag) in contours.points.iter_mut().zip(&contours.flags) {
        if flag & Y_SHORT != 0 {
            let delta = reader.u8()? as i32;
            y += if flag & Y_SAME_OR_POSITIVE != 0 {
                delta
            } else {
                -delta
            };
        } else if flag & Y_SAME_OR_POSITIVE == 0 {
            y += reader.i16()? as i32;
        }

        point.y = y;
    }

    Ok(())
}

/// Emits each decoded contour, inserting the implied on-curve midpoint
/// between consecutive off-curve points.
fn emit_contours(contours: &Contours, transform: &Transform, sink: &mut dyn OutlineSink) {
    let position = |point: &OutlinePoint| transform.apply(point.x as f32, point.y as f32);
    let midpoint = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);

    let mut start = 0usize;
    for &end in &contours.ends {
        let contour = &contours.points[start..end];
        start = end;

        let first = &contour[0];
        let last = &contour[contour.len() - 1];
        let (origin, skip_first) = if first.on_curve {
            (position(first), true)
        } else if last.on_curve {
            (position(last), false)
        } else {
            (midpoint(position(last), position(first)), false)
        };

        sink.move_to(origin.0, origin.1);

        let mut control: Option<(f32, f32)> = None;
        let iterate = contour
            .iter()
            .skip(usize::from(skip_first))
            .chain(core::iter::once(first).filter(|_| skip_first));
        for point in iterate {
            let current = position(point);
            match (point.on_curve, control) {
                (true, None) => sink.line_to(current.0, current.1),
                (true, Some(pending)) => {
                    sink.quad_to(pending.0, pending.1, current.0, current.1);
                    control = None;
                }
                (false, None) => control = Some(current),
                (false, Some(pending)) => {
                    let implied = midpoint(pending, current);
                    sink.quad_to(pending.0, pending.1, implied.0, implied.1);
                    control = Some(current);
                }
            }
        }

        if let Some(pending) = control {
            sink.quad_to(pending.0, pending.1, origin.0, origin.1);
        }

        sink.close();
    }
}

/// Queues the components of a composite glyph; `reader` is past the header.
fn parse_composite(
    reader: &mut Reader<'_>,
    parent: &Component,
    pending: &mut Vec<Component>,
) -> Result<(), GlyphError> {
    const ARGS_ARE_WORDS: u16 = 0x0001;
    const ARGS_ARE_XY_VALUES: u16 = 0x0002;
    const HAVE_SCALE: u16 = 0x0008;
    const MORE_COMPONENTS: u16 = 0x0020;
    const HAVE_XY_SCALE: u16 = 0x0040;
    const HAVE_TWO_BY_TWO: u16 = 0x0080;

    let f2dot14 = |reader: &mut Reader<'_>| reader.i16().map(|value| value as f32 / 16384.0);

    loop {
        let flags = reader.u16()?;
        let glyph = GlyphId(reader.u16()?);

        // Point-matched placement is not used by the fonts that were examined.
        if flags & ARGS_ARE_XY_VALUES == 0 {
            return Err(GlyphError::PointMatched { glyph: glyph.0 });
        }

        let (dx, dy) = if flags & ARGS_ARE_WORDS != 0 {
            (reader.i16()? as f32, reader.i16()? as f32)
        } else {
            (reader.i8()? as f32, reader.i8()? as f32)
        };

        let mut transform = Transform {
            dx,
            dy,
            ..Transform::IDENTITY
        };
        if flags & HAVE_SCALE != 0 {
            let scale = f2dot14(reader)?;
            transform.xx = scale;
            transform.yy = scale;
        } else if flags & HAVE_XY_SCALE != 0 {
            transform.xx = f2dot14(reader)?;
            transform.yy = f2dot14(reader)?;
        } else if flags & HAVE_TWO_BY_TWO != 0 {
            transform.xx = f2dot14(reader)?;
            transform.xy = f2dot14(reader)?;
            transform.yx = f2dot14(reader)?;
            transform.yy = f2dot14(reader)?;
        }

        pending.push(Component {
            glyph,
            transform: parent.transform.compose(&transform),
            depth: parent.depth + 1,
        });

        if flags & MORE_COMPONENTS == 0 {
            return Ok(());
        }
    }
}

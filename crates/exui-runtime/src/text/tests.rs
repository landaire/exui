use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use xbox360_draw::draw::Rgba8;
use xbox360_draw::draw::TextureId;
use xbox360_draw::error::TextureError;
use xbox360_draw::image::Image;
use xbox360_draw::texture::TextureSlots;
use xbox360_draw::texture::TextureStore;

use super::*;
use crate::text::sfnt::Font;

const UNITS_PER_EM: u16 = 1000;

/// Texture store that keeps uploaded images for inspection.
#[derive(Default)]
struct Store {
    slots: TextureSlots<Image>,
    live: usize,
    uploads: usize,
}

impl TextureStore for Store {
    fn create_texture(&mut self, image: Image) -> Result<TextureId, TextureError> {
        self.live += 1;
        self.uploads += 1;
        self.slots.insert(image)
    }

    fn update_texture(&mut self, id: TextureId, image: Image) -> Result<(), TextureError> {
        self.uploads += 1;
        *self
            .slots
            .get_mut(id)
            .ok_or(TextureError::UnknownTexture { id })? = image;

        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) -> Result<(), TextureError> {
        self.live -= 1;
        self.slots
            .remove(id)
            .ok_or(TextureError::UnknownTexture { id })?;

        Ok(())
    }
}

/// Outline of a test glyph.
enum Outline {
    Empty,
    /// Contours of (x, y, on curve) points.
    Simple(Vec<Vec<(i16, i16, bool)>>),
    /// Components of (glyph, dx, dy).
    Composite(Vec<(u16, i16, i16)>),
}

struct TestGlyph {
    advance: u16,
    outline: Outline,
}

fn rectangle(x0: i16, y0: i16, x1: i16, y1: i16) -> Vec<(i16, i16, bool)> {
    vec![
        (x0, y0, true),
        (x0, y1, true),
        (x1, y1, true),
        (x1, y0, true),
    ]
}

/// Glyphs: 0 notdef box, 1 space, 2 'I' bar, 3 'O' ring of off-curve
/// points, 4 'H' composite of two bars, 5 U+2026 three dots.
fn latin_glyphs() -> Vec<TestGlyph> {
    let ring = |radius: i16, center: (i16, i16), clockwise: bool| {
        let (cx, cy) = center;
        let mut points = vec![
            (cx - radius, cy, false),
            (cx, cy + radius, false),
            (cx + radius, cy, false),
            (cx, cy - radius, false),
        ];
        if !clockwise {
            points.reverse();
        }
        points
    };

    vec![
        TestGlyph {
            advance: 600,
            outline: Outline::Simple(vec![
                rectangle(50, 0, 550, 700),
                rectangle(150, 600, 450, 100),
            ]),
        },
        TestGlyph {
            advance: 300,
            outline: Outline::Empty,
        },
        TestGlyph {
            advance: 400,
            outline: Outline::Simple(vec![rectangle(100, 0, 300, 700)]),
        },
        TestGlyph {
            advance: 800,
            outline: Outline::Simple(vec![
                ring(380, (400, 350), true),
                ring(200, (400, 350), false),
            ]),
        },
        TestGlyph {
            advance: 700,
            outline: Outline::Composite(vec![(2, 0, 0), (2, 300, 0)]),
        },
        TestGlyph {
            advance: 900,
            outline: Outline::Simple(vec![
                rectangle(100, 0, 200, 100),
                rectangle(400, 0, 500, 100),
                rectangle(700, 0, 800, 100),
            ]),
        },
    ]
}

fn latin_cmap() -> Vec<(u16, u16)> {
    vec![(0x20, 1), (0x48, 4), (0x49, 2), (0x4F, 3), (0x2026, 5)]
}

/// cmap (format 4), head, hhea and hmtx tables in tag order.
fn metric_tables(glyphs: &[TestGlyph], cmap: &[(u16, u16)]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head[18..20].copy_from_slice(&UNITS_PER_EM.to_be_bytes());
    head[50..52].copy_from_slice(&1u16.to_be_bytes());

    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
    hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    hhea[8..10].copy_from_slice(&100i16.to_be_bytes());
    hhea[34..36].copy_from_slice(&(glyphs.len() as u16).to_be_bytes());

    let mut hmtx = Vec::new();
    for glyph in glyphs {
        hmtx.extend_from_slice(&glyph.advance.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());
    }

    vec![
        (*b"cmap", encode_cmap(cmap)),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
    ]
}

/// Lays out an sfnt. `external` entries are directory records whose
/// offset and length refer to bytes outside the sfnt.
fn assemble(tables: &[([u8; 4], Vec<u8>)], external: &[([u8; 4], u32, u32)]) -> Vec<u8> {
    let count = tables.len() + external.len();
    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    font.extend_from_slice(&(count as u16).to_be_bytes());
    font.extend_from_slice(&[0; 6]);

    let mut offset = 12 + count * 16;
    for (tag, data) in tables {
        font.extend_from_slice(tag);
        font.extend_from_slice(&0u32.to_be_bytes());
        font.extend_from_slice(&(offset as u32).to_be_bytes());
        font.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len().next_multiple_of(4);
    }

    for (tag, external_offset, len) in external {
        font.extend_from_slice(tag);
        font.extend_from_slice(&0u32.to_be_bytes());
        font.extend_from_slice(&external_offset.to_be_bytes());
        font.extend_from_slice(&len.to_be_bytes());
    }

    for (_, data) in tables {
        font.extend_from_slice(data);
        font.resize(font.len().next_multiple_of(4), 0);
    }

    font
}

/// Builds a TrueType file with cmap, glyf, head, hhea, hmtx and long loca.
fn build_font(glyphs: &[TestGlyph], cmap: &[(u16, u16)]) -> Vec<u8> {
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for glyph in glyphs {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        encode_glyph(&glyph.outline, glyphs, &mut glyf);
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());

    let mut tables = metric_tables(glyphs, cmap);
    tables.push((*b"glyf", glyf));
    tables.push((*b"loca", loca));
    tables.sort_by_key(|(tag, _)| *tag);

    assemble(&tables, &[])
}

/// Builds an `xttf` file whose glyph records sit in `xglf` pages after the
/// compressed tables, `glyphs_per_page` records per page.
fn build_paged_xtt(glyphs: &[TestGlyph], cmap: &[(u16, u16)], glyphs_per_page: usize) -> Vec<u8> {
    const PAGE_LEN: usize = 0x1000;

    let mut pages: Vec<Vec<u8>> = Vec::new();
    let mut xloc = Vec::new();
    for (index, glyph) in glyphs.iter().enumerate() {
        if index % glyphs_per_page == 0 {
            pages.push(Vec::new());
        }

        let page = pages.len() - 1;
        xloc.extend_from_slice(&((page as u32) << 16 | pages[page].len() as u32).to_be_bytes());
        encode_glyph(&glyph.outline, glyphs, &mut pages[page]);
    }
    let last = pages.len() - 1;
    xloc.extend_from_slice(&((last as u32) << 16 | pages[last].len() as u32).to_be_bytes());

    let mut xglf = Vec::new();
    for page in &pages {
        xglf.extend_from_slice(&zlib_stored(page));
        xglf.resize(xglf.len().next_multiple_of(PAGE_LEN), 0);
    }

    let mut tables = metric_tables(glyphs, cmap);
    tables.push((*b"xloc", xloc));

    // The compressed length does not depend on the xglf offset value.
    let placeholder = assemble(&tables, &[(*b"xglf", 0, xglf.len() as u32)]);
    let glyph_offset = (0x118 + zlib_stored(&placeholder).len()).next_multiple_of(PAGE_LEN);
    let sfnt = assemble(
        &tables,
        &[(*b"xglf", glyph_offset as u32, xglf.len() as u32)],
    );
    let stream = zlib_stored(&sfnt);

    let total = glyph_offset + xglf.len();
    let mut file = Vec::with_capacity(total);
    file.extend_from_slice(b"xttf");
    file.extend_from_slice(&[0; 256]);
    for value in [
        (glyph_offset - 0x104) as u32,
        total as u32,
        stream.len() as u32,
        sfnt.len() as u32,
        0x0001_0000,
    ] {
        file.extend_from_slice(&value.to_be_bytes());
    }
    file.extend_from_slice(&stream);
    file.resize(glyph_offset, 0);
    file.extend_from_slice(&xglf);

    file
}

fn encode_glyph(outline: &Outline, glyphs: &[TestGlyph], out: &mut Vec<u8>) {
    let push16 = |out: &mut Vec<u8>, value: i16| out.extend_from_slice(&value.to_be_bytes());

    match outline {
        Outline::Empty => {}
        Outline::Simple(contours) => {
            let points: Vec<(i16, i16, bool)> = contours.iter().flatten().copied().collect();
            push16(out, contours.len() as i16);
            push16(out, points.iter().map(|point| point.0).min().unwrap_or(0));
            push16(out, points.iter().map(|point| point.1).min().unwrap_or(0));
            push16(out, points.iter().map(|point| point.0).max().unwrap_or(0));
            push16(out, points.iter().map(|point| point.1).max().unwrap_or(0));

            let mut end = 0;
            for contour in contours {
                end += contour.len();
                push16(out, end as i16 - 1);
            }
            push16(out, 0);

            for point in &points {
                out.push(u8::from(point.2));
            }

            let mut previous = (0i16, 0i16);
            for point in &points {
                push16(out, point.0 - previous.0);
                previous.0 = point.0;
            }
            for point in &points {
                push16(out, point.1 - previous.1);
                previous.1 = point.1;
            }
        }
        Outline::Composite(components) => {
            let bounds: Vec<(i16, i16, i16, i16)> = components
                .iter()
                .filter_map(|(glyph, dx, dy)| match &glyphs[*glyph as usize].outline {
                    Outline::Simple(contours) => {
                        let points: Vec<_> = contours.iter().flatten().collect();
                        Some((
                            points.iter().map(|point| point.0).min()? + dx,
                            points.iter().map(|point| point.1).min()? + dy,
                            points.iter().map(|point| point.0).max()? + dx,
                            points.iter().map(|point| point.1).max()? + dy,
                        ))
                    }
                    _ => None,
                })
                .collect();

            push16(out, -1);
            push16(out, bounds.iter().map(|bound| bound.0).min().unwrap_or(0));
            push16(out, bounds.iter().map(|bound| bound.1).min().unwrap_or(0));
            push16(out, bounds.iter().map(|bound| bound.2).max().unwrap_or(0));
            push16(out, bounds.iter().map(|bound| bound.3).max().unwrap_or(0));

            for (index, (glyph, dx, dy)) in components.iter().enumerate() {
                let more = if index + 1 < components.len() {
                    0x20
                } else {
                    0
                };
                push16(out, 0x0003 | more);
                push16(out, *glyph as i16);
                push16(out, *dx);
                push16(out, *dy);
            }
        }
    }
}

fn encode_cmap(map: &[(u16, u16)]) -> Vec<u8> {
    let mut segments: Vec<(u16, u16)> = map.to_vec();
    segments.sort();
    segments.push((0xFFFF, 0));

    let count = segments.len() as u16;
    let mut subtable = Vec::new();
    for value in [4u16, 16 + count * 8, 0, count * 2, 0, 0, 0] {
        subtable.extend_from_slice(&value.to_be_bytes());
    }
    for (code, _) in &segments {
        subtable.extend_from_slice(&code.to_be_bytes());
    }
    subtable.extend_from_slice(&0u16.to_be_bytes());
    for (code, _) in &segments {
        subtable.extend_from_slice(&code.to_be_bytes());
    }
    for (code, glyph) in &segments {
        let delta = if *code == 0xFFFF {
            1
        } else {
            glyph.wrapping_sub(*code)
        };
        subtable.extend_from_slice(&delta.to_be_bytes());
    }
    for _ in &segments {
        subtable.extend_from_slice(&0u16.to_be_bytes());
    }

    // A format 6 subtable after the format 4 one must be skipped.
    let mut table = Vec::new();
    for value in [0u16, 2, 3, 1] {
        table.extend_from_slice(&value.to_be_bytes());
    }
    table.extend_from_slice(&20u32.to_be_bytes());
    for value in [3u16, 10] {
        table.extend_from_slice(&value.to_be_bytes());
    }
    table.extend_from_slice(&(20 + subtable.len() as u32).to_be_bytes());
    table.extend_from_slice(&subtable);
    for value in [6u16, 10, 0, 0, 0] {
        table.extend_from_slice(&value.to_be_bytes());
    }

    table
}

/// zlib stream of stored (uncompressed) blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut stream = vec![0x78, 0x01];
    let mut chunks = data.chunks(0xFFFF).peekable();
    while let Some(chunk) = chunks.next() {
        stream.push(u8::from(chunks.peek().is_none()));
        stream.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        stream.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        stream.extend_from_slice(chunk);
    }

    let (mut a, mut b) = (1u32, 0u32);
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    stream.extend_from_slice(&(b << 16 | a).to_be_bytes());

    stream
}

/// Wraps a file in an `xttf` container with a zero signature.
fn build_xtt(sfnt: &[u8], version: u32) -> Vec<u8> {
    let stream = zlib_stored(sfnt);

    let total = 0x118 + stream.len();
    let mut file = Vec::with_capacity(total);
    file.extend_from_slice(b"xttf");
    file.extend_from_slice(&[0; 256]);
    for value in [
        (total - 0x104) as u32,
        total as u32,
        stream.len() as u32,
        sfnt.len() as u32,
        version,
    ] {
        file.extend_from_slice(&value.to_be_bytes());
    }
    file.extend_from_slice(&stream);

    file
}

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// "Sans": 'I' 400, 'O' 800, 'H' 700 and space 300 units of 1000 per em.
pub(crate) fn library() -> FontLibrary {
    let fallback_glyphs = vec![
        TestGlyph {
            advance: 500,
            outline: Outline::Empty,
        },
        TestGlyph {
            advance: 1000,
            outline: Outline::Simple(vec![rectangle(100, -100, 900, 700)]),
        },
    ];

    let mut fonts = FontLibrary::new();
    let typeface = Typeface {
        primary: LoadedFont::load(FontData::Owned(build_xtt(
            &build_font(&latin_glyphs(), &latin_cmap()),
            0x0001_0000,
        )))
        .expect("primary font loads"),
        fallback: Some(
            LoadedFont::load(FontData::Owned(build_font(
                &fallback_glyphs,
                &[(0x4E00, 1)],
            )))
            .expect("fallback font loads"),
        ),
        line_height_reduction: 0.0,
    };
    fonts
        .register("Sans", typeface, true)
        .expect("test font registers");

    fonts
}

fn request<'a>(text: &'a [u16], style: u32, width: f32, height: f32) -> TextRequest<'a> {
    TextRequest {
        text,
        typeface: None,
        point_size: 12.0,
        style: TextStyle(style),
        color: Rgba8::WHITE,
        shadow_color: None,
        line_spacing_adjust: 0,
        box_width: width,
        box_height: height,
        scroll: 0.0,
    }
}

/// Quad geometry with texel coordinates instead of normalized UVs.
fn describe(quads: &[GlyphQuad], store: &Store) -> Vec<String> {
    quads
        .iter()
        .map(|quad| {
            let size = store
                .slots
                .get(quad.texture)
                .map_or(0.0, |image| image.width().get() as f32);
            let texels: Vec<f32> = quad.uv.iter().map(|uv| uv * size).collect();
            format!(
                "({}, {})-({}, {}) texels {texels:?} color {:08X}",
                quad.left, quad.top, quad.right, quad.bottom, quad.color.0
            )
        })
        .collect()
}

/// Coverage of a quad's texels as hexadecimal alpha rows.
fn coverage(quad: &GlyphQuad, store: &Store) -> Vec<String> {
    let image = store.slots.get(quad.texture).expect("quad texture exists");
    let size = image.width().get() as f32;
    let (x0, y0, x1, y1) = (
        (quad.uv[0] * size) as usize,
        (quad.uv[1] * size) as usize,
        (quad.uv[2] * size) as usize,
        (quad.uv[3] * size) as usize,
    );

    (y0..y1)
        .map(|y| {
            (x0..x1)
                .map(|x| format!("{:02X}", image.pixels()[y * size as usize + x].alpha()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[test]
fn parses_plain_and_xtt_fonts() {
    let plain = Font::parse(
        FontData::Owned(build_font(&latin_glyphs(), &latin_cmap())),
        None,
    )
    .expect("plain font parses");
    let fonts = library();
    let entry = fonts.resolve(None).expect("default typeface resolves");

    let describe_font = |font: &Font| {
        let glyphs: Vec<_> = ['I', 'O', 'H', ' ', 'Z']
            .iter()
            .map(|code| {
                font.glyph(*code as u16)
                    .map(|glyph| (glyph, font.advance_units(glyph)))
            })
            .collect();
        format!(
            "upem {} ascender {} line height {} glyphs {glyphs:?}",
            font.units_per_em(),
            font.ascender(),
            font.line_height_units()
        )
    };

    insta::assert_debug_snapshot!([describe_font(&plain), describe_font(&entry.primary)]);
}

#[test]
fn rejects_bad_containers() {
    let sfnt = build_font(&latin_glyphs(), &latin_cmap());
    let mut wrong_size = build_xtt(&sfnt, 0x0001_0000);
    wrong_size.push(0);
    let mut tables = sfnt.clone();
    tables[4..6].copy_from_slice(&2u16.to_be_bytes());

    let load = |data: Vec<u8>| {
        LoadedFont::load(FontData::Owned(data))
            .map(|_| ())
            .map_err(|error| format!("{error}"))
    };

    insta::assert_debug_snapshot!([
        load(build_xtt(&sfnt, 0x0002_0000)),
        load(wrong_size),
        load(b"xttf".to_vec()),
        load(tables),
        FontLibrary::new()
            .register(
                "a;b",
                Typeface {
                    primary: LoadedFont::load(FontData::Owned(sfnt)).expect("font loads"),
                    fallback: None,
                    line_height_reduction: 0.0
                },
                false
            )
            .map_err(|error| format!("{error}")),
    ]);
}

/// Quads and coverage of a text drawn with the default typeface.
fn render_report(fonts: &FontLibrary, text: &str) -> String {
    let mut renderer = TextRenderer::new();
    let mut store = Store::default();
    let text = utf16(text);
    let mut quads = Vec::new();
    renderer
        .layout(
            fonts,
            &request(&text, 0, 100.0, 40.0),
            &mut store,
            &mut quads,
        )
        .expect("layout succeeds");
    renderer.flush(&mut store).expect("flush succeeds");

    let mut report = describe(&quads, &store);
    for quad in &quads {
        report.push(String::new());
        report.extend(coverage(quad, &store));
    }

    report.join("\n")
}

#[test]
fn glyph_coverage() {
    insta::assert_snapshot!(render_report(&library(), "IOH"));
}

#[test]
fn paged_glyphs_match_plain_glyphs() {
    // Two records per page puts the composite 'H' and its component 'I' on
    // different pages.
    let mut paged = FontLibrary::new();
    let typeface = Typeface {
        primary: LoadedFont::load(FontData::Owned(build_paged_xtt(
            &latin_glyphs(),
            &latin_cmap(),
            2,
        )))
        .expect("paged font loads"),
        fallback: None,
        line_height_reduction: 0.0,
    };
    paged
        .register("Sans", typeface, true)
        .expect("paged font registers");

    let mut plain = FontLibrary::new();
    let typeface = Typeface {
        primary: LoadedFont::load(FontData::Owned(build_font(&latin_glyphs(), &latin_cmap())))
            .expect("plain font loads"),
        fallback: None,
        line_height_reduction: 0.0,
    };
    plain
        .register("Sans", typeface, true)
        .expect("plain font registers");

    assert_eq!(
        render_report(&paged, "IOH \u{2026}Z"),
        render_report(&plain, "IOH \u{2026}Z")
    );
}

#[test]
fn layout_quads() {
    let fonts = library();
    let mut renderer = TextRenderer::new();
    let mut store = Store::default();

    let mut run = |text: &str, style: u32, width: f32, height: f32, shadow: bool| {
        let text = utf16(text);
        let mut request = request(&text, style, width, height);
        request.shadow_color = shadow.then_some(Rgba8::new(0, 0, 0, 0x80));
        let mut quads = Vec::new();
        renderer
            .layout(&fonts, &request, &mut store, &mut quads)
            .expect("layout succeeds");
        describe(&quads, &store)
    };

    let cases = vec![
        ("wrap at space", run("IOI HHH", 0, 50.0, 100.0, false)),
        ("right aligned", run("II", 0x200, 50.0, 100.0, false)),
        ("centered both ways", run("II", 0x1400, 51.0, 40.0, false)),
        ("ellipsis", run("IIIIIIIIII", 0x4010, 40.0, 40.0, false)),
        (
            "right no wrap clips start",
            run("OIIIII", 0x210, 30.0, 40.0, false),
        ),
        ("drop shadow", run("I", 0, 40.0, 40.0, true)),
        ("vertical clip", run("I\nI", 0, 40.0, 25.0, false)),
        (
            "fallback and default glyph",
            run("\u{4E00}Z", 0, 100.0, 40.0, false),
        ),
    ];

    insta::assert_debug_snapshot!(cases);
}

#[test]
fn measures_text() {
    let fonts = library();
    let mut renderer = TextRenderer::new();
    let text = utf16("IOI HHH IOI");

    let extents: Vec<_> = [0, 0x20, 0x10, 0x2000]
        .iter()
        .map(|style| {
            (
                format!("{style:#X}"),
                renderer
                    .measure(&fonts, &request(&text, *style, 50.0, 0.0))
                    .expect("measure succeeds"),
            )
        })
        .collect();

    insta::assert_debug_snapshot!(extents);
}

#[test]
fn typeface_lookup() {
    let fonts = library();
    let mut renderer = TextRenderer::new();
    let text = utf16("I");
    let unknown = utf16("Unknown");
    let empty: [u16; 0] = [];

    let mut measure = |typeface: Option<&[u16]>, point_size: f32| {
        let mut request = request(&text, 0, 100.0, 40.0);
        request.typeface = typeface;
        request.point_size = point_size;
        renderer
            .measure(&fonts, &request)
            .map_err(|error| format!("{error}"))
    };

    insta::assert_debug_snapshot!([
        measure(Some(&empty), 12.0),
        measure(Some(&unknown), 12.0),
        measure(None, 0.5),
        measure(None, 400.0),
        measure(None, 150.0),
    ]);
}

#[test]
fn cache_overflow_clears_once() {
    let fonts = library();
    let mut renderer = TextRenderer::new();
    let mut store = Store::default();
    let text = utf16("abcdefghij");
    let mut request = request(&text, 0x10, 4000.0, 400.0);
    request.point_size = 150.0;

    let mut frame = |store: &mut Store| {
        let mut quads = Vec::new();
        renderer
            .layout(&fonts, &request, store, &mut quads)
            .expect("layout succeeds");
        renderer.flush(store).expect("flush succeeds");
        quads
            .iter()
            .map(|quad| quad.texture)
            .collect::<Vec<TextureId>>()
    };

    let first = frame(&mut store);
    let pages = store.live;
    let second = frame(&mut store);
    let uploads = store.uploads;
    frame(&mut store);

    assert!(pages > 1, "ten large glyphs overflow one page");
    assert!(
        first.windows(2).any(|pair| pair[0] != pair[1]),
        "quads sample several pages"
    );
    assert_eq!(
        (second, store.live),
        (first, pages),
        "the cleared cache refills its existing pages"
    );
    assert_eq!(
        store.uploads, uploads,
        "a cache that fits its pages is not cleared again"
    );
}

#[test]
fn flush_uploads_only_changed_pages() {
    let fonts = library();
    let mut renderer = TextRenderer::new();
    let mut store = Store::default();
    let text = utf16("IO");
    let mut quads = Vec::new();

    renderer
        .layout(
            &fonts,
            &request(&text, 0, 100.0, 40.0),
            &mut store,
            &mut quads,
        )
        .expect("layout succeeds");
    renderer.flush(&mut store).expect("flush succeeds");
    let after_first = store.uploads;

    renderer
        .layout(
            &fonts,
            &request(&text, 0, 100.0, 40.0),
            &mut store,
            &mut quads,
        )
        .expect("layout succeeds");
    renderer.flush(&mut store).expect("flush succeeds");
    let after_cached = store.uploads;

    let more = utf16("H");
    renderer
        .layout(
            &fonts,
            &request(&more, 0, 100.0, 40.0),
            &mut store,
            &mut quads,
        )
        .expect("layout succeeds");
    renderer.flush(&mut store).expect("flush succeeds");

    assert_eq!((after_first, after_cached, store.uploads), (1, 1, 2));
}

/// Glyphs 6 and up after the latin set: 6 a composite that contains
/// itself, 7 a ring of off-curve points that flattens to more edges than a
/// glyph may have at 150 points, 8 a contour with more points than a glyph
/// may have.
fn hostile_glyphs() -> Vec<TestGlyph> {
    let mut glyphs = latin_glyphs();
    glyphs.push(TestGlyph {
        advance: 500,
        outline: Outline::Composite(vec![(6, 0, 0)]),
    });
    let zigzag = (0..600)
        .map(|index| {
            if index % 2 == 0 {
                (0, 0, false)
            } else {
                (1000, 1000, false)
            }
        })
        .collect();
    glyphs.push(TestGlyph {
        advance: 500,
        outline: Outline::Simple(vec![zigzag]),
    });
    let line = (0..0x4001).map(|index| (index % 500, 0, true)).collect();
    glyphs.push(TestGlyph {
        advance: 500,
        outline: Outline::Simple(vec![line]),
    });

    glyphs
}

fn hostile_cmap() -> Vec<(u16, u16)> {
    let mut cmap = latin_cmap();
    cmap.extend([(0x58, 6), (0x59, 7), (0x5A, 8)]);

    cmap
}

#[test]
fn glyph_limits_are_typed_errors() {
    let font = Font::parse(
        FontData::Owned(build_font(&hostile_glyphs(), &hostile_cmap())),
        None,
    )
    .expect("font parses");
    let mut rasterizer = crate::text::raster::Rasterizer::default();
    let mut scratch = crate::text::sfnt::OutlineScratch::default();
    let placement = crate::text::raster::Placement {
        scale: 1.6,
        baseline: 1280.0,
    };
    let mut outline = |glyph: u16| {
        let mut sink = rasterizer.outline_sink(placement);
        font.outline(crate::text::sfnt::GlyphId(glyph), &mut sink, &mut scratch)
    };

    assert!(matches!(
        outline(6),
        Err(crate::text::error::GlyphError::TooDeep { limit: 8 })
    ));
    assert!(matches!(
        outline(8),
        Err(crate::text::error::GlyphError::TooManyPoints { limit: 0x4000 })
    ));
    assert!(outline(7).is_ok());
    assert!(rasterizer.overflowed());
}

#[test]
fn failed_glyphs_draw_nothing_and_the_rest_draws() {
    let mut fonts = FontLibrary::new();
    let typeface = Typeface {
        primary: LoadedFont::load(FontData::Owned(build_font(
            &hostile_glyphs(),
            &hostile_cmap(),
        )))
        .expect("font loads"),
        fallback: None,
        line_height_reduction: 0.0,
    };
    fonts
        .register("Sans", typeface, true)
        .expect("font registers");
    let mut renderer = TextRenderer::new();
    let mut store = Store::default();
    let text = utf16("IXIYIZI");
    let mut request = request(&text, 0x10, 4000.0, 400.0);
    request.point_size = 150.0;
    let mut quads = Vec::new();

    renderer
        .layout(&fonts, &request, &mut store, &mut quads)
        .expect("layout succeeds");

    assert_eq!(quads.len(), 4, "only the four I glyphs draw");
}

#[test]
fn glyph_cache_pages_are_bounded() {
    let fonts = library();
    let mut renderer = TextRenderer::new();
    let mut store = Store::default();
    // 80 code units without glyphs: each caches its own cell of the
    // default glyph, eight per 512-texel page at 150 points.
    let text: Vec<u16> = (0x100..0x150).collect();
    let mut request = request(&text, 0x10, 20000.0, 400.0);
    request.point_size = 150.0;
    let mut quads = Vec::new();

    renderer
        .layout(&fonts, &request, &mut store, &mut quads)
        .expect("layout succeeds");

    assert_eq!((quads.len(), store.live), (64, 8));
}

#[test]
fn font_instances_are_bounded() {
    let fonts = library();
    let mut renderer = TextRenderer::new();
    let mut store = Store::default();
    let text = utf16("I");

    for step in 0..17 {
        let mut request = request(&text, 0, 100.0, 40.0);
        request.point_size = 10.0 + step as f32;
        renderer
            .layout(&fonts, &request, &mut store, &mut Vec::new())
            .expect("layout succeeds");
    }
    renderer.flush(&mut store).expect("flush succeeds");
    let created = store.live;
    // The evicted instance's page outlives the frame that evicted it.
    renderer.flush(&mut store).expect("flush succeeds");

    assert_eq!((created, store.live), (17, 16));
}

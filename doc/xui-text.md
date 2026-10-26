# XUI text (dash 2.0.6717)

Scope: font files, typeface registration, font instances and metrics, TextStyle bits, layout (line breaking, alignment, ellipsis, vertical centering), drawing (glyph cache, antialiasing, drop shadow), and the string table loader. Rendering state (color factor, blending) is in `xui-render.md`.

Addresses are dash virtual addresses. RE status per section.

## 1. Font files (.xtt)

RE status: complete for the container and table use; rasterizer internals partial.

Typeface files are `file://media:/XenonCLatin.xtt`, `XenonJKLatin.xtt`, `XenonSCLatin.xtt` (flash, not in our material; DVD player variants `file://dvdmedia:/...xtt`, `.xttp1`, `.xttp2`). `XuiFont_OpenFile` (0x9218a7f8) reads the first 4 bytes:

- `xttf` (0x78747466): signed, zlib-compressed container (`XuiFont_LoadXttContainer` 0x9218a5c0). Header is 0x118 bytes, fields big-endian: +0x000 magic `xttf`; +0x004 256-byte RSA signature; +0x104 u32 signed length L (must be <= file size - 0x100); +0x108 u32 file size (must equal the actual file size); +0x10c u32 compressed byte count; +0x110 u32 uncompressed size; +0x114 u32 version (must be 0x00010000). L bytes are read from file offset 0x104 (so they cover the header fields and the compressed data) and verified (`XuiFont_VerifyXttSignature` 0x9218a4e8: XeCryptRotSumSha with salt "XBOX360XTT", XeKeysGetKey, XeCryptBnQwBeSigVerify against the signature at +4). Then zlib 1.1.4 `uncompress` (0x921893e8 -> 0x9218c120) inflates `[+0x10c]` bytes starting at file offset 0x118 into a buffer of `[+0x110]` bytes. The result is a TrueType sfnt. A failed check or a failed inflate rejects the file (0x80004005).
- anything else: plain TrueType sfnt, accepted only when the global at 0x9280b970 is 0 (`XuiFont_LoadSfnt` 0x9218a320). The offset table must have 3..32 tables. If every non-glyf table lies before `glyf`, only the bytes before `glyf` are kept in memory and glyphs are streamed from the file; otherwise the whole file is read.

Tables used (`XuiFont_ParseTables` 0x9218d6c8, all big-endian): `head` (magic 0x5F0F3CF5, unitsPerEm, bounding box, macStyle & 3, indexToLocFormat), `hhea` (ascender, descender, lineGap, advanceWidthMax, numberOfHMetrics; line height in font units = ascender - descender + lineGap), `cmap` (last encoding subtable whose format is 0 or 4), `loca` or `xloc`, `glyf` or `xglf`, `hmtx`, `name` (platform 3, language 0x409, nameID 1, up to 18 chars), `xchk`, `xttf` (u32 + 256-byte block). `xloc`/`xglf` replace loca/glyf in the genuine `xttf` files and use a paged, compressed layout (observed in XenonCLatin, XenonJKLatin and XenonSCLatin; page handling in the parser not yet traced):

- The zlib payload of the container holds every table except the glyph data. The `xglf` directory offset and length refer to the `xttf` file, not to the inflated tables: offset = 0x104 + signed length L, offset + length = file size.
- `xglf` is a run of 4096-byte pages [INFERENCE: page size from the file layout]. Each page is an independent zlib stream padded with zeros to the page end.
- `head.indexToLocFormat` is 1. Each `xloc` entry is `page << 16 | offset`, the offset counting bytes of that page's inflated data. Records do not span pages: a record ends at the next entry when it is in the same page, else at the end of the page's inflated data. Equal positions mean an empty glyph.

For a reimplementation: decompress `xttf` containers (signature check optional) and use a standard TrueType rasterizer.

## 2. Typeface registration and lookup

RE status: complete.

`XuiRegisterTypeface(desc, isDefault)` (0x9216d4f8). Descriptor: [0] name, [1] primary file URL, [2] optional second file URL, [3] f32 line-height reduction fraction, [4] optional string. Name, file and [4] must not contain ';'; file URLs must have a scheme. Typefaces live in a string map (0x9280b3f4); re-registering a name replaces it; `isDefault` stores the name as the default typeface (0x9280b3f0).

Dash registers (`DashSetLanguageAndFonts` 0x921ca820) name "ConvectionUI" with fraction 0.0. The primary file is XenonCLatin.xtt for most languages and XenonJKLatin.xtt for language id 8 (the other file is the second URL); the exact language mapping is DashApp's. The fallback descriptor name "ConvectionUIFallback" (0x92016b88) also appears.

`XuiCreateFont(typeface, ?, style, ?, out, pointSize)` (0x92182608): pointSize < 1 is rejected; empty/null name uses the default typeface; the renderer reports scalable fonts so pointSize is clamped to <= 150; fonts are cached by (typeface, style & 0xe, size).

The font renderer (vtable 0x9200ec88, object 0x9280b5f4) is initialized with 96 dpi. Pixel size = `pointSize * 96 / 72`. Style bits 0x2 and 0x4 become renderer flags 1 and 2 (`XuiFontRenderer_CreateFont` 0x92189f90): synthetic oblique and synthetic bold (section 4); 0x8 makes the draw loop start with the special code 0xFFFE.

Font instance (`XuiFont_InitInstance` 0x92182048): metrics from the renderer in pixels (rasterizer works at 8x, metrics scaled by 0.125). `lineHeight(+0x34) = m0 - m0 * fraction`, taking the maximum over linked fallback fonts. Glyph cache texture size: 128x128 if pixel size < 12, 256x256 if < 64, else 512x512 (A8 format 0x04900002). Shadow offset = (1/cacheW, 1/cacheH).

## 3. Text element properties

RE status: complete for offsets; defaults are in XuiReader's property tables.

XuiText (and XuiTextPresenter, same layout): Text +0x20 (string), TextColor +4, DropShadowColor +8, PointSize +0xc (f32), Font +0x10 (typeface name), font handle +0x14, TextStyle +0x18, LineSpacingAdjust +0x1c (integer pixels). XuiTextPresenter adds DataAssociation +0x28 and gets its text from the parent control. 6717 has no TextScale property. The font handle is recreated when PointSize, Font or style bits 0xe change (message 0x29 / render).

## 4. TextStyle bits

RE status: complete for bits listed except 0x8 and 0x8000, which are not traced.

| Bit | Meaning | Evidence |
|---|---|---|
| 0x0001 | drop shadow on (PS4 with DropShadowColor) | `XuiFont_DrawQuads` 0x9217f5a0; PS4 at 0x9200cef8 |
| 0x0002 | synthetic oblique (below) | 0x92189f90, 0x9218cc80 |
| 0x0004 | synthetic bold (below) | 0x92189f90, 0x9218cce8 |
| 0x0008 | font flag; draw loop begins with code 0xFFFE | 0x92183098 |
| 0x0010 | no word wrap: clip at the rect width (with right alignment and no ellipsis, clip from the left so the end stays visible) | `XuiFont_NextLine` 0x92182758 |
| 0x0020 | single line: stop after the first line, keep trailing spaces | 0x92182758 |
| 0x0100 | left-alignment marker used only by the Korean rule (see 6) | 0x92180040 |
| 0x0200 | align right | 0x92183098 |
| 0x0400 | center horizontally | 0x92183098 |
| 0x1000 | center vertically | 0x92183098 |
| 0x2000 | multi-line measurement (`XuiMeasureText` uses the wrapping measurer) | 0x92183be8 |
| 0x4000 | ellipsis when truncated | 0x92183098 |
| 0x010000, 0x040000, 0x100000 | Korean word-wrap rule enable for left (with 0x100), center, right alignment | 0x92180040 |

Synthetic styles. The font face instance (`XuiFontFace_InitInstance` 0x9218aa70) holds the flags at +0xc, the shear divisor 3.0 at +0x10 and `k = unitsPerEm * 0.0122` (f32) at +0x14 and +0x18. The outline loader (`XuiFontFace_LoadGlyphOutline` 0x9218dd48) stores each point as 16-bit font units with y negated, then applies the oblique shear and afterwards the emboldening to each simple glyph or composite component before its component transform. All arithmetic is f32; results are truncated toward zero and stored as 16 bits.

```
oblique (0x9218cc80):  x -= (int)(y / 3.0 + 0.5)                         (y negated: upright tops lean right)
bold (0x9218cce8), per contour, prev/next wrap and are read before they move:
    na = normalize(prev - cur), nb = normalize(cur - next)      (zero length gives 0)
    oa = (k * -na.y, k * na.x), ob = (k * -nb.y, k * nb.x)
    if (int)(ob.x * 100) == (int)(oa.x * 100) and (int)(ob.y * 100) != (int)(oa.y * 100): d = oa
    else:
        I = intersection of line (prev+oa, cur+oa) with line (cur+ob, next+ob)
            (+inf when |cross| <= 2^-23 and the lines coincide, NaN when parallel)
        if I.x is NaN or infinite: I = ((cur+ob) + (cur+oa)) * 0.5
        d = I - cur
    x += (int)(d.x + k + 0.5); y += (int)(d.y - k + 0.5)
```

The outline grows outward by k and moves right and up by k, so left and bottom edges stay. With bold, `c = (int)ceil(2k)` font units are added to the advance (`XuiFontFace_GlyphAdvance` 0x9218b6a8), the line height (0x9218b238), the ascender (0x9218b3a8), the maximum advance (0x9218b2f8), the bounds height (0x9218b5f0) and the glyph bounds xMax and yMax; oblique moves the glyph bounds to `xMin + yMin / 3` and `xMax + yMax / 3` (fused multiply-add, truncated; 0x9218ae28). The descender is unchanged.

If bits 0x3f0000 are all zero, 0x140000 is ORed in (0x9279ac70). XuiEdit uses (style & ~0x30) | 0x2000 for multi-line and (style | 0x30) & ~0x2000 for single-line, and clears 0x4000 when editable.

## 5. Layout and drawing

RE status: complete for layout; glyph quad clipping and cache allocation partial.

`XuiText_Draw` (0x9215c080): rect = (0, 0, W, H) of the element; color factor' = colorFactor * TextColor (per channel, 8-bit quantized); DC shadow color = DropShadowColor; then `XuiDrawText` (0x92183ad8). With formatted runs (TextPresenter) per-run colors are applied (partial).

`XuiFont_DrawTextLines` (0x92183098):

```
rect converted to integers (truncation)
pitch = XuiFont_LinePitch: h = ceil(lineHeight); pitch = adj < -h ? 0 : h + adj      (adj = LineSpacingAdjust)
rectW, rectH must be > 0
maxLines = ceil((rectH + scroll) / pitch)
y = 0
if style & 0x1000:
    n = number of lines (wrapping measurer, limited to maxLines)
    y = trunc((rectH + max(adj, -h) - n * pitch) / 2)
loop: next line (6); stop at end of text or when y - scroll > rect.y1
    if truncated and style & 0x4000 and rectW >= adv(U+2026):
        drop characters from the end while width > rectW - adv(U+2026) and count > 1
        if width <= rectW - adv(U+2026): append U+2026
    x = rect.x0                        default
    x = rect.x0 + trunc((rectW - width) / 2)   if 0x400
    x = rect.x0 + rectW - width        if 0x200 (and not 0x400)
    for each glyph: pen positions are rounded with floor(x + 0.5); advance = integer advance from the glyph record
    glyph quads are clipped vertically to the rect (v adjusted proportionally); no horizontal clipping of quads
    y += pitch
```

`scroll` is the float passed by the caller (0 for XuiText).

Missing glyphs: the code -1 (default glyph) is used; with a fallback font configured the renderer is asked first (renderer vtable +0x18).

`XuiMeasureText` (0x92183be8): width = widest line, height = lines * pitch; single-line measurer when 0x2000 is clear.

## 6. Line breaking

RE status: complete.

`XuiFont_NextLine` (0x92182758), maxWidth = rect width:

```
special case (0x200 && 0x10 && !0x4000): take the whole line up to NUL/CR/LF, then drop characters from the start until it fits; mark truncated
else: accumulate characters while width + adv <= maxWidth, stopping at NUL, CR, LF
at the stop character c (prev = last accepted):
    NUL: done
    CR/LF: line ends; consume one CR then one LF
    space/U+3000: break here if CanBreak(next, c) or next is NUL; the line is the accepted chars; with 0x20 the next pointer is null (truncated = style bit 0x10); otherwise skip all following spaces/U+3000, then one CR, then one LF; then if 0x10 is set and the text has not ended, skip to after the next CR/LF and mark truncated
    else if CanBreak(c, prev): break here (with 0x10: discard rest of the line, truncated)
    else if 0x10: keep accepted chars, discard rest of the line, truncated
    else: scan back for a position where CanBreak(cur, prev) holds; break there and skip following spaces/U+3000 and one CR, LF
          if none found: break inside the word at the fitted length
unless 0x20: trailing spaces/U+3000 are removed from the line width and length
with 0x20 the next-line pointer is null (single line)
```

`XuiFont_CanBreakBetween(next, prev, style)` (0x92180040):

1. next and prev both space/U+3000: yes.
2. prev in {',', U+FF0C, '.', U+FF0E, ':', U+FF1A} and next is a digit (0-9, U+FF10-FF19): no.
3. prev is U+2019 and next >= U+3040: yes.
4. Look up both in the kinsoku table (below). prev has noBreakAfter: no.
5. Korean rule applies if (0x400 and 0x40000) or (0x200 and 0x100000) or (0x100 and 0x10000). If it applies and next is Hangul (U+1100-11FF, U+3130-318F, U+AC00-D7A3): yes only if prev is space/U+3000 and next is not noBreakBefore, else (prev.breakAfter and not next.noBreakBefore).
6. Otherwise: if prev is space/U+3000, or U+3040 <= prev < U+F000, or prev >= U+FF66: yes unless next is noBreakBefore. Else: prev.breakAfter and not next.noBreakBefore.

Kinsoku table (0x9279ac78, 147 entries in binary order, binary search in `XuiFont_KinsokuLookup` 0x9217ffb8). Column A is used as "breakAfter" when the character is prev and as "noBreakBefore" when it is next; column B is "noBreakAfter" for prev. The table is sorted except that U+2236 precedes U+2122. The search is: code 0 returns (0, 0); lo = 0, hi = 146; while lo <= hi: mid = (lo + hi) / 2; equal returns the entry; entry > code sets hi = mid - 1, else lo = mid + 1; not found returns (0, 0). With this order U+2236 is never found.

| Code | A | B |
|---|---|---|
| U+0020 | 1 | 0 |
| U+0021 | 1 | 0 |
| U+0022 | 1 | 1 |
| U+0024 | 0 | 1 |
| U+0025 | 1 | 0 |
| U+0027 | 1 | 1 |
| U+0028 | 0 | 1 |
| U+0029 | 1 | 0 |
| U+002C | 1 | 0 |
| U+002E | 1 | 1 |
| U+002F | 1 | 1 |
| U+003A | 1 | 0 |
| U+003B | 1 | 0 |
| U+003F | 1 | 0 |
| U+005B | 0 | 1 |
| U+005C | 0 | 1 |
| U+005D | 1 | 0 |
| U+007B | 0 | 1 |
| U+007D | 1 | 0 |
| U+00A2 | 1 | 0 |
| U+00A3 | 0 | 1 |
| U+00A5 | 0 | 1 |
| U+00A7 | 0 | 1 |
| U+00A8 | 1 | 0 |
| U+00A9 | 1 | 0 |
| U+00AE | 1 | 0 |
| U+00B0 | 1 | 0 |
| U+00B7 | 1 | 1 |
| U+02C7 | 1 | 0 |
| U+02C9 | 1 | 0 |
| U+2013 | 1 | 0 |
| U+2014 | 1 | 0 |
| U+2015 | 1 | 0 |
| U+2016 | 1 | 0 |
| U+2018 | 0 | 1 |
| U+2019 | 1 | 1 |
| U+201C | 0 | 1 |
| U+201D | 1 | 0 |
| U+2022 | 1 | 0 |
| U+2025 | 1 | 0 |
| U+2026 | 1 | 0 |
| U+2027 | 1 | 0 |
| U+2032 | 1 | 0 |
| U+2033 | 1 | 0 |
| U+2035 | 0 | 1 |
| U+2103 | 1 | 0 |
| U+2236 | 1 | 0 |
| U+2122 | 1 | 0 |
| U+2574 | 1 | 0 |
| U+266F | 0 | 1 |
| U+3001 | 1 | 0 |
| U+3002 | 1 | 0 |
| U+3003 | 1 | 0 |
| U+3005 | 1 | 0 |
| U+3008 | 0 | 1 |
| U+3009 | 1 | 0 |
| U+300A | 0 | 1 |
| U+300B | 1 | 0 |
| U+300C | 0 | 1 |
| U+300D | 1 | 0 |
| U+300E | 0 | 1 |
| U+300F | 1 | 0 |
| U+3010 | 0 | 1 |
| U+3011 | 1 | 0 |
| U+3012 | 0 | 1 |
| U+3014 | 0 | 1 |
| U+3015 | 1 | 0 |
| U+3016 | 0 | 1 |
| U+3017 | 1 | 0 |
| U+301D | 0 | 1 |
| U+301E | 1 | 0 |
| U+301F | 1 | 0 |
| U+3041 | 1 | 0 |
| U+3043 | 1 | 0 |
| U+3045 | 1 | 0 |
| U+3047 | 1 | 0 |
| U+3049 | 1 | 0 |
| U+3063 | 1 | 0 |
| U+3083 | 1 | 0 |
| U+3085 | 1 | 0 |
| U+3087 | 1 | 0 |
| U+308E | 1 | 0 |
| U+3099 | 1 | 0 |
| U+309A | 1 | 0 |
| U+309B | 1 | 0 |
| U+309C | 1 | 0 |
| U+309D | 1 | 0 |
| U+309E | 1 | 0 |
| U+30A1 | 1 | 0 |
| U+30A3 | 1 | 0 |
| U+30A5 | 1 | 0 |
| U+30A7 | 1 | 0 |
| U+30A9 | 1 | 0 |
| U+30C3 | 1 | 0 |
| U+30E3 | 1 | 0 |
| U+30E5 | 1 | 0 |
| U+30E7 | 1 | 0 |
| U+30EE | 1 | 0 |
| U+30F5 | 1 | 0 |
| U+30F6 | 1 | 0 |
| U+30FB | 1 | 0 |
| U+30FC | 1 | 0 |
| U+30FD | 1 | 0 |
| U+30FE | 1 | 0 |
| U+FE30 | 1 | 0 |
| U+FE50 | 1 | 0 |
| U+FE51 | 1 | 0 |
| U+FE52 | 1 | 0 |
| U+FE54 | 1 | 0 |
| U+FE55 | 1 | 0 |
| U+FE56 | 1 | 0 |
| U+FE57 | 1 | 0 |
| U+FE59 | 0 | 1 |
| U+FE5A | 1 | 0 |
| U+FE5B | 0 | 1 |
| U+FE5C | 1 | 0 |
| U+FE5D | 0 | 1 |
| U+FE5E | 1 | 0 |
| U+FF01 | 1 | 0 |
| U+FF02 | 1 | 0 |
| U+FF04 | 0 | 1 |
| U+FF05 | 1 | 0 |
| U+FF07 | 1 | 0 |
| U+FF08 | 0 | 1 |
| U+FF09 | 1 | 0 |
| U+FF0C | 1 | 0 |
| U+FF0E | 1 | 0 |
| U+FF1A | 1 | 0 |
| U+FF1B | 1 | 0 |
| U+FF1F | 1 | 0 |
| U+FF20 | 0 | 1 |
| U+FF3B | 0 | 1 |
| U+FF3D | 1 | 0 |
| U+FF40 | 1 | 0 |
| U+FF5B | 0 | 1 |
| U+FF5C | 1 | 0 |
| U+FF5D | 1 | 0 |
| U+FF5E | 1 | 0 |
| U+FF61 | 1 | 0 |
| U+FF64 | 1 | 0 |
| U+FF70 | 1 | 0 |
| U+FF9E | 1 | 0 |
| U+FF9F | 1 | 0 |
| U+FFE0 | 1 | 1 |
| U+FFE1 | 0 | 1 |
| U+FFE5 | 0 | 1 |
| U+FFE6 | 0 | 1 |

## 7. Glyph rasterization and drawing

RE status: partial.

- Glyphs are rasterized on demand into the per-font A8 cache texture (`XuiFont_CacheGlyph` 0x921814c8). Cell width = ceil(advance) + 7 rounded up to a multiple of 16; cells have 2-3 texels of padding. UVs = cell / cache size.
- Antialiasing: outlines are scan converted at 8x vertical oversampling; `XuiFont_DownsampleRows` (0x92188d40) sums 8 sub-rows of per-pixel coverage bytes (saturating) and multiplies by 4, so alpha = min(255, 4 * covered subsamples), 0..64 subsamples per pixel ([INFERENCE] 8 horizontal subsamples per byte from the 8x scaling of glyph metrics).
- Quads (16-byte vertices: x, y, u, v) are drawn as QUADLIST with VS2. Outline glyphs use PS3, or PS4 when style 0x1 is set. For PS4, let `t0` be glyph-cache alpha at the quad UV, `t1` the alpha at `UV - (1/cacheW, 1/cacheH)`, `g = t0 * ColorFactor.a`, and `s = t1 * ShadowColor.a`. The output alpha is `1 - (1 - g)(1 - s)`. Premultiplied RGB is `t0 * ColorFactor.rgb + s(1 - g) * ShadowColor.rgb`. Sampling up and left makes the shadow appear one pixel down and right at native scale. ShadowColor is DropShadowColor with alpha multiplied by color-factor alpha. Bitmap glyphs (record flag +0x1c) use PS1 with ColorFactor = `(1, 1, 1, cf.A)` and no PS4 shadow.
- Sampling of the glyph cache uses the DC defaults (linear filtering).

## 8. String tables (.xus)

RE status: partial. `XuiStringTable_Load` (0x92171318): magic "XUIS", byte 4 must be 1, byte 5 = flags, u16 entry count at offset 0xa, entries from offset 0xc. Each string is u16 length + UTF-16BE characters (reader 0x92170d58). Flags bit 1: entries are values keyed by index; bit 0: value followed by u32 id; neither: value followed by a key string. How XUR text properties reference these tables (localized scene .xus files keyed by id) is in XuiReader's area.

## Nuances

- Text rect, line pitch, alignment offsets and pen positions are integers; vertical centering divides with truncation toward zero.
- Trailing spaces do not count for alignment unless 0x20 is set.
- Right-aligned single-line text without ellipsis clips its beginning, not its end.
- LineSpacingAdjust is in pixels and affects vertical centering through max(adj, -h).
- Drop shadow offset is one glyph-cache texel; it scales with the element transform.
- Point size is clamped to 150; pixel size uses 96 dpi.
- Glyph quads are clipped only vertically to the text rect.

## Reimplementation choices

Traced after the sections above, `XuiFont_NextLine` 0x92182758 and `XuiMeasureText` 0x92183be8:

- Only NextLine sets "truncated": right-align + 0x10 without 0x4000 when characters are dropped from the start; an overflow character that is no break opportunity with 0x10; a break at a space with 0x20 (truncated = 0x10); a break at a space or break opportunity with 0x10 and text left on that line. Running out of rect height never adds an ellipsis.
- Glyph record advance (+2, u16) = ceil(float advance in pixels). Font metrics are floats (8x value * 0.125) without rounding. Line height is ceiled only for the pitch and for the cache row height (ceil(lh) + 5).
- Quad top = line y - scroll - fraction * m0. Quad height = ceil(lh) + 1, quad width = ceil(glyph width) + 1, quad left = floor(pen + 0.5) + glyph x bearing. The baseline lies inside the line-height cell at the font ascent.
- The measurer used without 0x2000 (0x92182da0) loops NextLine with maxWidth = rect width and maxLines = 1 with 0x20, else unlimited. On a truncated line it calls 0x92182370 for a new maxWidth and restarts.
- `XuiMeasureText` argument 3 is a character count (-1 for the whole text; 0 leaves the rect unchanged). Both measurers pass the count less the characters already measured to NextLine as its character limit and stop when none remain or the next character is NUL, so an empty text measures no lines. NextLine ends a line at the limit; with right-align + 0x10 without 0x4000 it drops characters from the start until both the limit and the width are met.

[INFERENCE] choices of exui-runtime where the binary was not traced:

- The float advance is `advanceWidth * pixelSize / unitsPerEm`, not pre-rounded at 8x.
- Glyph x bearing = floor(glyf xMin in pixels); glyph width = xMax in pixels - bearing. The image sits one texel inside its cache cell. A cell is widened when the image does not fit ceil(advance) + 7 rounded to 16.
- Glyphs from the fallback font use the primary font's ascent.
- 0x92182370 returns the width of the whole line up to NUL, CR or LF.
- The 0x2000 measurer (0x92182f30) is the same NextLine loop without line limit and without restart.
- A next-line position at NUL ends the text (no empty last line). A line that consumes no characters (first character wider than the rect) ends the text.
- When a font's cache texture is full, a further texture is used for the rest of the frame; the whole cache is cleared before the font is used in a later frame. The binary evicts within the frame (`XuiFont_CacheGlyph` retries after an evict step and a reset step).
- Drop shadows use the PS4 composition in the glyph's existing quad and sample one cache texel up and left.
- Style bit 0x8 and rasterizer hinting are not reproduced. Composite glyphs with point-matched placement are rejected.
- The bold normalization and line intersection use exact f32 `1 / sqrt` and division where the original refines VMX estimates (`vrsqrtefp` with one Newton step, `vrefp` with two); results can differ when a value lies within rounding error of a truncation boundary.
- Bold and oblique apply to exui's metric model (float `units * pixelSize / unitsPerEm`), not to the original's fixed-point glyph metric records.

## Open questions

- Meaning of style bit 0x8 (code 0xFFFE in `XuiFont_CacheGlyph` 0x921814c8 and `XuiFont_GetGlyph` 0x92181c08) and of 0x8000. Neither appears in a genuine 6717 scene.
- Rasterizer hinting (mode chosen by char >= U+2500 and size < 20 px) not decoded.

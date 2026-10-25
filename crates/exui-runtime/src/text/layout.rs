//! Line breaking (`XuiFont_NextLine`), line placement
//! (`XuiFont_DrawTextLines`) and measurement (`XuiMeasureText`) in integer
//! pixels. Glyph advances come from the caller. Pixel sums wrap in 32 bits,
//! as the PowerPC integer instructions compute them.

use alloc::vec::Vec;

use crate::text::TextStyle;
use crate::text::kinsoku::can_break;
use crate::text::kinsoku::is_space;

const CR: u16 = 0x0D;
const LF: u16 = 0x0A;
pub(crate) const ELLIPSIS: u16 = 0x2026;

/// One line of text: code units `start..end` of the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    pub start: usize,
    pub end: usize,
    pub width: i32,
    pub truncated: bool,
    /// Start of the following line; `None` at the end of the text.
    pub next: Option<usize>,
}

/// A line with its pen origin, ready to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlacedLine {
    pub start: usize,
    /// End after ellipsis truncation.
    pub end: usize,
    pub x: i32,
    /// Top of the line.
    pub y: i32,
    /// U+2026 follows the characters.
    pub ellipsis: bool,
}

/// Vertical metrics of a font instance in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LinePitch {
    /// `ceil(line height)`.
    pub height: i32,
    pub adjust: i32,
}

impl LinePitch {
    /// `XuiFont_LinePitch`.
    pub(crate) fn pitch(self) -> i32 {
        if self.adjust < self.height.wrapping_neg() {
            0
        } else {
            self.height.wrapping_add(self.adjust)
        }
    }
}

/// Integer text rect of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextRect {
    pub width: i32,
    pub height: i32,
}

impl TextRect {
    /// Truncates an element size as XUI converts the rect to integers.
    pub(crate) fn from_size(width: f32, height: f32) -> Self {
        Self {
            width: width as i32,
            height: height as i32,
        }
    }
}

/// Measured size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Extent {
    pub width: i32,
    pub height: i32,
}

/// Breaks the line starting at `start`, which must be inside the text or at
/// its end. A NUL code unit or the end of `text` ends the text.
pub(crate) fn next_line(
    text: &[u16],
    start: usize,
    style: TextStyle,
    max_width: i32,
    advance: &mut dyn FnMut(u16) -> i32,
) -> Line {
    next_line_within(text, start, text.len(), style, max_width, advance)
}

/// `next_line` with the character limit of `XuiFont_NextLine`: a line
/// stops at code unit `limit`, which must not be below `start`.
/// Right-aligned clipping keeps at most `limit - start` characters of the
/// end of the line. The measurers stop at the limit, so a line ending
/// there returns no next line.
fn next_line_within(
    text: &[u16],
    start: usize,
    limit: usize,
    style: TextStyle,
    max_width: i32,
    advance: &mut dyn FnMut(u16) -> i32,
) -> Line {
    let at = |index: usize| text.get(index).copied().unwrap_or(0);

    let mut line = if style.contains(TextStyle::ALIGN_RIGHT)
        && style.contains(TextStyle::NO_WRAP)
        && !style.contains(TextStyle::ELLIPSIS)
    {
        clip_start(text, start, limit, max_width, advance)
    } else {
        wrap(text, start, limit, style, max_width, advance)
    };

    if style.contains(TextStyle::SINGLE_LINE) {
        line.next = None;
    } else {
        while line.end > line.start && is_space(at(line.end - 1)) {
            line.width = line.width.wrapping_sub(advance(at(line.end - 1)));
            line.end -= 1;
        }
    }

    // A line that consumes nothing cannot make progress.
    if line.next == Some(start) {
        line.next = None;
    }

    line
}

/// Breaks the text into lines and places them in `rect`, moved up by
/// `scroll` pixels. Placed `y` values do not include the scroll.
pub(crate) fn place_lines(
    text: &[u16],
    style: TextStyle,
    rect: TextRect,
    pitch: LinePitch,
    scroll: f32,
    advance: &mut dyn FnMut(u16) -> i32,
    out: &mut Vec<PlacedLine>,
) {
    out.clear();
    if rect.width <= 0 || rect.height <= 0 {
        return;
    }

    let line_pitch = pitch.pitch();
    let mut y = 0;
    if style.contains(TextStyle::CENTER_VERTICAL) {
        let max_lines = if line_pitch > 0 {
            Some(libm::ceilf((rect.height as f32 + scroll) / line_pitch as f32) as i32)
        } else {
            None
        };
        let count = count_lines(text, style, rect.width, max_lines, advance);
        y = rect
            .height
            .wrapping_add(pitch.adjust.max(pitch.height.wrapping_neg()))
            .wrapping_sub(count.wrapping_mul(line_pitch))
            / 2;
    }

    let ellipsis_advance = advance(ELLIPSIS);
    let mut position = Some(0);
    while let Some(start) = position {
        if y as f32 - scroll > rect.height as f32 {
            break;
        }

        let line = next_line(text, start, style, rect.width, advance);
        position = line.next;

        let mut end = line.end;
        let mut width = line.width;
        let mut ellipsis = false;
        if line.truncated && style.contains(TextStyle::ELLIPSIS) && rect.width >= ellipsis_advance {
            let available = rect.width - ellipsis_advance;
            while width > available && end - line.start > 1 {
                end -= 1;
                width = width.wrapping_sub(advance(text[end]));
            }

            if width <= available {
                ellipsis = true;
                width = width.wrapping_add(ellipsis_advance);
            }
        }

        let x = if style.contains(TextStyle::CENTER_HORIZONTAL) {
            rect.width.wrapping_sub(width) / 2
        } else if style.contains(TextStyle::ALIGN_RIGHT) {
            rect.width.wrapping_sub(width)
        } else {
            0
        };

        out.push(PlacedLine {
            start: line.start,
            end,
            x,
            y,
            ellipsis,
        });
        y = y.wrapping_add(line_pitch);
    }
}

/// `XuiFont_BuildLineStarts` 0x92183cb8: the start index of every line the
/// multi-line measurer counts, all lines regardless of a rect height.
pub(crate) fn line_starts(
    text: &[u16],
    style: TextStyle,
    max_width: i32,
    advance: &mut dyn FnMut(u16) -> i32,
    out: &mut Vec<usize>,
) {
    out.clear();
    let mut position = Some(0);
    while let Some(start) = position {
        out.push(start);
        position = next_line(text, start, style, max_width, advance).next;
    }
}

/// `XuiMeasureText` of the first `limit` code units: no line passes the
/// limit and no line starts at or after it.
pub(crate) fn measure_within(
    text: &[u16],
    limit: usize,
    style: TextStyle,
    max_width: i32,
    pitch: LinePitch,
    advance: &mut dyn FnMut(u16) -> i32,
) -> Extent {
    if style.contains(TextStyle::MULTI_LINE_MEASURE) {
        return measure_lines(text, limit, style, max_width, pitch, advance);
    }

    let max_lines = if style.contains(TextStyle::SINGLE_LINE) {
        Some(1)
    } else {
        None
    };
    let mut max_width = max_width;
    'restart: loop {
        let mut extent = Extent {
            width: 0,
            height: 0,
        };
        let mut count = 0i32;
        let mut position = Some(0);
        while let Some(start) = position {
            if max_lines.is_some_and(|lines| count >= lines) || start >= limit {
                break;
            }

            let line = next_line_within(text, start, limit, style, max_width, advance);

            // A truncated line widens the measurement to the full line.
            if line.truncated {
                let needed = full_line_width(text, start, advance);
                if needed > max_width {
                    max_width = needed;
                    continue 'restart;
                }
            }

            extent.width = extent.width.max(line.width);
            count += 1;
            position = line.next;
        }

        extent.height = count.wrapping_mul(pitch.pitch());
        return extent;
    }
}

/// Multi-line measurer (style 0x2000): every line at `max_width`.
fn measure_lines(
    text: &[u16],
    limit: usize,
    style: TextStyle,
    max_width: i32,
    pitch: LinePitch,
    advance: &mut dyn FnMut(u16) -> i32,
) -> Extent {
    let mut extent = Extent {
        width: 0,
        height: 0,
    };
    let mut count = 0i32;
    let mut position = Some(0);
    while let Some(start) = position {
        if start >= limit {
            break;
        }

        let line = next_line_within(text, start, limit, style, max_width, advance);
        extent.width = extent.width.max(line.width);
        count += 1;
        position = line.next;
    }

    extent.height = count.wrapping_mul(pitch.pitch());
    extent
}

fn count_lines(
    text: &[u16],
    style: TextStyle,
    max_width: i32,
    max_lines: Option<i32>,
    advance: &mut dyn FnMut(u16) -> i32,
) -> i32 {
    let mut count = 0;
    let mut position = Some(0);
    while let Some(start) = position {
        if max_lines.is_some_and(|limit| count >= limit) {
            break;
        }

        position = next_line(text, start, style, max_width, advance).next;
        count += 1;
    }

    count
}

fn is_line_end(code: u16) -> bool {
    code == 0 || code == CR || code == LF
}

/// Width of the line from `start` up to NUL, CR or LF.
fn full_line_width(text: &[u16], start: usize, advance: &mut dyn FnMut(u16) -> i32) -> i32 {
    text[start.min(text.len())..]
        .iter()
        .take_while(|code| !is_line_end(**code))
        .fold(0i32, |width, code| width.wrapping_add(advance(*code)))
}

/// Position after one CR then one LF at `index`; `None` at the end of text.
fn after_line_end(text: &[u16], mut index: usize) -> Option<usize> {
    let at = |index: usize| text.get(index).copied().unwrap_or(0);

    if at(index) == CR {
        index += 1;
    }

    if at(index) == LF {
        index += 1;
    }

    if at(index) == 0 { None } else { Some(index) }
}

/// Discards the rest of the line at `index`.
fn skip_line(text: &[u16], mut index: usize) -> Option<usize> {
    while !is_line_end(text.get(index).copied().unwrap_or(0)) {
        index += 1;
    }

    after_line_end(text, index)
}

/// Right-aligned no-wrap text without ellipsis keeps the end of the line.
fn clip_start(
    text: &[u16],
    start: usize,
    limit: usize,
    max_width: i32,
    advance: &mut dyn FnMut(u16) -> i32,
) -> Line {
    let at = |index: usize| text.get(index).copied().unwrap_or(0);

    let mut end = start;
    let mut width = 0i32;
    while !is_line_end(at(end)) {
        width = width.wrapping_add(advance(at(end)));
        end += 1;
    }

    let allowed = limit - start;
    let mut first = start;
    while (end - first > allowed || width > max_width) && first < end {
        width = width.wrapping_sub(advance(at(first)));
        first += 1;
    }

    Line {
        start: first,
        end,
        width,
        truncated: first > start,
        next: after_line_end(text, end),
    }
}

fn wrap(
    text: &[u16],
    start: usize,
    limit: usize,
    style: TextStyle,
    max_width: i32,
    advance: &mut dyn FnMut(u16) -> i32,
) -> Line {
    let at = |index: usize| text.get(index).copied().unwrap_or(0);
    let no_wrap = style.contains(TextStyle::NO_WRAP);
    let single = style.contains(TextStyle::SINGLE_LINE);

    let mut end = start;
    let mut width = 0i32;
    while end < limit {
        let code = at(end);
        if is_line_end(code) {
            break;
        }

        let code_advance = advance(code);
        if width.wrapping_add(code_advance) > max_width {
            break;
        }

        width = width.wrapping_add(code_advance);
        end += 1;
    }

    let stop = at(end);
    let prev = if end > start { Some(at(end - 1)) } else { None };
    let line = Line {
        start,
        end,
        width,
        truncated: false,
        next: None,
    };

    if stop == 0 || end == limit {
        return line;
    }

    if stop == CR || stop == LF {
        return Line {
            next: after_line_end(text, end),
            ..line
        };
    }

    let following = at(end + 1);
    if is_space(stop) && (following == 0 || can_break(following, stop, style)) {
        if single {
            return Line {
                truncated: no_wrap,
                ..line
            };
        }

        let mut resume = end;
        while is_space(at(resume)) {
            resume += 1;
        }

        if no_wrap && !is_line_end(at(resume)) {
            return Line {
                truncated: true,
                next: skip_line(text, resume),
                ..line
            };
        }

        return Line {
            next: after_line_end(text, resume),
            ..line
        };
    }

    // With no accepted character there is no pair to break between.
    if prev.is_some_and(|prev| can_break(stop, prev, style)) || no_wrap {
        if no_wrap {
            return Line {
                truncated: true,
                next: skip_line(text, end),
                ..line
            };
        }

        return Line {
            next: Some(end),
            ..line
        };
    }

    for candidate in (start + 1..end).rev() {
        if can_break(at(candidate), at(candidate - 1), style) {
            let width = text[start..candidate]
                .iter()
                .fold(0i32, |width, code| width.wrapping_add(advance(*code)));

            let mut resume = candidate;
            while is_space(at(resume)) {
                resume += 1;
            }

            return Line {
                start,
                end: candidate,
                width,
                truncated: false,
                next: after_line_end(text, resume),
            };
        }
    }

    Line {
        next: Some(end),
        ..line
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use super::*;

    /// Advance of 10 for every character except narrow 'i' (4) and space (5).
    fn advance(code: u16) -> i32 {
        match code {
            code if code == u16::from(b'i') => 4,
            code if code == u16::from(b' ') => 5,
            _ => 10,
        }
    }

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn measure(
        text: &[u16],
        style: TextStyle,
        max_width: i32,
        pitch: LinePitch,
        advance: &mut dyn FnMut(u16) -> i32,
    ) -> Extent {
        measure_within(text, text.len(), style, max_width, pitch, advance)
    }

    fn render(text: &str, style: u32, width: i32, height: i32) -> Vec<String> {
        let units = utf16(text);
        let mut lines = Vec::new();
        let pitch = LinePitch {
            height: 20,
            adjust: 0,
        };
        place_lines(
            &units,
            TextStyle(style),
            TextRect { width, height },
            pitch,
            0.0,
            &mut advance,
            &mut lines,
        );

        lines
            .iter()
            .map(|line| {
                let mut shown = String::from_utf16_lossy(&units[line.start..line.end]);
                if line.ellipsis {
                    shown.push_str("...");
                }

                alloc::format!("x={} y={} [{shown}]", line.x, line.y)
            })
            .collect()
    }

    #[test]
    fn line_breaking() {
        let cases = vec![
            ("word wrap", render("the quick brown fox", 0, 100, 200)),
            (
                "trailing spaces trimmed for alignment",
                render("ab   cd", TextStyle::ALIGN_RIGHT, 100, 200),
            ),
            (
                "single line keeps trailing spaces",
                render(
                    "ab   ",
                    TextStyle::ALIGN_RIGHT | TextStyle::SINGLE_LINE,
                    100,
                    200,
                ),
            ),
            (
                "hard breaks",
                render("one\r\ntwo\nthree\rfour", 0, 100, 200),
            ),
            (
                "long word broken at fitted length",
                render("abcdefghijklmn", 0, 50, 200),
            ),
            ("break after hyphen class", render("abc)defghi", 0, 80, 200)),
            (
                "no wrap ellipsis",
                render(
                    "the quick brown fox",
                    TextStyle::ELLIPSIS | TextStyle::NO_WRAP,
                    100,
                    200,
                ),
            ),
            (
                "no wrap without ellipsis",
                render("the quick brown fox\nsecond", TextStyle::NO_WRAP, 100, 200),
            ),
            (
                "right no wrap clips start",
                render(
                    "the quick brown fox",
                    TextStyle::ALIGN_RIGHT | TextStyle::NO_WRAP,
                    100,
                    200,
                ),
            ),
            (
                "center",
                render("ab cd", TextStyle::CENTER_HORIZONTAL, 101, 200),
            ),
            (
                "vertical center",
                render("ab cd", TextStyle::CENTER_VERTICAL, 30, 101),
            ),
            ("stops below rect", render("aa bb cc dd ee ff", 0, 25, 30)),
            ("negative pitch adjust", {
                let units = utf16("aa bb");
                let mut lines = Vec::new();
                place_lines(
                    &units,
                    TextStyle(TextStyle::CENTER_VERTICAL),
                    TextRect {
                        width: 25,
                        height: 50,
                    },
                    LinePitch {
                        height: 20,
                        adjust: -30,
                    },
                    0.0,
                    &mut advance,
                    &mut lines,
                );
                lines
                    .iter()
                    .map(|line| alloc::format!("{line:?}"))
                    .collect()
            }),
        ];

        insta::assert_debug_snapshot!(cases);
    }

    #[test]
    fn line_starts_cover_every_wrapped_line() {
        let units = utf16("aa bb\ncc");
        let mut starts = Vec::new();
        line_starts(&units, TextStyle(0), 25, &mut advance, &mut starts);

        assert_eq!(starts, [0, 3, 6]);
    }

    #[test]
    fn scroll_places_lines_below_the_rect_height() {
        let units = utf16("aa bb cc dd");
        let pitch = LinePitch {
            height: 20,
            adjust: 0,
        };
        let mut lines = Vec::new();

        place_lines(
            &units,
            TextStyle(0),
            TextRect {
                width: 25,
                height: 30,
            },
            pitch,
            0.0,
            &mut advance,
            &mut lines,
        );
        assert_eq!(lines.len(), 2);

        place_lines(
            &units,
            TextStyle(0),
            TextRect {
                width: 25,
                height: 30,
            },
            pitch,
            15.0,
            &mut advance,
            &mut lines,
        );
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn measurement() {
        let pitch = LinePitch {
            height: 20,
            adjust: 2,
        };
        let cases = vec![
            (
                "single line measurer wraps",
                measure(
                    &utf16("the quick brown fox"),
                    TextStyle(0),
                    100,
                    pitch,
                    &mut advance,
                ),
            ),
            (
                "single line style",
                measure(
                    &utf16("the quick brown fox"),
                    TextStyle(TextStyle::SINGLE_LINE),
                    100,
                    pitch,
                    &mut advance,
                ),
            ),
            (
                "no wrap widens to full line",
                measure(
                    &utf16("the quick brown fox"),
                    TextStyle(TextStyle::NO_WRAP),
                    100,
                    pitch,
                    &mut advance,
                ),
            ),
            (
                "right no wrap widens to full line",
                measure(
                    &utf16("the quick brown fox"),
                    TextStyle(TextStyle::ALIGN_RIGHT | TextStyle::NO_WRAP),
                    100,
                    pitch,
                    &mut advance,
                ),
            ),
            (
                "multi line measurer",
                measure(
                    &utf16("ab\ncdef"),
                    TextStyle(TextStyle::MULTI_LINE_MEASURE),
                    100,
                    pitch,
                    &mut advance,
                ),
            ),
        ];

        insta::assert_debug_snapshot!(cases);
    }

    #[test]
    fn pixel_sums_wrap_in_32_bits() {
        let pitch = LinePitch {
            height: 20,
            adjust: i32::MAX,
        };
        let extent = measure(
            &utf16("ab\ncd"),
            TextStyle(TextStyle::MULTI_LINE_MEASURE),
            100,
            pitch,
            &mut advance,
        );

        // Two lines of pitch 20 + i32::MAX, which wraps to i32::MIN + 19.
        assert_eq!(extent.height, 38);
    }

    #[test]
    fn measurement_stops_at_the_limit() {
        let pitch = LinePitch {
            height: 20,
            adjust: 0,
        };
        let text = utf16("abc def\nghi");
        let measure = |limit: usize, style: u32, width: i32| {
            let extent = measure_within(&text, limit, TextStyle(style), width, pitch, &mut advance);
            (extent.width, extent.height)
        };

        // Lines end at the limit and none starts at or after it.
        assert_eq!(measure(2, 0, 100), (20, 20));
        assert_eq!(measure(8, 0, 100), (65, 20));
        assert_eq!(measure(9, 0, 100), (65, 40));
        assert_eq!(measure(0, 0, 100), (0, 0));
        assert_eq!(measure(2, TextStyle::MULTI_LINE_MEASURE, 100), (20, 20));
        // The second line of a wrap at the space ends at the limit.
        assert_eq!(measure(6, 0, 30), (30, 40));
        // Right-aligned clipping keeps the end of the line.
        let clip = TextStyle::ALIGN_RIGHT | TextStyle::NO_WRAP | TextStyle::SINGLE_LINE;
        assert_eq!(measure(2, clip, 0), (20, 20));
        assert_eq!(measure(5, clip, 0), (45, 20));
    }
}

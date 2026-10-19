//! Line break opportunities (`XuiFont_CanBreakBetween`) and the kinsoku
//! table they consult.

use crate::text::TextStyle;

const A: u8 = 1;
const B: u8 = 2;
const AB: u8 = A | B;

/// Kinsoku table in XUI's order. U+2236 precedes U+2122, so the binary
/// search never finds it; the search reproduces that.
static TABLE: [(u16, u8); 147] = [
    (0x0020, A),
    (0x0021, A),
    (0x0022, AB),
    (0x0024, B),
    (0x0025, A),
    (0x0027, AB),
    (0x0028, B),
    (0x0029, A),
    (0x002C, A),
    (0x002E, AB),
    (0x002F, AB),
    (0x003A, A),
    (0x003B, A),
    (0x003F, A),
    (0x005B, B),
    (0x005C, B),
    (0x005D, A),
    (0x007B, B),
    (0x007D, A),
    (0x00A2, A),
    (0x00A3, B),
    (0x00A5, B),
    (0x00A7, B),
    (0x00A8, A),
    (0x00A9, A),
    (0x00AE, A),
    (0x00B0, A),
    (0x00B7, AB),
    (0x02C7, A),
    (0x02C9, A),
    (0x2013, A),
    (0x2014, A),
    (0x2015, A),
    (0x2016, A),
    (0x2018, B),
    (0x2019, AB),
    (0x201C, B),
    (0x201D, A),
    (0x2022, A),
    (0x2025, A),
    (0x2026, A),
    (0x2027, A),
    (0x2032, A),
    (0x2033, A),
    (0x2035, B),
    (0x2103, A),
    (0x2236, A),
    (0x2122, A),
    (0x2574, A),
    (0x266F, B),
    (0x3001, A),
    (0x3002, A),
    (0x3003, A),
    (0x3005, A),
    (0x3008, B),
    (0x3009, A),
    (0x300A, B),
    (0x300B, A),
    (0x300C, B),
    (0x300D, A),
    (0x300E, B),
    (0x300F, A),
    (0x3010, B),
    (0x3011, A),
    (0x3012, B),
    (0x3014, B),
    (0x3015, A),
    (0x3016, B),
    (0x3017, A),
    (0x301D, B),
    (0x301E, A),
    (0x301F, A),
    (0x3041, A),
    (0x3043, A),
    (0x3045, A),
    (0x3047, A),
    (0x3049, A),
    (0x3063, A),
    (0x3083, A),
    (0x3085, A),
    (0x3087, A),
    (0x308E, A),
    (0x3099, A),
    (0x309A, A),
    (0x309B, A),
    (0x309C, A),
    (0x309D, A),
    (0x309E, A),
    (0x30A1, A),
    (0x30A3, A),
    (0x30A5, A),
    (0x30A7, A),
    (0x30A9, A),
    (0x30C3, A),
    (0x30E3, A),
    (0x30E5, A),
    (0x30E7, A),
    (0x30EE, A),
    (0x30F5, A),
    (0x30F6, A),
    (0x30FB, A),
    (0x30FC, A),
    (0x30FD, A),
    (0x30FE, A),
    (0xFE30, A),
    (0xFE50, A),
    (0xFE51, A),
    (0xFE52, A),
    (0xFE54, A),
    (0xFE55, A),
    (0xFE56, A),
    (0xFE57, A),
    (0xFE59, B),
    (0xFE5A, A),
    (0xFE5B, B),
    (0xFE5C, A),
    (0xFE5D, B),
    (0xFE5E, A),
    (0xFF01, A),
    (0xFF02, A),
    (0xFF04, B),
    (0xFF05, A),
    (0xFF07, A),
    (0xFF08, B),
    (0xFF09, A),
    (0xFF0C, A),
    (0xFF0E, A),
    (0xFF1A, A),
    (0xFF1B, A),
    (0xFF1F, A),
    (0xFF20, B),
    (0xFF3B, B),
    (0xFF3D, A),
    (0xFF40, A),
    (0xFF5B, B),
    (0xFF5C, A),
    (0xFF5D, A),
    (0xFF5E, A),
    (0xFF61, A),
    (0xFF64, A),
    (0xFF70, A),
    (0xFF9E, A),
    (0xFF9F, A),
    (0xFFE0, AB),
    (0xFFE1, B),
    (0xFFE5, B),
    (0xFFE6, B),
];

/// Kinsoku flags of one character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Kinsoku {
    /// Column A: break allowed after it as prev, no break before it as next.
    class_a: bool,
    /// Column B: no break after it as prev.
    no_break_after: bool,
}

pub(crate) fn is_space(code: u16) -> bool {
    code == 0x20 || code == 0x3000
}

/// Whether a line may break between `prev` and `next`.
pub(crate) fn can_break(next: u16, prev: u16, style: TextStyle) -> bool {
    if is_space(next) && is_space(prev) {
        return true;
    }

    let decimal_separator = matches!(prev, 0x2C | 0x2E | 0x3A | 0xFF0C | 0xFF0E | 0xFF1A);
    let digit = matches!(next, 0x30..=0x39 | 0xFF10..=0xFF19);
    if decimal_separator && digit {
        return false;
    }

    if prev == 0x2019 && next >= 0x3040 {
        return true;
    }

    let prev_flags = lookup(prev);
    let next_flags = lookup(next);
    if prev_flags.no_break_after {
        return false;
    }

    let after_prev = prev_flags.class_a && !next_flags.class_a;

    if korean_rule(style) && matches!(next, 0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3) {
        return if is_space(prev) {
            !next_flags.class_a
        } else {
            after_prev
        };
    }

    if is_space(prev) || (0x3040..0xF000).contains(&prev) || prev >= 0xFF66 {
        return !next_flags.class_a;
    }

    after_prev
}

fn korean_rule(style: TextStyle) -> bool {
    // Styles without any Korean rule bit use center and right.
    let bits = if style.0 & 0x3F_0000 == 0 {
        style.0 | 0x14_0000
    } else {
        style.0
    };
    let style = TextStyle(bits);

    (style.contains(TextStyle::CENTER_HORIZONTAL) && style.contains(TextStyle::KOREAN_RULE_CENTER))
        || (style.contains(TextStyle::ALIGN_RIGHT) && style.contains(TextStyle::KOREAN_RULE_RIGHT))
        || (style.contains(TextStyle::KOREAN_LEFT_MARKER)
            && style.contains(TextStyle::KOREAN_RULE_LEFT))
}

/// `XuiFont_KinsokuLookup`: binary search with XUI's index arithmetic.
fn lookup(code: u16) -> Kinsoku {
    if code == 0 {
        return Kinsoku::default();
    }

    let mut low = 0i32;
    let mut high = TABLE.len() as i32 - 1;
    while low <= high {
        let middle = (low + high) / 2;
        let (entry, flags) = TABLE[middle as usize];
        if entry == code {
            return Kinsoku {
                class_a: flags & A != 0,
                no_break_after: flags & B != 0,
            };
        }

        if entry > code {
            high = middle - 1;
        } else {
            low = middle + 1;
        }
    }

    Kinsoku::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverted_entries_follow_xui_search() {
        let unreachable: alloc::vec::Vec<u16> = TABLE
            .iter()
            .map(|(code, _)| *code)
            .filter(|code| lookup(*code) == Kinsoku::default())
            .collect();

        insta::assert_debug_snapshot!(unreachable);
    }

    #[test]
    fn break_rules() {
        let style = TextStyle(0);
        let cases = [
            ("space after word", can_break(0x20, u16::from(b'a'), style)),
            (
                "letter after letter",
                can_break(u16::from(b'b'), u16::from(b'a'), style),
            ),
            (
                "letter after space",
                can_break(u16::from(b'a'), 0x20, style),
            ),
            (
                "comma before space",
                can_break(0x20, u16::from(b','), style),
            ),
            (
                "digit after decimal point",
                can_break(u16::from(b'5'), u16::from(b'.'), style),
            ),
            (
                "letter after open paren",
                can_break(u16::from(b'a'), u16::from(b'('), style),
            ),
            (
                "close paren after letter",
                can_break(u16::from(b')'), u16::from(b'a'), style),
            ),
            ("kanji after kanji", can_break(0x4E01, 0x4E00, style)),
            (
                "ideographic full stop after kanji",
                can_break(0x3002, 0x4E00, style),
            ),
            (
                "hangul after hangul, center",
                can_break(0xAC01, 0xAC00, TextStyle(TextStyle::CENTER_HORIZONTAL)),
            ),
            (
                "hangul after space, center",
                can_break(0xAC01, 0x20, TextStyle(TextStyle::CENTER_HORIZONTAL)),
            ),
            (
                "hangul after hangul, left",
                can_break(0xAC01, 0xAC00, style),
            ),
        ];

        insta::assert_debug_snapshot!(cases);
    }
}

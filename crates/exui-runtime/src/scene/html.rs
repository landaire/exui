//! Text extraction for the XUI HTML renderer.

use alloc::vec::Vec;

const AMP: u16 = b'&' as u16;
const LT: u16 = b'<' as u16;
const GT: u16 = b'>' as u16;
const SPACE: u16 = b' ' as u16;
const NEWLINE: u16 = b'\n' as u16;

/// Converts the HTML subset accepted by the dashboard parser to text for the
/// runtime text renderer. Formatting tags are removed, block boundaries and
/// `br` become line breaks, and the five entities in the original token table
/// are decoded.
pub(crate) fn plain_text(input: &[u16], output: &mut Vec<u16>) {
    output.clear();
    let mut position = 0;
    let mut pending_space = false;
    let mut skipped: Option<&[u16]> = None;

    while position < input.len() {
        if let Some(name) = skipped {
            if input[position] == LT
                && input.get(position + 1) == Some(&(b'/' as u16))
                && let Some(end) = tag_end(input, position + 2)
            {
                let closing = tag(input, position + 1, end);
                if closing.closing && units_eq(closing.name, name) {
                    skipped = None;
                    position = end + 1;
                    continue;
                }
            }
            position += 1;
            continue;
        }

        if input[position] == LT {
            if input.get(position + 1..position + 4) == Some(&[33, 45, 45]) {
                position = comment_end(input, position + 4).unwrap_or(input.len());
                continue;
            }
            let Some(end) = tag_end(input, position + 1) else {
                push_text(LT, output, &mut pending_space);
                position += 1;
                continue;
            };
            let tag = tag(input, position + 1, end);
            position = end + 1;

            if tag.comment {
                continue;
            }

            if !tag.closing && is_skipped(tag.name) {
                skipped = Some(tag.name);
                continue;
            }
            if ascii_eq(tag.name, b"br") {
                push_newline(output);
                pending_space = false;
            } else if ascii_eq(tag.name, b"td") && !tag.closing {
                pending_space = !output.is_empty() && output.last() != Some(&NEWLINE);
            } else if is_block(tag.name) {
                push_newline(output);
                pending_space = false;
            }
            continue;
        }

        if input[position] == AMP
            && let Some((value, next)) = entity(input, position + 1)
        {
            push_text(value, output, &mut pending_space);
            position = next;
            continue;
        }

        push_text(input[position], output, &mut pending_space);
        position += 1;
    }

    while matches!(output.last(), Some(&SPACE | &NEWLINE)) {
        output.pop();
    }
}

#[derive(Clone, Copy)]
struct Tag<'a> {
    name: &'a [u16],
    closing: bool,
    comment: bool,
}

fn tag(input: &[u16], start: usize, end: usize) -> Tag<'_> {
    let mut position = start;
    while position < end && is_space(input[position]) {
        position += 1;
    }
    let comment = input.get(position..position + 3) == Some(&[33, 45, 45]);
    let closing = input.get(position) == Some(&(b'/' as u16));
    if closing {
        position += 1;
        while position < end && is_space(input[position]) {
            position += 1;
        }
    }
    let name_start = position;
    while position < end && !is_space(input[position]) && input[position] != b'/' as u16 {
        position += 1;
    }
    Tag {
        name: &input[name_start..position],
        closing,
        comment,
    }
}

fn tag_end(input: &[u16], mut position: usize) -> Option<usize> {
    let mut quote = None;
    while position < input.len() {
        let unit = input[position];
        if let Some(delimiter) = quote {
            if unit == delimiter {
                quote = None;
            }
        } else if unit == b'\'' as u16 || unit == b'"' as u16 {
            quote = Some(unit);
        } else if unit == GT {
            return Some(position);
        }
        position += 1;
    }
    None
}

fn comment_end(input: &[u16], mut position: usize) -> Option<usize> {
    while position + 2 < input.len() {
        if input[position..position + 3] == [45, 45, GT] {
            return Some(position + 3);
        }
        position += 1;
    }
    None
}

fn entity(input: &[u16], start: usize) -> Option<(u16, usize)> {
    let relative_end = input
        .get(start..)?
        .iter()
        .position(|unit| *unit == b';' as u16)?;
    if relative_end > 5 {
        return None;
    }
    let end = start + relative_end;
    let value = if ascii_eq(&input[start..end], b"nbsp") {
        0x00a0
    } else if ascii_eq(&input[start..end], b"quot") {
        b'"' as u16
    } else if ascii_eq(&input[start..end], b"gt") {
        GT
    } else if ascii_eq(&input[start..end], b"lt") {
        LT
    } else if ascii_eq(&input[start..end], b"amp") {
        AMP
    } else {
        return None;
    };
    Some((value, end + 1))
}

fn push_text(unit: u16, output: &mut Vec<u16>, pending_space: &mut bool) {
    if is_space(unit) {
        *pending_space = !output.is_empty() && output.last() != Some(&NEWLINE);
        return;
    }
    if *pending_space {
        output.push(SPACE);
        *pending_space = false;
    }
    output.push(unit);
}

fn push_newline(output: &mut Vec<u16>) {
    while output.last() == Some(&SPACE) {
        output.pop();
    }
    if !output.is_empty() && output.last() != Some(&NEWLINE) {
        output.push(NEWLINE);
    }
}

fn is_space(unit: u16) -> bool {
    matches!(unit, 0x09..=0x0d | 0x20)
}

fn is_block(name: &[u16]) -> bool {
    [
        b"body".as_slice(),
        b"div",
        b"h1",
        b"h2",
        b"h3",
        b"tr",
        b"table",
    ]
    .iter()
    .any(|candidate| ascii_eq(name, candidate))
}

fn is_skipped(name: &[u16]) -> bool {
    [b"head".as_slice(), b"script", b"style"]
        .iter()
        .any(|candidate| ascii_eq(name, candidate))
}

fn units_eq(left: &[u16], right: &[u16]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            let lower = |unit: u16| {
                if (b'A' as u16..=b'Z' as u16).contains(&unit) {
                    unit + (b'a' - b'A') as u16
                } else {
                    unit
                }
            };
            lower(*left) == lower(*right)
        })
}

fn ascii_eq(units: &[u16], ascii: &[u8]) -> bool {
    units.len() == ascii.len()
        && units.iter().zip(ascii).all(|(unit, byte)| {
            let unit = if (b'A' as u16..=b'Z' as u16).contains(unit) {
                unit + (b'a' - b'A') as u16
            } else {
                *unit
            };
            let byte = byte.to_ascii_lowercase() as u16;
            unit == byte
        })
}

#[cfg(test)]
mod tests {
    use super::plain_text;
    use alloc::string::String;
    use alloc::vec::Vec;

    fn normalize(value: &str) -> String {
        let input: Vec<u16> = value.encode_utf16().collect();
        let mut output = Vec::new();
        plain_text(&input, &mut output);
        String::from_utf16(&output).unwrap()
    }

    #[test]
    fn extracts_dashboard_html_text() {
        assert_eq!(
            normalize(
                "<head><style>x</style></head><BODY color='#fff'> Hello&nbsp;Xbox<div>A &amp; B<br/>C</div><table><tr><td>one</td><td>two</td></tr></table><script>bad</script>"
            ),
            "Hello\u{a0}Xbox\nA & B\nC\none two"
        );
    }

    #[test]
    fn raw_text_comparisons_do_not_consume_the_closing_tag() {
        assert_eq!(
            normalize("<script>if (a < b) x();</script><div>visible</div>"),
            "visible"
        );
    }

    #[test]
    fn comments_end_at_the_closing_delimiter() {
        assert_eq!(normalize("<!-- 1 > 0 -->Visible"), "Visible");
    }

    #[test]
    fn preserves_text_from_incomplete_markup() {
        assert_eq!(normalize("a < b &unknown; c"), "a < b &unknown; c");
    }
}

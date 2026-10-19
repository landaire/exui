//! UTF-16 strings as stored by XUI files.

use alloc::vec::Vec;
use core::char::DecodeUtf16;
use core::fmt;
use core::iter::Map;
use core::slice::Iter;

/// Code units of a [`WideStr`].
pub type Units<'a> = Map<Iter<'a, [u8; 2]>, fn(&[u8; 2]) -> u16>;

/// Borrowed big-endian UTF-16 string. It ends before the first NUL unit, as
/// the original uses these strings as C strings. Units are not validated.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WideStr<'a>(&'a [[u8; 2]]);

impl<'a> WideStr<'a> {
    /// `bytes` holds big-endian units; a trailing odd byte is ignored.
    pub fn from_be_bytes(bytes: &'a [u8]) -> Self {
        let units = bytes.as_chunks::<2>().0;
        let end = units
            .iter()
            .position(|unit| *unit == [0, 0])
            .unwrap_or(units.len());

        Self(&units[..end])
    }

    pub fn units(self) -> Units<'a> {
        self.0.iter().map(unit_from_be as fn(&[u8; 2]) -> u16)
    }

    /// Length in code units.
    pub fn len(self) -> usize {
        self.0.len()
    }

    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    pub fn chars(self) -> DecodeUtf16<Units<'a>> {
        char::decode_utf16(self.units())
    }

    pub fn eq_str(self, text: &str) -> bool {
        self.units().eq(text.encode_utf16())
    }

    pub fn to_wide_string(self) -> WideString {
        WideString(self.units().collect())
    }
}

impl fmt::Display for WideStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_units(f, self.units())
    }
}

impl fmt::Debug for WideStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_units(f, self.units())
    }
}

/// Owned UTF-16 code units.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct WideString(Vec<u16>);

impl WideString {
    pub fn units(&self) -> &[u16] {
        &self.0
    }

    pub fn eq_str(&self, text: &str) -> bool {
        self.0.iter().copied().eq(text.encode_utf16())
    }
}

impl From<&str> for WideString {
    fn from(text: &str) -> Self {
        Self(text.encode_utf16().collect())
    }
}

impl fmt::Display for WideString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_units(f, self.0.iter().copied())
    }
}

impl fmt::Debug for WideString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_units(f, self.0.iter().copied())
    }
}

fn unit_from_be(unit: &[u8; 2]) -> u16 {
    u16::from_be_bytes(*unit)
}

pub(crate) fn display_units(
    f: &mut fmt::Formatter<'_>,
    units: impl Iterator<Item = u16>,
) -> fmt::Result {
    write_ascii(f, units, false)
}

pub(crate) fn debug_units(
    f: &mut fmt::Formatter<'_>,
    units: impl Iterator<Item = u16>,
) -> fmt::Result {
    f.write_str("\"")?;
    write_ascii(f, units, true)?;
    f.write_str("\"")
}

/// Writes printable ASCII as is and everything else as `\u{XXXX}`, so output
/// stays ASCII. Unpaired surrogates print as their code unit.
fn write_ascii(
    f: &mut fmt::Formatter<'_>,
    units: impl Iterator<Item = u16>,
    escape_quotes: bool,
) -> fmt::Result {
    for decoded in char::decode_utf16(units) {
        match decoded {
            Ok(c) if c == '"' || c == '\\' => {
                if escape_quotes {
                    f.write_str("\\")?;
                }
                fmt::Write::write_char(f, c)?;
            }
            Ok(c) if c.is_ascii_graphic() || c == ' ' => fmt::Write::write_char(f, c)?,
            Ok(c) => write!(f, "\\u{{{:04X}}}", u32::from(c))?,
            Err(err) => write!(f, "\\u{{{:04X}}}", err.unpaired_surrogate())?,
        }
    }

    Ok(())
}

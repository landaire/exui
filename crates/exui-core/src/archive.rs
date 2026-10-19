//! XUIZ archive reader.
//!
//! Version 1 follows `XuiZipArchiveOpen` (0x921737a8) and `XuiZipOpenFile`
//! (0x92173498) of dash 6717; version 3 follows the XUIZ reader of
//! `xam.xex` 2.0.21256.0 (0x81980120). See doc/xui-format.md. The reader
//! borrows the archive bytes and decodes entries on demand.

use core::fmt;
use core::str::EncodeUtf16;

use crate::cursor::Cursor;
use crate::cursor::Short;
use crate::error::ArchiveError;
use crate::wide::Units;
use crate::wide::WideStr;
use crate::wide::debug_units;
use crate::wide::display_units;

const HEADER_SIZE: usize = 0x16;
const ENTRY_FIXED_SIZE: usize = 9;
const TRAILER_SIZE: usize = 8;

/// XUIZ format version. Each XUI build reads one version only: dash 6717
/// reads version 1 and `xam.xex` 2.0.21256.0 reads version 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArchiveVersion {
    /// Names are UTF-16.
    V1,
    /// Names are UTF-8.
    V3,
}

impl ArchiveVersion {
    /// The version word of the header.
    pub const fn number(self) -> u32 {
        match self {
            Self::V1 => 1,
            Self::V3 => 3,
        }
    }

    const fn from_number(number: u32) -> Option<Self> {
        match number {
            1 => Some(Self::V1),
            3 => Some(Self::V3),
            _ => None,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::V1 => 0x1,
            Self::V3 => 0x2,
        }
    }
}

/// The versions [`Archive::parse`] accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AcceptedVersions(u8);

impl AcceptedVersions {
    /// What the XUI of dash 6717 accepts.
    pub const DASH_6717: Self = Self::only(ArchiveVersion::V1);
    /// What the XUI of `xam.xex` 2.0.21256.0 accepts.
    pub const XAM_21256: Self = Self::only(ArchiveVersion::V3);
    /// Every version this reader knows.
    pub const ALL: Self = Self::DASH_6717.union(Self::XAM_21256);

    pub const fn only(version: ArchiveVersion) -> Self {
        Self(version.bit())
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, version: ArchiveVersion) -> bool {
        self.0 & version.bit() != 0
    }
}

/// Archive header flag word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveFlags(u32);

impl ArchiveFlags {
    /// Each entry's data is followed by two words that the reader loads into
    /// the entry record.
    pub const ENTRY_TRAILERS: Self = Self(0x1);

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Byte offset of entry data from the end of the entry table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntryOffset(pub u32);

/// Entry data size in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntrySize(pub u32);

/// The two words stored after an entry's data when
/// [`ArchiveFlags::ENTRY_TRAILERS`] is set. Their use is not known. Version
/// 3 archives store a FILETIME there, followed by more bytes that the reader
/// does not load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryTrailer {
    pub word0: u32,
    pub word1: u32,
}

/// Entry name as stored. It ends before the first NUL, as the original uses
/// names as C strings.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryName<'a> {
    /// Version 1: big-endian UTF-16.
    Utf16(WideStr<'a>),
    /// Version 3: UTF-8, which the original converts to UTF-16 for lookup.
    Utf8(&'a str),
}

impl<'a> EntryName<'a> {
    /// The name as UTF-16 code units.
    pub fn units(self) -> NameUnits<'a> {
        match self {
            Self::Utf16(name) => NameUnits::Utf16(name.units()),
            Self::Utf8(name) => NameUnits::Utf8(name.encode_utf16()),
        }
    }

    pub fn eq_str(self, text: &str) -> bool {
        self.units().eq(text.encode_utf16())
    }
}

impl fmt::Display for EntryName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_units(f, self.units())
    }
}

impl fmt::Debug for EntryName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_units(f, self.units())
    }
}

/// Code units of an [`EntryName`].
#[derive(Clone, Debug)]
pub enum NameUnits<'a> {
    Utf16(Units<'a>),
    Utf8(EncodeUtf16<'a>),
}

impl Iterator for NameUnits<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<u16> {
        match self {
            Self::Utf16(units) => units.next(),
            Self::Utf8(units) => units.next(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    name: EntryName<'a>,
    offset: EntryOffset,
    size: EntrySize,
    trailer: Option<EntryTrailer>,
}

impl<'a> Entry<'a> {
    /// Name as stored. Lookups compare it after the original's
    /// normalization (ASCII letters lowercased, backslashes as slashes).
    pub fn name(&self) -> EntryName<'a> {
        self.name
    }

    pub fn offset(&self) -> EntryOffset {
        self.offset
    }

    pub fn size(&self) -> EntrySize {
        self.size
    }

    pub fn trailer(&self) -> Option<EntryTrailer> {
        self.trailer
    }

    fn matches(&self, path: impl Iterator<Item = u16>) -> bool {
        self.name
            .units()
            .map(normalize_unit)
            .eq(path.map(normalize_unit))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Archive<'a> {
    data: &'a [u8],
    version: ArchiveVersion,
    flags: ArchiveFlags,
    table: &'a [u8],
    entry_count: u16,
}

impl<'a> Archive<'a> {
    /// Validates the header and every entry record without allocating.
    /// Versions outside `accepted` fail with
    /// [`ArchiveError::UnsupportedVersion`].
    pub fn parse(data: &'a [u8], accepted: AcceptedVersions) -> Result<Self, ArchiveError> {
        // The original reads the whole header before checking any field.
        let Some(header_bytes) = data.get(..HEADER_SIZE) else {
            return Err(ArchiveError::Truncated {
                offset: 0,
                needed: HEADER_SIZE,
            });
        };
        let mut header = Cursor::new(header_bytes, 0);

        let magic = header.bytes(4).map_err(truncated)?;
        if magic != b"XUIZ" {
            let mut found = [0; 4];
            found.copy_from_slice(magic);
            return Err(ArchiveError::BadMagic { found });
        }

        let version_number = header.u32().map_err(truncated)?;
        let version = ArchiveVersion::from_number(version_number)
            .filter(|version| accepted.contains(*version))
            .ok_or(ArchiveError::UnsupportedVersion {
                version: version_number,
            })?;

        let file_size = header.u32().map_err(truncated)?;
        if usize::try_from(file_size).ok() != Some(data.len()) {
            return Err(ArchiveError::FileSizeMismatch {
                header: file_size,
                actual: data.len(),
            });
        }

        let flags = ArchiveFlags(header.u32().map_err(truncated)?);
        let table_size = header.u32().map_err(truncated)? as usize;
        let entry_count = header.u16().map_err(truncated)?;

        let table = HEADER_SIZE
            .checked_add(table_size)
            .and_then(|data_base| data.get(HEADER_SIZE..data_base));
        let Some(table) = table else {
            return Err(ArchiveError::Truncated {
                offset: HEADER_SIZE,
                needed: table_size,
            });
        };

        let archive = Self {
            data,
            version,
            flags,
            table,
            entry_count,
        };

        let mut cursor = Cursor::new(table, HEADER_SIZE);
        for _ in 0..entry_count {
            archive.read_entry(&mut cursor)?;
        }

        Ok(archive)
    }

    pub fn version(&self) -> ArchiveVersion {
        self.version
    }

    pub fn flags(&self) -> ArchiveFlags {
        self.flags
    }

    pub fn entry_count(&self) -> u16 {
        self.entry_count
    }

    /// Entries in table order.
    pub fn entries(&self) -> Entries<'a> {
        Entries {
            archive: *self,
            cursor: Cursor::new(self.table, HEADER_SIZE),
            remaining: self.entry_count,
        }
    }

    /// First entry whose normalized name equals the normalized `path`. The
    /// path ends at its first NUL, as a C string would.
    pub fn find(&self, path: &str) -> Option<Entry<'a>> {
        self.find_units(path.encode_utf16())
    }

    pub fn find_utf16(&self, path: &[u16]) -> Option<Entry<'a>> {
        self.find_units(path.iter().copied())
    }

    /// Data of the entry at `path`, with the original's error order: empty
    /// path, then lookup.
    pub fn open(&self, path: &str) -> Result<&'a [u8], ArchiveError> {
        self.open_units(path.encode_utf16())
    }

    pub fn open_utf16(&self, path: &[u16]) -> Result<&'a [u8], ArchiveError> {
        self.open_units(path.iter().copied())
    }

    /// The original does not check the range when opening; reading past the
    /// end fails later. This reader checks it here.
    pub fn entry_data(&self, entry: &Entry<'a>) -> Result<&'a [u8], ArchiveError> {
        self.range(entry.offset, entry.size.0 as usize)
            .ok_or(ArchiveError::EntryOutOfBounds {
                offset: entry.offset,
                size: entry.size,
            })
    }

    fn data_base(&self) -> usize {
        HEADER_SIZE + self.table.len()
    }

    /// Bytes at `offset` from the data base, `len` long.
    fn range(&self, offset: EntryOffset, len: usize) -> Option<&'a [u8]> {
        let start = self.data_base().checked_add(offset.0 as usize)?;
        let end = start.checked_add(len)?;

        self.data.get(start..end)
    }

    fn read_entry(&self, cursor: &mut Cursor<'a>) -> Result<Entry<'a>, ArchiveError> {
        let entry_offset = cursor.offset();
        let overrun = ArchiveError::EntryTableOverrun {
            offset: entry_offset,
        };

        if cursor.remaining() < ENTRY_FIXED_SIZE {
            return Err(overrun);
        }

        let size = EntrySize(cursor.u32().map_err(truncated)?);
        let offset = EntryOffset(cursor.u32().map_err(truncated)?);
        let name_len = usize::from(cursor.u8().map_err(truncated)?);
        let name = match self.version {
            ArchiveVersion::V1 => {
                let bytes = cursor.bytes(name_len * 2).map_err(|_| overrun)?;
                EntryName::Utf16(WideStr::from_be_bytes(bytes))
            }
            ArchiveVersion::V3 => {
                let bytes = cursor.bytes(name_len).map_err(|_| overrun)?;
                // How the original converts malformed UTF-8 is not known, so
                // such names are refused.
                let text = core::str::from_utf8(bytes).map_err(|_| ArchiveError::InvalidName {
                    offset: entry_offset,
                })?;
                EntryName::Utf8(text.split('\0').next().unwrap_or(text))
            }
        };

        let trailer = if self.flags.contains(ArchiveFlags::ENTRY_TRAILERS) {
            Some(self.read_trailer(offset, size)?)
        } else {
            None
        };

        Ok(Entry {
            name,
            offset,
            size,
            trailer,
        })
    }

    fn read_trailer(
        &self,
        offset: EntryOffset,
        size: EntrySize,
    ) -> Result<EntryTrailer, ArchiveError> {
        let out_of_bounds = ArchiveError::TrailerOutOfBounds { offset, size };
        let Some(trailer_offset) = offset.0.checked_add(size.0) else {
            return Err(out_of_bounds);
        };
        let Some(bytes) = self.range(EntryOffset(trailer_offset), TRAILER_SIZE) else {
            return Err(out_of_bounds);
        };

        let mut cursor = Cursor::new(bytes, 0);
        let word0 = cursor.u32().map_err(|_| out_of_bounds.clone())?;
        let word1 = cursor.u32().map_err(|_| out_of_bounds.clone())?;

        Ok(EntryTrailer { word0, word1 })
    }

    fn open_units(
        &self,
        path: impl Iterator<Item = u16> + Clone,
    ) -> Result<&'a [u8], ArchiveError> {
        if path.clone().take_while(|unit| *unit != 0).next().is_none() {
            return Err(ArchiveError::EmptyPath);
        }

        let entry = self.find_units(path).ok_or(ArchiveError::NotFound)?;

        self.entry_data(&entry)
    }

    fn find_units(&self, path: impl Iterator<Item = u16> + Clone) -> Option<Entry<'a>> {
        let terminated = path.take_while(|unit| *unit != 0);

        // An empty path matches nothing, as `open` rejects it.
        terminated.clone().next()?;

        self.entries()
            .find(|entry| entry.matches(terminated.clone()))
    }
}

/// Iterator over the entries of an [`Archive`].
#[derive(Clone, Debug)]
pub struct Entries<'a> {
    archive: Archive<'a>,
    cursor: Cursor<'a>,
    remaining: u16,
}

impl<'a> Iterator for Entries<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.remaining = self.remaining.checked_sub(1)?;

        // `Archive::parse` validated every record, so decoding cannot fail.
        self.archive.read_entry(&mut self.cursor).ok()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::from(self.remaining);

        (remaining, Some(remaining))
    }
}

/// `_wcslwr_s` in the C locale plus backslash folding, as
/// `XuiZipNormalizePath` (0x92172ca8) applies to stored names and queries.
/// The version 3 reader of `xam.xex` 2.0.21256.0 applies the same (0x8197f4c8).
fn normalize_unit(unit: u16) -> u16 {
    match unit {
        0x41..=0x5A => unit + 0x20,
        0x5C => 0x2F,
        _ => unit,
    }
}

fn truncated(short: Short) -> ArchiveError {
    ArchiveError::Truncated {
        offset: short.offset,
        needed: short.needed,
    }
}

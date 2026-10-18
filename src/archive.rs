//! XUIZ archives of version 1 (read by exui-core) and version 3.
//!
//! Version 3 has the version 1 layout except that entry names are stored
//! as one byte per character. It is not taken from a loader; the layout is
//! the one observed in the archives that carry XUR version 8 files.

use exui_core::archive::Archive as Version1;

use crate::error::ArchiveError;
use crate::error::Version3Error;
use crate::scene::wide_to_string;

#[derive(Clone, Debug)]
pub enum Archive<'a> {
    Version1(Version1<'a>),
    Version3(Version3<'a>),
}

impl<'a> Archive<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, ArchiveError> {
        if data.starts_with(b"XUIZ") && data.get(4..8) == Some(&3u32.to_be_bytes()) {
            return Ok(Self::Version3(Version3::parse(data)?));
        }

        Ok(Self::Version1(Version1::parse(data)?))
    }

    pub fn version(&self) -> u32 {
        match self {
            Self::Version1(_) => 1,
            Self::Version3(_) => 3,
        }
    }

    /// Entries in table order.
    pub fn entries(&self) -> Result<Vec<Entry<'a>>, ArchiveError> {
        match self {
            Self::Version1(archive) => {
                let mut entries = Vec::with_capacity(usize::from(archive.entry_count()));
                for entry in archive.entries() {
                    entries.push(Entry {
                        name: wide_to_string(entry.name()),
                        data: archive.entry_data(&entry)?,
                    });
                }
                Ok(entries)
            }
            Self::Version3(archive) => Ok(archive.entries.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// Name as stored, with `\` separators.
    pub name: String,
    pub data: &'a [u8],
}

/// Version 3 archive with every entry decoded.
#[derive(Clone, Debug)]
pub struct Version3<'a> {
    entries: Vec<Entry<'a>>,
}

impl<'a> Version3<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Version3Error> {
        const HEADER_SIZE: usize = 0x16;
        const ENTRY_FIXED_SIZE: usize = 9;

        let Some(header) = data.get(..HEADER_SIZE) else {
            return Err(Version3Error::Truncated {
                offset: 0,
                needed: HEADER_SIZE,
            });
        };

        let file_size = be_u32(&header[0x8..]);
        if usize::try_from(file_size).ok() != Some(data.len()) {
            return Err(Version3Error::FileSizeMismatch {
                header: file_size,
                actual: data.len(),
            });
        }

        // The version 1 trailer flag is not known to apply here.
        let flags = be_u32(&header[0xC..]);
        if flags != 0 {
            return Err(Version3Error::UnsupportedFlags { flags });
        }

        let table_size = be_u32(&header[0x10..]) as usize;
        let entry_count = u16::from_be_bytes([header[0x14], header[0x15]]);

        let data_base = HEADER_SIZE + table_size;
        let Some(table) = data.get(HEADER_SIZE..data_base) else {
            return Err(Version3Error::Truncated {
                offset: HEADER_SIZE,
                needed: table_size,
            });
        };

        let mut entries = Vec::with_capacity(usize::from(entry_count));
        let mut position = 0;
        for _ in 0..entry_count {
            let overrun = Version3Error::EntryTableOverrun {
                offset: HEADER_SIZE + position,
            };
            let Some(fixed) = table.get(position..position + ENTRY_FIXED_SIZE) else {
                return Err(overrun);
            };

            let size = be_u32(fixed);
            let offset = be_u32(&fixed[4..]);
            let name_start = position + ENTRY_FIXED_SIZE;
            let name_end = name_start + usize::from(fixed[8]);
            let Some(name) = table.get(name_start..name_end) else {
                return Err(overrun);
            };
            position = name_end;

            let entry_data = data_base
                .checked_add(offset as usize)
                .and_then(|start| data.get(start..start.checked_add(size as usize)?));
            let Some(entry_data) = entry_data else {
                return Err(Version3Error::EntryOutOfBounds { offset, size });
            };

            entries.push(Entry {
                name: name.iter().map(|byte| char::from(*byte)).collect(),
                data: entry_data,
            });
        }

        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[Entry<'a>] {
        &self.entries
    }
}

/// `bytes` holds at least four bytes.
fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

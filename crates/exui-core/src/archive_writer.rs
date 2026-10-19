//! XUIZ archive writer. Entries are stored in the given order, each one's
//! data directly after the previous one's, as in the 6717 archives.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::archive::ArchiveVersion;
use crate::error::ArchiveWriteError;

/// One file to store.
#[derive(Clone, Copy, Debug)]
pub struct NewEntry<'a> {
    /// Path inside the archive, stored as given. Lookups ignore ASCII case
    /// and treat `\` as `/`.
    pub name: &'a str,
    pub data: &'a [u8],
}

/// Archive bytes of `version` holding `entries` without entry trailers.
/// Fails on names that lookup could never reach: empty, holding NUL,
/// longer than 255 units, or equal to an earlier name after normalization.
pub fn write_archive(
    version: ArchiveVersion,
    entries: &[NewEntry<'_>],
) -> Result<Vec<u8>, ArchiveWriteError> {
    const HEADER_SIZE: usize = 0x16;

    let Ok(count) = u16::try_from(entries.len()) else {
        return Err(ArchiveWriteError::TooManyEntries {
            count: entries.len(),
        });
    };

    let mut names: BTreeMap<Vec<u16>, usize> = BTreeMap::new();
    let mut table = Vec::new();
    let mut offset = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        if entry.name.is_empty() || entry.name.contains('\0') {
            return Err(ArchiveWriteError::InvalidName { index });
        }

        let name = normalized(entry.name);
        if let Some(&first) = names.get(&name) {
            return Err(ArchiveWriteError::DuplicateName { first, index });
        }
        names.insert(name, index);

        table.extend_from_slice(&word(entry.data.len())?.to_be_bytes());
        table.extend_from_slice(&word(offset)?.to_be_bytes());
        match version {
            ArchiveVersion::V1 => {
                let Ok(length) = u8::try_from(entry.name.encode_utf16().count()) else {
                    return Err(ArchiveWriteError::NameTooLong { index });
                };
                table.push(length);
                for unit in entry.name.encode_utf16() {
                    table.extend_from_slice(&unit.to_be_bytes());
                }
            }
            ArchiveVersion::V3 => {
                let Ok(length) = u8::try_from(entry.name.len()) else {
                    return Err(ArchiveWriteError::NameTooLong { index });
                };
                table.push(length);
                table.extend_from_slice(entry.name.as_bytes());
            }
        }
        offset += entry.data.len();
    }

    let total = HEADER_SIZE + table.len() + offset;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"XUIZ");
    out.extend_from_slice(&version.number().to_be_bytes());
    out.extend_from_slice(&word(total)?.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&word(table.len())?.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&table);
    for entry in entries {
        out.extend_from_slice(entry.data);
    }

    Ok(out)
}

/// `name` as lookup compares it: ASCII lowercase, `\` as `/`.
fn normalized(name: &str) -> Vec<u16> {
    name.encode_utf16()
        .map(|unit| match u8::try_from(unit) {
            Ok(b'\\') => u16::from(b'/'),
            Ok(byte) => u16::from(byte.to_ascii_lowercase()),
            Err(_) => unit,
        })
        .collect()
}

fn word(size: usize) -> Result<u32, ArchiveWriteError> {
    u32::try_from(size).map_err(|_| ArchiveWriteError::TooLarge)
}

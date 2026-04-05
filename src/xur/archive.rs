//! XUIZ archive format parser (sans-io).
//!
//! XUIZ is a flat resource package containing one or more files (XUR binaries,
//! PNG images, XMA audio, XUS string tables, etc.). No compression, no
//! directory tree -- just a flat file table followed by concatenated data.
//!
//! ## Format (all multi-byte values big-endian)
//!
//! ```text
//! +0x00  "XUIZ"  magic (4 bytes)
//! +0x04  u32     version (observed: 1)
//! +0x08  u32     total_file_size
//! +0x0C  u32     reserved (0)
//! +0x10  u32     file_table_data_size
//! +0x14  u16     entry_count
//! +0x16  entries[entry_count]
//!          each: u32(resource_size) + u32(field2) + u8(name_len) + name
//!          name encoding depends on version: v1 = UTF-16BE (name_len*2 bytes),
//!          v3 = single-byte ASCII/Latin-1 (name_len bytes)
//! +vary  resource data (concatenated)
//! ```
//!
//! ## Sans-IO design
//!
//! [`XuizArchive`] only parses the file table from a `&[u8]` header region.
//! It records each entry's byte range but does NOT hold a reference to the
//! underlying data. To read an entry's contents, the caller provides the
//! data source (mmap, file, network, etc.) and calls [`XuizArchive::read`]
//! with a `ReadAt` implementation.

use std::ops::Range;

use byteorder::BigEndian;
use byteorder::ByteOrder;

use super::XUIB_MAGIC;
use super::XUIZ_MAGIC;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ArchiveError {
    TooShort,
    BadMagic([u8; 4]),
    TruncatedEntry { index: usize },
    ReadError(std::io::Error),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "data too short for XUIZ header"),
            Self::BadMagic(m) => write!(f, "bad magic: {m:02x?}"),
            Self::TruncatedEntry { index } => write!(f, "truncated entry at index {index}"),
            Self::ReadError(e) => write!(f, "read error: {e}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<std::io::Error> for ArchiveError {
    fn from(e: std::io::Error) -> Self {
        Self::ReadError(e)
    }
}

// ---------------------------------------------------------------------------
// Sans-IO read trait
// ---------------------------------------------------------------------------

/// Trait for reading a byte range from the underlying data source.
/// Implementations might be: a memmap, a `&[u8]`, a `File` with seek+read, etc.
pub trait ReadAt {
    fn read_at(&self, range: Range<usize>) -> std::io::Result<&[u8]>;
}

/// Blanket impl for any contiguous byte slice.
impl ReadAt for [u8] {
    fn read_at(&self, range: Range<usize>) -> std::io::Result<&[u8]> {
        self.get(range).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "range out of bounds")
        })
    }
}

impl ReadAt for memmap2::Mmap {
    fn read_at(&self, range: Range<usize>) -> std::io::Result<&[u8]> {
        self.as_ref().read_at(range)
    }
}

// ---------------------------------------------------------------------------
// Archive entry
// ---------------------------------------------------------------------------

/// A single entry in the XUIZ file table.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Filename (decoded from UTF-16BE).
    pub name: String,
    /// Byte range within the archive data.
    pub range: Range<usize>,
}

impl Entry {
    /// Entry size in bytes.
    pub fn size(&self) -> usize {
        self.range.len()
    }
}

// ---------------------------------------------------------------------------
// Archive
// ---------------------------------------------------------------------------

/// A parsed XUIZ archive file table.
///
/// This struct is sans-io: it only stores metadata parsed from the header.
/// To read entry contents, use [`read`] with a `ReadAt` source.
#[derive(Debug, Clone)]
pub struct XuizArchive {
    pub version: u32,
    pub entries: Vec<Entry>,
}

const HEADER_SIZE: usize = 0x16; // 22 bytes: magic(4) + version(4) + total(4) + reserved(4) + table_size(4) + count(2)

impl XuizArchive {
    /// Parse the XUIZ file table from the archive header bytes.
    ///
    /// `data` must contain at least the full header + file table. It does NOT
    /// need to contain the resource data -- only the table is parsed here.
    pub fn parse(data: &[u8]) -> Result<Self, ArchiveError> {
        if data.len() < HEADER_SIZE {
            return Err(ArchiveError::TooShort);
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&data[0..4]);
        if &magic != XUIZ_MAGIC {
            return Err(ArchiveError::BadMagic(magic));
        }

        let version = BigEndian::read_u32(&data[4..8]);
        let table_data_size = BigEndian::read_u32(&data[0x10..0x14]) as usize;
        let entry_count = BigEndian::read_u16(&data[0x14..0x16]) as usize;

        // v1 stores entry names as UTF-16BE (2 bytes/char); v3 stores them as
        // single-byte ASCII/Latin-1. Branch on version so both decode correctly.
        let bytes_per_char = if version == 1 { 2 } else { 1 };

        let resource_base = HEADER_SIZE + table_data_size;

        let mut entries = Vec::with_capacity(entry_count);
        let mut pos = HEADER_SIZE;
        let mut resource_offset = resource_base;

        for i in 0..entry_count {
            if pos + 9 > data.len() {
                return Err(ArchiveError::TruncatedEntry { index: i });
            }

            let size = BigEndian::read_u32(&data[pos..pos + 4]) as usize;
            // data[pos+4..pos+8] is field2 (flags/checksum, unused)
            let name_chars = data[pos + 8] as usize;
            pos += 9;

            let name_bytes = name_chars * bytes_per_char;
            if pos + name_bytes > data.len() {
                return Err(ArchiveError::TruncatedEntry { index: i });
            }

            let mut name = String::with_capacity(name_chars);
            for j in 0..name_chars {
                let cu = if bytes_per_char == 2 {
                    BigEndian::read_u16(&data[pos + j * 2..pos + j * 2 + 2])
                } else {
                    u16::from(data[pos + j])
                };
                name.push(char::from_u32(u32::from(cu)).unwrap_or('\u{FFFD}'));
            }
            pos += name_bytes;

            entries.push(Entry {
                name,
                range: resource_offset..resource_offset + size,
            });
            resource_offset += size;
        }

        Ok(XuizArchive { version, entries })
    }

    /// Read an entry's raw bytes from the given data source.
    pub fn read<'a, S: ReadAt + ?Sized>(&self, source: &'a S, entry: &Entry) -> std::io::Result<&'a [u8]> {
        source.read_at(entry.range.clone())
    }

    /// Look up an entry by name.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Find the first entry that contains a XUIB binary (by extension or magic).
    pub fn find_xuib<S: ReadAt + ?Sized>(&self, source: &S) -> Option<&Entry> {
        // Prefer .xur extension
        for entry in &self.entries {
            if entry.name.ends_with(".xur") {
                if let Ok(data) = self.read(source, entry) {
                    if data.len() >= 4 && &data[0..4] == XUIB_MAGIC {
                        return Some(entry);
                    }
                }
            }
        }
        // Fallback: any entry starting with XUIB magic
        for entry in &self.entries {
            if let Ok(data) = self.read(source, entry) {
                if data.len() >= 4 && &data[0..4] == XUIB_MAGIC {
                    return Some(entry);
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Convenience helpers
// ---------------------------------------------------------------------------

/// Check if data starts with XUIZ magic.
pub fn is_xuiz(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == XUIZ_MAGIC
}

/// Check if data starts with XUIB magic.
pub fn is_xuib(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == XUIB_MAGIC
}

use alloc::vec::Vec;

use exui_extension::StringKey;

use crate::resource::Locator;
use crate::resource::ModuleHandle;
use crate::resource::OverlayError;
use crate::resource::ResourceError;
use crate::resource::Resources;
use crate::resource::StringTableError;

/// Position of a string in a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StringIndex(pub u32);

/// How the entries of a `.xus` file are keyed (format byte 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKeys {
    /// Flag 0x2: the entry position is the key.
    Index,
    /// Flag 0x1: each value is followed by a u32 id.
    Id,
    /// Neither flag: each value is followed by a key string.
    Name,
}

/// A loaded `.xus` string table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringTable {
    keys: EntryKeys,
    units: Vec<u16>,
    entries: Vec<Entry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    value: Span,
    key: Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Index,
    Id(u32),
    Name(Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: u32,
    len: u32,
}

impl StringTable {
    /// Parses a `.xus` file (`XuiStringTable_Load`): magic `XUIS`, format
    /// byte 1, flags byte, u16 entry count at 0xA, entries from 0xC, each
    /// a u16 length and big-endian UTF-16 units.
    pub fn parse(data: &[u8]) -> Result<Self, StringTableError> {
        const MAGIC: &[u8; 4] = b"XUIS";
        const FORMAT_OFFSET: usize = 4;
        const FLAGS_OFFSET: usize = 5;
        const COUNT_OFFSET: usize = 0xA;
        const ENTRIES_OFFSET: usize = 0xC;
        const SUPPORTED_FORMAT: u8 = 1;
        const FLAG_ID_KEYS: u8 = 0x1;
        const FLAG_INDEX_KEYS: u8 = 0x2;

        if data.get(..MAGIC.len()) != Some(MAGIC) {
            return Err(StringTableError::Magic);
        }
        let truncated = |entry| StringTableError::Truncated {
            entry,
            len: data.len(),
        };
        let (Some(&format), Some(&flags), Some(count)) = (
            data.get(FORMAT_OFFSET),
            data.get(FLAGS_OFFSET),
            data.get(COUNT_OFFSET..COUNT_OFFSET + size_of::<u16>()),
        ) else {
            return Err(truncated(0));
        };
        if format != SUPPORTED_FORMAT {
            return Err(StringTableError::Format { format });
        }

        let keys = if flags & FLAG_INDEX_KEYS != 0 {
            EntryKeys::Index
        } else if flags & FLAG_ID_KEYS != 0 {
            EntryKeys::Id
        } else {
            EntryKeys::Name
        };
        let count = u16::from_be_bytes([count[0], count[1]]);

        let mut table = Self {
            keys,
            units: Vec::new(),
            entries: Vec::with_capacity(usize::from(count)),
        };
        let mut offset = ENTRIES_OFFSET;
        for entry in 0..count {
            let value = table
                .read_string(data, &mut offset)
                .ok_or_else(|| truncated(entry))?;
            let key = match keys {
                EntryKeys::Index => Key::Index,
                EntryKeys::Id => {
                    let id = data
                        .get(offset..offset + size_of::<u32>())
                        .ok_or_else(|| truncated(entry))?;
                    offset += size_of::<u32>();
                    Key::Id(u32::from_be_bytes([id[0], id[1], id[2], id[3]]))
                }
                EntryKeys::Name => Key::Name(
                    table
                        .read_string(data, &mut offset)
                        .ok_or_else(|| truncated(entry))?,
                ),
            };
            table.entries.push(Entry { value, key });
        }

        Ok(table)
    }

    pub fn keys(&self) -> EntryKeys {
        self.keys
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The string at `index`.
    pub fn get(&self, index: StringIndex) -> Option<&[u16]> {
        let entry = self.entries.get(usize::try_from(index.0).ok()?)?;

        Some(self.slice(entry.value))
    }

    /// The value whose u32 id is `id` (tables with [`EntryKeys::Id`]).
    pub fn by_id(&self, id: u32) -> Option<&[u16]> {
        let entry = self.entries.iter().find(|entry| entry.key == Key::Id(id))?;

        Some(self.slice(entry.value))
    }

    /// The value whose key string is `name` (tables with [`EntryKeys::Name`]).
    pub fn by_name(&self, name: &[u16]) -> Option<&[u16]> {
        let entry = self
            .entries
            .iter()
            .find(|entry| matches!(entry.key, Key::Name(key) if self.slice(key) == name))?;

        Some(self.slice(entry.value))
    }

    /// `0x92170EA0`: each empty value takes the value at the same position
    /// in `base`. The tables must have the same entry count.
    fn fill_empty(&mut self, base: &Self) -> Result<(), StringTableError> {
        if self.entries.len() != base.entries.len() {
            return Err(StringTableError::FallbackCount {
                localized: self.entries.len(),
                base: base.entries.len(),
            });
        }

        for (target, source) in self.entries.iter_mut().zip(&base.entries) {
            if target.value.len != 0 || source.value.len == 0 {
                continue;
            }
            let start = self.units.len() as u32;
            self.units.extend_from_slice(base.slice(source.value));
            target.value = Span {
                start,
                len: source.value.len,
            };
        }

        Ok(())
    }

    /// Replaces the value of the entry with `key`, or adds one: an index
    /// one past the last entry, or an id or name the table lacks.
    pub fn set(&mut self, key: &StringKey, value: &str) -> Result<(), OverlayError> {
        let existing = match (key, self.keys) {
            (StringKey::Index(index), EntryKeys::Index) => {
                let len = self.entries.len();
                match usize::try_from(*index).ok().filter(|&index| index <= len) {
                    Some(index) if index < len => Some(index),
                    Some(_) => None,
                    None => return Err(OverlayError::IndexPastEnd { index: *index, len }),
                }
            }
            (StringKey::Id(id), EntryKeys::Id) => self
                .entries
                .iter()
                .position(|entry| entry.key == Key::Id(*id)),
            (StringKey::Name(name), EntryKeys::Name) => {
                let wanted: Vec<u16> = name.encode_utf16().collect();
                self.entries
                    .iter()
                    .position(|entry| matches!(entry.key, Key::Name(key) if self.slice(key) == wanted.as_slice()))
            }
            _ => return Err(OverlayError::KeyKind { table: self.keys }),
        };

        let start = self.units.len() as u32;
        self.units.extend(value.encode_utf16());
        let value = Span {
            start,
            len: self.units.len() as u32 - start,
        };

        match existing {
            Some(index) => self.entries[index].value = value,
            None => {
                let key = match key {
                    StringKey::Index(_) => Key::Index,
                    StringKey::Id(id) => Key::Id(*id),
                    StringKey::Name(name) => {
                        let start = self.units.len() as u32;
                        self.units.extend(name.encode_utf16());
                        Key::Name(Span {
                            start,
                            len: self.units.len() as u32 - start,
                        })
                    }
                };
                self.entries.push(Entry { value, key });
            }
        }

        Ok(())
    }

    fn slice(&self, span: Span) -> &[u16] {
        // Spans are created for this table's units only.
        &self.units[span.start as usize..][..span.len as usize]
    }

    fn read_string(&mut self, data: &[u8], offset: &mut usize) -> Option<Span> {
        let length = data.get(*offset..*offset + 2)?;
        let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
        let bytes = data.get(*offset + 2..*offset + 2 + 2 * length)?;
        *offset += 2 + 2 * length;

        let start = self.units.len() as u32;
        self.units.extend(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|unit| u16::from_be_bytes(*unit)),
        );

        Some(Span {
            start,
            len: length as u32,
        })
    }
}

impl Resources<'_> {
    /// Loads a string table (`XuiStringTableLoad` 0x92150AB8). With a
    /// locale set, `<locale>\<path>` of the same archive is loaded instead
    /// when it exists (0x9216C390); when the loaded table has index keys,
    /// its empty values are filled from the table at `path` (0x9216C528).
    pub fn string_table(&self, locator: &Locator) -> Result<StringTable, StringTableError> {
        let mut table = self.load_string_table(locator)?;
        self.apply_string_overlays(locator, &mut table)?;

        Ok(table)
    }

    /// Entries of enabled extensions for the table at `locator`.
    fn apply_string_overlays(
        &self,
        locator: &Locator,
        table: &mut StringTable,
    ) -> Result<(), StringTableError> {
        let (
            Some(extensions),
            Locator::Section {
                module: ModuleHandle::TITLE,
                section,
                path,
            },
        ) = (self.extensions(), locator)
        else {
            return Ok(());
        };

        for (name, overlay) in extensions.string_overlays(*section, path, self.locale()) {
            for entry in &overlay.entries {
                table
                    .set(&entry.key, &entry.value)
                    .map_err(|source| StringTableError::Overlay { name, source })?;
            }
        }

        Ok(())
    }

    fn load_string_table(&self, locator: &Locator) -> Result<StringTable, StringTableError> {
        let localized = match (self.locale(), locator) {
            (
                Some(locale),
                Locator::Section {
                    module,
                    section,
                    path,
                },
            ) => {
                let mut localized: Vec<u16> = locale.encode_utf16().collect();
                localized.push(u16::from(b'\\'));
                localized.extend_from_slice(path);
                match self.open_in_section(*module, *section, &localized) {
                    Ok(bytes) => Some(StringTable::parse(bytes)?),
                    Err(ResourceError::NotInArchive { .. }) => None,
                    Err(error) => return Err(error.into()),
                }
            }
            _ => None,
        };

        let Some(mut table) = localized else {
            return StringTable::parse(&self.read(locator)?);
        };
        if table.keys == EntryKeys::Index {
            table.fill_empty(&StringTable::parse(&self.read(locator)?)?)?;
        }

        Ok(table)
    }
}

//! XUIZ archives of every version exui-core reads, with entries decoded for
//! the command line.

use exui_core::AcceptedVersions;
use exui_core::ArchiveError;
use exui_core::archive::Archive as CoreArchive;

#[derive(Clone, Copy, Debug)]
pub struct Archive<'a>(CoreArchive<'a>);

impl<'a> Archive<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, ArchiveError> {
        CoreArchive::parse(data, AcceptedVersions::ALL).map(Self)
    }

    /// The version word of the header.
    pub fn version(&self) -> u32 {
        self.0.version().number()
    }

    /// Entries in table order.
    pub fn entries(&self) -> Result<Vec<Entry<'a>>, ArchiveError> {
        let mut entries = Vec::with_capacity(usize::from(self.0.entry_count()));
        for entry in self.0.entries() {
            entries.push(Entry {
                name: char::decode_utf16(entry.name().units())
                    .map(|decoded| decoded.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect(),
                data: self.0.entry_data(&entry)?,
            });
        }

        Ok(entries)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// Name as stored, with `\` separators.
    pub name: String,
    pub data: &'a [u8],
}

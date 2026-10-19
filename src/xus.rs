//! `.xus` string tables: the binary form, its XML text form and the
//! compiler back.
//!
//! Binary: magic `XUIS`, format byte 1, flags byte (0x1 a u32 id after each
//! value, 0x2 the position is the key, neither a key string after each
//! value), u32 file size, u16 entry count, then each entry as a u16 length
//! and big-endian UTF-16 units, followed by its key.

use std::fmt;

use crate::error::CompileError;
use crate::error::StringTableError;
use crate::error::StringTableWriteError;
use crate::markup;
use crate::markup::Node;
use crate::xui::Text;

const MAGIC: &[u8; 4] = b"XUIS";
const FORMAT: u8 = 1;
const FLAG_ID_KEYS: u8 = 0x1;
const FLAG_INDEX_KEYS: u8 = 0x2;
const HEADER_SIZE: usize = 12;

/// Root element of the text form.
const ROOT: &str = "XuiStringTable";
const ENTRY: &str = "String";

/// Key of a string in an id-keyed table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StringKey(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringTable {
    pub entries: Entries,
}

/// Entries in file order, keyed as the flags byte selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entries {
    /// Flag 0x2: the position is the key.
    Index(Vec<String>),
    /// Flag 0x1.
    Id(Vec<IdEntry>),
    /// No flag.
    Name(Vec<NameEntry>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdEntry {
    pub id: StringKey,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEntry {
    pub name: String,
    pub value: String,
}

impl StringTable {
    /// Reads a `.xus` file. Fails on anything the text form cannot
    /// reproduce: flags other than 0, 1 or 2, a size field that is not the
    /// file size, bytes after the entries and strings that are not UTF-16.
    pub fn parse(data: &[u8]) -> Result<Self, StringTableError> {
        let header = data
            .get(..HEADER_SIZE)
            .ok_or(StringTableError::Truncated { entry: 0 })?;
        if &header[..4] != MAGIC {
            let mut found = [0; 4];
            found.copy_from_slice(&header[..4]);
            return Err(StringTableError::BadMagic { found });
        }
        if header[4] != FORMAT {
            return Err(StringTableError::UnsupportedFormat { format: header[4] });
        }
        let size = u32::from_be_bytes([header[6], header[7], header[8], header[9]]);
        if usize::try_from(size).ok() != Some(data.len()) {
            return Err(StringTableError::FileSizeMismatch {
                header: size,
                actual: data.len(),
            });
        }
        let count = u16::from_be_bytes([header[10], header[11]]);

        let mut reader = Reader {
            data,
            offset: HEADER_SIZE,
        };
        let entries = match header[5] {
            FLAG_INDEX_KEYS => {
                let mut values = Vec::with_capacity(usize::from(count));
                for entry in 0..count {
                    values.push(reader.string(entry)?);
                }
                Entries::Index(values)
            }
            FLAG_ID_KEYS => {
                let mut entries = Vec::with_capacity(usize::from(count));
                for entry in 0..count {
                    let value = reader.string(entry)?;
                    let id = StringKey(reader.u32(entry)?);
                    entries.push(IdEntry { id, value });
                }
                Entries::Id(entries)
            }
            0 => {
                let mut entries = Vec::with_capacity(usize::from(count));
                for entry in 0..count {
                    let value = reader.string(entry)?;
                    let name = reader.string(entry)?;
                    entries.push(NameEntry { name, value });
                }
                Entries::Name(entries)
            }
            flags => return Err(StringTableError::UnsupportedFlags { flags }),
        };

        let trailing = data.len() - reader.offset;
        if trailing != 0 {
            return Err(StringTableError::TrailingData { count: trailing });
        }

        Ok(Self { entries })
    }

    /// Reads the text form [`StringTableXml`] writes.
    pub fn from_xml(text: &str) -> Result<Self, CompileError> {
        let root = markup::parse(text)?;
        if root.name != ROOT {
            return Err(CompileError::UnexpectedElement {
                line: root.line,
                element: root.name.clone(),
                parent: "the document".to_string(),
            });
        }
        root.expect_attributes(&["keys"])?;
        root.expect_no_text()?;

        let Some(kind) = root.attribute("keys") else {
            return Err(CompileError::MissingAttribute {
                line: root.line,
                element: root.name.clone(),
                attribute: "keys",
            });
        };
        let entries = match kind {
            "index" => {
                let mut values = Vec::with_capacity(root.children.len());
                for node in &root.children {
                    entry_node(node, &[])?;
                    values.push(node.text.clone());
                }
                Entries::Index(values)
            }
            "id" => {
                let mut entries = Vec::with_capacity(root.children.len());
                for node in &root.children {
                    let id = entry_node(node, &["id"])?;
                    let Ok(id) = id.trim().parse() else {
                        return Err(bad_attribute(node, "id", id));
                    };
                    entries.push(IdEntry {
                        id: StringKey(id),
                        value: node.text.clone(),
                    });
                }
                Entries::Id(entries)
            }
            "name" => {
                let mut entries = Vec::with_capacity(root.children.len());
                for node in &root.children {
                    let name = entry_node(node, &["name"])?;
                    entries.push(NameEntry {
                        name: name.to_string(),
                        value: node.text.clone(),
                    });
                }
                Entries::Name(entries)
            }
            other => return Err(bad_attribute(&root, "keys", other)),
        };

        Ok(Self { entries })
    }

    /// The `.xus` bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, StringTableWriteError> {
        let (flags, count) = match &self.entries {
            Entries::Index(values) => (FLAG_INDEX_KEYS, values.len()),
            Entries::Id(entries) => (FLAG_ID_KEYS, entries.len()),
            Entries::Name(entries) => (0, entries.len()),
        };
        let Ok(count) = u16::try_from(count) else {
            return Err(StringTableWriteError::TooManyEntries { count });
        };

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(FORMAT);
        out.push(flags);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&count.to_be_bytes());
        match &self.entries {
            Entries::Index(values) => {
                for (entry, value) in values.iter().enumerate() {
                    write_string(&mut out, value, entry)?;
                }
            }
            Entries::Id(entries) => {
                for (entry, IdEntry { id, value }) in entries.iter().enumerate() {
                    write_string(&mut out, value, entry)?;
                    out.extend_from_slice(&id.0.to_be_bytes());
                }
            }
            Entries::Name(entries) => {
                for (entry, NameEntry { name, value }) in entries.iter().enumerate() {
                    write_string(&mut out, value, entry)?;
                    write_string(&mut out, name, entry)?;
                }
            }
        }

        let Ok(size) = u32::try_from(out.len()) else {
            return Err(StringTableWriteError::TooLarge);
        };
        out[6..10].copy_from_slice(&size.to_be_bytes());

        Ok(out)
    }
}

/// Writes a string table as XML through [`fmt::Display`].
pub struct StringTableXml<'t>(pub &'t StringTable);

impl fmt::Display for StringTableXml<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0.entries {
            Entries::Index(values) => {
                writeln!(f, "<{ROOT} keys=\"index\">")?;
                for value in values {
                    writeln!(f, "<{ENTRY}>{}</{ENTRY}>", Text(value))?;
                }
            }
            Entries::Id(entries) => {
                writeln!(f, "<{ROOT} keys=\"id\">")?;
                for IdEntry { id, value } in entries {
                    writeln!(f, "<{ENTRY} id=\"{}\">{}</{ENTRY}>", id.0, Text(value))?;
                }
            }
            Entries::Name(entries) => {
                writeln!(f, "<{ROOT} keys=\"name\">")?;
                for NameEntry { name, value } in entries {
                    writeln!(
                        f,
                        "<{ENTRY} name=\"{}\">{}</{ENTRY}>",
                        AttributeText(name),
                        Text(value)
                    )?;
                }
            }
        }

        writeln!(f, "</{ROOT}>")
    }
}

/// Compiles string table text to `.xus` bytes.
pub fn compile_string_table(text: &str) -> Result<Vec<u8>, CompileError> {
    Ok(StringTable::from_xml(text)?.to_bytes()?)
}

/// An attribute value: quotes and every whitespace character other than
/// space escaped as well, so attribute value normalization keeps them.
struct AttributeText<'t>(&'t str);

impl fmt::Display for AttributeText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '"' => f.write_str("&quot;")?,
                '\n' | '\t' => write!(f, "&#x{:X};", u32::from(c))?,
                c => write!(f, "{}", Text(c.encode_utf8(&mut [0; 4])))?,
            }
        }

        Ok(())
    }
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl Reader<'_> {
    fn bytes(&mut self, len: usize, entry: u16) -> Result<&[u8], StringTableError> {
        let bytes = self
            .offset
            .checked_add(len)
            .and_then(|end| self.data.get(self.offset..end))
            .ok_or(StringTableError::Truncated { entry })?;
        self.offset += len;

        Ok(bytes)
    }

    fn u32(&mut self, entry: u16) -> Result<u32, StringTableError> {
        let bytes = self.bytes(4, entry)?;

        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn string(&mut self, entry: u16) -> Result<String, StringTableError> {
        let length = self.bytes(2, entry)?;
        let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
        let units = self.bytes(2 * length, entry)?;

        char::decode_utf16(
            units
                .as_chunks::<2>()
                .0
                .iter()
                .map(|unit| u16::from_be_bytes(*unit)),
        )
        .collect::<Result<String, _>>()
        .map_err(|_| StringTableError::InvalidUtf16 { entry })
    }
}

/// Checks an entry element and returns its key attribute, or an empty
/// string for index keys.
fn entry_node<'n>(node: &'n Node, key: &[&'static str]) -> Result<&'n str, CompileError> {
    if node.name != ENTRY {
        return Err(CompileError::UnexpectedElement {
            line: node.line,
            element: node.name.clone(),
            parent: ROOT.to_string(),
        });
    }
    node.expect_attributes(key)?;
    node.expect_no_children()?;

    let Some(&attribute) = key.first() else {
        return Ok("");
    };
    node.attribute(attribute)
        .ok_or_else(|| CompileError::MissingAttribute {
            line: node.line,
            element: node.name.clone(),
            attribute,
        })
}

fn bad_attribute(node: &Node, attribute: &'static str, text: &str) -> CompileError {
    CompileError::BadAttribute {
        line: node.line,
        element: node.name.clone(),
        attribute,
        text: text.to_string(),
    }
}

fn write_string(out: &mut Vec<u8>, text: &str, entry: usize) -> Result<(), StringTableWriteError> {
    let Ok(length) = u16::try_from(text.encode_utf16().count()) else {
        return Err(StringTableWriteError::StringTooLong { entry });
    };
    out.extend_from_slice(&length.to_be_bytes());
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }

    Ok(())
}

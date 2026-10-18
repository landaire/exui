//! Byte builders for XUR and XUIZ fixtures.

use std::string::String;
use std::vec::Vec;

/// Writes big-endian values.
#[derive(Default)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.0.push(value);
        self
    }

    pub fn u16(&mut self, value: u16) -> &mut Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub fn f32(&mut self, value: f32) -> &mut Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }

    pub fn wide(&mut self, text: &str) -> &mut Self {
        for unit in text.encode_utf16() {
            self.u16(unit);
        }
        self
    }
}

#[derive(Default)]
pub struct XurBuilder {
    strings: Vec<String>,
    vectors: Vec<[f32; 3]>,
    quaternions: Vec<[f32; 4]>,
    pub custom: Bytes,
    pub data: Bytes,
    pub allocation_hints: Option<[u32; 10]>,
    pub extra_sections: Vec<([u8; 4], Vec<u8>)>,
}

impl XurBuilder {
    /// One-based string index, interned.
    pub fn string(&mut self, text: &str) -> u16 {
        let position = match self.strings.iter().position(|known| known == text) {
            Some(position) => position,
            None => {
                self.strings.push(String::from(text));
                self.strings.len() - 1
            }
        };

        u16::try_from(position + 1).expect("fixture string count fits u16")
    }

    pub fn vector(&mut self, value: [f32; 3]) -> u32 {
        self.vectors.push(value);
        u32::try_from(self.vectors.len() - 1).expect("fixture vector count fits u32")
    }

    pub fn quaternion(&mut self, value: [f32; 4]) -> u32 {
        self.quaternions.push(value);
        u32::try_from(self.quaternions.len() - 1).expect("fixture quaternion count fits u32")
    }

    /// Object header: class name string, flags.
    pub fn object(&mut self, class: &str, flags: u8) -> &mut Self {
        let class = self.string(class);
        self.data.u16(class).u8(flags);
        self
    }

    /// Property block header with a mask of `mask_bytes` bytes.
    pub fn mask(&mut self, mask_bytes: u8, mask: u32) -> &mut Self {
        self.data.u8(mask_bytes);
        match mask_bytes {
            1 => self.data.u8(u8::try_from(mask).expect("mask fits u8")),
            2 => self.data.u16(u16::try_from(mask).expect("mask fits u16")),
            4 => self.data.u32(mask),
            _ => panic!("unsupported fixture mask size"),
        };
        self
    }

    /// Property block header that selects no properties.
    pub fn empty_level(&mut self) -> &mut Self {
        self.data.u8(0);
        self
    }

    pub fn string_value(&mut self, text: &str) -> &mut Self {
        let index = self.string(text);
        self.data.u16(index);
        self
    }

    pub fn build(&self) -> Vec<u8> {
        let mut strn = Bytes::default();
        for text in &self.strings {
            let length = u16::try_from(text.encode_utf16().count()).expect("string fits u16");
            strn.u16(length).wide(text);
        }

        let mut vect = Bytes::default();
        for vector in &self.vectors {
            for component in vector {
                vect.f32(*component);
            }
        }

        let mut quat = Bytes::default();
        for quaternion in &self.quaternions {
            for component in quaternion {
                quat.f32(*component);
            }
        }

        let mut sections: Vec<([u8; 4], &[u8])> = Vec::new();
        sections.push((*b"STRN", &strn.0));
        if !self.vectors.is_empty() {
            sections.push((*b"VECT", &vect.0));
        }
        if !self.quaternions.is_empty() {
            sections.push((*b"QUAT", &quat.0));
        }
        if !self.custom.0.is_empty() {
            sections.push((*b"CUST", &self.custom.0));
        }
        for (tag, bytes) in &self.extra_sections {
            sections.push((*tag, bytes));
        }
        sections.push((*b"DATA", &self.data.0));

        let hints_size = if self.allocation_hints.is_some() {
            40
        } else {
            0
        };
        let table_end = 0x14 + hints_size + sections.len() * 12;
        let total = table_end + sections.iter().map(|(_, bytes)| bytes.len()).sum::<usize>();

        let mut out = Bytes::default();
        out.raw(b"XUIB")
            .u32(5)
            .u32(u32::from(self.allocation_hints.is_some()))
            .u16(0xC)
            .u32(u32::try_from(total).expect("fixture size fits u32"))
            .u16(u16::try_from(sections.len()).expect("section count fits u16"));
        if let Some(hints) = self.allocation_hints {
            for word in hints {
                out.u32(word);
            }
        }

        let mut offset = table_end;
        for (tag, bytes) in &sections {
            out.raw(tag)
                .u32(u32::try_from(offset).expect("offset fits u32"))
                .u32(u32::try_from(bytes.len()).expect("size fits u32"));
            offset += bytes.len();
        }
        for (_, bytes) in &sections {
            out.raw(bytes);
        }

        out.0
    }
}

pub struct ArchiveEntry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
    pub trailer: Option<[u32; 2]>,
}

/// XUIZ bytes with entries laid out in order after the table. Trailers are
/// written for entries that have one; `trailer_flag` is written as given so
/// fixtures can disagree with the entries on purpose.
pub fn build_archive(trailer_flag: bool, entries: &[ArchiveEntry<'_>]) -> Vec<u8> {
    let mut table = Bytes::default();
    let mut payload = Bytes::default();
    for entry in entries {
        let offset = u32::try_from(payload.0.len()).expect("offset fits u32");
        let size = u32::try_from(entry.data.len()).expect("size fits u32");
        let name_len = u8::try_from(entry.name.encode_utf16().count()).expect("name fits u8");
        table.u32(size).u32(offset).u8(name_len).wide(entry.name);

        payload.raw(entry.data);
        if let Some([word0, word1]) = entry.trailer {
            payload.u32(word0).u32(word1);
        }
    }

    let total = 0x16 + table.0.len() + payload.0.len();
    let mut out = Bytes::default();
    out.raw(b"XUIZ")
        .u32(1)
        .u32(u32::try_from(total).expect("archive size fits u32"))
        .u32(u32::from(trailer_flag))
        .u32(u32::try_from(table.0.len()).expect("table size fits u32"))
        .u16(u16::try_from(entries.len()).expect("entry count fits u16"))
        .raw(&table.0)
        .raw(&payload.0);

    out.0
}

//! Test fixtures: a schema-driven XUR writer, XUIZ archives, an in-memory
//! resource provider, PNG encoding and pixel dumps. Fixtures are built in
//! code; no genuine asset bytes are used.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::string::String;
use std::vec::Vec;

use exui_core::Registry;
use exui_core::schema::ClassDef;
use exui_core::schema::PropDef;
use xbox360_draw::draw::DrawList;
use xbox360_draw::draw::Rgba8;
use xbox360_raster::RenderTarget;
use xbox360_raster::SoftwareRenderer;

use crate::resource::ModuleHandle;
use crate::resource::ResourceProvider;
use crate::resource::SectionName;

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
        self.u32(value.to_bits())
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

/// A property value in a fixture scene.
#[derive(Clone, Debug)]
pub enum V {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    Str(&'static str),
    Color(u32),
    Vector([f32; 3]),
    Quaternion([f32; 4]),
    Points {
        width: f32,
        height: f32,
        points: Vec<[f32; 6]>,
    },
    Compound(Vec<P>),
}

/// A property with its values (several for indexed properties).
#[derive(Clone, Debug)]
pub struct P {
    pub def: &'static PropDef,
    pub values: Vec<V>,
}

/// A fixture object.
#[derive(Clone, Debug)]
pub struct Obj {
    pub class: &'static str,
    pub props: Vec<P>,
    pub children: Vec<Obj>,
}

impl Obj {
    pub fn new(class: &'static str) -> Self {
        Self {
            class,
            props: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Adds `name` of class level `level` with one value.
    pub fn with(mut self, level: &'static ClassDef, name: &str, value: V) -> Self {
        self.props.push(P {
            def: def(level, name),
            values: std::vec![value],
        });
        self
    }

    pub fn child(mut self, child: Obj) -> Self {
        self.children.push(child);
        self
    }
}

/// The definition `name` declared by `level`.
pub fn def(level: &'static ClassDef, name: &str) -> &'static PropDef {
    level
        .props
        .iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("{} has no property {name}", level.name))
}

/// Member `name` of compound `compound`.
pub fn member(compound: &'static PropDef, name: &str) -> &'static PropDef {
    compound
        .ty
        .compound()
        .and_then(|members| members.iter().find(|def| def.name == name))
        .unwrap_or_else(|| panic!("{} has no member {name}", compound.name))
}

pub fn prop(def: &'static PropDef, values: Vec<V>) -> P {
    P { def, values }
}

/// XUR v5 bytes for `root`, property blocks laid out per class level.
pub fn build_scene(registry: &Registry, root: &Obj) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.object(registry, root);

    writer.finish()
}

#[derive(Default)]
struct Writer {
    strings: Vec<String>,
    vectors: Bytes,
    quaternions: Bytes,
    custom: Bytes,
    data: Bytes,
}

impl Writer {
    fn string(&mut self, text: &str) -> u16 {
        let position = self
            .strings
            .iter()
            .position(|known| known == text)
            .unwrap_or_else(|| {
                self.strings.push(String::from(text));
                self.strings.len() - 1
            });

        u16::try_from(position + 1).expect("fixture string count fits u16")
    }

    fn object(&mut self, registry: &Registry, object: &Obj) {
        let class_name = self.string(object.class);
        let flags = u8::from(!object.props.is_empty()) | u8::from(!object.children.is_empty()) << 1;
        self.data.u16(class_name).u8(flags);

        if !object.props.is_empty() {
            let entries: usize = object
                .props
                .iter()
                .map(|property| {
                    if matches!(property.values[0], V::Compound(_)) {
                        1
                    } else {
                        property.values.len()
                    }
                })
                .sum();
            self.data.u16(entries as u16);
            let class = registry
                .find(object.class)
                .and_then(|id| registry.class(id))
                .unwrap_or_else(|| panic!("class {} is registered", object.class));
            for level in class.levels() {
                self.block(level.def().props, &object.props);
            }
        }

        if !object.children.is_empty() {
            self.data.u32(object.children.len() as u32);
            for child in &object.children {
                self.object(registry, child);
            }
        }
    }

    fn block(&mut self, defs: &'static [PropDef], props: &[P]) {
        let mut mask = 0u32;
        for property in props {
            if let Some(position) = defs.iter().position(|def| std::ptr::eq(def, property.def)) {
                mask |= 1 << position;
            }
        }
        if mask == 0 {
            self.data.u8(0);
            return;
        }

        self.data.u8(4).u32(mask);
        for def in defs {
            let Some(property) = props
                .iter()
                .find(|property| std::ptr::eq(def, property.def))
            else {
                continue;
            };
            if def.flags.is_indexed() {
                self.data.u8(property.values.len() as u8);
            }
            for value in &property.values {
                self.value(def, value);
            }
        }
    }

    fn value(&mut self, def: &'static PropDef, value: &V) {
        match value {
            V::Bool(value) => {
                self.data.u8(u8::from(*value));
            }
            V::Integer(value) => {
                self.data.u32(*value as u32);
            }
            V::Unsigned(value) | V::Color(value) => {
                self.data.u32(*value);
            }
            V::Float(value) => {
                self.data.f32(*value);
            }
            V::Str(text) => {
                let index = self.string(text);
                self.data.u16(index);
            }
            V::Vector(vector) => {
                let index = self.vectors.0.len() / 12;
                for component in vector {
                    self.vectors.f32(*component);
                }
                self.data.u32(index as u32);
            }
            V::Quaternion(quaternion) => {
                let index = self.quaternions.0.len() / 16;
                for component in quaternion {
                    self.quaternions.f32(*component);
                }
                self.data.u32(index as u32);
            }
            V::Points {
                width,
                height,
                points,
            } => {
                let offset = self.custom.0.len() as u32;
                self.custom
                    .u32(12 + 24 * points.len() as u32)
                    .f32(*width)
                    .f32(*height)
                    .u32(points.len() as u32);
                for point in points {
                    for coord in point {
                        self.custom.f32(*coord);
                    }
                }
                self.data.u32(offset);
            }
            V::Compound(members) => {
                let entries: usize = members.iter().map(|member| member.values.len()).sum();
                self.data.u16(entries as u16);
                let defs = def
                    .ty
                    .compound()
                    .expect("compound value for a compound definition");
                self.block(defs, members);
            }
        }
    }

    fn finish(self) -> Vec<u8> {
        let mut strn = Bytes::default();
        for text in &self.strings {
            strn.u16(text.encode_utf16().count() as u16).wide(text);
        }

        let mut sections: Vec<(&[u8; 4], &[u8])> = std::vec![(b"STRN", &strn.0)];
        if !self.vectors.0.is_empty() {
            sections.push((b"VECT", &self.vectors.0));
        }
        if !self.quaternions.0.is_empty() {
            sections.push((b"QUAT", &self.quaternions.0));
        }
        if !self.custom.0.is_empty() {
            sections.push((b"CUST", &self.custom.0));
        }
        sections.push((b"DATA", &self.data.0));

        let table_end = 0x14 + sections.len() * 12;
        let total = table_end + sections.iter().map(|(_, bytes)| bytes.len()).sum::<usize>();
        let mut out = Bytes::default();
        out.raw(b"XUIB")
            .u32(5)
            .u32(0)
            .u16(0xC)
            .u32(total as u32)
            .u16(sections.len() as u16);
        let mut offset = table_end;
        for (tag, bytes) in &sections {
            out.raw(*tag).u32(offset as u32).u32(bytes.len() as u32);
            offset += bytes.len();
        }
        for (_, bytes) in &sections {
            out.raw(bytes);
        }

        out.0
    }
}

/// XUIZ archive bytes with `entries` in order.
pub fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut table = Bytes::default();
    let mut payload = Bytes::default();
    for (name, data) in entries {
        table
            .u32(data.len() as u32)
            .u32(payload.0.len() as u32)
            .u8(name.encode_utf16().count() as u8)
            .wide(name);
        payload.raw(data);
    }

    let mut out = Bytes::default();
    out.raw(b"XUIZ")
        .u32(1)
        .u32((0x16 + table.0.len() + payload.0.len()) as u32)
        .u32(0)
        .u32(table.0.len() as u32)
        .u16(entries.len() as u16)
        .raw(&table.0)
        .raw(&payload.0);

    out.0
}

/// In-memory sections and files.
#[derive(Default)]
pub struct MapProvider {
    pub sections: BTreeMap<SectionName, Vec<u8>>,
    pub files: BTreeMap<Vec<u16>, Vec<u8>>,
}

impl MapProvider {
    pub fn with_section(mut self, name: &str, data: Vec<u8>) -> Self {
        self.sections
            .insert(SectionName::new(name).expect("fixture section name"), data);
        self
    }
}

impl ResourceProvider for MapProvider {
    /// Every module serves the same sections.
    fn section(&self, _module: ModuleHandle, name: SectionName) -> Option<&[u8]> {
        self.sections.get(&name).map(Vec::as_slice)
    }

    fn file(&self, path: &[u16]) -> Option<Cow<'_, [u8]>> {
        self.files
            .get(path)
            .map(|data| Cow::Borrowed(data.as_slice()))
    }
}

/// Straight RGBA PNG with stored (uncompressed) deflate blocks.
pub fn encode_png(width: u32, height: u32, rgba: &[[u8; 4]]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in rgba.chunks(width as usize) {
        raw.push(0);
        for pixel in row {
            raw.extend_from_slice(pixel);
        }
    }

    let mut zlib = std::vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(0xffff).collect();
    for (index, chunk) in chunks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == chunks.len()));
        let length = chunk.len() as u16;
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = std::vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Bytes::default();
    header.u32(width).u32(height).u8(8).u8(6).u8(0).u8(0).u8(0);
    for (tag, data) in [
        (b"IHDR", header.0.as_slice()),
        (b"IDAT", zlib.as_slice()),
        (b"IEND", &[][..]),
    ] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }

    !crc
}

/// Renders `list` into a `width` x `height` target cleared by the list.
pub fn rasterize(
    renderer: &mut SoftwareRenderer,
    list: &DrawList,
    width: u32,
    height: u32,
) -> Vec<Rgba8> {
    let mut pixels = std::vec![Rgba8::TRANSPARENT; (width * height) as usize];
    let mut target = RenderTarget::new(&mut pixels, width, height, width).expect("fixture target");
    renderer.render(list, &mut target);

    pixels
}

/// One line per row, three hex digits (4-bit red, green, blue) per pixel.
pub fn pixel_dump(pixels: &[Rgba8], width: u32) -> String {
    let mut out = String::new();
    for row in pixels.chunks(width as usize) {
        for pixel in row {
            write!(
                out,
                "{:x}{:x}{:x}",
                pixel.red() >> 4,
                pixel.green() >> 4,
                pixel.blue() >> 4
            )
            .expect("writing to a string");
        }
        out.push('\n');
    }

    out
}

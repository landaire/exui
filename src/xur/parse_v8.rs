//! XUIB v8 binary parser.
//!
//! Reverse-engineered from the xam.xex loader (`sub_817e0c10`, which calls
//! `sub_817e0970`, `sub_817dfc00`, and `sub_817df530`). v8 differs from v5 in
//! several fundamental ways:
//!
//! * Integers are LEB-like varints (see [`Cursor::varint`]): a byte below
//!   `0xf0` is its own value; `0xf0..=0xfe` is a 12-bit value
//!   `((b & 0x0f) << 8) | next`; `0xff` introduces a big-endian `u32`.
//! * The property bitmask for each class level is itself a varint.
//! * Property values are stored once in typed pools (`FLOT`/`COLR`/`VECT`/
//!   `QUAT`/`CUST`) and referenced by varint index; the only inline scalar is
//!   `Bool` (one raw byte).
//! * `STRN` holds a `u32` byte length, a `u16` count, then NUL-terminated
//!   ASCII strings (v5 used length-prefixed UTF-16BE).
//! * Object header: `class` (varint, 1-based string index) + `flags` (one
//!   byte). `flags`: `0x01` properties, `0x02` children, `0x04` timeline,
//!   `0x08` id. Counts (value count, child count, id) are varints.

use byteorder::BigEndian;
use byteorder::ByteOrder;

use super::Header;
use super::Keyframe;
use super::KeyframePath;
use super::NamedFrame;
use super::Object;
use super::ParseError;
use super::PropType;
use super::PropertyGroup;
use super::PropertyValue;
use super::Timeline;
use super::TimelineData;
use super::TimelineValue;
use super::Xur;
use super::object_flags;
use super::section_tag;
use crate::xur::parse::{
    XUIELEMENT_TYPES_V5, get_compound_sub_types, get_hierarchy, is_gradient_array_prop,
};

const HEADER_SIZE: usize = 0x14;
const SECTION_HEADER_SIZE: usize = 12;

/// A byte cursor over the DATA section that decodes v8 varints.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    /// Compound dedup slots already materialized (see [`read_compound`]).
    seen_compounds: std::collections::HashSet<u32>,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor {
            data,
            pos: 0,
            seen_compounds: std::collections::HashSet::new(),
        }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn byte(&mut self) -> Result<u8, ParseError> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or(ParseError::BadObject(format!("eof @{}", self.pos)))?;
        self.pos += 1;
        Ok(b)
    }

    /// Read a v8 varint (matches `sub_817df530`).
    fn varint(&mut self) -> Result<u32, ParseError> {
        let b = self.byte()?;
        if b < 0xf0 {
            Ok(u32::from(b))
        } else if b < 0xff {
            let lo = self.byte()?;
            Ok((u32::from(b & 0x0f) << 8) | u32::from(lo))
        } else {
            if self.remaining() < 4 {
                return Err(ParseError::BadObject(format!("eof u32 @{}", self.pos)));
            }
            let v = BigEndian::read_u32(&self.data[self.pos..self.pos + 4]);
            self.pos += 4;
            Ok(v)
        }
    }
}

/// Typed value pools referenced from DATA by index.
struct Pools<'a> {
    flot: &'a [u8],
    colr: &'a [u8],
}

impl Pools<'_> {
    fn float(&self, index: u32) -> f32 {
        let off = index as usize * 4;
        self.flot
            .get(off..off + 4)
            .map(BigEndian::read_f32)
            .unwrap_or(0.0)
    }

    fn color(&self, index: u32) -> u32 {
        let off = index as usize * 4;
        self.colr
            .get(off..off + 4)
            .map(BigEndian::read_u32)
            .unwrap_or(0)
    }
}

/// A raw KEYD keyframe: a `frame` time, interpolation flags + ease, and a
/// `property_base` index into the flat KEYP property-index list where this
/// frame's per-property animated values begin.
#[derive(Clone, Copy)]
struct RawKeyframe {
    frame: u32,
    interpolation: u8,
    ease: [u8; 3],
    property_base: u32,
}

/// Shared timeline tables. KEYD holds the keyframes; KEYP is a flat list of
/// per-keyframe animated property value indexes (each resolved against the
/// animated property's type into FLOT/COLR/VECT/QUAT/STRN, or used raw).
struct TimelineTables {
    named_frames: Vec<NamedFrame>,
    keyframes: Vec<RawKeyframe>,
    keyp: Vec<u32>,
}

/// NAME section: a run of named frames, each `name` (varint) + `time` (varint) +
/// `command` (byte) + `from_name` (varint, only when `command >= 2`).
fn parse_named_frames(data: &[u8]) -> Vec<NamedFrame> {
    let mut c = Cursor::new(data);
    let mut out = Vec::new();
    while c.remaining() >= 3 {
        let Ok(name) = c.varint() else { break };
        let Ok(time) = c.varint() else { break };
        let Ok(command) = c.byte() else { break };
        let from_name = if command >= 2 {
            c.varint().unwrap_or(0)
        } else {
            0
        };
        out.push(NamedFrame {
            name,
            time,
            command,
            from_name,
        });
    }
    out
}

/// KEYD section: a flat list of keyframes. Each is `frame` (varint) + a flag
/// byte (low 6 bits are interpolation: 0=linear, 1=none, 2=ease, 3=linear; high
/// 2 bits unused) + optional interpolation data (3 ease bytes when flags==2, a
/// varint when flags==0xA, one byte when flags==0xB) + `property_base` (varint,
/// offset into the flat KEYP property-index list).
fn parse_keyframes(data: &[u8]) -> Vec<RawKeyframe> {
    let mut c = Cursor::new(data);
    let mut out = Vec::new();
    while c.remaining() >= 2 {
        let Ok(frame) = c.varint() else { break };
        let Ok(flag) = c.byte() else { break };
        let flags = flag & 0x3f;
        let mut ease = [0u8; 3];
        match flags {
            2 => {
                let (Ok(a), Ok(b), Ok(d)) = (c.byte(), c.byte(), c.byte()) else {
                    break;
                };
                ease = [a, b, d];
            }
            0xA => {
                if c.varint().is_err() {
                    break;
                }
            }
            0xB => {
                if c.byte().is_err() {
                    break;
                }
            }
            _ => {}
        }
        let Ok(property_base) = c.varint() else { break };
        // Map interpolation flags to XUI types: 1=None, 2=Ease, everything else
        // (0, 3, and the rare 0xA/0xB) renders as the default Linear.
        let interpolation = match flags {
            1 => 1,
            2 => 2,
            _ => 0,
        };
        out.push(RawKeyframe {
            frame,
            interpolation,
            ease,
            property_base,
        });
    }
    out
}

/// KEYP section: a flat list of varint property-value indexes, indexed into by a
/// keyframe's `property_base`.
fn parse_keyp(data: &[u8]) -> Vec<u32> {
    let mut c = Cursor::new(data);
    let mut out = Vec::new();
    while c.remaining() >= 1 {
        match c.varint() {
            Ok(v) => out.push(v),
            Err(_) => break,
        }
    }
    out
}

pub fn parse(data: &[u8]) -> Result<Xur<'_>, ParseError> {
    if data.len() < HEADER_SIZE {
        return Err(ParseError::TooShort {
            needed: HEADER_SIZE,
            available: data.len(),
        });
    }

    let header = parse_header(data)?;
    let sections = locate_sections(data, header.section_count as usize)?;

    let strn = section(data, &sections, section_tag::STRN)
        .ok_or(ParseError::MissingSection("STRN"))?;
    let data_sect =
        section(data, &sections, section_tag::DATA).ok_or(ParseError::MissingSection("DATA"))?;
    let vect = section(data, &sections, section_tag::VECT).unwrap_or(&[]);
    let quat = section(data, &sections, section_tag::QUAT).unwrap_or(&[]);
    let cust = section(data, &sections, section_tag::CUST).unwrap_or(&[]);
    let flot = section(data, &sections, section_tag::FLOT).unwrap_or(&[]);
    let colr = section(data, &sections, section_tag::COLR).unwrap_or(&[]);

    let strings = parse_string_table(strn)?;
    let pools = Pools { flot, colr };
    let tables = TimelineTables {
        named_frames: parse_named_frames(section(data, &sections, b"NAME").unwrap_or(&[])),
        keyframes: parse_keyframes(section(data, &sections, b"KEYD").unwrap_or(&[])),
        keyp: parse_keyp(section(data, &sections, b"KEYP").unwrap_or(&[])),
    };

    let mut cursor = Cursor::new(data_sect);
    let root = parse_object(&mut cursor, data_sect, &strings, &pools, &tables)?;

    Ok(Xur {
        header,
        strings,
        vectors: vect,
        quaternions: quat,
        custom: cust,
        root,
    })
}

fn parse_header(data: &[u8]) -> Result<Header, ParseError> {
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&data[0..4]);
    if &magic != super::XUIB_MAGIC {
        return Err(ParseError::BadMagic(magic));
    }
    let raw_version = BigEndian::read_u32(&data[4..8]);
    Ok(Header {
        magic,
        format_version: super::FormatVersion::from_u32(raw_version),
        raw_version,
        reserved: BigEndian::read_u32(&data[8..12]),
        xui_version: BigEndian::read_u16(&data[0x0C..0x0E]),
        total_size: BigEndian::read_u32(&data[0x0E..0x12]),
        section_count: BigEndian::read_u16(&data[0x12..0x14]),
    })
}

/// Locate the section table generically (works for v5 and v8 regardless of the
/// variable header preamble): `STRN` is always section 0 and its data
/// immediately follows the table, so the table starts where the first section's
/// offset equals `table_start + count * 12`.
fn locate_sections(data: &[u8], count: usize) -> Result<Vec<super::SectionHeader>, ParseError> {
    let limit = (HEADER_SIZE + 64).min(data.len());
    let table_start = (HEADER_SIZE..limit).find(|&t| {
        if t + SECTION_HEADER_SIZE > data.len() || &data[t..t + 4] != section_tag::STRN {
            return false;
        }
        let off = BigEndian::read_u32(&data[t + 4..t + 8]) as usize;
        off == t + count * SECTION_HEADER_SIZE
    });
    let table_start =
        table_start.ok_or(ParseError::MissingSection("STRN"))?;
    let end = table_start + count * SECTION_HEADER_SIZE;
    if end > data.len() {
        return Err(ParseError::TooShort {
            needed: end,
            available: data.len(),
        });
    }
    Ok((0..count)
        .map(|i| {
            let b = table_start + i * SECTION_HEADER_SIZE;
            let mut tag = [0u8; 4];
            tag.copy_from_slice(&data[b..b + 4]);
            super::SectionHeader {
                tag,
                offset: BigEndian::read_u32(&data[b + 4..b + 8]),
                size: BigEndian::read_u32(&data[b + 8..b + 12]),
            }
        })
        .collect())
}

fn section<'a>(data: &'a [u8], sections: &[super::SectionHeader], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let s = sections.iter().find(|s| &s.tag == tag)?;
    let start = s.offset as usize;
    let end = start + s.size as usize;
    data.get(start..end)
}

/// v8 string table: `u32` byte length, `u16` count, then NUL-terminated ASCII.
fn parse_string_table(data: &[u8]) -> Result<Vec<String>, ParseError> {
    if data.len() < 6 {
        return Ok(Vec::new());
    }
    let count = BigEndian::read_u16(&data[4..6]) as usize;
    let body = &data[6..];
    let mut strings = Vec::with_capacity(count);
    let mut start = 0;
    for i in 0..body.len() {
        if body[i] == 0 {
            strings.push(String::from_utf8_lossy(&body[start..i]).into_owned());
            start = i + 1;
            if strings.len() == count {
                break;
            }
        }
    }
    Ok(strings)
}

fn class_name<'a>(strings: &'a [String], index: u32) -> &'a str {
    index
        .checked_sub(1)
        .and_then(|i| strings.get(i as usize))
        .map(String::as_str)
        .unwrap_or("")
}

fn parse_object<'a>(
    c: &mut Cursor<'a>,
    full: &'a [u8],
    strings: &[String],
    pools: &Pools<'_>,
    tables: &TimelineTables,
) -> Result<Object<'a>, ParseError> {
    let class_index = c.varint()?;
    if class_index == 0 {
        return Err(ParseError::BadObject(format!("null class @{}", c.pos)));
    }
    let class_str = class_name(strings, class_index);
    let flags = c.byte()?;

    let props_start = c.pos;
    let mut properties = Vec::new();
    if flags & object_flags::HAS_PROPERTIES != 0 {
        let value_count = c.varint()?;
        // The loader walks the full class hierarchy from base to derived,
        // reading one bitmask per level including empty ones. Rather than trust
        // a hardcoded depth, read levels until `value_count` set bits are
        // consumed, then skip trailing empty levels: their `0x00` bitmask byte
        // is unambiguous because no class index, id, or non-empty child count
        // is 0.
        let hierarchy = get_hierarchy(class_str);
        let mut consumed = 0u32;
        let mut level = 0usize;
        while consumed < value_count {
            let types = level_types(hierarchy, level);
            consumed += read_level(c, types, level, &mut properties, strings, pools)?;
            level += 1;
        }
        while c.peek() == Some(0) {
            c.pos += 1;
        }
    }
    let raw_props = &full[props_start..c.pos.min(full.len())];

    if flags & object_flags::HAS_ID != 0 {
        let _id = c.varint()?;
    }

    let mut children = Vec::new();
    if flags & object_flags::HAS_CHILDREN != 0 {
        let child_count = c.varint()?;
        for i in 0..child_count {
            children.push(parse_object(c, full, strings, pools, tables).map_err(|e| {
                ParseError::BadObject(format!("child {i}/{child_count}: {e}"))
            })?);
        }
    }

    let timelines = if flags & object_flags::HAS_TIMELINE != 0 {
        parse_timeline(
            c,
            flags & object_flags::HAS_CHILDREN != 0,
            &children,
            strings,
            pools,
            tables,
        )?
    } else {
        None
    };

    Ok(Object {
        class_name: class_index,
        properties,
        children,
        timelines,
        _raw_props: raw_props,
    })
}

/// Property types for a given hierarchy level: level 0 is always XuiElement,
/// levels 1.. come from the derived-class hierarchy. Levels past the known
/// hierarchy (e.g. a custom class's own level) default to all-varint, which is
/// correct for consumption since only Compound/Array need structural knowledge.
fn level_types(hierarchy: &'static [&'static [PropType]], level: usize) -> &'static [PropType] {
    if level == 0 {
        XUIELEMENT_TYPES_V5
    } else {
        hierarchy.get(level - 1).copied().unwrap_or(&[])
    }
}

/// Read one class level: a varint bitmask, then a value per set bit. Returns the
/// number of properties (set bits) read.
fn read_level(
    c: &mut Cursor<'_>,
    types: &[PropType],
    level: usize,
    out: &mut Vec<PropertyGroup>,
    strings: &[String],
    pools: &Pools<'_>,
) -> Result<u32, ParseError> {
    let bitmask = c.varint()?;
    if bitmask == 0 {
        return Ok(0);
    }
    let mut values = Vec::new();
    for bit in 0..32u32 {
        if bitmask & (1 << bit) == 0 {
            continue;
        }
        let ty = types.get(bit as usize).copied().unwrap_or(PropType::Unsigned);
        if is_array(types, bit) {
            let count = c.varint()?;
            for _ in 0..count {
                values.push(read_value(c, ty, types, bit, strings, pools)?);
            }
        } else {
            values.push(read_value(c, ty, types, bit, strings, pools)?);
        }
    }
    out.push(PropertyGroup {
        bitmask,
        values,
        level,
    });
    Ok(bitmask.count_ones())
}

fn read_value(
    c: &mut Cursor<'_>,
    ty: PropType,
    parent_types: &[PropType],
    bit: u32,
    strings: &[String],
    pools: &Pools<'_>,
) -> Result<PropertyValue, ParseError> {
    Ok(match ty {
        PropType::Bool => PropertyValue::Bool(c.byte()? != 0),
        PropType::Integer | PropType::Unsigned => PropertyValue::Unsigned(c.varint()?),
        PropType::Float => PropertyValue::Float(pools.float(c.varint()?)),
        PropType::String => PropertyValue::String(c.varint()?),
        PropType::Color => PropertyValue::Color(pools.color(c.varint()?)),
        PropType::Vector3 => PropertyValue::Vector(c.varint()?),
        PropType::Quaternion => PropertyValue::Quaternion(c.varint()?),
        PropType::Custom => PropertyValue::Custom(c.varint()?),
        PropType::Compound => read_compound(c, parent_types, bit, strings, pools)?,
    })
}

/// Compound (type 9): varint dedup slot, varint value count, then a nested
/// level (its own varint bitmask + values). On a fresh load the slot is unseen,
/// so the body is always present inline.
fn read_compound(
    c: &mut Cursor<'_>,
    parent_types: &[PropType],
    bit: u32,
    strings: &[String],
    pools: &Pools<'_>,
) -> Result<PropertyValue, ParseError> {
    let slot = c.varint()?;
    // A slot seen before is a shared reference with no inline body; the loader
    // reuses the previously materialized compound instead of reading.
    if !c.seen_compounds.insert(slot) {
        return Ok(PropertyValue::Object(Vec::new()));
    }
    let _count = c.varint()?;
    let sub_types = get_compound_sub_types(parent_types, bit).unwrap_or(&[]);
    let mut group = Vec::new();
    read_level(c, sub_types, 0, &mut group, strings, pools)?;
    Ok(PropertyValue::Object(group))
}

/// Parse an object's inline timeline block (`sub_817e0288`) into structured
/// timeline data. The keyframe values live in the KEYD pool (referenced by
/// index) and named frames in NAME; only the refs and keyframe paths are inline.
///
/// Layout: `named_frame_count` (varint); if non-zero, `named_frame_start`
/// (varint, into NAME). Then, if the object has children: `timeline_count`
/// (varint), and per timeline: `target` (varint) + `path_count` (varint) +
/// each path + `keyframe_count` (varint) + `keyframe_start` (varint, into KEYD).
/// A path is a `depth|flag` byte; if `depth > 0`, `depth + 1` path bytes follow
/// (the first two are `hier_level`, `prop_idx`); if the high bit is set, a varint
/// default value follows.
fn parse_timeline(
    c: &mut Cursor<'_>,
    has_children: bool,
    children: &[Object<'_>],
    strings: &[String],
    pools: &Pools<'_>,
    tables: &TimelineTables,
) -> Result<Option<TimelineData>, ParseError> {
    let named_frame_count = c.varint()?;
    let named_frame_start = if named_frame_count > 0 {
        c.varint()? as usize
    } else {
        0
    };
    let named_frames = tables
        .named_frames
        .get(named_frame_start..named_frame_start + named_frame_count as usize)
        .unwrap_or(&[])
        .to_vec();

    if !has_children {
        return Ok((!named_frames.is_empty()).then_some(TimelineData {
            named_frames,
            timelines: Vec::new(),
        }));
    }

    let timeline_count = c.varint()?;
    let mut timelines = Vec::with_capacity(timeline_count as usize);
    for _ in 0..timeline_count {
        let target_name = c.varint()?;
        let target_class = find_child_class(children, strings, target_name);

        let path_count = c.varint()?;
        let mut paths = Vec::with_capacity(path_count as usize);
        for _ in 0..path_count {
            let descriptor = c.byte()?;
            let depth = descriptor & 0x7f;
            let mut hier_level = 0u8;
            let mut prop_idx = 0u8;
            let mut extra_bytes = Vec::new();
            if depth > 0 {
                hier_level = c.byte()?;
                prop_idx = c.byte()?;
                for _ in 1..depth {
                    extra_bytes.push(c.byte()?);
                }
            }
            let default_value = if descriptor & 0x80 != 0 {
                Some(c.varint()?)
            } else {
                None
            };
            let prop_type =
                timeline_prop_type(target_class.as_deref().unwrap_or(""), hier_level, prop_idx);
            paths.push(KeyframePath {
                hier_level,
                prop_idx,
                prop_type,
                prop_name: String::new(),
                depth,
                extra_bytes,
                default_value,
            });
        }

        let keyframe_count = c.varint()? as usize;
        let keyframe_start = c.varint()? as usize;
        let keyframes = build_keyframes(tables, keyframe_start, keyframe_count, &paths, pools);

        timelines.push(Timeline {
            target_name,
            target_class,
            paths,
            keyframes,
        });
    }

    Ok(Some(TimelineData {
        named_frames,
        timelines,
    }))
}

/// Build a timeline's keyframes. The timeline references `count` KEYD keyframes
/// starting at `start`; each keyframe's `property_base` points into the flat
/// KEYP list where this frame's value index for each path begins (one per path,
/// in path order), resolved against each path's property type.
fn build_keyframes(
    tables: &TimelineTables,
    start: usize,
    count: usize,
    paths: &[KeyframePath],
    pools: &Pools<'_>,
) -> Vec<Keyframe> {
    let mut keyframes = Vec::with_capacity(count);
    for k in 0..count {
        let Some(raw) = tables.keyframes.get(start + k) else {
            continue;
        };
        let base = raw.property_base as usize;
        let mut values = Vec::with_capacity(paths.len());
        for (p, path) in paths.iter().enumerate() {
            let index = tables.keyp.get(base + p).copied().unwrap_or(0);
            values.push(type_timeline_value(path.prop_type, index, pools));
        }
        keyframes.push(Keyframe {
            time: raw.frame,
            interpolation: raw.interpolation,
            ease: raw.ease,
            values,
        });
    }
    keyframes
}

fn type_timeline_value(prop_type: Option<PropType>, value: u32, pools: &Pools<'_>) -> TimelineValue {
    match prop_type {
        Some(PropType::Bool) => TimelineValue::Bool(value != 0),
        Some(PropType::Float) => TimelineValue::Float(pools.float(value)),
        Some(PropType::String) => TimelineValue::String(value),
        Some(PropType::Color) => TimelineValue::Color(pools.color(value)),
        Some(PropType::Vector3) => TimelineValue::Vector(value),
        Some(PropType::Quaternion) => TimelineValue::Quaternion(value),
        _ => TimelineValue::Unsigned(value),
    }
}

/// Resolve a path's property type from the target child's class hierarchy,
/// mirroring the writer's name resolution (hier_level 0 = leaf class).
fn timeline_prop_type(class: &str, hier_level: u8, prop_idx: u8) -> Option<PropType> {
    let hierarchy = get_hierarchy(class);
    let hl = hier_level as usize;
    let idx = prop_idx as usize;
    if hl >= hierarchy.len() {
        XUIELEMENT_TYPES_V5.get(idx).copied()
    } else {
        hierarchy[hierarchy.len() - 1 - hl].get(idx).copied()
    }
}

/// Find a direct child by its Id (1-based string index) and return its class.
fn find_child_class(
    children: &[Object<'_>],
    strings: &[String],
    target_name: u32,
) -> Option<String> {
    if target_name == 0 {
        return None;
    }
    children
        .iter()
        .find(|child| child_id_index(child) == Some(target_name))
        .map(|child| class_name(strings, child.class_name).to_string())
}

/// The Id string index of an object (XuiElement bit 0 of its level-0 group).
fn child_id_index(obj: &Object<'_>) -> Option<u32> {
    let group = obj
        .properties
        .iter()
        .find(|g| g.level == 0 && g.bitmask & 1 != 0)?;
    match group.values.first()? {
        PropertyValue::String(idx) => Some(*idx),
        _ => None,
    }
}

fn is_array(types: &[PropType], bit: u32) -> bool {
    use crate::xur::parse::GRADIENT_TYPES;
    use crate::xur::parse::types_match;
    types_match(types, GRADIENT_TYPES) && is_gradient_array_prop(bit)
}

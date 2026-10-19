//! XUR version 5 writer: serializes a [`Tree`] in the layout the reader in
//! [`crate::xur`] accepts, following the conventions of the 6717 files (see
//! doc/xui-format.md, "Writer").

use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::borrow::Borrow;

use crate::error::StringOwner;
use crate::error::WriteError;
use crate::schema::ClassId;
use crate::schema::ClassRef;
use crate::schema::PropDef;
use crate::schema::PropKind;
use crate::schema::Registry;
use crate::schema::ScalarType;
use crate::tree::KeyframeValue;
use crate::tree::Object;
use crate::tree::Property;
use crate::tree::PropertyPath;
use crate::tree::Timeline;
use crate::tree::Tree;
use crate::tree::Value;
use crate::wide::WideString;
use crate::xur::AllocationHints;
use crate::xur::HeaderFlags;
use crate::xur::ObjectFlags;
use crate::xur::SectionTag;

/// Layout choices that do not change what the loader reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Writes the ten allocation hint words after the header. The 6717
    /// files have them or not depending on the tool that wrote them.
    pub allocation_hints: bool,
}

/// Tallies of what the loader allocates, written as the allocation hints.
/// [INFERENCE] The word meanings were matched against the ten 6717 files
/// that have hints; all of their objects have properties, so whether word
/// 2 counts objects or property arrays is not settled.
#[derive(Default)]
struct Counts {
    objects: u32,
    property_arrays: u32,
    values: u32,
    named_frames: u32,
    timelines: u32,
    timeline_properties: u32,
    keyframes: u32,
    keyframe_values: u32,
    path_defs: u32,
    parents: u32,
}

impl Counts {
    fn hints(&self) -> AllocationHints {
        AllocationHints([
            self.objects,
            self.values,
            self.property_arrays,
            self.keyframe_values,
            self.path_defs,
            self.timeline_properties,
            self.keyframes,
            self.timelines,
            self.named_frames,
            self.parents,
        ])
    }
}

/// First-use ordered table with deduplication: each key maps to its
/// zero-based position.
struct Interner<K> {
    index: BTreeMap<K, u32>,
}

impl<K: Ord> Interner<K> {
    fn new() -> Self {
        Self {
            index: BTreeMap::new(),
        }
    }

    fn intern<Q>(&mut self, key: &Q) -> u32
    where
        Q: Ord + ToOwned<Owned = K> + ?Sized,
        K: Borrow<Q>,
    {
        if let Some(&index) = self.index.get(key) {
            return index;
        }

        let index = self.index.len() as u32;
        self.index.insert(key.to_owned(), index);

        index
    }

    fn len(&self) -> usize {
        self.index.len()
    }

    /// Keys by position.
    fn ordered(&self) -> Vec<&K> {
        let mut keys: Vec<(&K, u32)> = self
            .index
            .iter()
            .map(|(key, &index)| (key, index))
            .collect();
        keys.sort_unstable_by_key(|&(_, index)| index);

        keys.into_iter().map(|(key, _)| key).collect()
    }
}

/// An object whose children are being written.
struct ObjectFrame<'t, 'r> {
    object: &'t Object,
    class: ClassRef<'r>,
    next_child: usize,
}

struct Writer<'r> {
    options: WriteOptions,
    registry: &'r Registry,

    strings: Interner<Vec<u16>>,
    /// String index of each class name already written.
    class_names: BTreeMap<ClassId, u16>,
    vectors: Interner<[u32; 3]>,
    quaternions: Interner<[u32; 4]>,
    custom: Vec<u8>,
    /// Offsets of the blobs in `custom`; equal blobs are stored once.
    blobs: BTreeMap<Vec<u8>, u32>,

    data: Vec<u8>,
    counts: Counts,
}

impl<'r> Writer<'r> {
    fn finish(self) -> Result<Vec<u8>, WriteError> {
        const MAGIC: &[u8; 4] = b"XUIB";
        const VERSION: u32 = 5;
        const SCHEMA: u16 = 0xC;
        const HEADER_SIZE: usize = 0x14;
        const SECTION_ENTRY_SIZE: usize = 12;

        let mut strn = Vec::new();
        for units in self.strings.ordered() {
            strn.extend_from_slice(&(units.len() as u16).to_be_bytes());
            for unit in units {
                strn.extend_from_slice(&unit.to_be_bytes());
            }
        }

        let mut vect = Vec::with_capacity(self.vectors.len() * 12);
        for word in self.vectors.ordered().into_iter().flatten() {
            vect.extend_from_slice(&word.to_be_bytes());
        }

        let mut quat = Vec::with_capacity(self.quaternions.len() * 16);
        for word in self.quaternions.ordered().into_iter().flatten() {
            quat.extend_from_slice(&word.to_be_bytes());
        }

        let hints = self.options.allocation_hints.then(|| self.counts.hints());
        let mut sections: Vec<(SectionTag, &[u8])> = Vec::with_capacity(5);
        sections.push((SectionTag::STRN, &strn));
        if !vect.is_empty() {
            sections.push((SectionTag::VECT, &vect));
        }
        if !quat.is_empty() {
            sections.push((SectionTag::QUAT, &quat));
        }
        if !self.custom.is_empty() {
            sections.push((SectionTag::CUST, &self.custom));
        }
        sections.push((SectionTag::DATA, &self.data));

        let hint_size = hints.map_or(0, |hints| hints.0.len() * 4);
        let table_start = HEADER_SIZE + hint_size;
        let mut offset = table_start + sections.len() * SECTION_ENTRY_SIZE;
        let mut table = Vec::with_capacity(sections.len() * SECTION_ENTRY_SIZE);
        for (tag, bytes) in &sections {
            table.extend_from_slice(&tag.bytes());
            table.extend_from_slice(&size_word(offset)?.to_be_bytes());
            table.extend_from_slice(&size_word(bytes.len())?.to_be_bytes());
            offset += bytes.len();
        }
        let file_size = size_word(offset)?;

        let flags = if hints.is_some() {
            HeaderFlags::ALLOCATION_HINTS.bits()
        } else {
            0
        };
        let mut out = Vec::with_capacity(offset);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_be_bytes());
        out.extend_from_slice(&flags.to_be_bytes());
        out.extend_from_slice(&SCHEMA.to_be_bytes());
        out.extend_from_slice(&file_size.to_be_bytes());
        out.extend_from_slice(&(sections.len() as u16).to_be_bytes());
        for word in hints.iter().flat_map(|hints| hints.0) {
            out.extend_from_slice(&word.to_be_bytes());
        }
        out.extend_from_slice(&table);
        for (_, bytes) in &sections {
            out.extend_from_slice(bytes);
        }

        Ok(out)
    }

    /// Writes the objects depth first: each object's record and children,
    /// then its timelines, as the reader reads them.
    fn tree(&mut self, root: &Object) -> Result<(), WriteError> {
        let class = self.object_head(root)?;
        let mut stack = alloc::vec![ObjectFrame {
            object: root,
            class,
            next_child: 0,
        }];

        while let Some(top) = stack.last_mut() {
            if let Some(child) = top.object.children.get(top.next_child) {
                top.next_child += 1;
                let class = self.object_head(child)?;
                stack.push(ObjectFrame {
                    object: child,
                    class,
                    next_child: 0,
                });
                continue;
            }

            let Some(done) = stack.pop() else {
                break;
            };
            if !done.object.named_frames.is_empty() || !done.object.timelines.is_empty() {
                self.timelines(done.object, done.class)?;
            }
        }

        Ok(())
    }

    /// Class, flags, properties and child count of one object.
    fn object_head(&mut self, object: &Object) -> Result<ClassRef<'r>, WriteError> {
        let Some(class) = self.registry.class(object.class) else {
            return Err(WriteError::ClassNotInRegistry {
                class: object.class,
            });
        };
        self.counts.objects += 1;

        if !object.timelines.is_empty() && object.children.is_empty() {
            return Err(WriteError::TimelinesWithoutChildren {
                class: class.name(),
            });
        }

        for property in &object.properties {
            let declared = class
                .levels()
                .any(|level| level.def().props.iter().any(|def| property.is(def)));
            if !declared {
                return Err(WriteError::PropertyNotInClass {
                    class: class.name(),
                    property: property.def.name,
                });
            }
        }

        let mut flags = 0;
        if !object.properties.is_empty() {
            flags |= ObjectFlags::PROPERTIES.bits();
        }
        if !object.children.is_empty() {
            flags |= ObjectFlags::CHILDREN.bits();
        }
        if !object.named_frames.is_empty() || !object.timelines.is_empty() {
            flags |= ObjectFlags::TIMELINES.bits();
        }

        let class_index = self.class_name_index(class)?;
        self.u16(class_index);
        self.u8(flags);

        if !object.properties.is_empty() {
            let count_at = self.data.len();
            self.u16(0);
            let mut entries = 0u32;
            for level in class.levels() {
                entries += self.block(level.def().props, &object.properties, class)?;
            }
            let Ok(entries) = u16::try_from(entries) else {
                return Err(WriteError::TooManyEntries {
                    class: class.name(),
                });
            };
            self.data[count_at..count_at + 2].copy_from_slice(&entries.to_be_bytes());
            self.counts.property_arrays += 1;
        }

        if !object.children.is_empty() {
            self.counts.parents += 1;
            self.u32(object.children.len() as u32);
        }

        Ok(class)
    }

    fn class_name_index(&mut self, class: ClassRef<'_>) -> Result<u16, WriteError> {
        if let Some(&index) = self.class_names.get(&class.id()) {
            return Ok(index);
        }

        let units: Vec<u16> = class.name().encode_utf16().collect();
        let index = self.string_units(&units, StringOwner::ClassName)?;
        self.class_names.insert(class.id(), index);

        Ok(index)
    }

    /// One property block over `defs`; returns its entry count. Compound
    /// values nest blocks; the depth is bounded by the static definition
    /// tables.
    fn block(
        &mut self,
        defs: &'static [PropDef],
        properties: &[Property],
        class: ClassRef<'_>,
    ) -> Result<u32, WriteError> {
        // Definitions at mask bit 32 or later can never be loaded.
        const MASK_BITS: usize = 32;

        let mut mask = 0u32;
        let mut present: Vec<(&'static PropDef, &Property)> = Vec::new();
        for (index, def) in defs.iter().enumerate() {
            let mut matching = properties.iter().filter(|property| property.is(def));
            let Some(property) = matching.next() else {
                continue;
            };
            if matching.next().is_some() {
                return Err(WriteError::DuplicateProperty { property: def.name });
            }

            if def.flags.is_indexed() && property.values.is_empty() {
                continue;
            }
            if index >= MASK_BITS {
                return Err(WriteError::UnreachableDefinition {
                    class: class.name(),
                    property: def.name,
                });
            }
            if !def.flags.is_indexed() && property.values.len() != 1 {
                return Err(WriteError::ValueCount {
                    property: def.name,
                    count: property.values.len(),
                });
            }

            mask |= 1 << index;
            present.push((def, property));
        }

        self.block_header(defs.len(), mask);

        let mut entries = 0u32;
        for (def, property) in present {
            if def.flags.is_indexed() {
                self.element_count(property.values.len() as u32);
            }
            for value in &property.values {
                entries += 1;
                self.counts.values += 1;
                self.value(def, value, class)?;
            }
        }

        Ok(entries)
    }

    /// Header byte: the mask size in the low three bits and the number of
    /// set mask bits minus one in the upper five; a block with no set bits
    /// is the byte 0 alone. Matched against the 6717 files.
    fn block_header(&mut self, def_count: usize, mask: u32) {
        if mask == 0 {
            self.u8(0);
            return;
        }

        let size: u8 = match def_count {
            0..=8 => 1,
            9..=16 => 2,
            _ => 4,
        };
        let set = mask.count_ones() as u8;
        self.u8(size | ((set - 1) << 3));
        match size {
            1 => self.u8(mask as u8),
            2 => self.u16(mask as u16),
            _ => self.u32(mask),
        }
    }

    fn element_count(&mut self, count: u32) {
        if count < 0x80 {
            self.u8(count as u8);
        } else if let Ok(count) = u8::try_from(count) {
            self.u8(0x81);
            self.u8(count);
        } else if let Ok(count) = u16::try_from(count) {
            self.u8(0x82);
            self.u16(count);
        } else {
            self.u8(0x84);
            self.u32(count);
        }
    }

    fn value(
        &mut self,
        def: &'static PropDef,
        value: &Value,
        class: ClassRef<'_>,
    ) -> Result<(), WriteError> {
        let mismatch = || WriteError::ValueType { property: def.name };

        match def.ty.kind() {
            PropKind::Compound(sub_defs) => {
                let Value::Compound(sub_properties) = value else {
                    return Err(mismatch());
                };
                for property in sub_properties {
                    if !sub_defs.iter().any(|sub| property.is(sub)) {
                        return Err(WriteError::PropertyNotInCompound {
                            compound: def.name,
                            property: property.def.name,
                        });
                    }
                }

                self.counts.property_arrays += 1;
                let count_at = self.data.len();
                self.u16(0);
                let entries = self.block(sub_defs, sub_properties, class)?;
                let Ok(entries) = u16::try_from(entries) else {
                    return Err(WriteError::CompoundTooManyEntries { compound: def.name });
                };
                self.data[count_at..count_at + 2].copy_from_slice(&entries.to_be_bytes());
            }
            PropKind::Scalar(scalar) => match (scalar, value) {
                (ScalarType::Bool, Value::Bool(value)) => self.u8(u8::from(*value)),
                (ScalarType::Integer, Value::Integer(value)) => self.u32(*value as u32),
                (ScalarType::Unsigned, Value::Unsigned(value)) => self.u32(*value),
                (ScalarType::Float, Value::Float(value)) => self.u32(value.to_bits()),
                (ScalarType::String, Value::String(text)) => {
                    let index =
                        self.string_index(text.as_ref(), StringOwner::Property(def.name))?;
                    self.u16(index);
                }
                (ScalarType::Color, Value::Color(color)) => self.u32(color.argb()),
                (ScalarType::Vector, Value::Vector(vector)) => {
                    let index = self.vectors.intern(&[
                        vector.x.to_bits(),
                        vector.y.to_bits(),
                        vector.z.to_bits(),
                    ]);
                    self.u32(index);
                }
                (ScalarType::Quaternion, Value::Quaternion(quaternion)) => {
                    let index = self.quaternions.intern(&[
                        quaternion.x.to_bits(),
                        quaternion.y.to_bits(),
                        quaternion.z.to_bits(),
                        quaternion.w.to_bits(),
                    ]);
                    self.u32(index);
                }
                (ScalarType::Custom, Value::Custom(blob)) => {
                    let offset = self.custom_blob(blob)?;
                    self.u32(offset);
                }
                _ => return Err(mismatch()),
            },
        }

        Ok(())
    }

    fn custom_blob(&mut self, blob: &[u8]) -> Result<u32, WriteError> {
        if let Some(&offset) = self.blobs.get(blob) {
            return Ok(offset);
        }

        let offset = size_word(self.custom.len())?;
        self.custom
            .extend_from_slice(&size_word(blob.len())?.to_be_bytes());
        self.custom.extend_from_slice(blob);
        self.blobs.insert(blob.to_vec(), offset);

        Ok(offset)
    }

    fn timelines(&mut self, object: &Object, class: ClassRef<'_>) -> Result<(), WriteError> {
        self.u32(object.named_frames.len() as u32);
        for frame in &object.named_frames {
            self.counts.named_frames += 1;
            let name = self.string_index(frame.name.as_ref(), StringOwner::NamedFrame)?;
            self.u16(name);
            self.u32(frame.time.0);
            self.u8(frame.command.0);
            let target = self.string_index(frame.target.as_ref(), StringOwner::NamedFrame)?;
            self.u16(target);
        }

        // The reader reads timelines only for objects with children.
        if object.children.is_empty() {
            return Ok(());
        }

        self.u32(object.timelines.len() as u32);
        for timeline in &object.timelines {
            self.timeline(object, timeline, class)?;
        }

        Ok(())
    }

    fn timeline(
        &mut self,
        object: &Object,
        timeline: &Timeline,
        class: ClassRef<'_>,
    ) -> Result<(), WriteError> {
        self.counts.timelines += 1;
        let element = self.string_index(timeline.element.as_ref(), StringOwner::Timeline)?;
        self.u16(element);
        self.u32(timeline.properties.len() as u32);

        // The reader searches the children only for timelines with
        // properties, and binds the first child with a matching `Id`.
        let target = if timeline.properties.is_empty() {
            None
        } else {
            let found = timeline
                .element
                .as_ref()
                .and_then(|id| object.child_by_id(id.units()));
            let Some(found) = found else {
                return Err(WriteError::TimelineTargetNotFound {
                    class: class.name(),
                    element: timeline.element.clone(),
                });
            };
            Some(found)
        };

        let mut value_types: Vec<ScalarType> = Vec::new();
        for property in &timeline.properties {
            self.counts.timeline_properties += 1;
            let element_flag = if property.element.is_some() { 0x80 } else { 0 };
            match (&property.path, target) {
                (None, _) => {
                    if !timeline.keyframes.is_empty() {
                        return Err(WriteError::EmptyPropertyPath);
                    }
                    self.u8(element_flag);
                }
                (Some(path), Some(target)) => {
                    self.path(path, target, element_flag)?;
                    if let Some(ty) = path.defs.last().and_then(|def| def.ty.scalar())
                        && ty != ScalarType::Custom
                    {
                        value_types.push(ty);
                    }
                }
                (Some(_), None) => unreachable!("timelines with properties have a target"),
            }
            if let Some(element) = property.element {
                self.u32(element.0);
            }
        }

        self.u32(timeline.keyframes.len() as u32);
        for keyframe in &timeline.keyframes {
            self.counts.keyframes += 1;
            if keyframe.values.len() != value_types.len() {
                return Err(WriteError::KeyframeValueCount {
                    expected: value_types.len(),
                    found: keyframe.values.len(),
                });
            }

            self.u32(keyframe.time.0);
            self.u8(keyframe.interpolation.0);
            self.u8(keyframe.ease_in.0);
            self.u8(keyframe.ease_out.0);
            self.u8(keyframe.ease_scale.0);
            for (ty, value) in value_types.iter().zip(&keyframe.values) {
                self.counts.keyframe_values += 1;
                self.keyframe_value(*ty, value)?;
            }
        }

        Ok(())
    }

    fn path(
        &mut self,
        path: &PropertyPath,
        target: &Object,
        element_flag: u8,
    ) -> Result<(), WriteError> {
        let Some(target_class) = self.registry.class(target.class) else {
            return Err(WriteError::ClassNotInRegistry {
                class: target.class,
            });
        };
        let invalid = || WriteError::InvalidPropertyPath {
            class: target_class.name(),
        };
        let Some(first) = path.defs.first() else {
            return Err(invalid());
        };

        let level = target_class
            .ancestor(path.levels_up.0)
            .ok_or_else(invalid)?;
        let declared = level
            .def()
            .props
            .get(usize::from(path.index.0))
            .ok_or_else(invalid)?;
        if !core::ptr::eq(declared, *first) {
            return Err(invalid());
        }
        let depth = u8::try_from(path.defs.len())
            .ok()
            .filter(|&depth| depth <= 0x7F)
            .ok_or_else(invalid)?;

        self.counts.path_defs += u32::from(depth);
        self.u8(depth | element_flag);
        self.u8(path.levels_up.0);
        self.u8(path.index.0);
        for pair in path.defs.windows(2) {
            let sub_defs = pair[0].ty.compound().ok_or_else(invalid)?;
            let position = sub_defs
                .iter()
                .position(|sub| core::ptr::eq(sub, pair[1]))
                .ok_or_else(invalid)?;
            self.u8(position as u8);
        }

        Ok(())
    }

    fn keyframe_value(&mut self, ty: ScalarType, value: &KeyframeValue) -> Result<(), WriteError> {
        match (ty, value) {
            (ScalarType::Bool, KeyframeValue::Bool(value)) => self.u8(u8::from(*value)),
            (ScalarType::Integer, KeyframeValue::Integer(value)) => self.u32(*value as u32),
            (ScalarType::Unsigned, KeyframeValue::Unsigned(value)) => self.u32(*value),
            (ScalarType::Float, KeyframeValue::Float(value)) => self.u32(value.to_bits()),
            (ScalarType::String, KeyframeValue::String(text)) => {
                let index = self.string_index(text.as_ref(), StringOwner::Keyframe)?;
                self.u16(index);
            }
            (ScalarType::Color, KeyframeValue::Color(color)) => self.u32(color.argb()),
            (ScalarType::Vector, KeyframeValue::Vector(vector)) => {
                let index = self.vectors.intern(&[
                    vector.x.to_bits(),
                    vector.y.to_bits(),
                    vector.z.to_bits(),
                ]);
                self.u32(index);
            }
            (ScalarType::Quaternion, KeyframeValue::Quaternion(quaternion)) => {
                let index = self.quaternions.intern(&[
                    quaternion.x.to_bits(),
                    quaternion.y.to_bits(),
                    quaternion.z.to_bits(),
                    quaternion.w.to_bits(),
                ]);
                self.u32(index);
            }
            _ => return Err(WriteError::KeyframeValueType),
        }

        Ok(())
    }

    /// One-based string index; 0 for the null string.
    fn string_index(
        &mut self,
        text: Option<&WideString>,
        owner: StringOwner,
    ) -> Result<u16, WriteError> {
        let Some(text) = text else {
            return Ok(0);
        };

        self.string_units(text.units(), owner)
    }

    fn string_units(&mut self, units: &[u16], owner: StringOwner) -> Result<u16, WriteError> {
        // A length with bit 15 set does not load consistently.
        const MAX_STRING_UNITS: usize = 0x7FFF;

        if units.len() > MAX_STRING_UNITS {
            return Err(WriteError::StringTooLong { owner });
        }
        // The reader ends strings at the first NUL.
        if units.contains(&0) {
            return Err(WriteError::StringHoldsNul { owner });
        }

        let index = self.strings.intern(units) + 1;

        u16::try_from(index).map_err(|_| WriteError::StringTableFull)
    }

    fn u8(&mut self, value: u8) {
        self.data.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }
}

/// Serializes `tree`. `registry` must hold every class of the tree, with
/// the definitions its properties refer to.
pub fn write(
    tree: &Tree,
    registry: &Registry,
    options: WriteOptions,
) -> Result<Vec<u8>, WriteError> {
    let mut writer = Writer {
        options,
        registry,
        strings: Interner::new(),
        class_names: BTreeMap::new(),
        vectors: Interner::new(),
        quaternions: Interner::new(),
        custom: Vec::new(),
        blobs: BTreeMap::new(),
        data: Vec::new(),
        counts: Counts::default(),
    };
    writer.tree(&tree.root)?;

    writer.finish()
}

fn size_word(size: usize) -> Result<u32, WriteError> {
    u32::try_from(size).map_err(|_| WriteError::TooLarge)
}

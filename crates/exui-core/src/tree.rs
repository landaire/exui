//! Owned XUR version 5 scene tree: the form [`crate::writer`] serializes,
//! built from a parsed [`Document`] or assembled by hand.
//!
//! Values are resolved: strings, vectors, quaternions and custom blobs are
//! held by value instead of as table references, so a tree can be edited
//! and written without tracking table layout.

use alloc::vec::Vec;

use crate::error::TreeError;
use crate::schema::ClassId;
use crate::schema::PropDef;
use crate::schema::Registry;
use crate::value::Color;
use crate::value::Quaternion;
use crate::value::Vector3;
use crate::wide::WideStr;
use crate::wide::WideString;
use crate::xur;
use crate::xur::Document;
use crate::xur::EaseIn;
use crate::xur::EaseOut;
use crate::xur::EaseScale;
use crate::xur::ElementIndex;
use crate::xur::FrameCommand;
use crate::xur::FrameTime;
use crate::xur::Interpolation;
use crate::xur::LevelsUp;
use crate::xur::ObjectRef;
use crate::xur::PropIndex;
use crate::xur::StringId;

/// A scene: one root object.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub root: Object,
}

impl Tree {
    /// Copies `document`. `registry` must be the one it was parsed with.
    /// Fails on table references the original would resolve to nothing
    /// (string, vector, quaternion or custom indices outside their tables),
    /// which a tree cannot express.
    pub fn from_document(document: &Document<'_>, registry: &Registry) -> Result<Self, TreeError> {
        // Objects are stored parents first; children are attached in reverse
        // file order so each object is complete when its parent takes it.
        let objects: Vec<ObjectRef<'_, '_>> = document.objects().collect();
        let mut built: Vec<Option<Object>> = Vec::with_capacity(objects.len());
        for object in &objects {
            built.push(Some(convert_object(document, registry, *object)?));
        }

        for object in objects.iter().rev() {
            let Some(parent) = object.parent() else {
                continue;
            };
            let Some(mut child) = built[object.id().index()].take() else {
                unreachable!("each object is taken once, by its parent");
            };
            // Its descendants follow it in file order, so they are attached.
            child.children.reverse();
            let Some(parent) = built[parent.id().index()].as_mut() else {
                unreachable!("parents precede their children in file order");
            };
            parent.children.push(child);
        }

        let Some(Some(mut root)) = built.into_iter().next() else {
            unreachable!("a parsed document has a root object");
        };
        root.children.reverse();

        Ok(Self { root })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub class: ClassId,
    /// Properties of any class level of `class`. The writer orders them by
    /// level and definition.
    pub properties: Vec<Property>,
    pub children: Vec<Object>,
    pub named_frames: Vec<NamedFrame>,
    /// Timelines animate children; the reader only reads them when the
    /// object has children.
    pub timelines: Vec<Timeline>,
}

impl Object {
    pub fn new(class: ClassId) -> Self {
        Self {
            class,
            properties: Vec::new(),
            children: Vec::new(),
            named_frames: Vec::new(),
            timelines: Vec::new(),
        }
    }

    /// The property stored for `def`, compared by address.
    pub fn property(&self, def: &PropDef) -> Option<&Property> {
        self.properties.iter().find(|property| property.is(def))
    }

    pub fn property_mut(&mut self, def: &PropDef) -> Option<&mut Property> {
        self.properties.iter_mut().find(|property| property.is(def))
    }

    /// The string of the first `Id` property that holds one.
    pub fn id(&self) -> Option<&WideString> {
        self.ids().next()
    }

    /// Strings of every `Id` property, as the timeline loader compares
    /// them: an `XuiEffect` has two.
    pub fn ids(&self) -> impl Iterator<Item = &WideString> {
        self.properties.iter().filter_map(|property| {
            if property.def.name != "Id" {
                return None;
            }
            match property.values.first() {
                Some(Value::String(Some(text))) => Some(text),
                _ => None,
            }
        })
    }

    pub fn has_id(&self, id: &[u16]) -> bool {
        self.ids().any(|text| text.units() == id)
    }

    /// The first child with an `Id` of `id`, the child a timeline for `id`
    /// animates.
    pub fn child_by_id(&self, id: &[u16]) -> Option<&Object> {
        self.children.iter().find(|child| child.has_id(id))
    }

    pub fn child_by_id_mut(&mut self, id: &[u16]) -> Option<&mut Object> {
        self.children.iter_mut().find(|child| child.has_id(id))
    }
}

/// Drops descendants with a worklist, so deep trees do not exhaust the
/// stack.
impl Drop for Object {
    fn drop(&mut self) {
        let mut pending = core::mem::take(&mut self.children);
        while let Some(mut child) = pending.pop() {
            pending.append(&mut child.children);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Property {
    pub def: &'static PropDef,
    /// One value, or the elements of an indexed property. An indexed
    /// property with no elements is the same as an absent one.
    pub values: Vec<Value>,
}

/// Definitions compare by address.
impl PartialEq for Property {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.def, other.def) && self.values == other.values
    }
}

impl Property {
    pub fn new(def: &'static PropDef, value: Value) -> Self {
        Self {
            def,
            values: alloc::vec![value],
        }
    }

    pub fn is(&self, def: &PropDef) -> bool {
        core::ptr::eq(self.def, def)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    /// `None` is the null string.
    String(Option<WideString>),
    Color(Color),
    Vector(Vector3),
    Quaternion(Quaternion),
    /// Blob contents, without the length prefix of the CUST section.
    Custom(Vec<u8>),
    /// Sub-properties of the compound definition.
    Compound(Vec<Property>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedFrame {
    pub name: Option<WideString>,
    pub time: FrameTime,
    pub command: FrameCommand,
    /// Named frame a jump command goes to.
    pub target: Option<WideString>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    /// `Id` of the animated child.
    pub element: Option<WideString>,
    pub properties: Vec<TimelineProperty>,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimelineProperty {
    /// `None` is a stored path of depth 0, which allows no keyframes.
    pub path: Option<PropertyPath>,
    pub element: Option<ElementIndex>,
}

impl TimelineProperty {
    /// Whether keyframes carry a value for this property.
    pub fn has_value(&self) -> bool {
        self.path
            .as_ref()
            .and_then(|path| path.defs.last())
            .is_some_and(|def| {
                def.ty
                    .scalar()
                    .is_some_and(|ty| ty != crate::schema::ScalarType::Custom)
            })
    }
}

/// A property of the animated child: definition `index` of the class
/// `levels_up` steps above the child's class, then sub-definitions through
/// compounds.
#[derive(Clone, Debug)]
pub struct PropertyPath {
    pub levels_up: LevelsUp,
    pub index: PropIndex,
    /// Definitions from the class level down; never empty.
    pub defs: Vec<&'static PropDef>,
}

/// Definitions compare by address.
impl PartialEq for PropertyPath {
    fn eq(&self, other: &Self) -> bool {
        self.levels_up == other.levels_up
            && self.index == other.index
            && self.defs.len() == other.defs.len()
            && self
                .defs
                .iter()
                .zip(&other.defs)
                .all(|(left, right)| core::ptr::eq(*left, *right))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    pub time: FrameTime,
    pub interpolation: Interpolation,
    pub ease_in: EaseIn,
    pub ease_out: EaseOut,
    pub ease_scale: EaseScale,
    /// One value per property whose [`TimelineProperty::has_value`] holds.
    pub values: Vec<KeyframeValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyframeValue {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    String(Option<WideString>),
    Color(Color),
    Vector(Vector3),
    Quaternion(Quaternion),
}

fn convert_object(
    document: &Document<'_>,
    registry: &Registry,
    object: ObjectRef<'_, '_>,
) -> Result<Object, TreeError> {
    let class = object.class();
    if registry.class(class).is_none() {
        return Err(TreeError::ClassNotInRegistry { class });
    }

    let named_frames = object
        .named_frames()
        .iter()
        .map(|frame| NamedFrame {
            name: string(document, frame.name),
            time: frame.time,
            command: frame.command,
            target: string(document, frame.target),
        })
        .collect();

    let mut timelines = Vec::with_capacity(object.timelines().len());
    for timeline in object.timelines() {
        timelines.push(convert_timeline(document, timeline));
    }

    Ok(Object {
        class,
        properties: convert_properties(document, object.properties())?,
        children: Vec::new(),
        named_frames,
        timelines,
    })
}

/// Compound nesting is bounded by the static definition tables.
fn convert_properties(
    document: &Document<'_>,
    properties: &[xur::Property],
) -> Result<Vec<Property>, TreeError> {
    let mut converted = Vec::with_capacity(properties.len());
    for property in properties {
        let def = property.def();
        let mut values = Vec::with_capacity(document.values(property).len());
        for value in document.values(property) {
            values.push(convert_value(document, def, value)?);
        }
        converted.push(Property { def, values });
    }

    Ok(converted)
}

fn convert_value(
    document: &Document<'_>,
    def: &'static PropDef,
    value: &xur::Value,
) -> Result<Value, TreeError> {
    let converted = match *value {
        xur::Value::Bool(value) => Value::Bool(value),
        xur::Value::Integer(value) => Value::Integer(value),
        xur::Value::Unsigned(value) => Value::Unsigned(value),
        xur::Value::Float(value) => Value::Float(value),
        xur::Value::String(None) => Value::String(None),
        xur::Value::String(Some(id)) => {
            let Some(text) = document.string(id) else {
                return Err(TreeError::InvalidString {
                    property: def.name,
                    index: id.get(),
                });
            };
            Value::String(Some(text.to_wide_string()))
        }
        xur::Value::Color(value) => Value::Color(value),
        xur::Value::Vector(index) => {
            let Some(vector) = document.vector(index) else {
                return Err(TreeError::InvalidVector {
                    property: def.name,
                    index: index.0,
                });
            };
            Value::Vector(vector)
        }
        xur::Value::Quaternion(index) => {
            let Some(quaternion) = document.quaternion(index) else {
                return Err(TreeError::InvalidQuaternion {
                    property: def.name,
                    index: index.0,
                });
            };
            Value::Quaternion(quaternion)
        }
        xur::Value::Custom(offset) => {
            let Some(blob) = document.custom_data(offset) else {
                return Err(TreeError::InvalidCustom {
                    property: def.name,
                    offset: offset.0,
                });
            };
            Value::Custom(blob.to_vec())
        }
        xur::Value::Compound(compound) => {
            Value::Compound(convert_properties(document, document.compound(compound))?)
        }
    };

    Ok(converted)
}

fn convert_timeline(document: &Document<'_>, timeline: &xur::Timeline) -> Timeline {
    let properties = document
        .timeline_properties(timeline)
        .iter()
        .map(|property| TimelineProperty {
            path: property.path.map(|path| PropertyPath {
                levels_up: path.levels_up,
                index: path.index,
                defs: document.path_defs(&path).to_vec(),
            }),
            element: property.element,
        })
        .collect();

    let keyframes = document
        .keyframes(timeline)
        .iter()
        .map(|keyframe| Keyframe {
            time: keyframe.time,
            interpolation: keyframe.interpolation,
            ease_in: keyframe.ease_in,
            ease_out: keyframe.ease_out,
            ease_scale: keyframe.ease_scale,
            values: document
                .keyframe_values(keyframe)
                .iter()
                .map(|value| convert_keyframe_value(document, value))
                .collect(),
        })
        .collect();

    Timeline {
        element: string(document, timeline.element),
        properties,
        keyframes,
    }
}

/// Keyframe strings were checked against the table at load.
fn convert_keyframe_value(document: &Document<'_>, value: &xur::KeyframeValue) -> KeyframeValue {
    match *value {
        xur::KeyframeValue::Bool(value) => KeyframeValue::Bool(value),
        xur::KeyframeValue::Integer(value) => KeyframeValue::Integer(value),
        xur::KeyframeValue::Unsigned(value) => KeyframeValue::Unsigned(value),
        xur::KeyframeValue::Float(value) => KeyframeValue::Float(value),
        xur::KeyframeValue::String(id) => KeyframeValue::String(string(document, id)),
        xur::KeyframeValue::Color(value) => KeyframeValue::Color(value),
        xur::KeyframeValue::Vector(value) => KeyframeValue::Vector(value),
        xur::KeyframeValue::Quaternion(value) => KeyframeValue::Quaternion(value),
    }
}

/// Indices checked at load; an index past the table cannot occur.
fn string(document: &Document<'_>, id: Option<StringId>) -> Option<WideString> {
    id.and_then(|id| document.string(id))
        .map(WideStr::to_wide_string)
}

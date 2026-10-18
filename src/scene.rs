//! Decompiled scene: owned elements with resolved values, built from a
//! version 5 [`Document`] or read from a version 8 file.

use std::ptr;

use exui_core::Document;
use exui_core::Registry;
use exui_core::custom::FigurePoint;
use exui_core::custom::FigurePoints;
use exui_core::schema::ClassDef;
use exui_core::schema::PropDef;
use exui_core::schema::xui;
use exui_core::value::Color;
use exui_core::value::Quaternion;
use exui_core::value::Vector3;
use exui_core::wide::WideStr;
use exui_core::xur;
use exui_core::xur::CustomOffset;
use exui_core::xur::EaseIn;
use exui_core::xur::EaseOut;
use exui_core::xur::EaseScale;
use exui_core::xur::ElementIndex;
use exui_core::xur::FrameCommand;
use exui_core::xur::FrameTime;
use exui_core::xur::Interpolation;
use exui_core::xur::KeyframeValue;
use exui_core::xur::PropIndex;
use exui_core::xur::StringId;

use crate::error::DecompileError;
use crate::error::SceneError;
use crate::v8;

/// XUI schema version of the file, written as the root `version` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaVersion(pub u16);

/// Index of an element in its [`Scene`]; the root is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ElementId(pub usize);

#[derive(Clone, Debug)]
pub struct Scene {
    pub schema: SchemaVersion,
    /// Elements in file order, parents before children; the root is first.
    pub elements: Vec<Element>,
}

impl Scene {
    /// Reads a XUR file of version 5 or 8.
    pub fn parse(data: &[u8], registry: &Registry) -> Result<Self, SceneError> {
        if data.get(4..8) == Some(&8u32.to_be_bytes()) {
            return Ok(v8::parse(data, registry)?);
        }

        let document = Document::parse(data, registry)?;

        Ok(Self::from_document(&document, registry)?)
    }

    /// `registry` must be the one `document` was parsed with.
    pub fn from_document(
        document: &Document<'_>,
        registry: &Registry,
    ) -> Result<Self, DecompileError> {
        // Document::parse only accepts schema 0xC.
        const SCHEMA: SchemaVersion = SchemaVersion(0xC);

        let mut elements = Vec::new();
        for object in document.objects() {
            let class_id = object.class();
            let Some(class) = registry.class(class_id) else {
                return Err(DecompileError::ClassNotInRegistry { class: class_id });
            };

            let named_frames = object
                .named_frames()
                .iter()
                .map(|frame| NamedFrame {
                    name: document_string(document, frame.name),
                    time: frame.time,
                    command: frame.command,
                })
                .collect();

            let mut timelines = Vec::new();
            for timeline in object.timelines() {
                timelines.push(convert_timeline(document, timeline)?);
            }

            elements.push(Element {
                class: ElementClass::Known(class.def()),
                properties: convert_properties(document, object.properties())?,
                children: object
                    .children()
                    .map(|child| ElementId(child.id().index()))
                    .collect(),
                named_frames,
                timelines,
            });
        }

        Ok(Self {
            schema: SCHEMA,
            elements,
        })
    }

    pub fn root(&self) -> &Element {
        &self.elements[0]
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(id.0)
    }
}

#[derive(Clone, Debug)]
pub enum ElementClass {
    Known(&'static ClassDef),
    /// A class the registry lacks. Only version 8 files load these.
    Unknown(String),
}

impl ElementClass {
    pub fn name(&self) -> &str {
        match self {
            Self::Known(def) => def.name,
            Self::Unknown(name) => name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Element {
    pub class: ElementClass,
    /// Properties of every class level, root base class first.
    pub properties: Vec<Property>,
    pub children: Vec<ElementId>,
    pub named_frames: Vec<NamedFrame>,
    pub timelines: Vec<Timeline>,
}

#[derive(Clone, Debug)]
pub struct Property {
    /// `None` for a version 8 value of a class level without known
    /// definitions.
    pub def: Option<&'static PropDef>,
    pub index: PropIndex,
    /// One value, or the elements of an indexed property.
    pub values: Vec<Value>,
}

impl Property {
    pub fn is_indexed(&self) -> bool {
        self.def.is_some_and(|def| def.flags.is_indexed())
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Scalar(Scalar),
    Points(Vec<FigurePoint>),
    /// Sub-properties in definition order.
    Compound(Vec<Property>),
}

/// A single stored value.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    /// `None` is the null string.
    String(Option<String>),
    Color(Color),
    Vector(Vector3),
    Quaternion(Quaternion),
    /// Version 8 value stored without a known definition.
    Raw(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedFrame {
    pub name: Option<String>,
    pub time: FrameTime,
    pub command: FrameCommand,
}

#[derive(Clone, Debug)]
pub struct Timeline {
    /// `Id` of the animated child.
    pub element: Option<String>,
    pub properties: Vec<TimelineProperty>,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Clone, Debug)]
pub struct TimelineProperty {
    /// Definitions from the class level down through compounds. `None`
    /// marks a version 8 step without a known definition.
    pub path: Vec<Option<&'static PropDef>>,
    pub element: Option<ElementIndex>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    pub time: FrameTime,
    pub interpolation: Interpolation,
    pub ease_in: EaseIn,
    pub ease_out: EaseOut,
    pub ease_scale: EaseScale,
    pub values: Vec<Scalar>,
}

/// Decodes a custom value. `XuiFigure.Points` is the only custom type.
pub(crate) fn custom_value(
    def: &'static PropDef,
    offset: CustomOffset,
    blob: Option<&[u8]>,
) -> Result<Value, DecompileError> {
    let is_points = xui::XUI_FIGURE
        .props
        .iter()
        .any(|figure_def| figure_def.name == "Points" && ptr::eq(figure_def, def));
    if !is_points {
        return Err(DecompileError::UnsupportedCustom { property: def.name });
    }

    let Some(blob) = blob else {
        return Err(DecompileError::CustomOutOfBounds {
            property: def.name,
            offset,
        });
    };

    // The original ignores an incomplete point list; the text form has no
    // way to express that.
    let points = FigurePoints::parse(blob).and_then(|figure| figure.points);
    let Some(points) = points else {
        return Err(DecompileError::MalformedFigurePoints { offset });
    };

    Ok(Value::Points(points))
}

pub(crate) fn wide_to_string(text: WideStr<'_>) -> String {
    text.chars()
        .map(|decoded| decoded.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// An index past the table reads as the null string, as in the original.
fn document_string(document: &Document<'_>, id: Option<StringId>) -> Option<String> {
    id.and_then(|id| document.string(id)).map(wide_to_string)
}

/// Compound nesting is bounded by the static definition tables, so the
/// recursion depth does not depend on the input.
fn convert_properties(
    document: &Document<'_>,
    properties: &[xur::Property],
) -> Result<Vec<Property>, DecompileError> {
    let mut converted = Vec::with_capacity(properties.len());
    for property in properties {
        let def = property.def();
        let mut values = Vec::new();
        for value in document.values(property) {
            values.push(convert_value(document, def, value)?);
        }

        converted.push(Property {
            def: Some(def),
            index: property.index(),
            values,
        });
    }

    Ok(converted)
}

fn convert_value(
    document: &Document<'_>,
    def: &'static PropDef,
    value: &xur::Value,
) -> Result<Value, DecompileError> {
    let scalar = match *value {
        xur::Value::Bool(value) => Scalar::Bool(value),
        xur::Value::Integer(value) => Scalar::Integer(value),
        xur::Value::Unsigned(value) => Scalar::Unsigned(value),
        xur::Value::Float(value) => Scalar::Float(value),
        xur::Value::String(id) => Scalar::String(document_string(document, id)),
        xur::Value::Color(value) => Scalar::Color(value),
        xur::Value::Vector(index) => {
            let Some(vector) = document.vector(index) else {
                return Err(DecompileError::InvalidVectorIndex {
                    property: def.name,
                    index: index.0,
                });
            };
            Scalar::Vector(vector)
        }
        xur::Value::Quaternion(index) => {
            let Some(quaternion) = document.quaternion(index) else {
                return Err(DecompileError::InvalidQuaternionIndex {
                    property: def.name,
                    index: index.0,
                });
            };
            Scalar::Quaternion(quaternion)
        }
        xur::Value::Custom(offset) => {
            return custom_value(def, offset, document.custom_data(offset));
        }
        xur::Value::Compound(compound) => {
            let properties = convert_properties(document, document.compound(compound))?;
            return Ok(Value::Compound(properties));
        }
    };

    Ok(Value::Scalar(scalar))
}

fn convert_timeline(
    document: &Document<'_>,
    timeline: &xur::Timeline,
) -> Result<Timeline, DecompileError> {
    let mut properties = Vec::new();
    for property in document.timeline_properties(timeline) {
        let Some(path) = &property.path else {
            return Err(DecompileError::EmptyPropertyPath);
        };

        properties.push(TimelineProperty {
            path: document.path_defs(path).iter().copied().map(Some).collect(),
            element: property.element,
        });
    }

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

    Ok(Timeline {
        element: document_string(document, timeline.element),
        properties,
        keyframes,
    })
}

fn convert_keyframe_value(document: &Document<'_>, value: &KeyframeValue) -> Scalar {
    match *value {
        KeyframeValue::Bool(value) => Scalar::Bool(value),
        KeyframeValue::Integer(value) => Scalar::Integer(value),
        KeyframeValue::Unsigned(value) => Scalar::Unsigned(value),
        KeyframeValue::Float(value) => Scalar::Float(value),
        KeyframeValue::String(id) => Scalar::String(document_string(document, id)),
        KeyframeValue::Color(value) => Scalar::Color(value),
        KeyframeValue::Vector(value) => Scalar::Vector(value),
        KeyframeValue::Quaternion(value) => Scalar::Quaternion(value),
    }
}

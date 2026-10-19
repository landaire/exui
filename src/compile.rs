//! XUI text to XUR: reads the text form [`crate::xui::Xml`] writes, and XUI
//! files of the XUI tool, into a [`Tree`] for the exui-core writer.

use std::ptr;

use exui_core::Registry;
use exui_core::WriteError;
use exui_core::custom::FigurePoint;
use exui_core::schema::ClassRef;
use exui_core::schema::PropDef;
use exui_core::schema::PropKind;
use exui_core::schema::ScalarType;
use exui_core::schema::xui;
use exui_core::tree::Keyframe;
use exui_core::tree::KeyframeValue;
use exui_core::tree::NamedFrame;
use exui_core::tree::Object;
use exui_core::tree::Property;
use exui_core::tree::PropertyPath;
use exui_core::tree::Timeline;
use exui_core::tree::TimelineProperty;
use exui_core::tree::Tree;
use exui_core::tree::Value;
use exui_core::value::Color;
use exui_core::value::Quaternion;
use exui_core::value::Vector3;
use exui_core::wide::WideString;
use exui_core::writer;
use exui_core::writer::WriteOptions;
use exui_core::xur::EaseIn;
use exui_core::xur::EaseOut;
use exui_core::xur::EaseScale;
use exui_core::xur::ElementIndex;
use exui_core::xur::FrameCommand;
use exui_core::xur::FrameTime;
use exui_core::xur::Interpolation;
use exui_core::xur::LevelsUp;
use exui_core::xur::PropIndex;

use crate::error::CompileError;
use crate::markup;
use crate::markup::Node;
use crate::scene::FigurePath;
use crate::xui::ALLOCATION_HINTS_ATTRIBUTE;
use crate::xui::DEFAULT_EASE_IN;
use crate::xui::DEFAULT_EASE_OUT;
use crate::xui::DEFAULT_EASE_SCALE;
use crate::xui::FRAME_COMMANDS;

/// A XUI document read into the form the XUR writer takes.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledScene {
    pub tree: Tree,
    /// From the root `allocationhints` attribute.
    pub options: WriteOptions,
}

impl CompiledScene {
    /// Reads XUI text. Class and property names resolve through `registry`.
    pub fn parse(text: &str, registry: &Registry) -> Result<Self, CompileError> {
        // The XUR writer produces schema 0xC only.
        const SCHEMA: u16 = 0xC;

        let root = markup::parse(text)?;

        let Some(version) = root.attribute("version") else {
            return Err(CompileError::MissingAttribute {
                line: root.line,
                element: root.name.clone(),
                attribute: "version",
            });
        };
        if u16::from_str_radix(version, 16) != Ok(SCHEMA) {
            return Err(CompileError::UnsupportedVersion {
                line: root.line,
                version: version.to_string(),
            });
        }

        let allocation_hints = match root.attribute(ALLOCATION_HINTS_ATTRIBUTE) {
            None | Some("false") => false,
            Some("true") => true,
            Some(other) => {
                return Err(CompileError::BadAttribute {
                    line: root.line,
                    element: root.name.clone(),
                    attribute: ALLOCATION_HINTS_ATTRIBUTE,
                    text: other.to_string(),
                });
            }
        };

        Ok(Self {
            tree: Tree {
                root: objects(&root, registry)?,
            },
            options: WriteOptions { allocation_hints },
        })
    }

    /// XUR bytes of the scene. `registry` must be the one it was parsed
    /// with.
    pub fn to_xur(&self, registry: &Registry) -> Result<Vec<u8>, WriteError> {
        writer::write(&self.tree, registry, self.options)
    }
}

/// Compiles XUI text to XUR bytes.
pub fn compile_scene(text: &str, registry: &Registry) -> Result<Vec<u8>, CompileError> {
    let scene = CompiledScene::parse(text, registry)?;

    Ok(scene.to_xur(registry)?)
}

/// Occurrences of one property name in a `Properties` element.
struct NameUse<'n> {
    name: &'n str,
    count: usize,
    /// Definition position of the latest occurrence.
    latest: usize,
}

/// Builds the object tree bottom-up: an object is built once its children
/// are, so its timelines can resolve against them.
fn objects(root: &Node, registry: &Registry) -> Result<Object, CompileError> {
    enum Step<'n> {
        Enter(&'n Node),
        Exit(&'n Node),
    }

    let mut stack = vec![Step::Enter(root)];
    let mut built: Vec<Object> = Vec::new();
    while let Some(step) = stack.pop() {
        match step {
            Step::Enter(node) => {
                stack.push(Step::Exit(node));
                stack.extend(
                    node.children
                        .iter()
                        .rev()
                        .filter(|child| is_object(child))
                        .map(Step::Enter),
                );
            }
            Step::Exit(node) => {
                let child_count = node
                    .children
                    .iter()
                    .filter(|child| is_object(child))
                    .count();
                let children = built.split_off(built.len() - child_count);
                let allowed: &[&str] = if ptr::eq(node, root) {
                    &["version", ALLOCATION_HINTS_ATTRIBUTE]
                } else {
                    &[]
                };
                built.push(object(node, allowed, children, registry)?);
            }
        }
    }

    let Some(root) = built.pop() else {
        unreachable!("the root is built last");
    };

    Ok(root)
}

fn is_object(node: &Node) -> bool {
    node.name != "Properties" && node.name != "Timelines"
}

fn object(
    node: &Node,
    attributes: &[&str],
    children: Vec<Object>,
    registry: &Registry,
) -> Result<Object, CompileError> {
    let class = registry.find(&node.name).and_then(|id| registry.class(id));
    let Some(class) = class else {
        return Err(CompileError::UnknownClass {
            line: node.line,
            name: node.name.clone(),
        });
    };
    node.expect_attributes(attributes)?;
    node.expect_no_text()?;

    let mut object = Object::new(class.id());
    object.children = children;

    let mut seen_properties = false;
    let mut seen_timelines = false;
    for child in &node.children {
        match child.name.as_str() {
            "Properties" => {
                once(child, &mut seen_properties)?;
                let defs: Vec<&'static PropDef> = class
                    .levels()
                    .flat_map(|level| level.def().props.iter())
                    .collect();
                object.properties = properties(child, &defs, class.name())?;
            }
            "Timelines" => {
                once(child, &mut seen_timelines)?;
                timelines(child, &mut object, registry)?;
            }
            _ => {
                // A child object, built already.
            }
        }
    }

    Ok(object)
}

/// Fails on the second element of a kind that may appear once.
fn once(node: &Node, seen: &mut bool) -> Result<(), CompileError> {
    if *seen {
        return Err(CompileError::DuplicateElement {
            line: node.line,
            element: node.name.clone(),
        });
    }
    *seen = true;

    Ok(())
}

/// Properties of `defs` in definition order. A name declared more than
/// once maps its occurrences to the declarations in order; an `index`
/// above 0 continues the latest occurrence. Compound nesting is bounded by
/// the static definition tables.
fn properties(
    node: &Node,
    defs: &[&'static PropDef],
    owner: &'static str,
) -> Result<Vec<Property>, CompileError> {
    node.expect_attributes(&[])?;
    node.expect_no_text()?;

    let mut slots: Vec<Option<Property>> = vec![None; defs.len()];
    let mut uses: Vec<NameUse<'_>> = Vec::new();
    for element in &node.children {
        let line = element.line;
        let index = match element.attribute("index") {
            Some(text) => Some(parse_attribute::<u32>(element, "index", text)?),
            None => None,
        };
        let name_use = uses
            .iter_mut()
            .find(|name_use| name_use.name == element.name);

        let position = match (index, name_use) {
            (Some(found), name_use) if found > 0 => {
                let Some(name_use) = name_use else {
                    return Err(CompileError::IndexOutOfOrder {
                        line,
                        property: element.name.clone(),
                        expected: 0,
                        found,
                    });
                };
                let position = name_use.latest;
                let expected = slots[position].as_ref().map_or(0, |slot| slot.values.len());
                if usize::try_from(found) != Ok(expected) {
                    return Err(CompileError::IndexOutOfOrder {
                        line,
                        property: element.name.clone(),
                        expected,
                        found,
                    });
                }
                position
            }
            (_, name_use) => {
                let occurrence = name_use.as_ref().map_or(0, |name_use| name_use.count);
                let declared = defs
                    .iter()
                    .enumerate()
                    .filter(|(_, def)| def.name == element.name)
                    .nth(occurrence)
                    .map(|(position, _)| position);
                let Some(position) = declared else {
                    if occurrence > 0 {
                        return Err(CompileError::DuplicateProperty {
                            line,
                            property: element.name.clone(),
                        });
                    }
                    return Err(CompileError::UnknownProperty {
                        line,
                        owner,
                        name: element.name.clone(),
                    });
                };
                match name_use {
                    Some(name_use) => {
                        name_use.count += 1;
                        name_use.latest = position;
                    }
                    None => uses.push(NameUse {
                        name: &element.name,
                        count: 1,
                        latest: position,
                    }),
                }
                slots[position] = Some(Property {
                    def: defs[position],
                    values: Vec::new(),
                });
                position
            }
        };

        let def = defs[position];
        match (def.flags.is_indexed(), index) {
            (true, None) => {
                return Err(CompileError::MissingIndex {
                    line,
                    property: def.name,
                });
            }
            (false, Some(_)) => {
                return Err(CompileError::UnexpectedIndex {
                    line,
                    property: def.name,
                });
            }
            _ => {}
        }

        let value = value(element, def)?;
        if let Some(slot) = slots[position].as_mut() {
            slot.values.push(value);
        }
    }

    Ok(slots.into_iter().flatten().collect())
}

fn value(node: &Node, def: &'static PropDef) -> Result<Value, CompileError> {
    let scalar = match def.ty.kind() {
        PropKind::Compound(sub_defs) => {
            node.expect_attributes(&["index"])?;
            node.expect_no_text()?;

            let defs: Vec<&'static PropDef> = sub_defs.iter().collect();
            let mut sub_properties = Vec::new();
            let mut seen = false;
            for child in &node.children {
                if child.name != "Properties" {
                    return Err(CompileError::UnexpectedElement {
                        line: child.line,
                        element: child.name.clone(),
                        parent: node.name.clone(),
                    });
                }
                once(child, &mut seen)?;
                sub_properties = properties(child, &defs, def.name)?;
            }
            return Ok(Value::Compound(sub_properties));
        }
        PropKind::Scalar(ScalarType::Custom) => {
            if !is_figure_points(def) {
                return Err(CompileError::UnsupportedCustom {
                    line: node.line,
                    property: def.name,
                });
            }
            return Ok(Value::Custom(figure_path(node)?.to_blob()));
        }
        PropKind::Scalar(ty) => {
            node.expect_attributes(&["index"])?;
            let text = node.leaf_text()?;
            let Some(value) = parse_scalar(ty, text) else {
                return Err(CompileError::BadValue {
                    line: node.line,
                    property: def.name,
                    text: text.to_string(),
                });
            };
            value
        }
    };

    let value = match scalar {
        KeyframeValue::Bool(value) => Value::Bool(value),
        KeyframeValue::Integer(value) => Value::Integer(value),
        KeyframeValue::Unsigned(value) => Value::Unsigned(value),
        KeyframeValue::Float(value) => Value::Float(value),
        KeyframeValue::String(value) => Value::String(value),
        KeyframeValue::Color(value) => Value::Color(value),
        KeyframeValue::Vector(value) => Value::Vector(value),
        KeyframeValue::Quaternion(value) => Value::Quaternion(value),
    };

    Ok(value)
}

fn is_figure_points(def: &'static PropDef) -> bool {
    xui::XUI_FIGURE
        .props
        .iter()
        .any(|figure_def| figure_def.name == "Points" && ptr::eq(figure_def, def))
}

/// `count,` then per point six coordinates and a type, each followed by a
/// comma. The type is not stored. Without `width` and `height` the size is
/// the anchor extent, as the XUI tool computes it.
fn figure_path(node: &Node) -> Result<FigurePath, CompileError> {
    const FIELDS_PER_POINT: usize = 7;

    node.expect_attributes(&["index", "width", "height"])?;
    let text = node.leaf_text()?;
    let line = node.line;

    let mut fields: Vec<&str> = text.split(',').map(str::trim).collect();
    if fields.last() == Some(&"") {
        fields.pop();
    }
    let bad_field = |field: usize| CompileError::BadPoints { line, field };

    let Some((count, point_fields)) = fields.split_first() else {
        return Err(bad_field(0));
    };
    let Ok(declared) = count.parse::<u32>() else {
        return Err(bad_field(0));
    };
    if usize::try_from(declared)
        .ok()
        .and_then(|count| count.checked_mul(FIELDS_PER_POINT))
        != Some(point_fields.len())
    {
        return Err(CompileError::PointCount {
            line,
            declared,
            fields: point_fields.len(),
        });
    }

    let mut points = Vec::with_capacity(point_fields.len() / FIELDS_PER_POINT);
    for (point, record) in point_fields.chunks(FIELDS_PER_POINT).enumerate() {
        let first_field = 1 + point * FIELDS_PER_POINT;
        let mut coords = [0.0; 6];
        for (offset, coord) in coords.iter_mut().enumerate() {
            *coord = record[offset]
                .parse()
                .map_err(|_| bad_field(first_field + offset))?;
        }
        if record[6].parse::<i32>().is_err() {
            return Err(bad_field(first_field + 6));
        }
        points.push(FigurePoint { coords });
    }

    let (width, height) = match (node.attribute("width"), node.attribute("height")) {
        (Some(width), Some(height)) => (
            parse_attribute::<f32>(node, "width", width)?,
            parse_attribute::<f32>(node, "height", height)?,
        ),
        (None, None) => FigurePath::anchor_extent(&points),
        (Some(_), None) => {
            return Err(CompileError::MissingAttribute {
                line,
                element: node.name.clone(),
                attribute: "height",
            });
        }
        (None, Some(_)) => {
            return Err(CompileError::MissingAttribute {
                line,
                element: node.name.clone(),
                attribute: "width",
            });
        }
    };

    Ok(FigurePath {
        width,
        height,
        points,
    })
}

fn timelines(node: &Node, object: &mut Object, registry: &Registry) -> Result<(), CompileError> {
    node.expect_attributes(&[])?;
    node.expect_no_text()?;

    let mut seen_frames = false;
    for child in &node.children {
        match child.name.as_str() {
            "NamedFrames" => {
                once(child, &mut seen_frames)?;
                child.expect_attributes(&[])?;
                child.expect_no_text()?;
                let mut frames = Vec::with_capacity(child.children.len());
                for frame in &child.children {
                    frames.push(named_frame(frame, &child.name)?);
                }
                object.named_frames = frames;
            }
            "Timeline" => {
                let timeline = timeline(child, object, registry)?;
                object.timelines.push(timeline);
            }
            _ => {
                return Err(CompileError::UnexpectedElement {
                    line: child.line,
                    element: child.name.clone(),
                    parent: node.name.clone(),
                });
            }
        }
    }

    Ok(())
}

/// Fields of a record element such as `NamedFrame` or `KeyFrame`, each at
/// most once except those listed as repeated.
struct Fields<'n> {
    node: &'n Node,
    found: Vec<&'n Node>,
}

impl<'n> Fields<'n> {
    fn parse(node: &'n Node, names: &[&str], repeated: &[&str]) -> Result<Self, CompileError> {
        node.expect_attributes(&[])?;
        node.expect_no_text()?;

        let mut found: Vec<&'n Node> = Vec::new();
        for child in &node.children {
            if !names.contains(&child.name.as_str()) {
                return Err(CompileError::UnexpectedElement {
                    line: child.line,
                    element: child.name.clone(),
                    parent: node.name.clone(),
                });
            }
            let repeats = repeated.contains(&child.name.as_str());
            if !repeats && found.iter().any(|other| other.name == child.name) {
                return Err(CompileError::DuplicateElement {
                    line: child.line,
                    element: child.name.clone(),
                });
            }
            found.push(child);
        }

        Ok(Self { node, found })
    }

    fn get(&self, name: &str) -> Option<&'n Node> {
        self.found.iter().copied().find(|field| field.name == name)
    }

    fn all(&self, name: &'n str) -> impl Iterator<Item = &'n Node> + '_ {
        self.found
            .iter()
            .copied()
            .filter(move |field| field.name == name)
    }

    fn require(&self, name: &'static str) -> Result<&'n Node, CompileError> {
        self.get(name).ok_or_else(|| CompileError::MissingElement {
            line: self.node.line,
            element: name,
            parent: self.node.name.clone(),
        })
    }

    /// Text of an optional string field; empty text is the null string.
    fn string(&self, name: &str) -> Result<Option<WideString>, CompileError> {
        let Some(field) = self.get(name) else {
            return Ok(None);
        };

        Ok(wide(field.leaf_text()?))
    }

    fn number<T: std::str::FromStr>(&self, name: &'static str) -> Result<T, CompileError> {
        parse_number(self.require(name)?)
    }

    fn number_or<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, CompileError> {
        match self.get(name) {
            Some(field) => parse_number(field),
            None => Ok(default),
        }
    }
}

fn named_frame(node: &Node, parent: &str) -> Result<NamedFrame, CompileError> {
    if node.name != "NamedFrame" {
        return Err(CompileError::UnexpectedElement {
            line: node.line,
            element: node.name.clone(),
            parent: parent.to_string(),
        });
    }
    let fields = Fields::parse(node, &["Name", "Time", "Command", "CommandParams"], &[])?;

    let command = match fields.get("Command") {
        None => FrameCommand(0),
        Some(field) => {
            let text = field.leaf_text()?.trim();
            let known = FRAME_COMMANDS.iter().position(|command| *command == text);
            match known {
                Some(command) => FrameCommand(command as u8),
                None => match text.parse::<u8>() {
                    Ok(command) => FrameCommand(command),
                    Err(_) => {
                        return Err(CompileError::UnknownCommand {
                            line: field.line,
                            text: text.to_string(),
                        });
                    }
                },
            }
        }
    };

    Ok(NamedFrame {
        name: fields.string("Name")?,
        time: FrameTime(fields.number("Time")?),
        command,
        target: fields.string("CommandParams")?,
    })
}

fn timeline(node: &Node, object: &Object, registry: &Registry) -> Result<Timeline, CompileError> {
    let fields = Fields::parse(
        node,
        &["Id", "TimelineProp", "KeyFrame"],
        &["TimelineProp", "KeyFrame"],
    )?;
    let element = fields.string("Id")?;

    let mut properties = Vec::new();
    for field in fields.all("TimelineProp") {
        properties.push(timeline_property(
            field,
            element.as_ref(),
            object,
            registry,
        )?);
    }

    let value_types: Vec<(&'static PropDef, ScalarType)> = properties
        .iter()
        .filter_map(|property| {
            let def = *property.path.as_ref()?.defs.last()?;
            match def.ty.scalar() {
                Some(ScalarType::Custom) | None => None,
                Some(ty) => Some((def, ty)),
            }
        })
        .collect();

    let mut keyframes = Vec::new();
    for field in fields.all("KeyFrame") {
        keyframes.push(keyframe(field, &value_types)?);
    }

    Ok(Timeline {
        element,
        properties,
        keyframes,
    })
}

fn timeline_property(
    node: &Node,
    id: Option<&WideString>,
    object: &Object,
    registry: &Registry,
) -> Result<TimelineProperty, CompileError> {
    node.expect_attributes(&["index"])?;
    let element = match node.attribute("index") {
        Some(text) => Some(ElementIndex(parse_attribute(node, "index", text)?)),
        None => None,
    };

    let text = node.leaf_text()?.trim();
    if text.is_empty() {
        return Ok(TimelineProperty {
            path: None,
            element,
        });
    }

    let target = id.and_then(|id| object.children.iter().find(|child| child.id() == Some(id)));
    let class = target.and_then(|target| registry.class(target.class));
    let Some(class) = class else {
        return Err(CompileError::UnknownTimelineTarget {
            line: node.line,
            id: id.map(ToString::to_string),
        });
    };

    let Some(path) = resolve_path(class, text) else {
        return Err(CompileError::UnknownTimelineProperty {
            line: node.line,
            class: class.name(),
            path: text.to_string(),
        });
    };

    Ok(TimelineProperty {
        path: Some(path),
        element,
    })
}

/// `Name.Sub.Sub`: the first name resolves through the class levels root
/// first, as property names do; later ones through compound definitions.
fn resolve_path(class: ClassRef<'_>, text: &str) -> Option<PropertyPath> {
    let mut names = text.split('.');
    let first = names.next()?;

    let levels: Vec<ClassRef<'_>> = class.levels().collect();
    let (level, index, def) = levels.iter().enumerate().find_map(|(level, class)| {
        let props = class.def().props;
        let index = props.iter().position(|def| def.name == first)?;
        Some((level, index, &props[index]))
    })?;

    let mut defs = vec![def];
    for name in names {
        let sub_defs = defs.last()?.ty.compound()?;
        defs.push(sub_defs.iter().find(|def| def.name == name)?);
    }

    Some(PropertyPath {
        levels_up: LevelsUp(u8::try_from(levels.len() - 1 - level).ok()?),
        index: PropIndex(u8::try_from(index).ok()?),
        defs,
    })
}

fn keyframe(
    node: &Node,
    value_types: &[(&'static PropDef, ScalarType)],
) -> Result<Keyframe, CompileError> {
    let fields = Fields::parse(
        node,
        &[
            "Time",
            "Interpolation",
            "EaseIn",
            "EaseOut",
            "EaseScale",
            "Prop",
        ],
        &["Prop"],
    )?;

    let props: Vec<&Node> = fields.all("Prop").collect();
    if props.len() != value_types.len() {
        return Err(CompileError::KeyframeValueCount {
            line: node.line,
            expected: value_types.len(),
            found: props.len(),
        });
    }

    let mut values = Vec::with_capacity(props.len());
    for (prop, (def, ty)) in props.iter().zip(value_types) {
        let text = prop.leaf_text()?;
        let Some(value) = parse_scalar(*ty, text) else {
            return Err(CompileError::BadValue {
                line: prop.line,
                property: def.name,
                text: text.to_string(),
            });
        };
        values.push(value);
    }

    Ok(Keyframe {
        time: FrameTime(fields.number("Time")?),
        interpolation: Interpolation(fields.number("Interpolation")?),
        ease_in: EaseIn(fields.number_or("EaseIn", DEFAULT_EASE_IN)?),
        ease_out: EaseOut(fields.number_or("EaseOut", DEFAULT_EASE_OUT)?),
        ease_scale: EaseScale(fields.number_or("EaseScale", DEFAULT_EASE_SCALE)?),
        values,
    })
}

/// A scalar in the text form of its type. Strings are taken as written;
/// other values may be surrounded by whitespace.
fn parse_scalar(ty: ScalarType, text: &str) -> Option<KeyframeValue> {
    if ty == ScalarType::String {
        return Some(KeyframeValue::String(wide(text)));
    }

    let text = text.trim();
    let value = match ty {
        ScalarType::Bool => match text {
            "true" => KeyframeValue::Bool(true),
            "false" => KeyframeValue::Bool(false),
            _ => return None,
        },
        ScalarType::Integer => KeyframeValue::Integer(text.parse().ok()?),
        ScalarType::Unsigned => KeyframeValue::Unsigned(text.parse().ok()?),
        ScalarType::Float => KeyframeValue::Float(text.parse().ok()?),
        ScalarType::Color => {
            let digits = text
                .strip_prefix("0x")
                .or_else(|| text.strip_prefix("0X"))?;
            KeyframeValue::Color(Color::from_argb(u32::from_str_radix(digits, 16).ok()?))
        }
        ScalarType::Vector => {
            let [x, y, z] = floats(text)?;
            KeyframeValue::Vector(Vector3::new(x, y, z))
        }
        ScalarType::Quaternion => {
            let [x, y, z, w] = floats(text)?;
            KeyframeValue::Quaternion(Quaternion::new(x, y, z, w))
        }
        ScalarType::String | ScalarType::Custom => return None,
    };

    Some(value)
}

/// Exactly `N` comma-separated floats.
fn floats<const N: usize>(text: &str) -> Option<[f32; N]> {
    let mut values = [0.0; N];
    let mut parts = text.split(',');
    for value in &mut values {
        *value = parts.next()?.trim().parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }

    Some(values)
}

/// Empty text is the null string.
fn wide(text: &str) -> Option<WideString> {
    if text.is_empty() {
        return None;
    }

    Some(WideString::from(text))
}

fn parse_number<T: std::str::FromStr>(node: &Node) -> Result<T, CompileError> {
    let text = node.leaf_text()?;
    text.trim().parse().map_err(|_| CompileError::BadNumber {
        line: node.line,
        element: node.name.clone(),
        text: text.to_string(),
    })
}

fn parse_attribute<T: std::str::FromStr>(
    node: &Node,
    attribute: &'static str,
    text: &str,
) -> Result<T, CompileError> {
    text.trim().parse().map_err(|_| CompileError::BadAttribute {
        line: node.line,
        element: node.name.clone(),
        attribute,
        text: text.to_string(),
    })
}

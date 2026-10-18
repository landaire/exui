//! XUI text form of a [`Scene`] and class extension files for the classes
//! it uses.

use std::collections::BTreeMap;
use std::fmt;
use std::ptr;

use exui_core::Document;
use exui_core::Registry;
use exui_core::schema::ClassDef;
use exui_core::schema::PropType;
use exui_core::schema::xui;

use crate::error::DecompileError;
use crate::scene::Element;
use crate::scene::ElementClass;
use crate::scene::ElementId;
use crate::scene::Property;
use crate::scene::Scalar;
use crate::scene::Scene;
use crate::scene::Timeline;
use crate::scene::Value;

/// Name written for a value without a known definition.
const UNKNOWN_NAME: &str = "Unknown";

/// Writes `scene` as XUI text through [`fmt::Display`].
pub struct Xml<'s>(pub &'s Scene);

impl fmt::Display for Xml<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        enum Step {
            Open(ElementId),
            Close(ElementId),
        }

        let scene = self.0;
        let mut stack = vec![Step::Open(ElementId(0))];
        while let Some(step) = stack.pop() {
            match step {
                Step::Open(id) => {
                    let element = element(scene, id)?;
                    let class = element.class.name();
                    if id.0 == 0 {
                        writeln!(f, "<{class} version=\"{:04x}\">", scene.schema.0)?;
                    } else {
                        writeln!(f, "<{class}>")?;
                    }

                    if !element.properties.is_empty() {
                        writeln!(f, "<Properties>")?;
                        write_properties(f, &element.properties)?;
                        writeln!(f, "</Properties>")?;
                    }

                    stack.push(Step::Close(id));
                    stack.extend(
                        element
                            .children
                            .iter()
                            .rev()
                            .map(|child| Step::Open(*child)),
                    );
                }
                Step::Close(id) => {
                    let element = element(scene, id)?;
                    write_timelines(f, element)?;
                    writeln!(f, "</{}>", element.class.name())?;
                }
            }
        }

        Ok(())
    }
}

/// XUI class extension file declaring one class and its own properties.
pub struct ClassExtension(pub &'static ClassDef);

impl fmt::Display for ClassExtension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let def = self.0;
        let name = def.name;
        let base = def.base.unwrap_or("");

        writeln!(f, "<XUIClassExtension version=\"0001\">")?;
        writeln!(
            f,
            "<XUIClass Name=\"{name}\" BaseClassName=\"{base}\" Bitmap=\"\" Icon=\"\" DefaultWidth=\"100\" DefaultHeight=\"100\" Description=\"{name}\">"
        )?;
        for prop in def.props {
            let type_name = match prop.ty {
                PropType::Bool => "bool",
                PropType::Integer => "integer",
                PropType::Unsigned => "unsigned",
                PropType::Float => "float",
                PropType::String => "string",
                PropType::Color => "color",
                PropType::Vector => "vector",
                PropType::Quaternion => "quaternion",
                PropType::Compound(_) => "object",
                PropType::Custom => "custom",
            };
            writeln!(
                f,
                "<PropDef Flags=\"\" Name=\"{}\" Type=\"{type_name}\" Editor=\"\">",
                prop.name
            )?;
            writeln!(f, "<DefaultVal></DefaultVal>")?;
            writeln!(f, "</PropDef>")?;
        }
        writeln!(f, "</XUIClass>")?;
        writeln!(f, "</XUIClassExtension>")
    }
}

/// Text form of a scalar inside an XML element.
struct ScalarText<'v>(&'v Scalar);

impl fmt::Display for ScalarText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Scalar::Bool(value) => write!(f, "{value}"),
            Scalar::Integer(value) => write!(f, "{value}"),
            Scalar::Unsigned(value) | Scalar::Raw(value) => write!(f, "{value}"),
            Scalar::Float(value) => write!(f, "{value:.6}"),
            Scalar::String(text) => write_text(f, text.as_deref().unwrap_or("")),
            Scalar::Color(color) => write!(f, "0x{:08x}", color.argb()),
            Scalar::Vector(vector) => {
                write!(f, "{:.6},{:.6},{:.6}", vector.x, vector.y, vector.z)
            }
            Scalar::Quaternion(quaternion) => {
                write!(
                    f,
                    "{:.6},{:.6},{:.6},{:.6}",
                    quaternion.x, quaternion.y, quaternion.z, quaternion.w
                )
            }
        }
    }
}

/// Decompiles a version 5 document to XUI text. `registry` must be the one
/// `document` was parsed with.
pub fn to_xui(document: &Document<'_>, registry: &Registry) -> Result<String, DecompileError> {
    let scene = Scene::from_document(document, registry)?;

    Ok(Xml(&scene).to_string())
}

/// Adds the classes of `scene` that the XUI library does not register,
/// keyed by name. Classes unknown to the registry have no definition to
/// describe and are skipped.
pub fn collect_custom_classes(scene: &Scene, out: &mut BTreeMap<&'static str, &'static ClassDef>) {
    for element in &scene.elements {
        let ElementClass::Known(def) = element.class else {
            continue;
        };

        let library = xui::BUILTIN
            .iter()
            .chain(&xui::SOUND_XAUDIO)
            .chain(&xui::HTML)
            .any(|library_def| ptr::eq(*library_def, def));
        if !library {
            out.insert(def.name, def);
        }
    }
}

fn element(scene: &Scene, id: ElementId) -> Result<&Element, fmt::Error> {
    // Scenes are built with child ids of their own elements.
    scene.element(id).ok_or(fmt::Error)
}

/// Compound nesting is bounded by the static definition tables.
fn write_properties(f: &mut fmt::Formatter<'_>, properties: &[Property]) -> fmt::Result {
    for property in properties {
        let name = property.def.map_or(UNKNOWN_NAME, |def| def.name);
        let indexed = property.is_indexed();

        for (element, value) in property.values.iter().enumerate() {
            let index = if indexed {
                format!(" index=\"{element}\"")
            } else {
                String::new()
            };

            match value {
                Value::Scalar(scalar) => {
                    writeln!(f, "<{name}{index}>{}</{name}>", ScalarText(scalar))?;
                }
                Value::Points(points) => {
                    write!(f, "<{name}{index}>{},", points.len())?;
                    for point in points {
                        for coord in point.coords {
                            write!(f, "{coord:.6},")?;
                        }
                        // The binary form leaves the point type 0.
                        write!(f, "0,")?;
                    }
                    writeln!(f, "</{name}>")?;
                }
                Value::Compound(sub_properties) => {
                    writeln!(f, "<{name}{index}>")?;
                    if !sub_properties.is_empty() {
                        writeln!(f, "<Properties>")?;
                        write_properties(f, sub_properties)?;
                        writeln!(f, "</Properties>")?;
                    }
                    writeln!(f, "</{name}>")?;
                }
            }
        }
    }

    Ok(())
}

fn write_timelines(f: &mut fmt::Formatter<'_>, element: &Element) -> fmt::Result {
    if element.named_frames.is_empty() && element.timelines.is_empty() {
        return Ok(());
    }

    writeln!(f, "<Timelines>")?;

    if !element.named_frames.is_empty() {
        writeln!(f, "<NamedFrames>")?;
        for frame in &element.named_frames {
            writeln!(f, "<NamedFrame>")?;
            writeln!(f, "<Name>{}</Name>", frame.name.as_deref().unwrap_or(""))?;
            writeln!(f, "<Time>{}</Time>", frame.time.0)?;
            if frame.command.0 == 1 {
                writeln!(f, "<Command>stop</Command>")?;
            }
            writeln!(f, "</NamedFrame>")?;
        }
        writeln!(f, "</NamedFrames>")?;
    }

    for timeline in &element.timelines {
        write_timeline(f, timeline)?;
    }

    writeln!(f, "</Timelines>")
}

fn write_timeline(f: &mut fmt::Formatter<'_>, timeline: &Timeline) -> fmt::Result {
    // Interpolation 2 is ease, the only mode with ease parameters.
    const EASE: u8 = 2;

    writeln!(f, "<Timeline>")?;
    writeln!(
        f,
        "<Id>{}</Id>",
        timeline.element.as_deref().unwrap_or("").trim()
    )?;

    for property in &timeline.properties {
        let mut path = String::new();
        for (step, def) in property.path.iter().enumerate() {
            if step > 0 {
                path.push('.');
            }
            path.push_str(def.map_or(UNKNOWN_NAME, |def| def.name));
        }

        let indexed = property
            .path
            .last()
            .copied()
            .flatten()
            .is_some_and(|def| def.flags.is_indexed());
        match property.element {
            Some(element) if indexed => writeln!(
                f,
                "<TimelineProp index=\"{}\">{path}</TimelineProp>",
                element.0
            )?,
            _ => writeln!(f, "<TimelineProp>{path}</TimelineProp>")?,
        }
    }

    for keyframe in &timeline.keyframes {
        writeln!(f, "<KeyFrame>")?;
        writeln!(f, "<Time>{}</Time>", keyframe.time.0)?;
        writeln!(
            f,
            "<Interpolation>{}</Interpolation>",
            keyframe.interpolation.0
        )?;
        if keyframe.interpolation.0 == EASE {
            writeln!(f, "<EaseIn>{}</EaseIn>", keyframe.ease_in.0)?;
            writeln!(f, "<EaseOut>{}</EaseOut>", keyframe.ease_out.0)?;
            writeln!(f, "<EaseScale>{}</EaseScale>", keyframe.ease_scale.0)?;
        }
        for value in &keyframe.values {
            writeln!(f, "<Prop>{}</Prop>", ScalarText(value))?;
        }
        writeln!(f, "</KeyFrame>")?;
    }

    writeln!(f, "</Timeline>")
}

/// Writes a string with CRLF line ends as LF, trailing spaces of each line
/// removed and markup characters escaped.
fn write_text(f: &mut fmt::Formatter<'_>, text: &str) -> fmt::Result {
    let unified = text.replace("\r\n", "\n");
    let mut clean = unified
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    if unified.ends_with('\n') {
        clean.push('\n');
    }

    f.write_str(&quick_xml::escape::partial_escape(&clean))
}

//! XUI text form of a [`Scene`] and class extension files for the classes
//! it uses. doc/xui-format.md ("Text form") lists the rules that make the
//! text of a version 5 scene compile back to the same bytes.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;
use std::ptr;

use exui_core::Document;
use exui_core::Registry;
use exui_core::schema::ClassDef;
use exui_core::schema::PropType;
use exui_core::schema::xui;
use exui_core::xur::FrameCommand;

use crate::error::DecompileError;
use crate::scene::Element;
use crate::scene::ElementClass;
use crate::scene::ElementId;
use crate::scene::FigurePath;
use crate::scene::Keyframe;
use crate::scene::NamedFrame;
use crate::scene::Property;
use crate::scene::Scalar;
use crate::scene::Scene;
use crate::scene::Timeline;
use crate::scene::Value;

/// Name written for a value without a known definition.
const UNKNOWN_NAME: &str = "Unknown";

/// Root attribute recording that the file has allocation hints.
pub const ALLOCATION_HINTS_ATTRIBUTE: &str = "allocationhints";

/// Keyframe ease values the XUI tool stores for interpolations other than
/// ease. Text that leaves them out gets these.
pub const DEFAULT_EASE_IN: u8 = 0;
pub const DEFAULT_EASE_OUT: u8 = 0;
pub const DEFAULT_EASE_SCALE: u8 = 50;

/// Interpolation 2 is ease, the only mode the XUI tool writes ease values
/// for.
const EASE: u8 = 2;

/// Named frame commands by byte value.
pub const FRAME_COMMANDS: [&str; 5] = ["play", "stop", "goto", "gotoandplay", "gotoandstop"];

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
                        write!(f, "<{class} version=\"{:04x}\"", scene.schema.0)?;
                        if scene.allocation_hints {
                            write!(f, " {ALLOCATION_HINTS_ATTRIBUTE}=\"true\"")?;
                        }
                        writeln!(f, ">")?;
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
            Scalar::Float(value) => write!(f, "{}", FloatText(*value)),
            Scalar::String(text) => write!(f, "{}", Text(text.as_deref().unwrap_or(""))),
            Scalar::Color(color) => write!(f, "0x{:08x}", color.argb()),
            Scalar::Vector(vector) => write!(
                f,
                "{},{},{}",
                FloatText(vector.x),
                FloatText(vector.y),
                FloatText(vector.z)
            ),
            Scalar::Quaternion(quaternion) => write!(
                f,
                "{},{},{},{}",
                FloatText(quaternion.x),
                FloatText(quaternion.y),
                FloatText(quaternion.z),
                FloatText(quaternion.w)
            ),
        }
    }
}

/// A float with six decimals, as the XUI tool writes them, or the shortest
/// text that reads back to the same bits when six decimals do not.
pub struct FloatText(pub f32);

impl fmt::Display for FloatText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fixed = TextBuffer::new();
        write!(fixed, "{:.6}", self.0)?;
        let exact = fixed
            .as_str()
            .parse::<f32>()
            .is_ok_and(|parsed| parsed.to_bits() == self.0.to_bits());
        if exact {
            f.write_str(fixed.as_str())
        } else {
            write!(f, "{}", self.0)
        }
    }
}

/// String contents as XML character data: markup characters and every
/// control character other than line feed and tab escaped, so carriage
/// returns survive line end normalization.
pub struct Text<'t>(pub &'t str);

impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '&' => f.write_str("&amp;")?,
                '<' => f.write_str("&lt;")?,
                '>' => f.write_str("&gt;")?,
                '\n' | '\t' => f.write_char(c)?,
                c if c.is_control() => write!(f, "&#x{:X};", u32::from(c))?,
                c => f.write_char(c)?,
            }
        }

        Ok(())
    }
}

/// Fixed buffer for the six-decimal form of any f32: at most 39 integer
/// digits, a sign, a point and six decimals.
struct TextBuffer {
    bytes: [u8; 48],
    len: usize,
}

impl TextBuffer {
    fn new() -> Self {
        Self {
            bytes: [0; 48],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        // Only whole `&str` values are appended.
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for TextBuffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        let Some(slot) = self.bytes.get_mut(self.len..end) else {
            return Err(fmt::Error);
        };
        slot.copy_from_slice(text.as_bytes());
        self.len = end;

        Ok(())
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
            write!(f, "<{name}")?;
            if indexed {
                write!(f, " index=\"{element}\"")?;
            }

            match value {
                Value::Scalar(scalar) => {
                    writeln!(f, ">{}</{name}>", ScalarText(scalar))?;
                }
                Value::Points(path) => {
                    write_points(f, path)?;
                    writeln!(f, "</{name}>")?;
                }
                Value::Compound(sub_properties) => {
                    writeln!(f, ">")?;
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

/// The size attributes, when the stored size is not the anchor extent the
/// compiler assumes without them, then the closing `>` and the point list.
fn write_points(f: &mut fmt::Formatter<'_>, path: &FigurePath) -> fmt::Result {
    let (width, height) = FigurePath::anchor_extent(&path.points);
    if width.to_bits() != path.width.to_bits() || height.to_bits() != path.height.to_bits() {
        write!(
            f,
            " width=\"{}\" height=\"{}\"",
            FloatText(path.width),
            FloatText(path.height)
        )?;
    }

    write!(f, ">{},", path.points.len())?;
    for point in &path.points {
        for coord in point.coords {
            write!(f, "{},", FloatText(coord))?;
        }
        // The binary form leaves the point type 0.
        write!(f, "0,")?;
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
            write_named_frame(f, frame)?;
        }
        writeln!(f, "</NamedFrames>")?;
    }

    for timeline in &element.timelines {
        write_timeline(f, timeline)?;
    }

    writeln!(f, "</Timelines>")
}

fn write_named_frame(f: &mut fmt::Formatter<'_>, frame: &NamedFrame) -> fmt::Result {
    // Play is the command of a frame without one.
    const PLAY: FrameCommand = FrameCommand(0);

    writeln!(f, "<NamedFrame>")?;
    if let Some(name) = &frame.name {
        writeln!(f, "<Name>{}</Name>", Text(name))?;
    }
    writeln!(f, "<Time>{}</Time>", frame.time.0)?;
    if frame.command != PLAY {
        match FRAME_COMMANDS.get(usize::from(frame.command.0)) {
            Some(command) => writeln!(f, "<Command>{command}</Command>")?,
            None => writeln!(f, "<Command>{}</Command>", frame.command.0)?,
        }
    }
    if let Some(target) = &frame.target {
        writeln!(f, "<CommandParams>{}</CommandParams>", Text(target))?;
    }
    writeln!(f, "</NamedFrame>")
}

fn write_timeline(f: &mut fmt::Formatter<'_>, timeline: &Timeline) -> fmt::Result {
    writeln!(f, "<Timeline>")?;
    if let Some(id) = &timeline.element {
        writeln!(f, "<Id>{}</Id>", Text(id))?;
    }

    for property in &timeline.properties {
        write!(f, "<TimelineProp")?;
        if let Some(element) = property.element {
            write!(f, " index=\"{}\"", element.0)?;
        }
        write!(f, ">")?;
        for (step, def) in property.path.iter().enumerate() {
            if step > 0 {
                write!(f, ".")?;
            }
            f.write_str(def.map_or(UNKNOWN_NAME, |def| def.name))?;
        }
        writeln!(f, "</TimelineProp>")?;
    }

    for keyframe in &timeline.keyframes {
        write_keyframe(f, keyframe)?;
    }

    writeln!(f, "</Timeline>")
}

fn write_keyframe(f: &mut fmt::Formatter<'_>, keyframe: &Keyframe) -> fmt::Result {
    let ease = keyframe.interpolation.0 == EASE;

    writeln!(f, "<KeyFrame>")?;
    writeln!(f, "<Time>{}</Time>", keyframe.time.0)?;
    writeln!(
        f,
        "<Interpolation>{}</Interpolation>",
        keyframe.interpolation.0
    )?;
    if ease || keyframe.ease_in.0 != DEFAULT_EASE_IN {
        writeln!(f, "<EaseIn>{}</EaseIn>", keyframe.ease_in.0)?;
    }
    if ease || keyframe.ease_out.0 != DEFAULT_EASE_OUT {
        writeln!(f, "<EaseOut>{}</EaseOut>", keyframe.ease_out.0)?;
    }
    if ease || keyframe.ease_scale.0 != DEFAULT_EASE_SCALE {
        writeln!(f, "<EaseScale>{}</EaseScale>", keyframe.ease_scale.0)?;
    }
    for value in &keyframe.values {
        writeln!(f, "<Prop>{}</Prop>", ScalarText(value))?;
    }
    writeln!(f, "</KeyFrame>")
}

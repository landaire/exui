use std::collections::BTreeMap;
use std::fmt::Write;

use crate::xur::Object;
use crate::xur::PropType;
use crate::xur::PropertyGroup;
use crate::xur::PropertyValue;
use crate::xur::TimelineData;
use crate::xur::TimelineValue;
use crate::xur::parse::get_hierarchy;
use crate::xur::Xur;

// Property name tables: maps (class_level, bit_index) to XML element name.
// v5-specific ordering. v8 differs (e.g. XuiElement Show/Anchor swapped at 7/9).

const XUIELEMENT_NAMES: &[&str] = &[
    "Id",                      // 0
    "Width",                   // 1
    "Height",                  // 2
    "Position",                // 3
    "Scale",                   // 4
    "Rotation",                // 5
    "Opacity",                 // 6
    "Anchor",                  // 7 (v5 type=Unsigned; v8 name at this index)
    "Pivot",                   // 8
    "Show",                    // 9 (v5 type=Bool; v8 name at this index)
    "BlendMode",               // 10
    "DisableTimelineRecursion", // 11
    "ColorWriteFlags",         // 12
    "ColorFactor",             // 13
];

const XUIFIGURE_NAMES: &[&str] = &[
    "Stroke",  // 0
    "Fill",    // 1
    "Closed",  // 2
    "Points",  // 3
];

const STROKE_NAMES: &[&str] = &[
    "StrokeWidth", // 0
    "StrokeColor", // 1
];

const FILL_NAMES: &[&str] = &[
    "FillType",         // 0
    "FillColor",        // 1
    "TextureFileName",  // 2
    "Gradient",         // 3
    "Translation",      // 4
    "Scale",            // 5
    "Rotation",         // 6
    "WrapX",            // 7
    "WrapY",            // 8
    "BrushFlags",       // 9
    "TransformVersion", // 10
];

const XUICONTROL_NAMES: &[&str] = &[
    "ClassOverride",    // 0
    "Visual",           // 1
    "Enabled",          // 2
    "UnfocusedInput",   // 3
    "NavLeft",          // 4
    "NavRight",         // 5
    "NavUp",            // 6
    "NavDown",          // 7
    "NavTabForward",    // 8
    "NavTabBackward",   // 9
    "Text",             // 10
    "PointSize",        // 11
    "ImagePath",        // 12
    "HasContextMenu",   // 13
    "SizeToText",       // 14
    "UseNuiAsMouse",    // 15
    "AutoId",           // 16
    "HoverSelectTimer", // 17
    "QuickInput",       // 18
];

const XUISCENE_NAMES: &[&str] = &[
    "DefaultFocus",         // 0
    "TransFrom",            // 1
    "TransTo",              // 2
    "TransBackFrom",        // 3
    "TransBackTo",          // 4
    "InterruptTransitions", // 5
    "IgnorePresses",        // 6
    "RecurseTransitions",   // 7
];

const XUIBUTTON_NAMES: &[&str] = &[
    "PressKey",    // 0
    "PressPath",   // 1
    "PressAction", // 2
    "BackPath",    // 3
    "BackAction",  // 4
    "OverPath",    // 5
    "OverAction",  // 6
];

const XUINAVBUTTON_NAMES: &[&str] = &[
    "PressPath",      // 0
    "StayVisible",    // 1
    "SrcTransIndex",  // 2
    "DestTransIndex", // 3
];

const XUITABSCENE_NAMES: &[&str] = &[
    "TabCount",     // 0
    "Wrap",         // 1
    "UserInterrupt",// 2
    "VerticalTabs", // 3
    "NoAutoHide",   // 4
    "DefaultTab",   // 5
];

const XUITEXT_NAMES: &[&str] = &[
    "Text",              // 0
    "TextColor",         // 1
    "DropShadowColor",   // 2
    "PointSize",         // 3
    "Font",              // 4
    "TextStyle",         // 5
    "LineSpacingAdjust", // 6
    "TextScale",         // 7
];

const XUIIMAGE_NAMES: &[&str] = &[
    "SizeMode",              // 0
    "ImagePath",             // 1
    "BrushFlags",            // 2
    "TextureSurfaceElement", // 3
    "LoadType",              // 4
];

const GRADIENT_NAMES: &[&str] = &[
    "Radial",    // 0
    "NumStops",  // 1
    "StopColor", // 2
    "StopPos",   // 3
];

pub fn to_xui(xur: &Xur<'_>) -> Result<String, std::fmt::Error> {
    let mut out = String::new();
    write_object(xur, &xur.root, &mut out, 0, true)?;
    Ok(out)
}

fn write_object(
    xur: &Xur<'_>,
    obj: &Object<'_>,
    out: &mut String,
    depth: usize,
    is_root: bool,
) -> Result<(), std::fmt::Error> {
    let class = xur.get_string(obj.class_name).unwrap_or("Unknown");

    // Opening tag
    if is_root {
        writeln!(
            out,
            "<{class} version=\"{:04x}\">",
            xur.header.xui_version
        )?;
    } else {
        writeln!(out, "<{class}>")?;
    }

    // Properties block
    if !obj.properties.is_empty() {
        writeln!(out, "<Properties>")?;

        for group in &obj.properties {
            let names = get_names_for_group(xur, obj, group);
            write_property_group(xur, group, names, out)?;
        }

        writeln!(out, "</Properties>")?;
    }

    // Children
    for child in &obj.children {
        write_object(xur, child, out, depth + 1, false)?;
    }

    // Timelines
    if let Some(tl) = &obj.timelines {
        write_timelines(xur, class, tl, out)?;
    }

    writeln!(out, "</{class}>")?;
    Ok(())
}

fn get_names_for_group(xur: &Xur<'_>, obj: &Object<'_>, group: &PropertyGroup) -> &'static [&'static str] {
    if group.level == 0 {
        return XUIELEMENT_NAMES;
    }

    let class = xur.get_string(obj.class_name).unwrap_or("");

    // Map (class_name, hierarchy_level) to the correct name table.
    // Level 1 = first derived (e.g. XuiControl for XuiButton).
    // Level 2 = second derived (e.g. XuiButton for XuiButton).
    get_names_for_hierarchy_level(class, group.level)
}

fn get_names_for_hierarchy_level(class: &str, level: usize) -> &'static [&'static str] {
    // The hierarchy mirrors get_hierarchy() in parse.rs.
    // Level 1 = first derived class above XuiElement.
    match class {
        // Direct children of XuiElement (level 1 = leaf)
        "XuiFigure" => if level == 1 { XUIFIGURE_NAMES } else { &[] },
        "XuiCanvas" => &[],
        "XuiText" => if level == 1 { XUITEXT_NAMES } else { &[] },
        "XuiImage" => if level == 1 { XUIIMAGE_NAMES } else { &[] },

        // XuiElement -> XuiControl -> Leaf
        "XuiControl" => if level == 1 { XUICONTROL_NAMES } else { &[] },
        "XuiButton" => match level {
            1 => XUICONTROL_NAMES,
            2 => XUIBUTTON_NAMES,
            _ => &[],
        },
        "XuiScene" => match level {
            1 => XUICONTROL_NAMES,
            2 => XUISCENE_NAMES,
            _ => &[],
        },

        // 3 derived levels
        "XuiNavButton" => match level {
            1 => XUICONTROL_NAMES,
            2 => XUIBUTTON_NAMES,
            3 => XUINAVBUTTON_NAMES,
            _ => &[],
        },
        "XuiTabScene" => match level {
            1 => XUICONTROL_NAMES,
            2 => XUISCENE_NAMES,
            3 => XUITABSCENE_NAMES,
            _ => &[],
        },

        // XuiControl subtypes with own property names
        "XuiLabel" => match level { 1 => XUICONTROL_NAMES, 2 => XUILABEL_NAMES, _ => &[] },
        "XuiCheckbox" => match level { 1 => XUICONTROL_NAMES, 2 => XUICHECKBOX_NAMES, _ => &[] },
        "XuiSlider" => match level { 1 => XUICONTROL_NAMES, 2 => XUISLIDER_NAMES, _ => &[] },
        "XuiEdit" => match level { 1 => XUICONTROL_NAMES, 2 => XUIEDIT_NAMES, _ => &[] },
        "XuiList" => match level { 1 => XUICONTROL_NAMES, 2 => XUILIST_NAMES, _ => &[] },
        "XuiCommonList" => match level { 1 => XUICONTROL_NAMES, 2 => XUILIST_NAMES, 3 => XUICOMMONLIST_NAMES, _ => &[] },
        "XuiListItem" => match level { 1 => XUICONTROL_NAMES, 2 => XUICHECKBOX_NAMES, _ => &[] },

        // XuiControl subtypes without own property names
        "XuiRadioButton" | "XuiRadioGroup" | "XuiScrollBar"
        | "XuiProgressBar" | "XuiCaret"
        | "XuiMessageBox" | "XuiPerspectiveScene" => {
            if level == 1 { XUICONTROL_NAMES } else { &[] }
        }

        // XuiSound hierarchy
        "XuiSoundXAudio" => match level { 1 => XUISOUND_NAMES, 2 => XUISOUNDXAUDIO_NAMES, _ => &[] },

        // Presenter classes (direct children of XuiElement)
        "XuiTextPresenter" => if level == 1 { XUITEXTPRESENTER_NAMES } else { &[] },
        "XuiImagePresenter" => if level == 1 { XUIIMAGEPRESENTER_NAMES } else { &[] },

        // XuiScrollEnd extends XuiControl
        "XuiScrollEnd" => match level { 1 => XUICONTROL_NAMES, 2 => XUISCROLLEND_OWN_NAMES, _ => &[] },

        // XuiListItem extends XuiCheckbox extends XuiControl
        "XuiListItem" => match level {
            1 => XUICONTROL_NAMES, 2 => XUICHECKBOX_NAMES, 3 => XUILISTITEM_NAMES, _ => &[]
        },

        // XuiGamerCard extends XuiControl
        "XuiGamerCard" => match level { 1 => XUICONTROL_NAMES, 2 => XUIGAMERCARD_NAMES, _ => &[] },

        // XuiBackButton extends XuiButton extends XuiControl
        "XuiBackButton" => match level {
            1 => XUICONTROL_NAMES, 2 => XUIBUTTON_NAMES, _ => &[]
        },

        // XuiBOTDScene/XuiBOTDContainer
        "XuiBOTDScene" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, 3 => XUIBOTDSCENE_NAMES, _ => &[]
        },
        "XuiBOTDContainer" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, _ => &[]
        },
        "XuiBOTDOfflineContainer" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, 3 => &["Unknown", "BannerPath"], _ => &[]
        },
        "XuiBOTDOfflineScene" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, 3 => &["BannerPath"], _ => &[]
        },
        "XuiFall07BOTDScene" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES,
            3 => &["Unknown0", "Unknown1", "Unknown2", "Unknown3", "Unknown4", "Unknown5", "VisualOverride"],
            _ => &[]
        },
        "LiveVisionControl" => match level {
            1 => XUICONTROL_NAMES, _ => &[]
        },
        "VideoData" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, _ => &[]
        },
        "ScriptImage" => match level {
            1 => XUICONTROL_NAMES, _ => &[]
        },
        "ScriptList" => match level {
            1 => XUICONTROL_NAMES, 2 => XUILIST_NAMES, _ => &[]
        },
        "ScriptScene" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, 3 => &["ScriptPath"], _ => &[]
        },

        // v5-only classes
        "XuiHtmlElement" => if level == 1 { XUIHTMLELEMENT_NAMES } else { &[] },

        // Blades dashboard classes
        "DashScene" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, 3 => DASHSCENE_NAMES, _ => &[]
        },
        "DashMainScene" | "DashMediaScene" | "DashSystemScene"
        | "DashBladeTab" | "DashLiveScene"
        | "DashLiveSignedIn" | "DashLiveSignedOut" | "DashLiveConnected" => match level {
            1 => XUICONTROL_NAMES, 2 => XUISCENE_NAMES, 3 => DASHSCENE_NAMES, _ => &[]
        },

        _ => &[],
    }
}

const XUILABEL_NAMES: &[&str] = &["MaxFlowLines"];
const XUICHECKBOX_NAMES: &[&str] = &["PressKey"];
const XUISLIDER_NAMES: &[&str] = &["RangeMin", "RangeMax", "Value", "Step", "Vertical", "AccelInc", "AccelTime"];
const XUIEDIT_NAMES: &[&str] = &["TextLimit", "AllowedChars", "PasswordChar", "ReadOnly", "Multiline", "SmoothScroll"];
const XUILIST_NAMES: &[&str] = &["Wrap", "WrapBump"];
const XUICOMMONLIST_NAMES: &[&str] = &["ItemsText", "ItemsImage", "ItemsNavPath"];
const XUISOUND_NAMES: &[&str] = &["State", "Loop", "Finish", "Volume"];
const XUISOUNDXAUDIO_NAMES: &[&str] = &["File"];
const XUIHTMLELEMENT_NAMES: &[&str] = &["Text"];

const XUITEXTPRESENTER_NAMES: &[&str] = &[
    "TextColor", "DropShadowColor", "PointSize", "Font",
    "TextStyle", "LineSpacing", "Unknown", "TextScale",
];

const XUIIMAGEPRESENTER_NAMES: &[&str] = &[
    "SizeMode", "ImagePath", "BrushFlags",
];

const XUILISTITEM_NAMES: &[&str] = &[
    "Layout", "Smooth", "BaseSpeed", "MaxSpeed", "Acceleration",
];

const XUIGAMERCARD_NAMES: &[&str] = &["Format", "ShowExtendedPanel"];

const XUIBOTDSCENE_NAMES: &[&str] = &[
    "Unknown", "Unknown", "Unknown", "Unknown", "DefaultVisual",
];

const XUIBOTDCONTAINER_NAMES: &[&str] = &[];
const XUISCROLLEND_OWN_NAMES: &[&str] = &["ScrollEndType"];

// Blades dashboard custom class names (from strings near DashScene in dash.exe)
const DASHSCENE_NAMES: &[&str] = &[
    "PanelScenePaths", // 0: null-delimited list of child nav IDs
    "PanelStrings",    // 1: null-delimited list of description texts
    "PanelSettings",   // 2: null-delimited list of settings
    "Unknown",         // 3
    "MetaPanelScene",  // 4: scene class name (used in some instances)
    "Unknown",         // 5
];

fn write_property_group(
    xur: &Xur<'_>,
    group: &PropertyGroup,
    names: &[&str],
    out: &mut String,
) -> Result<(), std::fmt::Error> {
    // Compute per-array-bit element count.
    // Non-array bits consume 1 value each; array bits split the remainder equally.
    let set_bits: Vec<u32> = (0..32u32).filter(|b| group.bitmask & (1 << b) != 0).collect();
    let non_array_count = set_bits.iter().filter(|&&b| !is_compound_array_prop(names, b)).count();
    let array_bit_count = set_bits.len() - non_array_count;
    let elements_per_array = if array_bit_count > 0 {
        (group.values.len() - non_array_count) / array_bit_count
    } else {
        0
    };

    let mut value_idx = 0;
    for &bit in &set_bits {
        if value_idx >= group.values.len() {
            break;
        }

        let name = names
            .get(bit as usize)
            .copied()
            .unwrap_or("Unknown");

        if is_compound_array_prop(names, bit) {
            for arr_idx in 0..elements_per_array {
                if value_idx >= group.values.len() {
                    break;
                }
                write_property(xur, name, &group.values[value_idx], Some(arr_idx), out)?;
                value_idx += 1;
            }
        } else {
            write_property(xur, name, &group.values[value_idx], None, out)?;
            value_idx += 1;
        }
    }
    Ok(())
}

fn is_compound_array_prop(names: &[&str], bit: u32) -> bool {
    // Gradient properties at bits 2 (StopColor) and 3 (StopPos) are arrays
    names == GRADIENT_NAMES && matches!(bit, 2 | 3)
}


fn write_property(
    xur: &Xur<'_>,
    name: &str,
    val: &PropertyValue,
    array_index: Option<usize>,
    out: &mut String,
) -> Result<(), std::fmt::Error> {
    let idx_attr = match array_index {
        Some(i) => format!(" index=\"{i}\""),
        None => String::new(),
    };
    match val {
        PropertyValue::Float(v) => {
            writeln!(out, "<{name}{idx_attr}>{v:.6}</{name}>")?;
        }
        PropertyValue::String(idx) => {
            let s = xur.get_string(*idx).unwrap_or("");
            let clean: String = s
                .replace("\r\n", "\n")
                .lines()
                .map(|l| l.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            // Preserve trailing newline if original had one
            let clean = if s.ends_with('\n') || s.ends_with("\r\n") {
                clean + "\n"
            } else {
                clean
            };
            let escaped = quick_xml::escape::partial_escape(&clean);
            writeln!(out, "<{name}{idx_attr}>{escaped}</{name}>")?;
        }
        PropertyValue::Bool(v) => {
            writeln!(out, "<{name}{idx_attr}>{v}</{name}>")?;
        }
        PropertyValue::Unsigned(v) | PropertyValue::Integer(v) => {
            writeln!(out, "<{name}{idx_attr}>{v}</{name}>")?;
        }
        PropertyValue::Color(v) => {
            writeln!(out, "<{name}{idx_attr}>0x{v:08x}</{name}>")?;
        }
        PropertyValue::Vector(idx) => {
            if let Some((x, y, z)) = xur.get_vector(*idx) {
                writeln!(out, "<{name}>{x:.6},{y:.6},{z:.6}</{name}>")?;
            } else {
                writeln!(out, "<{name}>VECT[{idx}]</{name}>")?;
            }
        }
        PropertyValue::Quaternion(idx) => {
            if let Some((x, y, z, w)) = xur.get_quaternion(*idx) {
                writeln!(out, "<{name}>{x:.6},{y:.6},{z:.6},{w:.6}</{name}>")?;
            } else {
                writeln!(out, "<{name}>QUAT[{idx}]</{name}>")?;
            }
        }
        PropertyValue::Object(groups) => {
            // Compound property: wrap in <Name><Properties>...</Properties></Name>
            writeln!(out, "<{name}>")?;
            if !groups.is_empty() {
                writeln!(out, "<Properties>")?;
                for group in groups {
                    let sub_names = get_compound_names(name);
                    write_property_group(xur, group, sub_names, out)?;
                }
                writeln!(out, "</Properties>")?;
            }
            writeln!(out, "</{name}>")?;
        }
        PropertyValue::Custom(offset) => {
            // Points: decode CUST data
            if name == "Points" {
                write_points(xur, *offset, out)?;
            } else {
                writeln!(out, "<{name}>CUST[{offset}]</{name}>")?;
            }
        }
        PropertyValue::Unknown(bytes) => {
            // Raw bytes for undecodable compounds
            writeln!(out, "<{name}>")?;
            writeln!(out, "<Properties>")?;
            writeln!(out, "<!-- raw: {} bytes -->", bytes.len())?;
            writeln!(out, "</Properties>")?;
            writeln!(out, "</{name}>")?;
        }
    }
    Ok(())
}

fn get_compound_names(compound_name: &str) -> &'static [&'static str] {
    match compound_name {
        "Stroke" => STROKE_NAMES,
        "Fill" => FILL_NAMES,
        "Gradient" => GRADIENT_NAMES,
        _ => &[],
    }
}

fn write_points(xur: &Xur<'_>, offset: u32, out: &mut String) -> Result<(), std::fmt::Error> {
    let cust = xur.custom;
    let off = offset as usize;

    if off + 12 > cust.len() {
        writeln!(out, "<Points>CUST[{offset}]</Points>")?;
        return Ok(());
    }

    // CUST figure data: size(u32), width(f32), height(f32), point_count(u32),
    // then point_count * 6 floats (x, y, cp1x, cp1y, cp2x, cp2y)
    // Actually from our earlier analysis the CUST block is:
    // u32 size, f32 width, f32 height, u32 point_count, then points data

    // The format of points in the XUI is:
    // count,x1,y1,cx1a,cy1a,cx1b,cy1b,type1,x2,y2,...
    // Each point: 6 floats + 1 int (type, always 0 in test data)

    // But looking at CUST data, it seems to be just: count then 6 floats per point (no type int)
    // Let me read the CUST block and format

    // First, read how many bytes this CUST block is
    // From our analysis: each CUST block is prefixed with a u32 size
    if off + 4 > cust.len() {
        writeln!(out, "<Points>CUST[{offset}]</Points>")?;
        return Ok(());
    }

    let block_size =
        u32::from_be_bytes(cust[off..off + 4].try_into().unwrap()) as usize;
    let block = &cust[off + 4..off + 4 + block_size.min(cust.len() - off - 4)];

    if block.len() < 8 {
        writeln!(out, "<Points>CUST[{offset}]</Points>")?;
        return Ok(());
    }

    // Read width, height (unused for output but part of the format)
    let _width = f32::from_be_bytes(block[0..4].try_into().unwrap());
    let _height = f32::from_be_bytes(block[4..8].try_into().unwrap());

    // The remaining data after width+height should be:
    // u32(0x0000) + u32(point_count), then point_count * 6 floats
    // Actually let me just look at the raw structure from the hex analysis:
    // Block = width(f32) + height(f32) + zero(u32?) + point_count(u32) + 6*count floats

    // Let me read carefully based on what works
    if block.len() < 12 {
        writeln!(out, "<Points>CUST[{offset}]</Points>")?;
        return Ok(());
    }

    let point_count = u32::from_be_bytes(block[8..12].try_into().unwrap()) as usize;

    write!(out, "<Points>{point_count},")?;

    // Each point: 6 floats (cp1x, cp1y, cpx, cpy, cp2x, cp2y) + type (u32? or not present)
    // From the XUI format: x,y,cx1,cy1,cx2,cy2,type
    // 6 floats = 24 bytes per point
    let point_data = &block[12..];
    let floats_per_point = 6;
    let bytes_per_point = floats_per_point * 4;

    for i in 0..point_count {
        let base = i * bytes_per_point;
        if base + bytes_per_point > point_data.len() {
            break;
        }
        for j in 0..floats_per_point {
            let foff = base + j * 4;
            let f = f32::from_be_bytes(
                point_data[foff..foff + 4].try_into().unwrap(),
            );
            write!(out, "{f:.6},")?;
        }
        // Type field: in the XUI it's always "0," for straight lines
        write!(out, "0,")?;
    }

    writeln!(out, "</Points>")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Timeline writing
// ---------------------------------------------------------------------------

fn write_timelines(
    xur: &Xur<'_>,
    _class_name: &str,
    tl: &TimelineData,
    out: &mut String,
) -> Result<(), std::fmt::Error> {
    writeln!(out, "<Timelines>")?;

    // Named frames
    if !tl.named_frames.is_empty() {
        writeln!(out, "<NamedFrames>")?;
        for nf in &tl.named_frames {
            writeln!(out, "<NamedFrame>")?;
            let name = xur.get_string(nf.name).unwrap_or("");
            writeln!(out, "<Name>{name}</Name>")?;
            writeln!(out, "<Time>{}</Time>", nf.time)?;
            if nf.command == 1 {
                writeln!(out, "<Command>stop</Command>")?;
            }
            writeln!(out, "</NamedFrame>")?;
        }
        writeln!(out, "</NamedFrames>")?;
    }

    // Animated timelines
    for timeline in &tl.timelines {
        writeln!(out, "<Timeline>")?;

        let id = xur.get_string(timeline.target_name).unwrap_or("").trim();
        writeln!(out, "<Id>{id}</Id>")?;

        for path in &timeline.paths {
            let tc = timeline.target_class.as_deref().unwrap_or("");
            let (prop_name, index_attr) = format_timeline_prop(tc, path);
            if let Some(idx) = index_attr {
                writeln!(out, "<TimelineProp index=\"{idx}\">{prop_name}</TimelineProp>")?;
            } else {
                writeln!(out, "<TimelineProp>{prop_name}</TimelineProp>")?;
            }
        }

        for kf in &timeline.keyframes {
            writeln!(out, "<KeyFrame>")?;
            writeln!(out, "<Time>{}</Time>", kf.time)?;
            writeln!(out, "<Interpolation>{}</Interpolation>", kf.interpolation)?;
            if kf.interpolation == 2 {
                writeln!(out, "<EaseIn>{}</EaseIn>", kf.ease[0])?;
                writeln!(out, "<EaseOut>{}</EaseOut>", kf.ease[1])?;
                writeln!(out, "<EaseScale>{}</EaseScale>", kf.ease[2])?;
            }
            for val in &kf.values {
                write_timeline_value(xur, val, out)?;
            }
            writeln!(out, "</KeyFrame>")?;
        }

        writeln!(out, "</Timeline>")?;
    }

    writeln!(out, "</Timelines>")?;
    Ok(())
}

fn resolve_timeline_prop_name(class_name: &str, hier_level: u8, prop_idx: u8) -> &'static str {
    let hierarchy = get_hierarchy(class_name);
    let hl = hier_level as usize;
    if hl >= hierarchy.len() {
        // XuiElement base (writer level 0)
        return XUIELEMENT_NAMES
            .get(prop_idx as usize)
            .copied()
            .unwrap_or("Unknown");
    }
    // Derived level: hier_level=0 is leaf (last entry), etc.
    // Writer levels: 0=XuiElement, 1=first derived, ...
    // hierarchy[0] = first derived above XuiElement = writer level 1
    // hier_level=0 -> hierarchy[last] -> writer level = hierarchy.len()
    let writer_level = hierarchy.len() - hl;
    let names = get_names_for_hierarchy_level(class_name, writer_level);
    names.get(prop_idx as usize).copied().unwrap_or("Unknown")
}

/// Format a timeline property as a dotted path with optional array index.
/// Examples: "Position", "Fill.FillType", "Fill.Gradient.StopColor" (index=0)
fn format_timeline_prop(
    class_name: &str,
    path: &crate::xur::KeyframePath,
) -> (String, Option<u32>) {
    use crate::xur::parse::{get_compound_sub_types, is_gradient_array_prop};

    if path.depth <= 1 || path.extra_bytes.is_empty() {
        // Simple property (no compound drilling)
        let name = if !path.prop_name.is_empty() {
            path.prop_name.clone()
        } else {
            resolve_timeline_prop_name(class_name, path.hier_level, path.prop_idx).to_string()
        };
        return (name, None);
    }

    // Compound path: build dotted name like "Fill.Gradient.StopPos"
    let hierarchy = get_hierarchy(class_name);
    let hl = path.hier_level as usize;
    let parent_types = if hl >= hierarchy.len() {
        crate::xur::parse::XUIELEMENT_TYPES_V5
    } else {
        let idx = hierarchy.len() - 1 - hl;
        hierarchy[idx]
    };

    // Start with the top-level compound property name
    let top_name = resolve_timeline_prop_name(class_name, path.hier_level, path.prop_idx);
    let mut dotted = top_name.to_string();

    // Walk through compound sub-types using extra_bytes
    let mut current_types = parent_types;
    let mut array_index: Option<u32> = None;

    if let Some(sub_types) = get_compound_sub_types(current_types, path.prop_idx as u32) {
        current_types = sub_types;

        for (i, &sub_idx) in path.extra_bytes.iter().enumerate() {
            let sub_name = get_compound_prop_name(current_types, sub_idx as u32);
            dotted.push('.');
            dotted.push_str(sub_name);

            // Check if this is an array property (StopColor, StopPos in Gradient)
            if i == path.extra_bytes.len() - 1 && is_gradient_array_prop(sub_idx as u32) {
                // The default_value in the path contains the array index
                array_index = path.default_value;
            }

            // Drill deeper if not last
            if i < path.extra_bytes.len() - 1 {
                if let Some(deeper) = get_compound_sub_types(current_types, sub_idx as u32) {
                    current_types = deeper;
                } else {
                    break;
                }
            }
        }
    }

    (dotted, array_index)
}

fn resolve_compound_timeline_prop_name(
    class_name: &str,
    hier_level: u8,
    prop_idx: u8,
    extra_bytes: &[u8],
) -> String {
    use crate::xur::parse::get_compound_sub_types;

    let hierarchy = get_hierarchy(class_name);
    let hl = hier_level as usize;

    // Get the parent type table
    let parent_types = if hl >= hierarchy.len() {
        crate::xur::parse::XUIELEMENT_TYPES_V5
    } else {
        let idx = hierarchy.len() - 1 - hl;
        hierarchy[idx]
    };

    // Walk the compound chain using extra_bytes
    let mut current_types = parent_types;
    let mut last_name = resolve_timeline_prop_name(class_name, hier_level, prop_idx).to_string();

    // The first level is the compound property itself. Get its sub-types.
    if let Some(sub_types) = get_compound_sub_types(current_types, prop_idx as u32) {
        current_types = sub_types;

        // Each extra byte indexes into the compound sub-type table
        for (i, &sub_idx) in extra_bytes.iter().enumerate() {
            // Get the name for this sub-property
            let sub_name = get_compound_prop_name(current_types, sub_idx as u32);
            last_name = sub_name.to_string();

            // If not the last level and the sub-property is a compound, drill deeper
            if i < extra_bytes.len() - 1 {
                if let Some(deeper) = get_compound_sub_types(current_types, sub_idx as u32) {
                    current_types = deeper;
                } else {
                    break;
                }
            }
        }
    }

    last_name
}

fn get_compound_prop_name(parent_types: &[crate::xur::PropType], bit: u32) -> &'static str {
    use crate::xur::parse::types_match;
    if types_match(parent_types, crate::xur::parse::XUIFIGURE_TYPES) {
        return XUIFIGURE_NAMES.get(bit as usize).copied().unwrap_or("Unknown");
    }
    if types_match(parent_types, crate::xur::parse::FILL_TYPES) {
        return FILL_NAMES.get(bit as usize).copied().unwrap_or("Unknown");
    }
    if types_match(parent_types, crate::xur::parse::GRADIENT_TYPES) {
        return GRADIENT_NAMES.get(bit as usize).copied().unwrap_or("Unknown");
    }
    if types_match(parent_types, crate::xur::parse::STROKE_TYPES) {
        return STROKE_NAMES.get(bit as usize).copied().unwrap_or("Unknown");
    }
    "Unknown"
}

fn write_timeline_value(
    xur: &Xur<'_>,
    val: &TimelineValue,
    out: &mut String,
) -> Result<(), std::fmt::Error> {
    match val {
        TimelineValue::Bool(v) => writeln!(out, "<Prop>{v}</Prop>"),
        TimelineValue::Float(v) => writeln!(out, "<Prop>{v:.6}</Prop>"),
        TimelineValue::Unsigned(v) => writeln!(out, "<Prop>{v}</Prop>"),
        TimelineValue::String(idx) => {
            let s = xur.get_string(*idx).unwrap_or("");
            writeln!(out, "<Prop>{s}</Prop>")
        }
        TimelineValue::Color(v) => writeln!(out, "<Prop>0x{v:08x}</Prop>"),
        TimelineValue::Vector(idx) => {
            if let Some((x, y, z)) = xur.get_vector(*idx) {
                writeln!(out, "<Prop>{x:.6},{y:.6},{z:.6}</Prop>")
            } else {
                writeln!(out, "<Prop>VECT[{idx}]</Prop>")
            }
        }
        TimelineValue::Quaternion(idx) => {
            if let Some((x, y, z, w)) = xur.get_quaternion(*idx) {
                writeln!(out, "<Prop>{x:.6},{y:.6},{z:.6},{w:.6}</Prop>")
            } else {
                writeln!(out, "<Prop>QUAT[{idx}]</Prop>")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// XUI Class Extension generation
// ---------------------------------------------------------------------------

/// Standard XUI SDK classes that don't need extension files.
const STANDARD_CLASSES: &[&str] = &[
    "XuiElement", "XuiCanvas", "XuiFigure", "XuiText", "XuiImage",
    "XuiGroup", "XuiNineGrid", "XuiSound", "XuiVisual", "XuiTransition",
    "XuiImagePresenter", "XuiTextPresenter", "XuiGridPanel", "XuiShader",
    "XuiVariable", "XuiControl", "XuiLabel", "XuiCheckbox", "XuiRadioButton",
    "XuiRadioGroup", "XuiScrollEnd", "XuiScrollBar", "XuiList",
    "XuiProgressBar", "XuiSlider", "XuiEdit", "XuiCaret", "XuiButton",
    "XuiScene", "XuiNavButton", "XuiBackButton", "XuiTabScene",
    "XuiMessageBox", "XuiPerspectiveScene", "XuiListItem", "XuiCommonList",
    "XuiTextureSurface", "XuiSoundXAudio", "XuiHtmlElement",
];

/// Determine the base class name for a custom class.
fn base_class_for(class_name: &str) -> String {
    match class_name {
        "DashScene" => "XuiScene".into(),
        "DashBladeTab" | "DashMainScene" | "DashMediaScene"
        | "DashSystemScene" | "DashLiveScene"
        | "DashLiveSignedIn" | "DashLiveSignedOut" | "DashLiveConnected" => "DashScene".into(),
        "XuiBOTDScene" => "XuiScene".into(),
        "XuiBOTDContainer" => "XuiScene".into(),
        "XuiBOTDOfflineContainer" => "XuiScene".into(),
        "XuiBOTDOfflineScene" => "XuiScene".into(),
        "XuiFall07BOTDScene" => "XuiScene".into(),
        "ScriptScene" => "XuiScene".into(),
        "VideoData" => "XuiScene".into(),
        "XuiGamerCard" => "XuiControl".into(),
        "XuiPanel" => "XuiControl".into(),
        "LiveVisionControl" => "XuiControl".into(),
        "ScriptImage" => "XuiControl".into(),
        "ScriptList" => "XuiList".into(),
        "XuiButton_Multiline" => "XuiButton".into(),
        _ => {
            let base = class_name.trim_end_matches(|c: char| c.is_ascii_digit());
            if base != class_name && STANDARD_CLASSES.contains(&base) {
                return base.to_string();
            }
            "XuiControl".into()
        }
    }
}

/// Get the own (leaf-level) property definitions for a custom class.
/// Returns (name, type) pairs for properties defined at the class's own level.
fn own_properties_for(class_name: &str) -> Vec<(&'static str, &'static str)> {
    let hierarchy = get_hierarchy(class_name);
    if hierarchy.is_empty() {
        return vec![];
    }
    // The last entry in the hierarchy is the class's own level
    let own_types = hierarchy[hierarchy.len() - 1];
    if own_types.is_empty() {
        return vec![];
    }

    let names = get_names_for_hierarchy_level(class_name, hierarchy.len());

    own_types
        .iter()
        .enumerate()
        .map(|(i, prop_type)| {
            let name = names.get(i).copied().unwrap_or("Unknown");
            let type_str = match prop_type {
                PropType::Bool => "bool",
                PropType::Integer => "integer",
                PropType::Unsigned => "unsigned",
                PropType::Float => "float",
                PropType::String => "string",
                PropType::Color => "color",
                PropType::Vector3 => "vector",
                PropType::Quaternion => "quaternion",
                PropType::Compound => "object",
                PropType::Custom => "custom",
            };
            (name, type_str)
        })
        .collect()
}

/// Collect all non-standard class names from an object tree.
fn collect_custom_classes<'a>(obj: &'a Object<'_>, strings: &[String], out: &mut BTreeMap<String, ()>) {
    if let Some(class) = strings.get((obj.class_name as usize).wrapping_sub(1)) {
        if !STANDARD_CLASSES.contains(&class.as_str()) {
            // Skip suffixed variants that map to standard classes
            let base = class.trim_end_matches(|c: char| c.is_ascii_digit());
            if base == class || !STANDARD_CLASSES.contains(&base) {
                out.insert(class.clone(), ());
            }
        }
    }
    for child in &obj.children {
        collect_custom_classes(child, strings, out);
    }
}

/// Collect custom classes from a XUR into a shared map (for archive-wide consolidation).
pub fn collect_custom_classes_from_xur(xur: &Xur<'_>, out: &mut BTreeMap<String, ()>) {
    collect_custom_classes(&xur.root, &xur.strings, out);
}

/// Generate XUI Class Extension XML for all non-standard classes in a XUR file.
/// Returns `None` if there are no custom classes.
pub fn generate_class_extensions(xur: &Xur<'_>) -> Option<String> {
    let mut custom = BTreeMap::new();
    collect_custom_classes(&xur.root, &xur.strings, &mut custom);
    generate_class_extensions_from_map(&custom)
}

/// Generate XUI Class Extension XML from a pre-collected map of custom class names.
pub fn generate_class_extensions_from_map(custom: &BTreeMap<String, ()>) -> Option<String> {
    if custom.is_empty() {
        return None;
    }

    let mut out = String::new();
    writeln!(out, "<XUIClassExtension version=\"0001\">").unwrap();

    for class_name in custom.keys() {
        let base = base_class_for(class_name);
        let props = own_properties_for(class_name);

        write!(
            out,
            "<XUIClass Name=\"{class_name}\" BaseClassName=\"{base}\""
        ).unwrap();
        writeln!(out, " Bitmap=\"\" Icon=\"\" DefaultWidth=\"100\" DefaultHeight=\"100\" Description=\"{class_name}\">").unwrap();

        for (name, type_str) in &props {
            if *name == "Unknown" {
                continue; // Skip unnamed property slots
            }
            writeln!(out, "<PropDef Flags=\"\" Name=\"{name}\" Type=\"{type_str}\" Editor=\"\">").unwrap();
            writeln!(out, "<DefaultVal></DefaultVal>").unwrap();
            writeln!(out, "</PropDef>").unwrap();
        }

        writeln!(out, "</XUIClass>").unwrap();
    }

    writeln!(out, "</XUIClassExtension>").unwrap();
    Some(out)
}

/// Generate a XUI Class Extension XML file for a single class.
pub fn generate_single_class_extension(class_name: &str) -> String {
    let base = base_class_for(class_name);
    let props = own_properties_for(class_name);

    let mut out = String::new();
    writeln!(out, "<XUIClassExtension version=\"0001\">").unwrap();

    write!(
        out,
        "<XUIClass Name=\"{class_name}\" BaseClassName=\"{base}\""
    ).unwrap();
    writeln!(out, " Bitmap=\"\" Icon=\"\" DefaultWidth=\"100\" DefaultHeight=\"100\" Description=\"{class_name}\">").unwrap();

    for (name, type_str) in &props {
        if *name == "Unknown" {
            continue;
        }
        writeln!(out, "<PropDef Flags=\"\" Name=\"{name}\" Type=\"{type_str}\" Editor=\"\">").unwrap();
        writeln!(out, "<DefaultVal></DefaultVal>").unwrap();
        writeln!(out, "</PropDef>").unwrap();
    }

    writeln!(out, "</XUIClass>").unwrap();
    writeln!(out, "</XUIClassExtension>").unwrap();
    out
}

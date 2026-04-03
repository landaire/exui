use std::fmt::Write;

use crate::xur::Object;
use crate::xur::PropertyGroup;
use crate::xur::PropertyValue;
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
    "Show",                    // 7
    "Pivot",                   // 8
    "Anchor",                  // 9
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
    "Angle",            // 6
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

        // Classes with XuiControl at level 1
        "XuiLabel" | "XuiCheckbox" | "XuiRadioButton" | "XuiRadioGroup"
        | "XuiScrollEnd" | "XuiScrollBar" | "XuiList" | "XuiProgressBar"
        | "XuiSlider" | "XuiEdit" | "XuiCaret" | "XuiBackButton"
        | "XuiListItem" | "XuiCommonList" | "XuiMessageBox"
        | "XuiPerspectiveScene" => {
            if level == 1 { XUICONTROL_NAMES } else { &[] }
        }

        // v5-only classes
        "XuiHtmlElement" => if level == 1 { XUIHTMLELEMENT_NAMES } else { &[] },

        _ => &[],
    }
}

const XUIHTMLELEMENT_NAMES: &[&str] = &[
    "Text", // 0: HTML text content
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
    let ptr = names.as_ptr();
    // Gradient properties at bits 2 (StopColor) and 3 (StopPos) are arrays
    std::ptr::eq(ptr, GRADIENT_NAMES.as_ptr()) && matches!(bit, 2 | 3)
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
            let clean = s.replace("\r\n", "\n");
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
            writeln!(out, "<{name}>QUAT[{idx}]</{name}>")?;
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

use std::fmt::Write;
use std::string::String;
use std::vec::Vec;

use super::builder::Bytes;
use super::builder::XurBuilder;
use crate::custom::FigurePoints;
use crate::error::XurError;
use crate::schema::ClassDef;
use crate::schema::DefaultValue;
use crate::schema::PropDef;
use crate::schema::PropFlags;
use crate::schema::PropType;
use crate::schema::Registry;
use crate::schema::xui;
use crate::xur::Document;
use crate::xur::KeyframeValue;
use crate::xur::ObjectRef;
use crate::xur::Property;
use crate::xur::Value;

static WIDGET_SHAPE_PROPS: [PropDef; 2] = [
    PropDef::new("Size", PropType::Float, PropFlags::from_bits(0), None),
    PropDef::new("Tint", PropType::Color, PropFlags::from_bits(0), None),
];

static WIDGET_LAYER_PROPS: [PropDef; 1] = [PropDef::new(
    "Size",
    PropType::Float,
    PropFlags::from_bits(0),
    Some(DefaultValue::Float(1.0)),
)];

static WIDGET_PROPS: [PropDef; 12] = [
    PropDef::new("Flag", PropType::Bool, PropFlags::from_bits(0), None),
    PropDef::new("Count", PropType::Integer, PropFlags::from_bits(0), None),
    PropDef::new("Mode", PropType::Unsigned, PropFlags::from_bits(0), None),
    PropDef::new("Scale", PropType::Float, PropFlags::from_bits(0), None),
    PropDef::new("Label", PropType::String, PropFlags::from_bits(0), None),
    PropDef::new("Tint", PropType::Color, PropFlags::from_bits(0), None),
    PropDef::new("Offset", PropType::Vector, PropFlags::from_bits(0), None),
    PropDef::new("Turn", PropType::Quaternion, PropFlags::from_bits(0), None),
    PropDef::new(
        "Shape",
        PropType::Compound(&WIDGET_SHAPE_PROPS),
        PropFlags::from_bits(0),
        None,
    ),
    PropDef::new("Blob", PropType::Custom, PropFlags::from_bits(0), None),
    PropDef::new("Samples", PropType::Float, PropFlags::INDEXED, None),
    PropDef::new(
        "Layers",
        PropType::Compound(&WIDGET_LAYER_PROPS),
        PropFlags::INDEXED,
        None,
    ),
];

static WIDGET: ClassDef = ClassDef::new("TestWidget", Some("XuiControl"), &WIDGET_PROPS);

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(&WIDGET).expect("test class registers");

    registry
}

fn class_name(registry: &Registry, object: ObjectRef<'_, '_>) -> &'static str {
    registry
        .class(object.class())
        .map_or("<foreign class>", |class| class.name())
}

fn dump(doc: &Document<'_>, registry: &Registry) -> String {
    let mut out = String::new();

    writeln!(out, "allocation_hints: {:?}", doc.allocation_hints()).unwrap();
    writeln!(out, "strings: {:?}", doc.strings()).unwrap();
    writeln!(out, "vectors: {}", doc.vector_count()).unwrap();
    writeln!(out, "quaternions: {}", doc.quaternion_count()).unwrap();
    writeln!(out, "custom: {:02x?}", doc.custom()).unwrap();

    for object in doc.objects() {
        let parent = object.parent().map(|parent| parent.id());
        writeln!(
            out,
            "object {:?}: {} parent {parent:?}",
            object.id(),
            class_name(registry, object)
        )
        .unwrap();
        for property in object.properties() {
            dump_property(&mut out, doc, property, 1);
        }
        let children: Vec<_> = object.children().map(|child| child.id()).collect();
        if !children.is_empty() {
            writeln!(out, "  children: {children:?}").unwrap();
        }
        for frame in object.named_frames() {
            writeln!(
                out,
                "  named frame {:?} at {:?} command {:?} target {:?}",
                frame.name.and_then(|id| doc.string(id)),
                frame.time,
                frame.command,
                frame.target.and_then(|id| doc.string(id)),
            )
            .unwrap();
        }
        for timeline in object.timelines() {
            writeln!(
                out,
                "  timeline {:?} target {:?}",
                timeline.element.and_then(|id| doc.string(id)),
                timeline.target,
            )
            .unwrap();
            for property in doc.timeline_properties(timeline) {
                let names: Vec<&str> = property
                    .path
                    .map(|path| doc.path_defs(&path).iter().map(|def| def.name).collect())
                    .unwrap_or_default();
                let level = property.path.map(|path| {
                    (
                        registry.class(path.class).map(|class| class.name()),
                        path.levels_up,
                        path.index,
                    )
                });
                writeln!(
                    out,
                    "    path {names:?} level {level:?} element {:?} has_value {}",
                    property.element,
                    property.has_value()
                )
                .unwrap();
            }
            for keyframe in doc.keyframes(timeline) {
                writeln!(
                    out,
                    "    keyframe {:?} {:?} {:?} {:?} {:?}",
                    keyframe.time,
                    keyframe.interpolation,
                    keyframe.ease_in,
                    keyframe.ease_out,
                    keyframe.ease_scale,
                )
                .unwrap();
                for value in doc.keyframe_values(keyframe) {
                    match value {
                        KeyframeValue::String(Some(id)) => {
                            writeln!(out, "      String({:?})", doc.string(*id)).unwrap();
                        }
                        value => writeln!(out, "      {value:?}").unwrap(),
                    }
                }
            }
        }
    }

    out
}

fn dump_property(out: &mut String, doc: &Document<'_>, property: &Property, depth: usize) {
    let indent = "  ".repeat(depth);
    let def = property.def();
    let values = doc.values(property);
    if def.flags.is_indexed() {
        writeln!(out, "{indent}{} [{}]:", def.name, values.len()).unwrap();
    } else {
        writeln!(out, "{indent}{}:", def.name).unwrap();
    }

    for value in values {
        match value {
            Value::Compound(compound) => {
                writeln!(out, "{indent}  -").unwrap();
                for child in doc.compound(*compound) {
                    dump_property(out, doc, child, depth + 2);
                }
            }
            value => writeln!(out, "{indent}  - {}", render(doc, value)).unwrap(),
        }
    }
}

fn render(doc: &Document<'_>, value: &Value) -> String {
    match value {
        Value::String(Some(id)) => std::format!("String({:?})", doc.string(*id)),
        Value::Vector(index) => std::format!("{index:?} = {:?}", doc.vector(*index)),
        Value::Quaternion(index) => std::format!("{index:?} = {:?}", doc.quaternion(*index)),
        Value::Custom(offset) => std::format!("{offset:?} = {:02x?}", doc.custom_data(*offset)),
        value => std::format!("{value:?}"),
    }
}

/// Parse outcome without borrowing the fixture bytes.
fn parse(builder: &XurBuilder) -> Result<(), XurError> {
    Document::parse(&builder.build(), &registry()).map(|_| ())
}

/// Scene exercising every property type, compounds, indexed values, children
/// and timelines.
fn full_scene() -> XurBuilder {
    let mut b = XurBuilder::default();
    b.allocation_hints = Some([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    b.extra_sections.push((*b"FLOT", std::vec![0; 4]));
    // Two length-prefixed blobs; the second is referenced.
    b.custom
        .u32(1)
        .u8(0x55)
        .u32(4)
        .raw(&[0xDE, 0xAD, 0xBE, 0xEF]);
    let position = b.vector([1.0, 2.0, 3.0]);
    let rotation = b.quaternion([0.0, 0.0, 0.0, 1.0]);
    let offset = b.vector([-4.0, 5.5, 0.0]);
    let turn = b.quaternion([0.5, 0.5, 0.5, 0.5]);

    // Root: XuiElement Id, Width, Position, Rotation, Anchor, Show.
    b.object("XuiCanvas", 7);
    b.data.u16(6);
    b.mask(4, 0b10_1010_1011);
    b.string_value("root");
    b.data.f32(640.0).u32(position).u32(rotation).u32(15).u8(1);
    b.empty_level();
    b.data.u32(3);

    // Child 0: every type on a runtime-registered class.
    b.object("TestWidget", 1);
    b.data.u16(16);
    b.mask(1, 1);
    b.string_value("widget");
    // XuiControl level with an unsupported mask size: three bytes skipped.
    b.data.u8(3).raw(&[0xFF, 0xFF, 0xFF]);
    b.mask(2, 0x0FFF);
    b.data.u8(2).u32((-5i32) as u32).u32(7).f32(1.5);
    b.string_value("hello");
    b.data.u32(0x80FF_0000).u32(offset).u32(turn);
    // Shape: two sub-values.
    b.data.u16(2);
    b.mask(1, 0b11);
    b.data.f32(8.0).u32(0xFF00_FF00);
    b.data.u32(5);
    // Samples: three values, one-byte count.
    b.data.u8(3).f32(0.25).f32(0.5).f32(0.75);
    // Layers: two compound values, extended count form.
    b.data.u8(0x81).u8(2);
    b.data.u16(1);
    b.mask(1, 1);
    b.data.f32(10.0);
    b.data.u16(1);
    b.mask(1, 1);
    b.data.f32(20.0);

    // Child 1: figure with nested compounds and indexed gradient stops.
    b.object("XuiFigure", 1);
    b.data.u16(3);
    b.mask(1, 1);
    b.string_value("fig");
    b.mask(1, 0b110);
    b.data.u16(3);
    b.mask(2, 0b1011);
    b.data.u32(2).u32(0xFF11_2233);
    b.data.u16(6);
    b.mask(1, 0b1111);
    b.data.u8(1).u32(2);
    b.data.u8(2).u32(0xFFFF_FFFF).u32(0xFF00_0000);
    b.data.u8(2).f32(0.0).f32(1.0);
    b.data.u8(1);

    // Child 2: indexed property with a zero count records nothing.
    b.object("TestWidget", 1);
    b.data.u16(1);
    b.mask(1, 1);
    b.string_value("empty");
    b.empty_level();
    b.mask(2, 1 << 10);
    b.data.u8(0);

    // Root timelines.
    b.data.u32(2);
    b.string_value("Normal");
    b.data.u32(0).u8(0).u16(0);
    b.string_value("EndNormal");
    b.data.u32(10).u8(3);
    b.string_value("Normal");

    b.data.u32(2);
    b.string_value("fig");
    b.data.u32(4);
    // Width on XuiElement, one level above XuiFigure.
    b.data.u8(1).u8(1).u8(1);
    // Fill.FillColor.
    b.data.u8(2).u8(0).u8(1).u8(1);
    // Position with an element index.
    b.data.u8(0x81).u8(1).u8(3).u32(0);
    // Stroke: compound, no keyframe value.
    b.data.u8(1).u8(0).u8(0);
    b.data.u32(2);
    b.data.u32(0).u8(0).u8(0).u8(0).u8(0);
    b.data.f32(100.0).u32(0xFF00_0000).u32(position);
    b.data.u32(20).u8(2).u8(10).u8(20).u8(30);
    b.data.f32(200.0).u32(0xFFFF_FFFF).u32(offset);

    b.string_value("widget");
    b.data.u32(5);
    for index in [4, 7, 0, 1, 2] {
        b.data.u8(1).u8(0).u8(index);
    }
    b.data.u32(1);
    b.data.u32(5).u8(1).u8(0).u8(0).u8(0);
    b.string_value("Normal");
    b.data.u32(turn).u8(0).u32(9).u32(0x1234_5678);

    b
}

#[test]
fn full_scene_snapshot() {
    let registry = registry();
    let bytes = full_scene().build();
    let doc = Document::parse(&bytes, &registry).expect("scene parses");

    insta::assert_snapshot!(dump(&doc, &registry));
}

#[test]
fn timelines_skipped_when_data_ends() {
    let mut b = XurBuilder::default();
    b.object("XuiCanvas", 4);

    let registry = registry();
    let bytes = b.build();
    let doc = Document::parse(&bytes, &registry).expect("scene parses");

    insta::assert_snapshot!(dump(&doc, &registry));
}

#[test]
fn timelines_need_children() {
    let mut b = XurBuilder::default();
    b.object("XuiCanvas", 4);
    b.data.u32(1);
    b.string_value("Normal");
    b.data.u32(0).u8(1).u16(0);
    // Not a timeline count: objects without children have none.
    b.data.u32(0xFFFF_FFFF);

    let registry = registry();
    let bytes = b.build();
    let doc = Document::parse(&bytes, &registry).expect("scene parses");

    insta::assert_snapshot!(dump(&doc, &registry));
}

#[test]
fn header_errors() {
    let valid = {
        let mut b = XurBuilder::default();
        b.object("XuiCanvas", 0);
        b.build()
    };

    let mut bad_magic = valid.clone();
    bad_magic[0] = b'X' + 1;

    let mut bad_version = valid.clone();
    bad_version[7] = 8;

    let mut bad_schema = valid.clone();
    bad_schema[13] = 0xD;

    let mut bad_size = valid.clone();
    bad_size.push(0);

    let mut one_section = valid.clone();
    one_section[0x13] = 1;

    let no_data = {
        let mut b = XurBuilder::default();
        b.object("XuiCanvas", 0);
        let mut bytes = b.build();
        // Rename the DATA entry.
        bytes[0x14 + 12..0x14 + 16].copy_from_slice(b"ATAD");
        bytes
    };

    let out_of_bounds = {
        let mut bytes = valid.clone();
        // Grow the DATA size past the end.
        bytes[0x14 + 12 + 8..0x14 + 12 + 12].copy_from_slice(&0x100u32.to_be_bytes());
        bytes
    };

    let truncated_header = valid[..0x10].to_vec();

    let registry = registry();
    let results: Vec<Result<(), XurError>> = [
        bad_magic,
        bad_version,
        bad_schema,
        bad_size,
        one_section,
        no_data,
        out_of_bounds,
        truncated_header,
    ]
    .iter()
    .map(|bytes| Document::parse(bytes, &registry).map(|_| ()))
    .collect();

    insta::assert_debug_snapshot!(results);
}

#[test]
fn table_errors() {
    let registry = registry();

    let mut bad_string = XurBuilder::default();
    bad_string.object("XuiCanvas", 0);
    let mut bad_string = bad_string.build();
    // First string length gains bit 15.
    let strn_offset = u32::from_be_bytes(bad_string[0x18..0x1C].try_into().unwrap()) as usize;
    bad_string[strn_offset] |= 0x80;

    let mut bad_vect = XurBuilder::default();
    bad_vect.object("XuiCanvas", 0);
    bad_vect.extra_sections.push((*b"VECT", std::vec![0; 13]));

    insta::assert_debug_snapshot!([
        Document::parse(&bad_string, &registry).map(|_| ()),
        Document::parse(&bad_vect.build(), &registry).map(|_| ()),
    ]);
}

#[test]
fn object_errors() {
    let mut unknown = XurBuilder::default();
    unknown.object("XuiScene1", 0);

    let mut null_class = XurBuilder::default();
    null_class.string("XuiCanvas");
    null_class.data.u16(0).u8(0);

    let mut count_mismatch = XurBuilder::default();
    count_mismatch.object("XuiCanvas", 1);
    count_mismatch.data.u16(2);
    count_mismatch.mask(1, 0b10);
    count_mismatch.data.f32(1.0);
    count_mismatch.empty_level();

    let mut compound_overflow = XurBuilder::default();
    compound_overflow.object("XuiFigure", 1);
    compound_overflow.data.u16(1);
    compound_overflow.empty_level();
    compound_overflow.mask(1, 0b1);
    compound_overflow.data.u16(1);
    compound_overflow.mask(1, 0b11);
    compound_overflow.data.f32(1.0).u32(0);

    let mut truncated = XurBuilder::default();
    truncated.object("XuiCanvas", 1);
    truncated.data.u16(1);
    truncated.mask(1, 0b10);
    truncated.data.u16(0);

    let mut short_children = XurBuilder::default();
    short_children.object("XuiCanvas", 2);
    short_children.data.u16(0);

    insta::assert_debug_snapshot!([
        parse(&unknown),
        parse(&null_class),
        parse(&count_mismatch),
        parse(&compound_overflow),
        parse(&truncated),
        parse(&short_children),
    ]);
}

/// Root canvas with one child canvas `child` and the given timeline bytes.
fn timeline_scene(write: impl FnOnce(&mut XurBuilder)) -> XurBuilder {
    let mut b = XurBuilder::default();
    b.object("XuiCanvas", 6);
    b.data.u32(1);
    b.object("XuiCanvas", 1);
    b.data.u16(1);
    b.mask(1, 1);
    b.string_value("child");
    b.empty_level();
    b.data.u32(0);
    write(&mut b);

    b
}

#[test]
fn timeline_errors() {
    let missing_target = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("other");
        b.data.u32(1).u8(1).u8(0).u8(0).u32(0);
    });

    let target_without_properties = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("other");
        b.data.u32(0).u32(0);
    });

    let bad_level = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("child");
        b.data.u32(1).u8(1).u8(2).u8(0).u32(0);
    });

    let bad_index = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("child");
        b.data.u32(1).u8(1).u8(0).u8(0).u32(0);
    });

    let not_compound = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("child");
        b.data.u32(1).u8(2).u8(1).u8(1).u8(0).u32(0);
    });

    let empty_path = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("child");
        b.data.u32(1).u8(0).u32(1);
        b.data.u32(0).u8(0).u8(0).u8(0).u8(0);
    });

    let bad_vector = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("child");
        b.data.u32(1).u8(1).u8(1).u8(3).u32(1);
        b.data.u32(0).u8(0).u8(0).u8(0).u8(0).u32(0);
    });

    let bad_frame_string = timeline_scene(|b| {
        // Replace the empty named frame list with one bad entry.
        b.data.0.truncate(b.data.0.len() - 4);
        b.data.u32(1).u16(0x7FFF).u32(0).u8(0).u16(0);
    });

    let results = [
        parse(&missing_target),
        parse(&target_without_properties),
        parse(&bad_level),
        parse(&bad_index),
        parse(&not_compound),
        parse(&empty_path),
        parse(&bad_vector),
        parse(&bad_frame_string),
    ];

    insta::assert_debug_snapshot!(results);
}

#[test]
fn figure_points() {
    let mut full = Bytes::default();
    full.f32(300.0).f32(200.0).u32(2);
    for coord in [0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 300.0, 0.0, 5.0, 6.0, 7.0, 8.0] {
        full.f32(coord);
    }

    let mut short_points = Bytes::default();
    short_points.f32(1.0).f32(2.0).u32(2).f32(0.0);

    let mut size_only = Bytes::default();
    size_only.f32(1.0).f32(2.0).u16(0);

    insta::assert_debug_snapshot!([
        FigurePoints::parse(&full.0),
        FigurePoints::parse(&short_points.0),
        FigurePoints::parse(&size_only.0),
        FigurePoints::parse(&[0; 7]),
    ]);
}

/// An effect child carries `XuiElement.Id` and `XuiEffect.Id`; a timeline
/// matches either, and the earliest child wins.
#[test]
fn effect_child_with_two_ids() {
    let mut b = XurBuilder::default();
    b.object("XuiCanvas", 6);
    b.data.u32(2);

    b.object("XuiEdgeEffect", 1);
    b.data.u16(2);
    b.mask(1, 1);
    b.string_value("a");
    b.mask(1, 1);
    b.string_value("b");
    b.empty_level();

    b.object("XuiCanvas", 1);
    b.data.u16(1);
    b.mask(1, 1);
    b.string_value("b");
    b.empty_level();

    b.data.u32(0);
    b.data.u32(2);
    for element in ["b", "a"] {
        b.string_value(element);
        // Opacity on XuiElement, two levels above XuiEdgeEffect.
        b.data.u32(1).u8(1).u8(2).u8(6);
        b.data.u32(1);
        b.data.u32(0).u8(0).u8(0).u8(0).u8(0).f32(0.5);
    }

    let registry = registry();
    let bytes = b.build();
    let doc = Document::parse(&bytes, &registry).expect("scene parses");
    let effect = doc.root().children().next().expect("first child");
    let effect_id = effect
        .property(&xui::XUI_EFFECT.props[0])
        .and_then(|property| doc.value(property))
        .and_then(Value::as_string)
        .flatten()
        .and_then(|id| doc.string(id));

    insta::assert_snapshot!(std::format!(
        "{}effect Id: {effect_id:?}\n",
        dump(&doc, &registry)
    ));
}

/// Counts far beyond the data end in a truncation error without
/// allocating for the declared count.
#[test]
fn hostile_counts() {
    let mut children = XurBuilder::default();
    children.object("XuiCanvas", 2);
    children.data.u32(u32::MAX);

    let mut elements = XurBuilder::default();
    elements.object("TestWidget", 1);
    elements.data.u16(0xFFFF);
    elements.empty_level();
    elements.empty_level();
    elements.mask(2, 1 << 10);
    elements.data.u8(0x84).u32(u32::MAX).f32(1.0).f32(2.0);

    let mut frames = XurBuilder::default();
    frames.object("XuiCanvas", 4);
    frames.data.u32(u32::MAX);
    frames.string_value("Normal");
    frames.data.u32(0).u8(0).u16(0);

    let timelines = timeline_scene(|b| {
        b.data.u32(u32::MAX);
        b.string_value("child");
        b.data.u32(0).u32(0);
    });

    let paths = timeline_scene(|b| {
        b.data.u32(1);
        b.string_value("child");
        b.data.u32(u32::MAX).u8(1).u8(1).u8(1);
    });

    let keyframes = figure_timeline_scene(1, u32::MAX, 3);

    insta::assert_debug_snapshot!([
        parse(&children),
        parse(&elements),
        parse(&frames),
        parse(&timelines),
        parse(&paths),
        parse(&keyframes),
    ]);
}

/// Root canvas with one child `XuiFigure` "fig" and a timeline of
/// `path_count` value-less paths (`Stroke`) and `keyframe_count` declared
/// keyframes, of which `keyframes_present` are stored.
fn figure_timeline_scene(
    path_count: u32,
    keyframe_count: u32,
    keyframes_present: u32,
) -> XurBuilder {
    let mut b = XurBuilder::default();
    b.object("XuiCanvas", 6);
    b.data.u32(1);
    b.object("XuiFigure", 1);
    b.data.u16(1);
    b.mask(1, 1);
    b.string_value("fig");
    b.empty_level();
    b.data.u32(0);
    b.data.u32(1);
    b.string_value("fig");
    b.data.u32(path_count);
    for _ in 0..path_count {
        b.data.u8(1).u8(0).u8(0);
    }
    b.data.u32(keyframe_count);
    for time in 0..keyframes_present {
        b.data.u32(time).u8(0).u8(0).u8(0).u8(0);
    }

    b
}

/// Shapes that cost paths times keyframes, or timelines times children,
/// with a naive reader.
#[test]
fn large_timelines_parse() {
    const PATHS: u32 = 20_000;
    const KEYFRAMES: u32 = 20_000;
    const CHILDREN: u32 = 5_000;

    let registry = registry();

    let bytes = figure_timeline_scene(PATHS, KEYFRAMES, KEYFRAMES).build();
    let doc = Document::parse(&bytes, &registry).expect("scene parses");
    let timeline = &doc.root().timelines()[0];
    let keyframe_values: usize = doc
        .keyframes(timeline)
        .iter()
        .map(|keyframe| doc.keyframe_values(keyframe).len())
        .sum();

    let mut b = XurBuilder::default();
    b.object("XuiCanvas", 6);
    b.data.u32(CHILDREN);
    for index in 0..CHILDREN {
        b.object("XuiCanvas", 1);
        b.data.u16(1);
        b.mask(1, 1);
        b.string_value(&std::format!("child{index}"));
        b.empty_level();
    }
    b.data.u32(0);
    b.data.u32(CHILDREN);
    for index in (0..CHILDREN).rev() {
        b.string_value(&std::format!("child{index}"));
        // Width on XuiElement, one level above XuiCanvas.
        b.data.u32(1).u8(1).u8(1).u8(1);
        b.data.u32(0);
    }
    let many_children = b.build();
    let doc_children = Document::parse(&many_children, &registry).expect("scene parses");
    let targets_match = doc_children
        .root()
        .timelines()
        .iter()
        .zip((1..=CHILDREN).rev())
        .all(|(timeline, child)| timeline.target.map(|id| id.index()) == Some(child as usize));

    assert_eq!(
        (
            doc.timeline_properties(timeline).len(),
            doc.keyframes(timeline).len(),
            keyframe_values,
            doc_children.root().timelines().len(),
            targets_match,
        ),
        (
            PATHS as usize,
            KEYFRAMES as usize,
            0,
            CHILDREN as usize,
            true
        )
    );
}

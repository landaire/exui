use std::fmt::Write;
use std::string::String;
use std::vec;
use std::vec::Vec;

use crate::archive::AcceptedVersions;
use crate::archive::Archive;
use crate::archive::ArchiveVersion;
use crate::archive_writer::NewEntry;
use crate::archive_writer::write_archive;
use crate::error::ArchiveWriteError;
use crate::error::WriteError;
use crate::schema::ClassDef;
use crate::schema::PropDef;
use crate::schema::PropFlags;
use crate::schema::PropType;
use crate::schema::Registry;
use crate::schema::xui;
use crate::tree::Keyframe;
use crate::tree::KeyframeValue;
use crate::tree::NamedFrame;
use crate::tree::Object;
use crate::tree::Property;
use crate::tree::PropertyPath;
use crate::tree::Timeline;
use crate::tree::TimelineProperty;
use crate::tree::Tree;
use crate::tree::Value;
use crate::value::Color;
use crate::value::Quaternion;
use crate::value::Vector3;
use crate::wide::WideString;
use crate::writer::WriteOptions;
use crate::writer::write;
use crate::xur::Document;
use crate::xur::EaseIn;
use crate::xur::EaseOut;
use crate::xur::EaseScale;
use crate::xur::ElementIndex;
use crate::xur::FrameCommand;
use crate::xur::FrameTime;
use crate::xur::Interpolation;
use crate::xur::LevelsUp;
use crate::xur::PropIndex;

static GAUGE_SHAPE_PROPS: [PropDef; 2] = [
    PropDef::new("Size", PropType::Float, PropFlags::from_bits(0), None),
    PropDef::new("Tint", PropType::Color, PropFlags::from_bits(0), None),
];

static GAUGE_PROPS: [PropDef; 11] = [
    PropDef::new("Flag", PropType::Bool, PropFlags::from_bits(0), None),
    PropDef::new("Count", PropType::Integer, PropFlags::from_bits(0), None),
    PropDef::new("Mode", PropType::Unsigned, PropFlags::from_bits(0), None),
    PropDef::new("Label", PropType::String, PropFlags::from_bits(0), None),
    PropDef::new("Tint", PropType::Color, PropFlags::from_bits(0), None),
    PropDef::new("Offset", PropType::Vector, PropFlags::from_bits(0), None),
    PropDef::new("Turn", PropType::Quaternion, PropFlags::from_bits(0), None),
    PropDef::new(
        "Shape",
        PropType::Compound(&GAUGE_SHAPE_PROPS),
        PropFlags::from_bits(0),
        None,
    ),
    PropDef::new("Blob", PropType::Custom, PropFlags::from_bits(0), None),
    PropDef::new("Samples", PropType::Float, PropFlags::INDEXED, None),
    PropDef::new("Note", PropType::String, PropFlags::from_bits(0), None),
];

static GAUGE: ClassDef = ClassDef::new("TestGauge", Some("XuiControl"), &GAUGE_PROPS);

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(&GAUGE).expect("test class registers");

    registry
}

fn def(defs: &'static [PropDef], name: &str) -> &'static PropDef {
    defs.iter()
        .find(|def| def.name == name)
        .expect("definition exists")
}

fn text(value: &str) -> Value {
    Value::String(Some(WideString::from(value)))
}

fn gauge(registry: &Registry) -> Object {
    let mut object = Object::new(registry.find("TestGauge").expect("registered"));
    object.properties = vec![
        Property::new(def(xui::XUI_ELEMENT.props, "Id"), text("Needle")),
        Property::new(def(xui::XUI_ELEMENT.props, "Width"), Value::Float(64.0)),
        Property::new(def(&GAUGE_PROPS, "Flag"), Value::Bool(true)),
        Property::new(def(&GAUGE_PROPS, "Count"), Value::Integer(-3)),
        Property::new(def(&GAUGE_PROPS, "Mode"), Value::Unsigned(7)),
        Property::new(def(&GAUGE_PROPS, "Label"), text("Speed")),
        Property::new(
            def(&GAUGE_PROPS, "Tint"),
            Value::Color(Color::from_argb(0xFF10_2030)),
        ),
        Property::new(
            def(&GAUGE_PROPS, "Offset"),
            Value::Vector(Vector3::new(1.0, 2.0, 3.0)),
        ),
        Property::new(
            def(&GAUGE_PROPS, "Turn"),
            Value::Quaternion(Quaternion::new(0.0, 0.0, 0.5, 1.0)),
        ),
        Property::new(
            def(&GAUGE_PROPS, "Shape"),
            Value::Compound(vec![Property::new(
                def(&GAUGE_SHAPE_PROPS, "Tint"),
                Value::Color(Color::from_argb(1)),
            )]),
        ),
        Property::new(def(&GAUGE_PROPS, "Blob"), Value::Custom(vec![0xAA, 0xBB])),
        Property {
            def: def(&GAUGE_PROPS, "Samples"),
            values: (0..0x90)
                .map(|sample| Value::Float(sample as f32))
                .collect(),
        },
        Property::new(def(&GAUGE_PROPS, "Note"), Value::String(None)),
    ];

    object
}

/// Canvas with a gauge child, animated by the canvas.
fn sample_tree(registry: &Registry) -> Tree {
    let mut root = Object::new(registry.find("XuiCanvas").expect("built in"));
    root.properties = vec![Property::new(
        def(xui::XUI_ELEMENT.props, "Height"),
        Value::Float(720.0),
    )];
    root.children.push(gauge(registry));

    let mut second = gauge(registry);
    second.properties[0] = Property::new(def(xui::XUI_ELEMENT.props, "Id"), text("Dial"));
    // A shared blob is stored once.
    root.children.push(second);

    root.named_frames = vec![
        NamedFrame {
            name: Some(WideString::from("Start")),
            time: FrameTime(0),
            command: FrameCommand(0),
            target: None,
        },
        NamedFrame {
            name: Some(WideString::from("Loop")),
            time: FrameTime(20),
            command: FrameCommand(2),
            target: Some(WideString::from("Start")),
        },
    ];

    let width = def(xui::XUI_ELEMENT.props, "Width");
    let width_index = xui::XUI_ELEMENT
        .props
        .iter()
        .position(|candidate| core::ptr::eq(candidate, width))
        .expect("Width is declared");
    let shape = def(&GAUGE_PROPS, "Shape");
    let samples = def(&GAUGE_PROPS, "Samples");
    root.timelines = vec![Timeline {
        element: Some(WideString::from("Dial")),
        properties: vec![
            TimelineProperty {
                path: Some(PropertyPath {
                    levels_up: LevelsUp(2),
                    index: PropIndex(width_index as u8),
                    defs: vec![width],
                }),
                element: None,
            },
            TimelineProperty {
                path: Some(PropertyPath {
                    levels_up: LevelsUp(0),
                    index: PropIndex(7),
                    defs: vec![shape, def(&GAUGE_SHAPE_PROPS, "Size")],
                }),
                element: None,
            },
            TimelineProperty {
                path: Some(PropertyPath {
                    levels_up: LevelsUp(0),
                    index: PropIndex(9),
                    defs: vec![samples],
                }),
                element: Some(ElementIndex(4)),
            },
        ],
        keyframes: vec![
            Keyframe {
                time: FrameTime(0),
                interpolation: Interpolation(0),
                ease_in: EaseIn(0),
                ease_out: EaseOut(0),
                ease_scale: EaseScale(0),
                values: vec![
                    KeyframeValue::Float(1.0),
                    KeyframeValue::Float(2.0),
                    KeyframeValue::Float(3.0),
                ],
            },
            Keyframe {
                time: FrameTime(20),
                interpolation: Interpolation(2),
                ease_in: EaseIn(10),
                ease_out: EaseOut(20),
                ease_scale: EaseScale(30),
                values: vec![
                    KeyframeValue::Float(4.0),
                    KeyframeValue::Float(5.0),
                    KeyframeValue::Float(6.0),
                ],
            },
        ],
    }];

    Tree { root }
}

fn hex_lines(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(16) {
        for byte in chunk {
            write!(out, "{byte:02x} ").unwrap();
        }
        out.pop();
        out.push('\n');
    }
    out
}

#[test]
fn tree_round_trips_through_the_reader() {
    let registry = registry();
    let tree = sample_tree(&registry);

    for allocation_hints in [false, true] {
        let bytes =
            write(&tree, &registry, WriteOptions { allocation_hints }).expect("tree writes");
        let document = Document::parse(&bytes, &registry).expect("written scene parses");
        assert_eq!(document.allocation_hints().is_some(), allocation_hints);

        let again = Tree::from_document(&document, &registry).expect("document converts");
        assert_eq!(again, tree);
    }
}

#[test]
fn layout_snapshot() {
    let registry = registry();
    let bytes = write(
        &sample_tree(&registry),
        &registry,
        WriteOptions {
            allocation_hints: true,
        },
    )
    .expect("tree writes");

    insta::assert_snapshot!(hex_lines(&bytes));
}

#[test]
fn writer_rejects_trees_without_a_binary_form() {
    let registry = registry();
    let canvas = registry.find("XuiCanvas").expect("built in");
    let mut errors = Vec::new();

    let mut foreign = Object::new(canvas);
    foreign
        .properties
        .push(Property::new(def(&GAUGE_PROPS, "Flag"), Value::Bool(true)));
    errors.push(write(
        &Tree { root: foreign },
        &registry,
        WriteOptions::default(),
    ));

    let mut mistyped = gauge(&registry);
    mistyped.properties[2] = Property::new(def(&GAUGE_PROPS, "Flag"), Value::Integer(1));
    errors.push(write(
        &Tree { root: mistyped },
        &registry,
        WriteOptions::default(),
    ));

    let mut childless = sample_tree(&registry);
    childless.root.children.clear();
    errors.push(write(&childless, &registry, WriteOptions::default()));

    let mut short_keyframe = sample_tree(&registry);
    short_keyframe.root.timelines[0].keyframes[0].values.pop();
    errors.push(write(&short_keyframe, &registry, WriteOptions::default()));

    let mut wrong_target = sample_tree(&registry);
    wrong_target.root.timelines[0].element = Some(WideString::from("Missing"));
    errors.push(write(&wrong_target, &registry, WriteOptions::default()));

    let mut two_values = gauge(&registry);
    two_values.properties[2].values.push(Value::Bool(false));
    errors.push(write(
        &Tree { root: two_values },
        &registry,
        WriteOptions::default(),
    ));

    let mut duplicate = gauge(&registry);
    duplicate
        .properties
        .push(Property::new(def(&GAUGE_PROPS, "Flag"), Value::Bool(false)));
    errors.push(write(
        &Tree { root: duplicate },
        &registry,
        WriteOptions::default(),
    ));

    let mut nul = gauge(&registry);
    nul.properties[5] = Property::new(
        def(&GAUGE_PROPS, "Label"),
        Value::String(Some(WideString::new(vec![0x61, 0, 0x62]))),
    );
    errors.push(write(
        &Tree { root: nul },
        &registry,
        WriteOptions::default(),
    ));

    let errors: Vec<WriteError> = errors
        .into_iter()
        .map(|result| result.expect_err("tree is rejected"))
        .collect();
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn archive_round_trips() {
    let entries = [
        NewEntry {
            name: "scene.xur",
            data: b"first",
        },
        NewEntry {
            name: "de-de\\Strings.xus",
            data: b"second entry",
        },
    ];

    for version in [ArchiveVersion::V1, ArchiveVersion::V3] {
        let bytes = write_archive(version, &entries).expect("archive writes");
        let archive = Archive::parse(&bytes, AcceptedVersions::ALL).expect("archive parses");
        assert_eq!(archive.version(), version);
        assert_eq!(archive.open("DE-DE/strings.xus"), Ok(&b"second entry"[..]));
        assert_eq!(archive.open("scene.xur"), Ok(&b"first"[..]));
    }

    let bytes = write_archive(ArchiveVersion::V1, &entries).expect("archive writes");
    insta::assert_snapshot!(hex_lines(&bytes));
}

#[test]
fn archive_writer_rejects_unreachable_names() {
    let duplicate = [
        NewEntry {
            name: "A\\b.png",
            data: b"",
        },
        NewEntry {
            name: "a/B.png",
            data: b"",
        },
    ];
    let long_name = "n".repeat(256);
    let errors = [
        write_archive(ArchiveVersion::V1, &duplicate),
        write_archive(
            ArchiveVersion::V1,
            &[NewEntry {
                name: "",
                data: b"",
            }],
        ),
        write_archive(
            ArchiveVersion::V1,
            &[NewEntry {
                name: &long_name,
                data: b"",
            }],
        ),
    ];

    assert_eq!(
        errors.map(|result| result.expect_err("entries are rejected")),
        [
            ArchiveWriteError::DuplicateName { first: 0, index: 1 },
            ArchiveWriteError::InvalidName { index: 0 },
            ArchiveWriteError::NameTooLong { index: 0 },
        ]
    );
}

/// The reader matches a timeline against every `Id` of each child, so an
/// effect is found by its second `Id` and the first matching child wins.
#[test]
fn timelines_find_children_by_any_id() {
    let registry = registry();
    let effect_id = xui::XUI_EFFECT
        .props
        .iter()
        .find(|def| def.name == "Id")
        .expect("XuiEffect declares Id");
    let mut effect = Object::new(registry.find("XuiEffect").expect("built in"));
    effect.properties = vec![
        Property::new(def(xui::XUI_ELEMENT.props, "Id"), text("Element")),
        Property::new(effect_id, text("Effect")),
    ];
    let mut later = gauge(&registry);
    later.properties[0] = Property::new(def(xui::XUI_ELEMENT.props, "Id"), text("Effect"));

    let width = def(xui::XUI_ELEMENT.props, "Width");
    let width_index = xui::XUI_ELEMENT
        .props
        .iter()
        .position(|candidate| core::ptr::eq(candidate, width))
        .unwrap();
    let mut root = Object::new(registry.find("XuiCanvas").expect("built in"));
    root.children = vec![effect, later];
    root.timelines = vec![Timeline {
        element: Some(WideString::from("Effect")),
        properties: vec![TimelineProperty {
            path: Some(PropertyPath {
                levels_up: LevelsUp(1),
                index: PropIndex(width_index as u8),
                defs: vec![width],
            }),
            element: None,
        }],
        keyframes: vec![],
    }];
    let tree = Tree { root };

    let bytes = write(&tree, &registry, WriteOptions::default()).expect("tree writes");
    let document = Document::parse(&bytes, &registry).expect("written scene parses");
    let timeline = document.root().timelines()[0];
    assert_eq!(
        timeline.target,
        document.root().children().next().map(|child| child.id())
    );
    assert_eq!(
        Tree::from_document(&document, &registry).expect("document converts"),
        tree
    );
}

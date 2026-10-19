use std::format;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::Document;
use exui_core::Registry;
use exui_core::schema::PropDef;
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
use exui_core::wide::WideString;
use exui_core::writer::WriteOptions;
use exui_core::writer::write;
use exui_core::xur::EaseIn;
use exui_core::xur::EaseOut;
use exui_core::xur::EaseScale;
use exui_core::xur::FrameCommand;
use exui_core::xur::FrameTime;
use exui_core::xur::Interpolation;
use exui_core::xur::LevelsUp;
use exui_core::xur::PropIndex;
use exui_extension::ElementPath;
use exui_extension::EntryTarget;
use exui_extension::PoseHold;
use exui_extension::ScenePatch;
use exui_extension::StringEntry;
use exui_extension::StringKey;
use exui_extension::StringOverlay;

use super::*;
use crate::resource::ModuleHandle;
use crate::resource::ResourceError;
use crate::resource::ResourceProvider;
use crate::resource::Resources;
use crate::resource::StringIndex;
use crate::test_support::MapProvider;
use crate::test_support::build_archive;

fn element_def(name: &str) -> &'static PropDef {
    xui::XUI_ELEMENT
        .props
        .iter()
        .find(|def| def.name == name)
        .expect("XuiElement declares it")
}

fn opacity_index() -> PropIndex {
    let position = xui::XUI_ELEMENT
        .props
        .iter()
        .position(|def| def.name == "Opacity")
        .expect("XuiElement declares Opacity");
    PropIndex(position as u8)
}

fn object(registry: &Registry, class: &str, id: Option<&str>) -> Object {
    let mut object = Object::new(registry.find(class).expect("class is registered"));
    if let Some(id) = id {
        object.properties.push(Property::new(
            element_def("Id"),
            Value::String(Some(WideString::from(id))),
        ));
    }
    object
}

fn frame(name: &str, time: u32) -> NamedFrame {
    NamedFrame {
        name: Some(WideString::from(name)),
        time: FrameTime(time),
        command: FrameCommand(0),
        target: None,
    }
}

/// Fades child `element` (an XuiScene, two levels below XuiElement).
fn fade(element: &str, keys: &[(u32, f32)]) -> Timeline {
    Timeline {
        element: Some(WideString::from(element)),
        properties: vec![TimelineProperty {
            path: Some(PropertyPath {
                levels_up: LevelsUp(2),
                index: opacity_index(),
                defs: vec![element_def("Opacity")],
            }),
            element: None,
        }],
        keyframes: keys
            .iter()
            .map(|&(time, opacity)| Keyframe {
                time: FrameTime(time),
                interpolation: Interpolation(0),
                ease_in: EaseIn(0),
                ease_out: EaseOut(0),
                ease_scale: EaseScale(0),
                values: vec![KeyframeValue::Float(opacity)],
            })
            .collect(),
    }
}

/// `Main` with children `A` and `B`, a named frame and a fade of `A`.
fn target_scene(registry: &Registry) -> Vec<u8> {
    let mut main = object(registry, "XuiScene", Some("Main"));
    main.properties
        .push(Property::new(element_def("Width"), Value::Float(100.0)));
    main.children = vec![
        object(registry, "XuiScene", Some("A")),
        object(registry, "XuiScene", Some("B")),
    ];
    main.named_frames = vec![frame("Start", 0), frame("Mid", 5)];
    main.timelines = vec![fade("A", &[(0, 0.0), (10, 1.0)])];

    let mut root = object(registry, "XuiCanvas", None);
    root.children.push(main);

    write(&Tree { root }, registry, WriteOptions::default()).expect("target writes")
}

/// Sets `Main` taller, inserts `C` after `A`, adds frames, keys `A` at 20.
fn overlay_scene(registry: &Registry) -> Vec<u8> {
    let mut main = object(registry, "XuiScene", Some("Main"));
    main.properties
        .push(Property::new(element_def("Height"), Value::Float(50.0)));
    main.children = vec![
        object(registry, "XuiScene", Some("A")),
        object(registry, "XuiScene", Some("C")),
    ];
    main.named_frames = vec![frame("Extra", 20), frame("Held", 25), frame("Early", 3)];
    main.timelines = vec![fade("A", &[(20, 0.5)])];

    let mut root = object(registry, "XuiCanvas", None);
    root.children.push(main);

    write(&Tree { root }, registry, WriteOptions::default()).expect("overlay writes")
}

/// Index-keyed string table with the given values.
fn string_table(values: &[&str]) -> Vec<u8> {
    let mut entries = Vec::new();
    for value in values {
        let units: Vec<u16> = value.encode_utf16().collect();
        entries.extend_from_slice(&(units.len() as u16).to_be_bytes());
        for unit in units {
            entries.extend_from_slice(&unit.to_be_bytes());
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"XUIS");
    out.push(1);
    out.push(2);
    out.extend_from_slice(&((12 + entries.len()) as u32).to_be_bytes());
    out.extend_from_slice(&(values.len() as u16).to_be_bytes());
    out.extend_from_slice(&entries);
    out
}

fn target(section: &str, path: &str) -> EntryTarget {
    EntryTarget {
        section: String::from(section),
        path: String::from(path),
    }
}

fn extension_archive(descriptor: &Descriptor, files: &[(&str, &[u8])]) -> Vec<u8> {
    let descriptor = descriptor.to_bytes().expect("descriptor encodes");
    let mut entries: Vec<(&str, &[u8])> = vec![(DESCRIPTOR_PATH, &descriptor)];
    entries.extend_from_slice(files);
    build_archive(&entries)
}

fn sample_descriptor() -> Descriptor {
    Descriptor {
        name: ExtensionName::new("sample").expect("valid name"),
        patches: vec![ScenePatch {
            target: target("scenes", "Main.xur"),
            remove: vec![ElementPath::parse("Main/B").expect("valid path")],
            overlay: Some(String::from("overlay.xur")),
            holds: vec![PoseHold {
                element: ElementPath::parse("Main").expect("valid path"),
                source: String::from("Mid"),
                frames: vec![String::from("Extra"), String::from("Held")],
            }],
        }],
        strings: vec![StringOverlay {
            target: target("scenes", "strings.xus"),
            locale: None,
            entries: vec![
                StringEntry {
                    key: StringKey::Index(0),
                    value: String::from("First, replaced"),
                },
                StringEntry {
                    key: StringKey::Index(2),
                    value: String::from("Third, added"),
                },
            ],
        }],
        blade: None,
    }
}

fn dump_tree(out: &mut String, registry: &Registry, root: &Object) {
    let mut pending: Vec<(usize, &Object)> = vec![(0, root)];
    while let Some((depth, object)) = pending.pop() {
        let indent = "  ".repeat(depth);
        let class = registry
            .class(object.class)
            .map_or("?", |class| class.name());
        out.push_str(&format!("{indent}{class}\n"));
        for property in &object.properties {
            out.push_str(&format!(
                "{indent}  {} = {:?}\n",
                property.def.name, property.values
            ));
        }
        for frame in &object.named_frames {
            out.push_str(&format!(
                "{indent}  frame {:?} at {}\n",
                frame.name, frame.time.0
            ));
        }
        for timeline in &object.timelines {
            let keys: Vec<(u32, u8, &KeyframeValue)> = timeline
                .keyframes
                .iter()
                .map(|keyframe| {
                    (
                        keyframe.time.0,
                        keyframe.interpolation.0,
                        &keyframe.values[0],
                    )
                })
                .collect();
            out.push_str(&format!(
                "{indent}  timeline {:?} {keys:?}\n",
                timeline.element
            ));
        }
        pending.extend(object.children.iter().rev().map(|child| (depth + 1, child)));
    }
}

#[test]
fn patches_apply_on_first_read_and_other_files_stay_as_they_are() {
    let registry = Registry::new();
    let scene = target_scene(&registry);
    let untouched = target_scene(&registry);
    let provider = MapProvider::default().with_section(
        "scenes",
        build_archive(&[
            ("main.xur", &scene),
            ("other.xur", &untouched),
            ("strings.xus", &string_table(&["First", "Second"])),
        ]),
    );
    let archive = extension_archive(
        &sample_descriptor(),
        &[("overlay.xur", &overlay_scene(&registry))],
    );
    let extensions = ExtensionSet::new(
        Registry::new(),
        AcceptedVersions::DASH_6717,
        [(ExtensionName::new("sample").unwrap(), &archive[..])],
    )
    .expect("extension loads");
    let mut resources = Resources::new(&provider);
    resources.set_extensions(Some(&extensions));
    let section = SectionName::new("scenes").unwrap();
    let units = |text: &str| text.encode_utf16().collect::<Vec<u16>>();

    let other = resources
        .open_in_section(ModuleHandle::TITLE, section, &units("other.xur"))
        .expect("other reads");
    // Unpatched files are the provider's bytes, not a copy.
    assert_eq!(other, &untouched[..]);
    let section_bytes = provider.section(ModuleHandle::TITLE, section).unwrap();
    assert!(section_bytes.as_ptr_range().contains(&other.as_ptr()));

    let patched = resources
        .open_in_section(ModuleHandle::TITLE, section, &units("MAIN.XUR"))
        .expect("patched reads");
    let again = resources
        .open_in_section(ModuleHandle::TITLE, section, &units("main.xur"))
        .expect("patched reads");
    assert!(core::ptr::eq(patched, again));

    let document = Document::parse(patched, &registry).expect("patched scene parses");
    let tree = Tree::from_document(&document, &registry).expect("patched scene converts");
    let mut dump = String::new();
    dump_tree(&mut dump, &registry, &tree.root);

    let locator = resources
        .resolve(&units("section://FFFFFFFF,scenes#strings.xus"), None)
        .unwrap();
    let table = resources.string_table(&locator).expect("table loads");
    for index in 0..table.len() {
        let value = String::from_utf16_lossy(table.get(StringIndex(index as u32)).unwrap());
        dump.push_str(&format!("string {index}: {value}\n"));
    }

    let overlay = resources
        .read_path(&units("ext://sample/overlay.xur"), None)
        .expect("extension file reads");
    dump.push_str(&format!(
        "ext://sample/overlay.xur: {} bytes\n",
        overlay.len()
    ));
    let missing = resources.read_path(&units("ext://other/overlay.xur"), None);
    dump.push_str(&format!(
        "ext://other: {}\n",
        missing
            .err()
            .map(|error| format!("{error}"))
            .unwrap_or_default()
    ));

    insta::assert_snapshot!(dump);
}

#[test]
fn patch_failures_are_reported_on_every_read() {
    let registry = Registry::new();
    let mut descriptor = sample_descriptor();
    descriptor.patches[0].remove = vec![ElementPath::parse("Main/Missing").unwrap()];
    let provider = MapProvider::default().with_section(
        "scenes",
        build_archive(&[("main.xur", &target_scene(&registry))]),
    );
    let archive = extension_archive(&descriptor, &[("overlay.xur", &overlay_scene(&registry))]);
    let extensions = ExtensionSet::new(
        Registry::new(),
        AcceptedVersions::DASH_6717,
        [(ExtensionName::new("sample").unwrap(), &archive[..])],
    )
    .unwrap();
    let mut resources = Resources::new(&provider);
    resources.set_extensions(Some(&extensions));
    let section = SectionName::new("scenes").unwrap();
    let path: Vec<u16> = "main.xur".encode_utf16().collect();

    for _ in 0..2 {
        let error = resources
            .open_in_section(ModuleHandle::TITLE, section, &path)
            .expect_err("patch fails");
        let ResourceError::Patch { source, .. } = error else {
            panic!("unexpected error {error:?}");
        };
        let missing = ElementPath::parse("Main/Missing").unwrap();
        assert!(matches!(
            source.error(),
            PatchError::Extension {
                patch: PatchIndex(0),
                source: ScenePatchError::ElementNotFound { path },
                ..
            } if *path == missing
        ));
    }
}

#[test]
fn overlay_classes_must_match() {
    let registry = Registry::new();
    let mut root = object(&registry, "XuiCanvas", None);
    root.children
        .push(object(&registry, "XuiGroup", Some("Main")));
    let overlay = write(&Tree { root }, &registry, WriteOptions::default()).unwrap();

    let mut descriptor = sample_descriptor();
    descriptor.patches[0].remove.clear();
    let provider = MapProvider::default().with_section(
        "scenes",
        build_archive(&[("main.xur", &target_scene(&registry))]),
    );
    let archive = extension_archive(&descriptor, &[("overlay.xur", &overlay)]);
    let extensions = ExtensionSet::new(
        Registry::new(),
        AcceptedVersions::DASH_6717,
        [(ExtensionName::new("sample").unwrap(), &archive[..])],
    )
    .unwrap();
    let mut resources = Resources::new(&provider);
    resources.set_extensions(Some(&extensions));
    let path: Vec<u16> = "main.xur".encode_utf16().collect();

    let error = resources
        .open_in_section(
            ModuleHandle::TITLE,
            SectionName::new("scenes").unwrap(),
            &path,
        )
        .expect_err("classes differ");
    insta::assert_snapshot!(format!("{:?}", core::error::Error::source(&error)));
}

#[test]
fn archives_must_describe_themselves() {
    let archive = extension_archive(&sample_descriptor(), &[]);
    let errors = [
        ExtensionSet::new(
            Registry::new(),
            AcceptedVersions::DASH_6717,
            [(ExtensionName::new("other").unwrap(), &archive[..])],
        ),
        ExtensionSet::new(
            Registry::new(),
            AcceptedVersions::DASH_6717,
            [
                (ExtensionName::new("sample").unwrap(), &archive[..]),
                (ExtensionName::new("sample").unwrap(), &archive[..]),
            ],
        ),
        ExtensionSet::new(
            Registry::new(),
            AcceptedVersions::DASH_6717,
            [(ExtensionName::new("sample").unwrap(), &b"XUIZ"[..])],
        ),
    ]
    .map(|result| format!("{}", result.expect_err("rejected")));

    insta::assert_snapshot!(errors.join("\n"));
}

#[test]
fn locale_overlays_follow_those_without_a_locale() {
    let provider = MapProvider::default().with_section(
        "scenes",
        build_archive(&[("strings.xus", &string_table(&["First", "Second"]))]),
    );
    let mut descriptor = sample_descriptor();
    descriptor.patches.clear();
    let entry = |text: &str| StringEntry {
        key: StringKey::Index(0),
        value: String::from(text),
    };
    descriptor.strings = vec![
        StringOverlay {
            target: target("scenes", "strings.xus"),
            locale: Some(String::from("de-DE")),
            entries: vec![entry("Erste")],
        },
        StringOverlay {
            target: target("scenes", "STRINGS.XUS"),
            locale: None,
            entries: vec![entry("First, replaced")],
        },
    ];
    let archive = extension_archive(&descriptor, &[]);
    let extensions = ExtensionSet::new(
        Registry::new(),
        AcceptedVersions::DASH_6717,
        [(ExtensionName::new("sample").unwrap(), &archive[..])],
    )
    .unwrap();
    let mut resources = Resources::new(&provider);
    resources.set_extensions(Some(&extensions));
    let units: Vec<u16> = "section://FFFFFFFF,scenes#strings.xus"
        .encode_utf16()
        .collect();
    let locator = resources.resolve(&units, None).unwrap();
    let first = |resources: &Resources<'_>| {
        let table = resources.string_table(&locator).expect("table loads");
        String::from_utf16_lossy(table.get(StringIndex(0)).unwrap())
    };

    assert_eq!(first(&resources), "First, replaced");
    resources.set_locale(Some("de-de"));
    assert_eq!(first(&resources), "Erste");
}

#[test]
fn holds_leave_timelines_without_a_keyframe_at_the_source() {
    let registry = Registry::new();
    let mut main = object(&registry, "XuiScene", Some("Main"));
    main.named_frames = vec![frame("Mid", 5), frame("Extra", 20)];
    main.timelines = vec![fade("A", &[(10, 1.0)])];
    let before = main.timelines.clone();
    let hold = PoseHold {
        element: ElementPath::default(),
        source: String::from("Mid"),
        frames: vec![String::from("Extra")],
    };

    overlay::hold(&mut main, &hold).expect("hold applies");
    assert_eq!(main.timelines, before);
}

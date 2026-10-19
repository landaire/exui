use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;

use exui::archive::Archive;
use exui::compile::CompiledScene;
use exui::compile::compile_scene;
use exui::error::SceneError;
use exui::registry;
use exui::scene::ElementId;
use exui::scene::Property;
use exui::scene::Scalar;
use exui::scene::Scene;
use exui::scene::Value;
use exui::xui::Xml;
use exui::xui::to_xui;
use exui_core::Document;
use exui_core::XurError;
use exui_core::tree::Tree;

fn parse_file(path: &str) -> Scene {
    let data = fs::read(path).expect("failed to read file");
    let registry = registry::dash_6717().expect("registry builds");

    Scene::parse(&data, &registry).expect("failed to parse")
}

fn decompile_file(path: &str) -> String {
    let data = fs::read(path).expect("failed to read file");
    let registry = registry::dash_6717().expect("registry builds");
    let document = Document::parse(&data, &registry).expect("failed to parse");

    to_xui(&document, &registry).expect("failed to decompile")
}

fn read_reference(path: &str) -> String {
    fs::read_to_string(path).expect("failed to read reference")
}

fn read_hud() -> Vec<u8> {
    fs::read("test-data/hud").expect("read")
}

/// Element classes and property values, one line each, indented by depth.
fn debug_tree(scene: &Scene) -> String {
    let mut out = String::new();
    let mut stack = vec![(ElementId(0), 0)];
    while let Some((id, depth)) = stack.pop() {
        let element = scene.element(id).expect("element exists");
        let indent = "  ".repeat(depth);
        writeln!(out, "{indent}{}", element.class.name()).unwrap();

        for property in &element.properties {
            for value in &property.values {
                writeln!(
                    out,
                    "{indent}  [{}] {}",
                    property.index.0,
                    format_value(value)
                )
                .unwrap();
            }
        }

        stack.extend(
            element
                .children
                .iter()
                .rev()
                .map(|child| (*child, depth + 1)),
        );
    }

    out
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Scalar(scalar) => format_scalar(scalar),
        Value::Points(path) => format!("points={}", path.points.len()),
        Value::Compound(properties) => {
            let parts: Vec<String> = properties
                .iter()
                .flat_map(|property: &Property| property.values.iter().map(format_value))
                .collect();
            format!("compound={{{}}}", parts.join(", "))
        }
    }
}

fn format_scalar(scalar: &Scalar) -> String {
    match scalar {
        Scalar::Bool(value) => format!("bool={value}"),
        Scalar::Integer(value) => format!("int={value}"),
        Scalar::Unsigned(value) => format!("uint={value}"),
        Scalar::Float(value) => format!("float={value}"),
        Scalar::String(Some(text)) => format!("str={text:?}"),
        Scalar::String(None) => "str=null".to_string(),
        Scalar::Color(color) => format!("color=0x{:08x}", color.argb()),
        Scalar::Vector(vector) => format!("vec=({},{},{})", vector.x, vector.y, vector.z),
        Scalar::Quaternion(quaternion) => format!(
            "quat=({},{},{},{})",
            quaternion.x, quaternion.y, quaternion.z, quaternion.w
        ),
        Scalar::Raw(value) => format!("raw={value}"),
    }
}

#[test]
fn snapshot_simple() {
    insta::assert_snapshot!(debug_tree(&parse_file("test-data/simple.xur")));
}

#[test]
fn snapshot_controls_demo1() {
    insta::assert_snapshot!(debug_tree(&parse_file(
        "test-data/samples/controls_demo1.xur"
    )));
}

#[test]
fn snapshot_pause_menu() {
    insta::assert_snapshot!(debug_tree(&parse_file("test-data/samples/PauseMenu.xur")));
}

#[test]
fn snapshot_tabbed_scene() {
    insta::assert_snapshot!(debug_tree(&parse_file(
        "test-data/samples/tabbed_scene.xur"
    )));
}

#[test]
fn snapshot_xuieffect_main() {
    insta::assert_snapshot!(debug_tree(&parse_file(
        "test-data/samples/xuieffect_main.xur"
    )));
}

#[test]
fn roundtrip_simple() {
    let decompiled = decompile_file("test-data/simple.xur");
    let reference = read_reference("test-data/simple.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

// The XUI tool's controls_demo1 and xuieffect_main files hold strings with
// CRLF line ends and trailing spaces as plain text; the decompiler writes
// CR as `&#xD;` and keeps trailing spaces so the text compiles back to the
// same strings. These compare against .expected.xui files holding that form.
#[test]
fn roundtrip_controls_demo1() {
    let decompiled = decompile_file("test-data/samples/controls_demo1.xur");
    let reference = read_reference("test-data/samples/controls_demo1.expected.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

#[test]
fn roundtrip_xuieffect_main() {
    let decompiled = decompile_file("test-data/samples/xuieffect_main.xur");
    let reference = read_reference("test-data/samples/xuieffect_main.expected.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

// The PauseMenu and tabbed_scene reference .xui files hold design-time
// elements and lack compiler-generated template children, so these compare
// against .expected.xui files holding the decompilation of the binary.
#[test]
fn roundtrip_pause_menu() {
    let decompiled = decompile_file("test-data/samples/PauseMenu.xur");
    let reference = read_reference("test-data/samples/PauseMenu.expected.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

#[test]
fn roundtrip_tabbed_scene() {
    let decompiled = decompile_file("test-data/samples/tabbed_scene.xur");
    let reference = read_reference("test-data/samples/tabbed_scene.expected.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

/// Decompiled sample scenes compile back to the same tree, and those
/// written in the 6717 layout to their bytes. The sceneviewer and
/// tabbed_scene samples come from XUI tools whose layout the exui-core
/// writer does not follow.
#[test]
fn samples_recompile() {
    let registry = registry::dash_6717().expect("registry builds");
    for (path, same_layout) in [
        ("test-data/simple.xur", true),
        ("test-data/samples/controls_demo1.xur", true),
        ("test-data/samples/PauseMenu.xur", true),
        ("test-data/samples/sceneviewer_skin_default.xur", false),
        ("test-data/samples/tabbed_scene.xur", false),
        ("test-data/samples/xuieffect_main.xur", true),
    ] {
        let data = fs::read(path).expect("read");
        let document = Document::parse(&data, &registry).expect("parses");
        let expected = Tree::from_document(&document, &registry).expect("converts");

        let compiled = CompiledScene::parse(&decompile_file(path), &registry)
            .expect("decompiled text compiles");
        assert!(compiled.tree == expected, "{path} compiles to another tree");

        let bytes = compiled.to_xur(&registry).expect("writes");
        assert_eq!(bytes == data, same_layout, "{path}");
    }
}

/// The XUI tool's own text of the simple scene compiles to its XUR.
#[test]
fn xui_tool_text_compiles_byte_for_byte() {
    let registry = registry::dash_6717().expect("registry builds");
    let text = read_reference("test-data/simple.xui");
    let compiled = compile_scene(&text, &registry).expect("compiles");

    assert!(compiled == fs::read("test-data/simple.xur").expect("read"));
}

#[test]
fn xuiz_archive_dashskn1() {
    let data = fs::read("test-data/blades/dashskn1").expect("read");
    let archive = Archive::parse(&data).expect("parse XUIZ");
    let entries = archive.entries().expect("entries");

    assert_eq!(archive.version(), 1);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "BladeSkin.xur");
    assert!(entries[0].data.starts_with(b"XUIB"));
}

#[test]
fn xuiz_archive_dashmain() {
    let data = fs::read("test-data/blades/dashmain").expect("read");
    let archive = Archive::parse(&data).expect("parse XUIZ");
    let entries = archive.entries().expect("entries");

    assert_eq!(archive.version(), 1);
    assert!(entries.len() > 1);
    assert!(entries[0].name.ends_with(".xur"));
    assert!(entries[0].data.starts_with(b"XUIB"));
    assert!(entries.iter().any(|entry| entry.name.ends_with(".png")));
}

/// The test archives come from an older dashboard than 6717. Its scenes
/// decompile except those using classes 6717 no longer registers, which the
/// original cannot load either.
#[test]
fn blades_archives_decompile() {
    let registry = registry::dash_6717().expect("registry builds");
    let mut paths: Vec<_> = fs::read_dir("test-data/blades")
        .expect("read dir")
        .map(|entry| entry.expect("entry").path())
        .collect();
    paths.sort();

    let mut decompiled = 0;
    let mut unknown_classes = BTreeSet::new();
    for path in paths {
        let data = fs::read(&path).expect("read");
        let entries = Archive::parse(&data)
            .expect("parse XUIZ")
            .entries()
            .expect("entries");
        for entry in entries.iter().filter(|entry| entry.name.ends_with(".xur")) {
            match Scene::parse(entry.data, &registry) {
                Ok(scene) => {
                    Xml(&scene).to_string();
                    decompiled += 1;
                }
                Err(SceneError::Version5(XurError::UnknownClass { name, .. })) => {
                    unknown_classes.insert(name.to_string());
                }
                Err(err) => panic!("{}/{}: {err}", path.display(), entry.name),
            }
        }
    }

    assert_eq!(decompiled, 194);
    assert_eq!(
        unknown_classes.into_iter().collect::<Vec<_>>(),
        ["XuiBOTDContainer", "XuiBOTDScene"]
    );
}

#[test]
fn xuiz_archive_hud_is_v3() {
    let data = read_hud();
    let archive = Archive::parse(&data).expect("parse XUIZ");
    let entries = archive.entries().expect("entries");

    assert_eq!(archive.version(), 3);
    // Version 3 names are UTF-8; a UTF-16 decode yields CJK text.
    assert!(entries.iter().any(|entry| entry.name == "Strings.xus"));
    assert!(
        entries
            .iter()
            .any(|entry| entry.name == "ConsoleContract.xur")
    );
}

/// Every XUR version 8 resource in the dashboard archive decompiles.
#[test]
fn v8_all_hud_xurs_decompile() {
    let data = read_hud();
    let entries = Archive::parse(&data)
        .expect("parse XUIZ")
        .entries()
        .expect("entries");
    let registry = registry::dash_6717().expect("registry builds");

    let mut total = 0;
    let mut keyframe_files = 0;
    for entry in entries.iter().filter(|entry| entry.name.ends_with(".xur")) {
        total += 1;
        assert_eq!(&entry.data[0..4], b"XUIB", "{}: not XUIB", entry.name);
        assert_eq!(
            &entry.data[4..8],
            &8u32.to_be_bytes(),
            "{}: not version 8",
            entry.name
        );

        let scene = Scene::parse(entry.data, &registry)
            .unwrap_or_else(|err| panic!("{}: parse failed: {err}", entry.name));
        let xml = Xml(&scene).to_string();
        assert!(xml.starts_with("<Xui"), "{}: unexpected XML", entry.name);
        if xml.contains("<KeyFrame>") {
            keyframe_files += 1;
        }
    }

    assert_eq!(total, 34, "expected 34 XUR resources");
    assert!(
        keyframe_files >= 5,
        "expected v8 files with timeline keyframes"
    );
}

#[test]
fn snapshot_v8_console_contract() {
    let data = read_hud();
    let entries = Archive::parse(&data)
        .expect("parse XUIZ")
        .entries()
        .expect("entries");
    let entry = entries
        .iter()
        .find(|entry| entry.name == "ConsoleContract.xur")
        .expect("ConsoleContract.xur");
    let registry = registry::dash_6717().expect("registry builds");
    let scene = Scene::parse(entry.data, &registry).expect("parse");

    insta::assert_snapshot!(debug_tree(&scene));
}

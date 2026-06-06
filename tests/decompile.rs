use exui::xur::Xur;
use exui::xur::archive::ReadAt;
use exui::xur::archive::XuizArchive;

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

fn parse_file(path: &str) -> Xur<'static> {
    init_tracing();
    let data = std::fs::read(path).expect("failed to read file");
    let data = Box::leak(data.into_boxed_slice());
    Xur::parse(data).expect("failed to parse")
}

fn decompile_file(path: &str) -> String {
    let xur = parse_file(path);
    exui::xui::to_xui(&xur).expect("failed to decompile")
}

fn read_reference(path: &str) -> String {
    std::fs::read_to_string(path).expect("failed to read reference")
}

// ---------------------------------------------------------------------------
// Snapshot helper
// ---------------------------------------------------------------------------

fn debug_tree(xur: &Xur<'_>) -> String {
    let mut out = String::new();
    debug_object(xur, &xur.root, &mut out, 0);
    out
}

fn debug_object(xur: &Xur<'_>, obj: &exui::xur::Object<'_>, out: &mut String, depth: usize) {
    use std::fmt::Write;
    let indent = "  ".repeat(depth);
    let class = xur.get_string(obj.class_name).unwrap_or("??");
    writeln!(out, "{indent}{class}").unwrap();

    for group in &obj.properties {
        for (i, val) in group.values.iter().enumerate() {
            let bit = (0..32)
                .filter(|b| group.bitmask & (1 << b) != 0)
                .nth(i)
                .unwrap_or(99);
            writeln!(out, "{indent}  [{bit}] {}", format_value(xur, val)).unwrap();
        }
    }

    for child in &obj.children {
        debug_object(xur, child, out, depth + 1);
    }
}

fn format_value(xur: &Xur<'_>, val: &exui::xur::PropertyValue) -> String {
    match val {
        exui::xur::PropertyValue::Bool(v) => format!("bool={v}"),
        exui::xur::PropertyValue::Integer(v) => format!("int={v}"),
        exui::xur::PropertyValue::Unsigned(v) => format!("uint={v}"),
        exui::xur::PropertyValue::Float(v) => format!("float={v}"),
        exui::xur::PropertyValue::String(idx) => {
            let s = xur.get_string(*idx).unwrap_or("??");
            format!("str={s:?}")
        }
        exui::xur::PropertyValue::Color(v) => format!("color=0x{v:08x}"),
        exui::xur::PropertyValue::Vector(idx) => {
            if let Some((x, y, z)) = xur.get_vector(*idx) {
                format!("vec=({x},{y},{z})")
            } else {
                format!("vec=VECT[{idx}]")
            }
        }
        exui::xur::PropertyValue::Quaternion(idx) => {
            if let Some((x, y, z, w)) = xur.get_quaternion(*idx) {
                format!("quat=({x},{y},{z},{w})")
            } else {
                format!("quat=QUAT[{idx}]")
            }
        }
        exui::xur::PropertyValue::Object(groups) => {
            let mut parts = Vec::new();
            for g in groups {
                for v in &g.values {
                    parts.push(format_value(xur, v));
                }
            }
            format!("compound={{{}}}", parts.join(", "))
        }
        exui::xur::PropertyValue::Custom(offset) => format!("custom=CUST[{offset}]"),
        exui::xur::PropertyValue::Unknown(bytes) => format!("raw={}B", bytes.len()),
    }
}

// ===========================================================================
// Snapshot tests: verify parsing produces correct tree structure
// ===========================================================================

#[test]
fn snapshot_simple() {
    let xur = parse_file("test-data/simple.xur");
    insta::assert_snapshot!(debug_tree(&xur));
}

#[test]
fn snapshot_controls_demo1() {
    let xur = parse_file("test-data/samples/controls_demo1.xur");
    insta::assert_snapshot!(debug_tree(&xur));
}

#[test]
fn snapshot_pause_menu() {
    let xur = parse_file("test-data/samples/PauseMenu.xur");
    insta::assert_snapshot!(debug_tree(&xur));
}

#[test]
fn snapshot_tabbed_scene() {
    let xur = parse_file("test-data/samples/tabbed_scene.xur");
    insta::assert_snapshot!(debug_tree(&xur));
}

#[test]
fn snapshot_xuieffect_main() {
    let xur = parse_file("test-data/samples/xuieffect_main.xur");
    insta::assert_snapshot!(debug_tree(&xur));
}

// ===========================================================================
// Round-trip tests: decompile XUR and compare against reference XUI
// ===========================================================================

#[test]
fn roundtrip_simple() {
    let decompiled = decompile_file("test-data/simple.xur");
    let reference = read_reference("test-data/simple.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

#[test]
fn roundtrip_controls_demo1() {
    let decompiled = decompile_file("test-data/samples/controls_demo1.xur");
    let reference = read_reference("test-data/samples/controls_demo1.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

#[test]
fn roundtrip_xuieffect_main() {
    let decompiled = decompile_file("test-data/samples/xuieffect_main.xur");
    let reference = read_reference("test-data/samples/xuieffect_main.xui");
    pretty_assertions::assert_eq!(decompiled, reference);
}

// These files contain timeline data that we skip during parsing.
// The reference .xui files include <Timelines> XML that we don't yet emit.
// PauseMenu and tabbed_scene reference .xui files contain design-time elements
// and omit compiler-generated template children, so they can't be used for
// exact roundtrip comparison. We use .expected.xui files instead, which
// represent the correct decompilation of the binary.
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

// ===========================================================================
// XUIZ archive tests
// ===========================================================================

#[test]
fn xuiz_archive_dashskn1() {
    let data: &'static [u8] = Box::leak(
        std::fs::read("test-data/blades/dashskn1")
            .expect("read")
            .into_boxed_slice(),
    );
    let archive = XuizArchive::parse(data).expect("parse XUIZ");
    assert_eq!(archive.version, 1);
    assert_eq!(archive.entries.len(), 1);
    assert_eq!(archive.entries[0].name, "BladeSkin.xur");
    let entry = archive.find_xuib(data).expect("no XUIB in archive");
    let xuib_data = data.read_at(entry.range.clone()).expect("read entry");
    assert_eq!(&xuib_data[0..4], b"XUIB");
}

#[test]
fn xuiz_archive_dashmain() {
    let data: &'static [u8] = Box::leak(
        std::fs::read("test-data/blades/dashmain")
            .expect("read")
            .into_boxed_slice(),
    );
    let archive = XuizArchive::parse(data).expect("parse XUIZ");
    assert_eq!(archive.version, 1);
    assert!(archive.entries.len() > 1);
    assert!(archive.entries[0].name.ends_with(".xur"));
    assert!(archive.entries.iter().any(|e| e.name.ends_with(".png")));
    let xuib_entry = archive.find_xuib(data).expect("no XUIB");
    let xuib_data = data.read_at(xuib_entry.range.clone()).expect("read");
    assert_eq!(&xuib_data[0..4], b"XUIB");
}

// ===========================================================================
// XUIB v8 (the `hud` dashboard archive: XUIZ v3 of XUIB v8 resources)
// ===========================================================================

#[test]
fn xuiz_archive_hud_is_v3() {
    let data: &'static [u8] = Box::leak(
        std::fs::read("test-data/hud")
            .expect("read")
            .into_boxed_slice(),
    );
    let archive = XuizArchive::parse(data).expect("parse XUIZ");
    assert_eq!(archive.version, 3);
    // v3 names are single-byte ASCII; a botched decode yields CJK mojibake.
    assert!(archive.entries.iter().any(|e| e.name == "Strings.xus"));
    assert!(
        archive
            .entries
            .iter()
            .any(|e| e.name == "ConsoleContract.xur")
    );
}

/// Every XUIB v8 resource in the dashboard archive must decompile to XUI XML.
#[test]
fn v8_all_hud_xurs_decompile() {
    init_tracing();
    let data: &'static [u8] = Box::leak(
        std::fs::read("test-data/hud")
            .expect("read")
            .into_boxed_slice(),
    );
    let archive = XuizArchive::parse(data).expect("parse XUIZ");

    let mut total = 0;
    for entry in &archive.entries {
        if !entry.name.ends_with(".xur") {
            continue;
        }
        total += 1;
        let bytes = data.read_at(entry.range.clone()).expect("read entry");
        assert_eq!(&bytes[0..4], b"XUIB", "{}: not XUIB", entry.name);
        assert_eq!(
            u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            8,
            "{}: expected XUIB v8",
            entry.name
        );
        let xur = Xur::parse(bytes).unwrap_or_else(|e| panic!("{}: parse failed: {e}", entry.name));
        let xml = exui::xui::to_xui(&xur)
            .unwrap_or_else(|e| panic!("{}: to_xui failed: {e}", entry.name));
        assert!(xml.starts_with("<Xui"), "{}: unexpected XML", entry.name);
    }
    assert_eq!(total, 34, "expected 34 XUR resources");
}

/// Every v8 timeline keyframe value must resolve to a real pool entry.
/// A regression in the KEYD/KEYP keyframe decoding shows up as out-of-range
/// pool indices, which the writer renders as `VECT[..]`/`FLOT[..]` placeholders.
#[test]
fn v8_timeline_keyframe_values_resolve() {
    init_tracing();
    let data: &'static [u8] = Box::leak(
        std::fs::read("test-data/hud")
            .expect("read")
            .into_boxed_slice(),
    );
    let archive = XuizArchive::parse(data).expect("parse XUIZ");

    let mut keyframe_files = 0;
    for entry in &archive.entries {
        if !entry.name.ends_with(".xur") {
            continue;
        }
        let bytes = data.read_at(entry.range.clone()).expect("read entry");
        let xur = Xur::parse(bytes).unwrap_or_else(|e| panic!("{}: parse failed: {e}", entry.name));
        let xml = exui::xui::to_xui(&xur)
            .unwrap_or_else(|e| panic!("{}: to_xui failed: {e}", entry.name));
        for placeholder in ["VECT[", "FLOT[", "QUAT[", "COLR["] {
            assert!(
                !xml.contains(placeholder),
                "{}: unresolved {placeholder} pool reference in output",
                entry.name
            );
        }
        if xml.contains("<KeyFrame>") {
            keyframe_files += 1;
        }
    }
    assert!(
        keyframe_files >= 5,
        "expected v8 files with timeline keyframes"
    );
}

#[test]
fn snapshot_v8_console_contract() {
    let data: &'static [u8] = Box::leak(
        std::fs::read("test-data/hud")
            .expect("read")
            .into_boxed_slice(),
    );
    let archive = XuizArchive::parse(data).expect("parse XUIZ");
    let entry = archive
        .entries
        .iter()
        .find(|e| e.name == "ConsoleContract.xur")
        .expect("ConsoleContract.xur");
    let bytes = data.read_at(entry.range.clone()).expect("read");
    let xur = Xur::parse(bytes).expect("parse");
    insta::assert_snapshot!(debug_tree(&xur));
}

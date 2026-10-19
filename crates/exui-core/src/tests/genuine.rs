//! Parses the genuine 6717 assets. Set `EXUI_GENUINE_DIR` to the
//! directory holding the XUIZ archives (`.../2.0.6717.0/retail/assets`). No
//! content from them is recorded.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::println;
use std::string::String;
use std::string::ToString;
use std::vec::Vec;

use crate::archive::AcceptedVersions;
use crate::archive::Archive;
use crate::custom::FigurePoints;
use crate::error::XurError;
use crate::schema::PropType;
use crate::schema::Registry;
use crate::schema::dash;
use crate::schema::xui;
use crate::xur::Document;
use crate::xur::Value;

fn dash_registry() -> Registry {
    let mut registry = Registry::new();
    xui::register_sound_xaudio(&mut registry).expect("XAudio group registers");
    xui::register_html(&mut registry).expect("HTML group registers");
    dash::register(&mut registry).expect("dash classes register");

    registry
}

/// Decodes every custom blob, which in 6717 are all `XuiFigure.Points`.
fn count_figure_paths(doc: &Document<'_>, failures: &mut Vec<String>, scene: &str) -> usize {
    let mut decoded = 0;
    for object in doc.objects() {
        for property in object.properties() {
            if !matches!(property.def().ty, PropType::Custom) {
                continue;
            }

            let name = property.def().name;
            let Some(Value::Custom(offset)) = doc.value(property) else {
                failures.push(std::format!("{scene}: {name} is not a single custom value"));
                continue;
            };

            match doc.custom_data(*offset).and_then(FigurePoints::parse) {
                Some(_) => decoded += 1,
                None => failures.push(std::format!(
                    "{scene}: {name} blob {offset:?} does not decode"
                )),
            }
        }
    }

    decoded
}

#[test]
#[ignore = "needs the genuine 6717 assets in EXUI_GENUINE_DIR"]
fn parse_all_genuine_scenes() {
    let dir = PathBuf::from(
        env::var_os("EXUI_GENUINE_DIR")
            .expect("EXUI_GENUINE_DIR names the genuine asset directory"),
    );

    let registry = dash_registry();
    let mut parsed = 0usize;
    let mut figure_paths = 0usize;
    let mut unknown_class: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("asset directory is readable")
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    paths.sort();

    for path in paths {
        // Test-only: archives are under 2 MiB, so reading them whole is fine.
        if !path.is_file() {
            continue;
        }
        let data = fs::read(&path).expect("asset is readable");
        if !data.starts_with(b"XUIZ") {
            continue;
        }

        let archive_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string());
        let archive = Archive::parse(&data, AcceptedVersions::DASH_6717).expect("archive parses");
        for entry in archive.entries() {
            let name = entry.name().to_string();
            if !name.to_ascii_lowercase().ends_with(".xur") {
                continue;
            }

            let scene = archive.entry_data(&entry).expect("entry data is in bounds");
            match Document::parse(scene, &registry) {
                Ok(doc) => {
                    parsed += 1;
                    figure_paths += count_figure_paths(&doc, &mut failures, &name);
                }
                Err(XurError::UnknownClass { name: class, .. }) => {
                    unknown_class.push(std::format!("{archive_name:?}/{name}: {class}"));
                }
                Err(err) => failures.push(std::format!("{archive_name:?}/{name}: {err}")),
            }
        }
    }

    println!("parsed {parsed} scenes, decoded {figure_paths} figure paths");
    for failure in &unknown_class {
        println!("unknown class, unloadable by the original: {failure}");
    }

    assert!(failures.is_empty(), "scene failures: {failures:#?}");
    assert!(parsed > 0, "no scenes found in {}", dir.display());
}

/// Tag, offset and size of each section of a XUR file.
fn sections(data: &[u8]) -> Vec<(String, usize, usize)> {
    let flags = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let section_count = u16::from_be_bytes([data[0x12], data[0x13]]) as usize;
    let table = 0x14 + if flags & 1 != 0 { 40 } else { 0 };
    (0..section_count)
        .map(|index| {
            let entry = &data[table + index * 12..][..12];
            let start = u32::from_be_bytes([entry[4], entry[5], entry[6], entry[7]]) as usize;
            let size = u32::from_be_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            (
                String::from_utf8_lossy(&entry[..4]).to_string(),
                start,
                size,
            )
        })
        .collect()
}

/// Sections whose bytes differ, with the first differing offset.
fn section_differences(original: &[u8], written: &[u8]) -> String {
    let left = sections(original);
    let right = sections(written);
    let mut out = String::new();
    for (tag, start, size) in &left {
        let Some((_, other_start, other_size)) = right.iter().find(|(other, ..)| other == tag)
        else {
            out.push_str(&std::format!("{tag}:missing "));
            continue;
        };
        let a = &original[*start..][..*size];
        let b = &written[*other_start..][..*other_size];
        if a != b {
            let first = a
                .iter()
                .zip(b)
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            out.push_str(&std::format!("{tag}@{first:#x}({size}/{other_size}) "));
        }
    }
    for (tag, ..) in &right {
        if !left.iter().any(|(other, ..)| other == tag) {
            out.push_str(&std::format!("{tag}:extra "));
        }
    }
    out
}

#[test]
#[ignore = "needs the genuine 6717 assets in EXUI_GENUINE_DIR"]
fn write_all_genuine_scenes() {
    use crate::tree::Tree;
    use crate::writer::write;

    let dir = PathBuf::from(
        env::var_os("EXUI_GENUINE_DIR")
            .expect("EXUI_GENUINE_DIR names the genuine asset directory"),
    );
    let registry = dash_registry();
    let mut identical = 0usize;
    let mut differing: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("asset directory is readable")
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    paths.sort();

    for path in paths {
        if !path.is_file() {
            continue;
        }
        let data = fs::read(&path).expect("asset is readable");
        if !data.starts_with(b"XUIZ") {
            continue;
        }
        let archive = Archive::parse(&data, AcceptedVersions::DASH_6717).expect("archive parses");
        for entry in archive.entries() {
            let name = std::format!(
                "{}/{}",
                path.file_name().unwrap().to_string_lossy(),
                entry.name()
            );
            if !name.to_ascii_lowercase().ends_with(".xur") {
                continue;
            }
            let scene = archive.entry_data(&entry).expect("entry data is in bounds");
            let Ok(doc) = Document::parse(scene, &registry) else {
                continue;
            };
            let tree = match Tree::from_document(&doc, &registry) {
                Ok(tree) => tree,
                Err(err) => {
                    failures.push(std::format!("{name}: tree: {err}"));
                    continue;
                }
            };
            let options = crate::writer::WriteOptions {
                allocation_hints: doc.allocation_hints().is_some(),
            };
            let written = match write(&tree, &registry, options) {
                Ok(bytes) => bytes,
                Err(err) => {
                    failures.push(std::format!("{name}: write: {err}"));
                    continue;
                }
            };
            match Document::parse(&written, &registry)
                .map_err(|err| err.to_string())
                .and_then(|reparsed| {
                    Tree::from_document(&reparsed, &registry).map_err(|err| err.to_string())
                }) {
                Ok(again) if again == tree => {}
                Ok(again) => failures.push(std::format!(
                    "{name}: reparsed tree differs: {}",
                    first_difference(&tree.root, &again.root)
                )),
                Err(err) => failures.push(std::format!("{name}: reparse: {err}")),
            }

            if written == scene {
                identical += 1;
            } else {
                differing.push(std::format!(
                    "{name}: {} hints {:?}",
                    section_differences(scene, &written),
                    doc.allocation_hints(),
                ));
            }
        }
    }

    println!("{identical} scenes written byte for byte");
    assert!(failures.is_empty(), "failures: {failures:#?}");
    assert!(differing.is_empty(), "scenes differ: {differing:#?}");
}

fn first_difference(left: &crate::tree::Object, right: &crate::tree::Object) -> String {
    if left.class != right.class {
        return "class".to_string();
    }
    if left.properties != right.properties {
        for (a, b) in left.properties.iter().zip(&right.properties) {
            if a != b {
                return std::format!("property {:?} vs {:?}", a, b);
            }
        }
        return std::format!(
            "property count {} vs {}",
            left.properties.len(),
            right.properties.len()
        );
    }
    if left.named_frames != right.named_frames {
        return "named frames".to_string();
    }
    if left.timelines != right.timelines {
        for (a, b) in left.timelines.iter().zip(&right.timelines) {
            if a != b {
                return std::format!("timeline {:?} vs {:?}", a, b);
            }
        }
        return "timeline count".to_string();
    }
    if left.children.len() != right.children.len() {
        return "children count".to_string();
    }
    for (a, b) in left.children.iter().zip(&right.children) {
        if a != b {
            return first_difference(a, b);
        }
    }
    "none".to_string()
}

#[test]
#[ignore = "needs the genuine 6717 assets in EXUI_GENUINE_DIR"]
fn write_all_genuine_archives() {
    use crate::archive_writer::NewEntry;
    use crate::archive_writer::write_archive;

    let dir = PathBuf::from(
        env::var_os("EXUI_GENUINE_DIR")
            .expect("EXUI_GENUINE_DIR names the genuine asset directory"),
    );
    let mut checked = 0usize;
    let mut differing: Vec<String> = Vec::new();
    for entry in fs::read_dir(&dir).expect("asset directory is readable") {
        let path = entry.expect("directory entry").path();
        if !path.is_file() {
            continue;
        }
        let data = fs::read(&path).expect("asset is readable");
        let Ok(archive) = Archive::parse(&data, AcceptedVersions::DASH_6717) else {
            continue;
        };
        let names: Vec<String> = archive
            .entries()
            .map(|entry| entry.name().to_string())
            .collect();
        let entries: Vec<NewEntry<'_>> = archive
            .entries()
            .zip(&names)
            .map(|(entry, name)| NewEntry {
                name,
                data: archive.entry_data(&entry).expect("entry data is in bounds"),
            })
            .collect();
        let written = write_archive(archive.version(), &entries).expect("archive writes");
        checked += 1;
        if written != data {
            differing.push(path.display().to_string());
        }
    }

    println!("{checked} archives checked");
    assert!(differing.is_empty(), "archives differ: {differing:#?}");
}

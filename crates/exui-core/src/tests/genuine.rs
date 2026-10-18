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
        let data = fs::read(&path).expect("asset is readable");
        if !data.starts_with(b"XUIZ") {
            continue;
        }

        let archive_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string());
        let archive = Archive::parse(&data).expect("archive parses");
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

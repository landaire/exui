//! Decompiles and recompiles the genuine 6717 assets. Set
//! `EXUI_GENUINE_DIR` to the directory holding the XUIZ archives. No content
//! from them is recorded.

use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use exui::compile::CompiledScene;
use exui::pack;
use exui::pack::Conversion;
use exui::pack::SourceContents;
use exui::registry;
use exui::scene::Scene;
use exui::xui::Xml;
use exui::xus::StringTable;
use exui::xus::StringTableXml;
use exui::xus::compile_string_table;
use exui_core::AcceptedVersions;
use exui_core::Archive;
use exui_core::Document;
use exui_core::tree::Tree;
use exui_core::writer::WriteOptions;

fn archives() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(
        env::var_os("EXUI_GENUINE_DIR")
            .expect("EXUI_GENUINE_DIR names the genuine asset directory"),
    );
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("asset directory is readable")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();

    paths
        .into_iter()
        .filter_map(|path| {
            let data = fs::read(&path).expect("asset is readable");
            let name = path.file_name()?.to_string_lossy().into_owned();
            data.starts_with(b"XUIZ").then_some((name, data))
        })
        .collect()
}

#[test]
#[ignore = "needs the genuine 6717 assets in EXUI_GENUINE_DIR"]
fn recompile_genuine_scenes() {
    let registry = registry::dash_6717().expect("registry builds");
    let mut loaded = 0;
    let mut semantic = 0;
    let mut identical = 0;
    let mut failures = Vec::new();

    for (archive_name, data) in archives() {
        let archive = Archive::parse(&data, AcceptedVersions::DASH_6717).expect("archive parses");
        for entry in archive.entries() {
            let name = format!("{archive_name}/{}", entry.name());
            if !name.ends_with(".xur") {
                continue;
            }
            let original = archive.entry_data(&entry).expect("entry data is in bounds");
            let Ok(document) = Document::parse(original, &registry) else {
                continue;
            };
            loaded += 1;

            let expected = Tree::from_document(&document, &registry).expect("tree converts");
            let text = match Scene::from_document(&document, &registry) {
                Ok(scene) => Xml(&scene).to_string(),
                Err(err) => {
                    failures.push(format!("{name}: decompile: {err}"));
                    continue;
                }
            };
            let compiled = match CompiledScene::parse(&text, &registry) {
                Ok(compiled) => compiled,
                Err(err) => {
                    failures.push(format!("{name}: compile: {err}"));
                    continue;
                }
            };

            let options = WriteOptions {
                allocation_hints: document.allocation_hints().is_some(),
            };
            if compiled.tree == expected && compiled.options == options {
                semantic += 1;
            } else {
                failures.push(format!("{name}: tree differs"));
            }

            match compiled.to_xur(&registry) {
                Ok(bytes) if bytes == original => identical += 1,
                Ok(_) => failures.push(format!("{name}: bytes differ")),
                Err(err) => failures.push(format!("{name}: write: {err}")),
            }
        }
    }

    println!("{loaded} scenes, {semantic} semantic matches, {identical} byte identical");
    assert!(failures.is_empty(), "failures: {failures:#?}");
    assert_eq!(loaded, 262);
}

#[test]
#[ignore = "needs the genuine 6717 assets in EXUI_GENUINE_DIR"]
fn recompile_genuine_string_tables() {
    let mut identical = 0;
    let mut failures = Vec::new();

    for (archive_name, data) in archives() {
        let archive = Archive::parse(&data, AcceptedVersions::DASH_6717).expect("archive parses");
        for entry in archive.entries() {
            let name = format!("{archive_name}/{}", entry.name());
            if !name.ends_with(".xus") {
                continue;
            }
            let original = archive.entry_data(&entry).expect("entry data is in bounds");
            let table = match StringTable::parse(original) {
                Ok(table) => table,
                Err(err) => {
                    failures.push(format!("{name}: {err}"));
                    continue;
                }
            };
            match compile_string_table(&StringTableXml(&table).to_string()) {
                Ok(bytes) if bytes == original => identical += 1,
                Ok(_) => failures.push(format!("{name}: bytes differ")),
                Err(err) => failures.push(format!("{name}: compile: {err}")),
            }
        }
    }

    println!("{identical} string tables byte identical");
    assert!(failures.is_empty(), "failures: {failures:#?}");
}

/// Decompiles each archive into a directory as `exui decompile` does and
/// packs it back in the original entry order.
#[test]
#[ignore = "needs the genuine 6717 assets in EXUI_GENUINE_DIR"]
fn repack_genuine_archives() {
    let registry = registry::dash_6717().expect("registry builds");
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("genuine-repack");
    if root.exists() {
        fs::remove_dir_all(&root).expect("stale directory is removable");
    }
    let mut identical = 0;
    let mut stored = Vec::new();
    let mut failures = Vec::new();

    for (archive_name, data) in archives() {
        let archive = Archive::parse(&data, AcceptedVersions::DASH_6717).expect("archive parses");
        let dir = root.join(&archive_name);
        let mut order = Vec::new();
        for entry in archive.entries() {
            let name = entry.name().to_string();
            let entry_data = archive.entry_data(&entry).expect("entry data is in bounds");
            let source = pack::source_file(&name, entry_data, &registry);
            let contents = match &source.contents {
                SourceContents::Text(text) => text.as_bytes(),
                SourceContents::Data { data, reason } => {
                    if let Some(reason) = reason {
                        stored.push(format!("{archive_name}/{name}: {reason}"));
                    }
                    data
                }
            };
            write(&dir, &source.name, contents);
            order.push(name);
        }

        match pack::pack_directory(&dir, Some(&order), Conversion::Compile, &registry) {
            Ok(bytes) if bytes == data => identical += 1,
            Ok(_) => failures.push(format!("{archive_name}: bytes differ")),
            Err(err) => failures.push(format!("{archive_name}: {err}")),
        }
    }

    fs::remove_dir_all(&root).expect("temporary directory is removable");
    println!("{identical} archives byte identical; stored as data: {stored:#?}");
    assert!(failures.is_empty(), "failures: {failures:#?}");
}

fn write(dir: &Path, name: &str, data: &[u8]) {
    let path = dir.join(name.replace('\\', std::path::MAIN_SEPARATOR_STR));
    fs::create_dir_all(path.parent().expect("entry path has a parent")).expect("directory");
    fs::write(path, data).expect("entry is writable");
}

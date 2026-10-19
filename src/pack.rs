//! XUIZ archives from a directory of entries as `exui extract` or
//! `exui decompile` writes them. Scene and string table sources are
//! compiled back to the entry they came from.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use exui_core::ArchiveVersion;
use exui_core::Registry;
use exui_core::archive_writer::NewEntry;
use exui_core::archive_writer::write_archive;

use crate::compile::compile_scene;
use crate::error::PackError;
use crate::error::TextFormError;
use crate::scene::Scene;
use crate::xui::Xml;
use crate::xus::StringTable;
use crate::xus::StringTableXml;
use crate::xus::compile_string_table;

const SCENE_EXTENSION: &str = ".xur";
const SCENE_SOURCE_EXTENSION: &str = ".xui";
const STRING_TABLE_EXTENSION: &str = ".xus";
/// Appended to a `.xus` entry name for its text form.
const STRING_TABLE_SOURCE_SUFFIX: &str = ".xml";

/// Whether text sources are compiled or stored as they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    Compile,
    Store,
}

/// How a source file becomes its entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// Stored as it is.
    Raw,
    /// XUI text compiled to XUR.
    Scene,
    /// String table text compiled to `.xus`.
    StringTable,
}

/// One archive entry and the file it is built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackSource {
    /// Entry name with `\` separators.
    pub name: String,
    pub path: PathBuf,
    pub kind: SourceKind,
}

/// The file `exui decompile` writes for one archive entry.
#[derive(Debug, PartialEq)]
pub struct SourceFile<'d> {
    /// Path relative to the output directory, with `\` separators.
    pub name: String,
    pub contents: SourceContents<'d>,
}

#[derive(Debug, PartialEq)]
pub enum SourceContents<'d> {
    /// XUI or string table text.
    Text(String),
    /// The entry data. `reason` is set for an entry whose kind has a text
    /// form that cannot reproduce this entry.
    Data {
        data: &'d [u8],
        reason: Option<TextFormError>,
    },
}

/// Decompiles archive entry `name` holding `data` to its source file:
/// scenes to XUI text, string tables to their XML form, other entries
/// and those the text form cannot reproduce as they are.
pub fn source_file<'d>(name: &str, data: &'d [u8], registry: &Registry) -> SourceFile<'d> {
    let (source, kind) = source_name(name);
    let text = match kind {
        SourceKind::Raw => {
            return SourceFile {
                name: name.to_string(),
                contents: SourceContents::Data { data, reason: None },
            };
        }
        SourceKind::Scene => Scene::parse(data, registry)
            .map(|scene| Xml(&scene).to_string())
            .map_err(TextFormError::from),
        SourceKind::StringTable => StringTable::parse(data)
            .map(|table| StringTableXml(&table).to_string())
            .map_err(TextFormError::from),
    };

    match text {
        Ok(text) => SourceFile {
            name: source,
            contents: SourceContents::Text(text),
        },
        Err(reason) => SourceFile {
            name: name.to_string(),
            contents: SourceContents::Data {
                data,
                reason: Some(reason),
            },
        },
    }
}

/// Source file name for the entry `name` when its data has a text form:
/// `x.xur` is decompiled to `x.xui` and `x.xus` to `x.xus.xml`. Other
/// entries are written as they are.
pub fn source_name(name: &str) -> (String, SourceKind) {
    if let Some(stem) = name.strip_suffix(SCENE_EXTENSION) {
        return (format!("{stem}{SCENE_SOURCE_EXTENSION}"), SourceKind::Scene);
    }
    if name.ends_with(STRING_TABLE_EXTENSION) {
        return (
            format!("{name}{STRING_TABLE_SOURCE_SUFFIX}"),
            SourceKind::StringTable,
        );
    }

    (name.to_string(), SourceKind::Raw)
}

/// Entry names of an order file: one per line, empty lines skipped.
pub fn parse_order(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Order file text listing `names`.
pub fn format_order<'n>(names: impl IntoIterator<Item = &'n str>) -> String {
    let mut text = String::new();
    for name in names {
        text.push_str(name);
        text.push('\n');
    }

    text
}

/// One entry per file under `dir`, named by its path relative to `dir`
/// with `\` separators. With [`Conversion::Compile`], `.xui` and `.xus.xml`
/// files become the entries [`source_name`] maps them from. Entries come in
/// `order` when given, which must list each exactly, else sorted by name.
pub fn plan_directory(
    dir: &Path,
    order: Option<&[String]>,
    conversion: Conversion,
) -> Result<Vec<PackSource>, PackError> {
    let mut sources: Vec<PackSource> = Vec::new();
    for (path, relative) in files(dir)? {
        let (name, kind) = match conversion {
            Conversion::Compile => entry_name(&relative),
            Conversion::Store => (relative, SourceKind::Raw),
        };
        if let Some(first) = sources.iter().find(|source| source.name == name) {
            return Err(PackError::DuplicateEntry {
                name,
                first: first.path.clone(),
                second: path,
            });
        }
        sources.push(PackSource { name, path, kind });
    }

    let Some(order) = order else {
        sources.sort_by(|left, right| left.name.cmp(&right.name));
        return Ok(sources);
    };

    let mut ordered = Vec::with_capacity(order.len());
    for name in order {
        let Some(position) = sources.iter().position(|source| &source.name == name) else {
            return Err(PackError::MissingSource { name: name.clone() });
        };
        ordered.push(sources.swap_remove(position));
    }
    if let Some(unlisted) = sources.into_iter().next() {
        return Err(PackError::Unlisted {
            name: unlisted.name,
        });
    }

    Ok(ordered)
}

/// Archive bytes holding `sources` in order, as a version 1 archive like
/// those of the 6717 dashboard.
pub fn pack(sources: &[PackSource], registry: &Registry) -> Result<Vec<u8>, PackError> {
    let mut data = Vec::with_capacity(sources.len());
    for source in sources {
        let io_error = |error| PackError::Io {
            path: source.path.clone(),
            source: error,
        };
        let compile_error = |error| PackError::Compile {
            path: source.path.clone(),
            source: error,
        };

        let bytes = match source.kind {
            SourceKind::Raw => fs::read(&source.path).map_err(io_error)?,
            SourceKind::Scene => {
                let text = fs::read_to_string(&source.path).map_err(io_error)?;
                compile_scene(&text, registry).map_err(compile_error)?
            }
            SourceKind::StringTable => {
                let text = fs::read_to_string(&source.path).map_err(io_error)?;
                compile_string_table(&text).map_err(compile_error)?
            }
        };
        data.push(bytes);
    }

    let entries: Vec<NewEntry<'_>> = sources
        .iter()
        .zip(&data)
        .map(|(source, data)| NewEntry {
            name: &source.name,
            data,
        })
        .collect();

    Ok(write_archive(ArchiveVersion::V1, &entries)?)
}

/// [`plan_directory`] then [`pack`].
pub fn pack_directory(
    dir: &Path,
    order: Option<&[String]>,
    conversion: Conversion,
    registry: &Registry,
) -> Result<Vec<u8>, PackError> {
    pack(&plan_directory(dir, order, conversion)?, registry)
}

/// Entry name and kind of a source file name; the inverse of
/// [`source_name`].
pub fn entry_name(file: &str) -> (String, SourceKind) {
    if let Some(stem) = file.strip_suffix(SCENE_SOURCE_EXTENSION) {
        return (format!("{stem}{SCENE_EXTENSION}"), SourceKind::Scene);
    }
    let string_table = file
        .strip_suffix(STRING_TABLE_SOURCE_SUFFIX)
        .filter(|name| name.ends_with(STRING_TABLE_EXTENSION));
    if let Some(name) = string_table {
        return (name.to_string(), SourceKind::StringTable);
    }

    (file.to_string(), SourceKind::Raw)
}

/// Files under `dir` with their relative names, `\` separated.
fn files(dir: &Path) -> Result<Vec<(PathBuf, String)>, PackError> {
    let io_error = |path: &Path| {
        let path = path.to_path_buf();
        move |error| PackError::Io {
            path,
            source: error,
        }
    };

    let mut found = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), String::new())];
    while let Some((path, relative)) = pending.pop() {
        for entry in fs::read_dir(&path).map_err(io_error(&path))? {
            let entry = entry.map_err(io_error(&path))?;
            let child = entry.path();
            let Some(file_name) = entry.file_name().to_str().map(str::to_string) else {
                return Err(PackError::InvalidFileName { path: child });
            };
            let name = if relative.is_empty() {
                file_name
            } else {
                format!("{relative}\\{file_name}")
            };

            if fs::metadata(&child).map_err(io_error(&child))?.is_dir() {
                pending.push((child, name));
            } else {
                found.push((child, name));
            }
        }
    }

    Ok(found)
}

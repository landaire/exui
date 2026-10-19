use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::path::MAIN_SEPARATOR_STR;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use clap::Subcommand;
use exui::archive::Archive;
use exui::archive::Entry;
use exui::compile::compile_scene;
use exui::pack;
use exui::pack::Conversion;
use exui::pack::SourceContents;
use exui::pack::SourceKind;
use exui::registry;
use exui::scene::Scene;
use exui::xui::ClassExtension;
use exui::xui::Xml;
use exui::xui::collect_custom_classes;
use exui::xus::StringTable;
use exui::xus::StringTableXml;
use exui::xus::compile_string_table;
use exui_core::Registry;
use exui_core::schema::ClassDef;
use memmap2::Mmap;

#[derive(Parser)]
#[command(name = "exui", about = "Xbox 360 XUI/XUR tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Decompile .xur and .xus files to XML.
    /// For XUIZ archives, decompiles every scene and string table inside
    /// and extracts the other entries.
    Decompile {
        /// Input .xur, .xus or XUIZ archive
        input: PathBuf,
        /// Output directory for archives, output file otherwise
        /// (default: <input_stem>/ for archives, standard output otherwise)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also write the archive's entry names in order to this file
        #[arg(long)]
        order_file: Option<PathBuf>,
    },
    /// Compile a .xui scene to .xur or a .xus.xml string table to .xus.
    Compile {
        /// Input .xui or .xus.xml file
        input: PathBuf,
        /// Output file (default: the input with the compiled extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Pack a directory into a XUIZ archive, compiling .xui and .xus.xml
    /// files.
    Pack {
        /// Directory holding the entries
        input: PathBuf,
        /// Output archive
        #[arg(short, long)]
        output: PathBuf,
        /// File listing the entry names in archive order, one per line
        /// (default: sorted by name)
        #[arg(long)]
        order: Option<PathBuf>,
        /// Store .xui and .xus.xml files as they are
        #[arg(long)]
        no_compile: bool,
    },
    /// Extract all resources from a XUIZ archive.
    Extract {
        /// Input XUIZ archive
        input: PathBuf,
        /// Output directory (default: <input_stem>/)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also write the entry names in order to this file
        #[arg(long)]
        order_file: Option<PathBuf>,
    },
    /// List contents of a XUIZ archive.
    List {
        /// Input XUIZ archive
        input: PathBuf,
    },
}

type CliResult = Result<(), Box<dyn Error>>;

/// Custom classes by name, collected over every decompiled scene.
type CustomClasses = BTreeMap<&'static str, &'static ClassDef>;

fn map_file(path: &Path) -> Result<Mmap, Box<dyn Error>> {
    let file = File::open(path)?;

    // SAFETY: the mapping is read-only and the tool does not modify inputs
    // while it runs.
    Ok(unsafe { Mmap::map(&file)? })
}

/// Default output directory: the input path without its extension, or
/// prefixed with `_` when it has none (`accountm` -> `_accountm/`).
fn default_output_dir(input: &Path) -> PathBuf {
    if input.extension().is_some() {
        let stem = input.file_stem().unwrap_or_default();
        input.with_file_name(stem)
    } else {
        let name = input.file_name().unwrap_or_default();
        input.with_file_name(format!("_{}", name.to_string_lossy()))
    }
}

/// Writes `data` to the archive entry path `name` under `dir`.
fn write_entry(dir: &Path, name: &str, data: &[u8]) -> CliResult {
    let path = dir.join(name.replace('\\', MAIN_SEPARATOR_STR));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, data)?;

    Ok(())
}

fn write_order(path: Option<&Path>, entries: &[Entry<'_>]) -> CliResult {
    let Some(path) = path else {
        return Ok(());
    };

    fs::write(
        path,
        pack::format_order(entries.iter().map(|entry| entry.name.as_str())),
    )?;
    eprintln!("entry order -> {}", path.display());

    Ok(())
}

/// Writes an extension file for each class in `classes` that does not have
/// one in `dir` yet.
fn write_extensions(dir: &Path, classes: &CustomClasses) -> CliResult {
    if classes.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(dir)?;
    for (name, def) in classes {
        let path = dir.join(format!("{name}.xml"));
        if !path.exists() {
            fs::write(&path, ClassExtension(def).to_string())?;
            eprintln!("class extension -> {}", path.display());
        }
    }

    Ok(())
}

fn cmd_decompile(input: &Path, output: Option<&Path>, order_file: Option<&Path>) -> CliResult {
    let data = map_file(input)?;
    let registry = registry::dash_6717()?;

    if data.starts_with(b"XUIZ") {
        let out_dir = output.map_or_else(|| default_output_dir(input), Path::to_path_buf);
        decompile_archive(&data, &registry, &out_dir, order_file)
    } else if data.starts_with(b"XUIB") {
        decompile_scene(&data, &registry, output)
    } else if data.starts_with(b"XUIS") {
        let xml = StringTableXml(&StringTable::parse(&data)?).to_string();
        match output {
            Some(path) => fs::write(path, xml)?,
            None => print!("{xml}"),
        }
        Ok(())
    } else {
        Err(format!(
            "unknown file format (magic: {:02x?})",
            &data[..4.min(data.len())]
        )
        .into())
    }
}

/// Entries without a text form, or whose data the text form cannot
/// reproduce, are written as they are so the directory packs back to the
/// same entries.
fn decompile_archive(
    data: &[u8],
    registry: &Registry,
    out_dir: &Path,
    order_file: Option<&Path>,
) -> CliResult {
    let entries = Archive::parse(data)?.entries()?;
    fs::create_dir_all(out_dir)?;

    let mut custom = CustomClasses::new();
    let mut stored = 0;
    for entry in &entries {
        let source = pack::source_file(&entry.name, entry.data, registry);
        match &source.contents {
            SourceContents::Text(text) => {
                write_entry(out_dir, &source.name, text.as_bytes())?;
                eprintln!("{} -> {}", entry.name, source.name);
                // Parsed again only to name the classes it uses.
                if let Ok(scene) = Scene::parse(entry.data, registry) {
                    collect_custom_classes(&scene, &mut custom);
                }
            }
            SourceContents::Data { data, reason } => {
                write_entry(out_dir, &source.name, data)?;
                match reason {
                    Some(reason) => {
                        stored += 1;
                        eprintln!("{}: {reason}; stored as it is", entry.name);
                    }
                    None => eprintln!("{} ({} bytes)", entry.name, data.len()),
                }
            }
        }
    }

    let extension_dir = out_dir.parent().unwrap_or(out_dir).join("extensions");
    write_extensions(&extension_dir, &custom)?;
    write_order(order_file, &entries)?;

    if stored > 0 {
        eprintln!("{stored} scenes or string tables have no text form and were stored as they are");
    }

    Ok(())
}

fn decompile_scene(data: &[u8], registry: &Registry, output: Option<&Path>) -> CliResult {
    let scene = Scene::parse(data, registry)?;
    let xml = Xml(&scene).to_string();

    let mut custom = CustomClasses::new();
    collect_custom_classes(&scene, &mut custom);

    let Some(path) = output else {
        print!("{xml}");
        for def in custom.values() {
            eprintln!("--- Class Extension: {} ---", def.name);
            eprint!("{}", ClassExtension(def));
        }
        return Ok(());
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    fs::write(path, xml)?;

    write_extensions(&parent.join("extensions"), &custom)
}

fn cmd_compile(input: &Path, output: Option<&Path>) -> CliResult {
    let Some(file_name) = input.file_name().and_then(|name| name.to_str()) else {
        return Err(format!("{}: file name is not UTF-8", input.display()).into());
    };
    let (compiled_name, kind) = pack::entry_name(file_name);
    let text = fs::read_to_string(input)?;

    let data = match kind {
        SourceKind::Scene => compile_scene(&text, &registry::dash_6717()?)?,
        SourceKind::StringTable => compile_string_table(&text)?,
        SourceKind::Raw => {
            return Err(format!("{}: not a .xui or .xus.xml file", input.display()).into());
        }
    };

    let path = output.map_or_else(|| input.with_file_name(compiled_name), Path::to_path_buf);
    fs::write(&path, data)?;
    eprintln!("{} -> {}", input.display(), path.display());

    Ok(())
}

fn cmd_pack(input: &Path, output: &Path, order: Option<&Path>, no_compile: bool) -> CliResult {
    let order = match order {
        Some(path) => Some(pack::parse_order(&fs::read_to_string(path)?)),
        None => None,
    };
    let conversion = if no_compile {
        Conversion::Store
    } else {
        Conversion::Compile
    };

    let sources = pack::plan_directory(input, order.as_deref(), conversion)?;
    let data = pack::pack(&sources, &registry::dash_6717()?)?;
    fs::write(output, data)?;
    eprintln!("packed {} entries into {}", sources.len(), output.display());

    Ok(())
}

fn cmd_extract(input: &Path, output: Option<&Path>, order_file: Option<&Path>) -> CliResult {
    let data = map_file(input)?;
    let entries = Archive::parse(&data)?.entries()?;
    let out_dir = output.map_or_else(|| default_output_dir(input), Path::to_path_buf);
    fs::create_dir_all(&out_dir)?;

    for entry in &entries {
        write_entry(&out_dir, &entry.name, entry.data)?;
        eprintln!("{} ({} bytes)", entry.name, entry.data.len());
    }
    write_order(order_file, &entries)?;

    eprintln!("extracted {} files to {}", entries.len(), out_dir.display());

    Ok(())
}

fn cmd_list(input: &Path) -> CliResult {
    let data = map_file(input)?;
    let archive = Archive::parse(&data)?;
    let entries = archive.entries()?;

    println!("XUIZ v{} ({} entries)", archive.version(), entries.len());
    for entry in &entries {
        println!("  {:>8}  {}", entry.data.len(), entry.name);
    }

    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Decompile {
            input,
            output,
            order_file,
        } => cmd_decompile(&input, output.as_deref(), order_file.as_deref()),
        Command::Compile { input, output } => cmd_compile(&input, output.as_deref()),
        Command::Pack {
            input,
            output,
            order,
            no_compile,
        } => cmd_pack(&input, &output, order.as_deref(), no_compile),
        Command::Extract {
            input,
            output,
            order_file,
        } => cmd_extract(&input, output.as_deref(), order_file.as_deref()),
        Command::List { input } => cmd_list(&input),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

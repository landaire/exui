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
use exui::registry;
use exui::scene::Scene;
use exui::xui::ClassExtension;
use exui::xui::Xml;
use exui::xui::collect_custom_classes;
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
    /// Decompile .xur/.xuiz files to .xui XML.
    /// For XUIZ archives, decompiles all XUR resources inside.
    Decompile {
        /// Input .xur or XUIZ archive
        input: PathBuf,
        /// Output directory (default: <input_stem>/)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Extract all resources from a XUIZ archive.
    Extract {
        /// Input XUIZ archive
        input: PathBuf,
        /// Output directory (default: <input_stem>/)
        #[arg(short, long)]
        output: Option<PathBuf>,
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

fn cmd_decompile(input: &Path, output: Option<&Path>) -> CliResult {
    let data = map_file(input)?;
    let registry = registry::dash_6717()?;

    if data.starts_with(b"XUIZ") {
        let out_dir = output.map_or_else(|| default_output_dir(input), Path::to_path_buf);
        decompile_archive(&data, &registry, &out_dir)
    } else if data.starts_with(b"XUIB") {
        decompile_single(&data, &registry, output)
    } else {
        Err(format!(
            "unknown file format (magic: {:02x?})",
            &data[..4.min(data.len())]
        )
        .into())
    }
}

fn decompile_archive(data: &[u8], registry: &Registry, out_dir: &Path) -> CliResult {
    let entries = Archive::parse(data)?.entries()?;
    fs::create_dir_all(out_dir)?;

    let mut custom = CustomClasses::new();
    let mut scene_count = 0;
    let mut failures = 0;
    for entry in &entries {
        let normalized = entry.name.replace('\\', MAIN_SEPARATOR_STR);

        if !entry.name.ends_with(".xur") {
            let out_path = out_dir.join(&normalized);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out_path, entry.data)?;
            eprintln!("{} ({} bytes)", entry.name, entry.data.len());
            continue;
        }

        scene_count += 1;
        let scene = match Scene::parse(entry.data, registry) {
            Ok(scene) => scene,
            Err(err) => {
                eprintln!("{}: {err}", entry.name);
                failures += 1;
                continue;
            }
        };

        let out_path = out_dir.join(normalized.replace(".xur", ".xui"));
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, Xml(&scene).to_string())?;
        eprintln!("{} -> {}", entry.name, out_path.display());
        collect_custom_classes(&scene, &mut custom);
    }

    let extension_dir = out_dir.parent().unwrap_or(out_dir).join("extensions");
    write_extensions(&extension_dir, &custom)?;

    if failures > 0 {
        return Err(format!("{failures} of {scene_count} XUR files failed to decompile").into());
    }

    Ok(())
}

fn decompile_single(data: &[u8], registry: &Registry, output: Option<&Path>) -> CliResult {
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

fn cmd_extract(input: &Path, output: Option<&Path>) -> CliResult {
    let data = map_file(input)?;
    let entries = Archive::parse(&data)?.entries()?;
    let out_dir = output.map_or_else(|| default_output_dir(input), Path::to_path_buf);
    fs::create_dir_all(&out_dir)?;

    for entry in &entries {
        let out_path = out_dir.join(entry.name.replace('\\', MAIN_SEPARATOR_STR));
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, entry.data)?;
        eprintln!("{} ({} bytes)", entry.name, entry.data.len());
    }

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
        Command::Decompile { input, output } => cmd_decompile(&input, output.as_deref()),
        Command::Extract { input, output } => cmd_extract(&input, output.as_deref()),
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

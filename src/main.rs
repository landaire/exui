use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use clap::Subcommand;

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

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Command::Decompile { input, output } => cmd_decompile(&input, output.as_deref()),
        Command::Extract { input, output } => cmd_extract(&input, output.as_deref()),
        Command::List { input } => cmd_list(&input),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Derive the default output directory from the input path.
/// With extension: strip it (`archive.xuiz` -> `archive/`).
/// Without extension: prefix with `_` (`accountm` -> `_accountm/`).
fn default_output_dir(input: &std::path::Path) -> PathBuf {
    if input.extension().is_some() {
        let stem = input.file_stem().unwrap_or_default();
        input.with_file_name(stem)
    } else {
        let name = input.file_name().unwrap_or_default();
        input.with_file_name(format!("_{}", name.to_string_lossy()))
    }
}

// ---------------------------------------------------------------------------
// decompile
// ---------------------------------------------------------------------------

fn cmd_decompile(
    input: &std::path::Path,
    output: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    use exui::xur::archive;

    let file = std::fs::File::open(input)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    if archive::is_xuiz(&mmap) {
        let out_dir = output
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| default_output_dir(input));
        decompile_xuiz(&mmap, &out_dir)
    } else if archive::is_xuib(&mmap) {
        decompile_single_xur(&mmap, output)
    } else {
        Err(format!(
            "unknown file format (magic: {:02x?})",
            &mmap[..4.min(mmap.len())]
        )
        .into())
    }
}

fn decompile_xuiz(
    data: &[u8],
    out_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use exui::xur::archive::{self, ReadAt};

    let archive = archive::XuizArchive::parse(data)?;

    std::fs::create_dir_all(out_dir)?;

    // Collect all custom classes across all XURs for a single extension file
    let mut all_custom = std::collections::BTreeMap::new();

    let mut errors = Vec::new();
    for entry in &archive.entries {
        let entry_data = data.read_at(entry.range.clone())?;
        let normalized = entry.name.replace('\\', std::path::MAIN_SEPARATOR_STR);

        if entry.name.ends_with(".xur") {
            // Decompile XUR to XUI XML
            let xui_name = normalized.replace(".xur", ".xui");
            let out_path = out_dir.join(&xui_name);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match exui::xur::Xur::parse(entry_data) {
                Ok(xur) => {
                    let xml = exui::xui::to_xui(&xur)
                        .map_err(|e| exui::xur::ParseError::BadObject(e.to_string()))?;
                    std::fs::write(&out_path, &xml)?;
                    eprintln!("{} -> {}", entry.name, out_path.display());

                    // Collect custom classes for consolidated extension file
                    exui::xui::collect_custom_classes_from_xur(&xur, &mut all_custom);
                }
                Err(e) => {
                    eprintln!("{}: {e}", entry.name);
                    errors.push(format!("{}: {e}", entry.name));
                }
            }
        } else {
            // Extract non-XUR resources as-is (PNG, XMA, XUS, etc.)
            let out_path = out_dir.join(&normalized);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out_path, entry_data)?;
            eprintln!("{} ({} bytes)", entry.name, entry.size());
        }
    }

    // Emit consolidated class extension file for the entire archive
    if !all_custom.is_empty() {
        if let Some(ext_xml) = exui::xui::generate_class_extensions_from_map(&all_custom) {
            let ext_path = out_dir.join("classes.xml");
            std::fs::write(&ext_path, &ext_xml)?;
            eprintln!("class extensions -> {}", ext_path.display());
        }
    }

    if !errors.is_empty() {
        return Err(format!(
            "{} of {} XUR files failed to decompile",
            errors.len(),
            archive.entries.iter().filter(|e| e.name.ends_with(".xur")).count()
        )
        .into());
    }

    Ok(())
}

fn decompile_single_xur(
    data: &[u8],
    output: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let xur = exui::xur::Xur::parse(data)?;
    let xml = exui::xui::to_xui(&xur)?;
    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &xml)?;

            // Generate class extension file if there are custom classes
            if let Some(ext_xml) = exui::xui::generate_class_extensions(&xur) {
                let ext_path = path.with_extension("xml");
                std::fs::write(&ext_path, &ext_xml)?;
                eprintln!("class extensions -> {}", ext_path.display());
            }
        }
        None => {
            print!("{xml}");
            // Also print extension to stderr if custom classes exist
            if let Some(ext_xml) = exui::xui::generate_class_extensions(&xur) {
                eprintln!("--- Class Extensions ---");
                eprint!("{ext_xml}");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// extract
// ---------------------------------------------------------------------------

fn cmd_extract(
    input: &std::path::Path,
    output: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    use exui::xur::archive::{self, ReadAt};

    let file = std::fs::File::open(input)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    if !archive::is_xuiz(&mmap) {
        return Err("input is not a XUIZ archive".into());
    }

    let archive = archive::XuizArchive::parse(&*mmap)?;
    let out_dir = match output {
        Some(p) => p.to_path_buf(),
        None => default_output_dir(input),
    };

    std::fs::create_dir_all(&out_dir)?;

    for entry in &archive.entries {
        let data = mmap.as_ref().read_at(entry.range.clone())?;
        // Normalize Windows backslash paths to platform separator
        let normalized = entry.name.replace('\\', std::path::MAIN_SEPARATOR_STR);
        let out_path = out_dir.join(&normalized);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out_path, data)?;
        eprintln!("{} ({} bytes)", entry.name, entry.size());
    }

    eprintln!("extracted {} files to {}", archive.entries.len(), out_dir.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

fn cmd_list(input: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use exui::xur::archive;

    let file = std::fs::File::open(input)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    if !archive::is_xuiz(&mmap) {
        return Err("input is not a XUIZ archive".into());
    }

    let archive = archive::XuizArchive::parse(&*mmap)?;
    println!("XUIZ v{} ({} entries)", archive.version, archive.entries.len());
    for entry in &archive.entries {
        println!("  {:>8}  {}", entry.size(), entry.name);
    }
    Ok(())
}

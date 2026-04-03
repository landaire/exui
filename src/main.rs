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

/// Derive the default output directory from the input path (strip extension).
fn default_output_dir(input: &std::path::Path) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default();
    input.with_file_name(stem)
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

    let xur_entries: Vec<_> = archive
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".xur"))
        .collect();

    if xur_entries.is_empty() {
        return Err("no .xur resources found in XUIZ archive".into());
    }

    std::fs::create_dir_all(&out_dir)?;

    for entry in &xur_entries {
        let xur_data = data.read_at(entry.range.clone())?;
        let xur = exui::xur::Xur::parse(xur_data)?;
        let xml = exui::xui::to_xui(&xur)?;

        let xui_name = entry.name.replace(".xur", ".xui").replace('\\', std::path::MAIN_SEPARATOR_STR);
        let out_path = out_dir.join(&xui_name);
        std::fs::write(&out_path, &xml)?;
        eprintln!("{} -> {}", entry.name, out_path.display());
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
        }
        None => print!("{xml}"),
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

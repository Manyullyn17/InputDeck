//! mpcfg: Macropad OS config tool.

mod toml_config;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use macropad_engine::codec::{self, Bundle};
use macropad_engine::{Config, KEY_COUNT};

#[derive(Parser)]
#[command(about = "Macropad OS config tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and validate a TOML config and print a summary.
    Check { file: PathBuf },
    /// Convert a TOML config into the device's binary format.
    Encode {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn load(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml_config::parse(&text).with_context(|| format!("in {}", path.display()))
}

fn summary(config: &Config) {
    for (i, layer) in config.layers.iter().enumerate() {
        let name = if layer.name.is_empty() { "-" } else { &layer.name };
        let parent = layer.parent.map_or("-".to_string(), |p| format!("L{}", p.0));
        let bound = layer.buttons[..KEY_COUNT].iter().filter(|b| **b != macropad_engine::KeyBinding::Inherit).count();
        println!("  L{i:<3} {name:<16} parent {parent:<5} {bound:>2} keys set");
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Check { file } => {
            let config = load(&file)?;
            let size = codec::encode(&Bundle::single(config.clone())).len();
            println!("{}: OK, {} layers, {size} bytes encoded", file.display(), config.layers.len());
            summary(&config);
        }
        Command::Encode { file, output } => {
            let bytes = codec::encode(&Bundle::single(load(&file)?));
            fs::write(&output, &bytes).with_context(|| format!("writing {}", output.display()))?;
            println!("wrote {} ({} bytes)", output.display(), bytes.len());
        }
    }
    Ok(())
}

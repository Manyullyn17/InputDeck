//! mpcfg: Macropad OS config tool.

mod device;
mod layouts;
mod to_toml;
mod toml_config;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use macropad_engine::codec::{self, Bundle, FORMAT_VERSION};
use macropad_engine::{Config, KEY_COUNT, KeyBinding};

use device::Device;

#[derive(Parser)]
#[command(about = "Macropad OS config tool")]
struct Cli {
    /// Serial port of the device (default: find it automatically).
    #[arg(long, global = true)]
    port: Option<String>,
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
    /// Show the connected device's versions and stored config.
    Info,
    /// Upload a config (.toml, or an encoded .bin). The device applies it right away.
    Upload { file: PathBuf },
    /// Download the device's stored config (.toml output is readable, anything else is binary).
    Pull {
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Reboot the device into its UF2 bootloader for flashing firmware.
    Bootloader,
}

fn load(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml_config::parse(&text).with_context(|| format!("in {}", path.display()))
}

/// A config file as encoded bytes: TOML is compiled, anything else is
/// treated as already encoded and checked.
fn load_encoded(path: &Path) -> Result<Vec<u8>> {
    if path.extension().is_some_and(|e| e == "toml") {
        return Ok(codec::encode(&Bundle::single(load(path)?)));
    }
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    codec::decode(&bytes).map_err(|e| anyhow::anyhow!("{}: not a valid encoded config: {e:?}", path.display()))?;
    Ok(bytes)
}

fn summary(config: &Config) {
    for (i, layer) in config.layers.iter().enumerate() {
        let name = if layer.name.is_empty() { "-" } else { &layer.name };
        let parent = layer.parent.map_or("-".to_string(), |p| format!("L{}", p.0));
        let set = layer.buttons[..KEY_COUNT].iter().filter(|b| **b != KeyBinding::Inherit).count();
        println!("  L{i:<3} {name:<16} parent {parent:<5} {set:>2} keys set");
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let port = cli.port.as_deref();
    match cli.command {
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
        Command::Info => {
            let mut dev = Device::open(port)?;
            let info = dev.info()?;
            println!("Macropad OS: config format v{}, protocol v{}", info.format_version, info.protocol_version);
            match dev.get_config() {
                Ok(bytes) => {
                    let bundle = codec::decode(&bytes).map_err(|e| anyhow::anyhow!("stored config: {e:?}"))?;
                    let config = bundle.active_config().expect("decode checks the active profile");
                    println!("stored config: {} bytes, {} layers", bytes.len(), config.layers.len());
                    summary(config);
                }
                Err(e) => println!("{e}"),
            }
        }
        Command::Upload { file } => {
            let bytes = load_encoded(&file)?;
            let mut dev = Device::open(port)?;
            let info = dev.info()?;
            ensure!(
                info.format_version == FORMAT_VERSION,
                "device uses config format v{}, this tool writes v{FORMAT_VERSION}; update the firmware or tool",
                info.format_version
            );
            dev.put_config(&bytes)?;
            println!("uploaded {} bytes; the device is using the new config", bytes.len());
        }
        Command::Pull { output } => {
            let bytes = Device::open(port)?.get_config()?;
            if output.extension().is_some_and(|e| e == "toml") {
                let bundle = codec::decode(&bytes).map_err(|e| anyhow::anyhow!("stored config: {e:?}"))?;
                let config = bundle.active_config().expect("decode checks the active profile");
                fs::write(&output, to_toml::write(config)).with_context(|| format!("writing {}", output.display()))?;
                println!("wrote {}", output.display());
            } else {
                fs::write(&output, &bytes).with_context(|| format!("writing {}", output.display()))?;
                println!("wrote {} ({} bytes)", output.display(), bytes.len());
            }
        }
        Command::Bootloader => {
            Device::open(port)?.bootloader()?;
            println!("device is rebooting into the bootloader");
        }
    }
    Ok(())
}

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
    /// Upload one or more configs (.toml) as profiles. The device applies
    /// the active one right away. A single encoded .bin is uploaded as is.
    Upload {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Index of the profile to activate (default: the first).
        #[arg(long, default_value_t = 0)]
        active: u8,
    },
    /// Download a stored profile (.toml output is readable; anything else
    /// gets the whole binary bundle).
    Pull {
        #[arg(short, long)]
        output: PathBuf,
        /// Profile index (default: the active one).
        #[arg(long)]
        profile: Option<u8>,
    },
    /// Reboot the device into its UF2 bootloader for flashing firmware.
    Bootloader,
}

fn load(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml_config::parse(&text).with_context(|| format!("in {}", path.display()))
}

/// Config files as one encoded bundle: TOML files become profiles in the
/// given order; a single other file is treated as an already encoded bundle.
fn load_encoded(paths: &[PathBuf], active: u8) -> Result<Vec<u8>> {
    if let [path] = paths
        && !path.extension().is_some_and(|e| e == "toml")
    {
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        codec::decode(&bytes).map_err(|e| anyhow::anyhow!("{}: not a valid encoded config: {e:?}", path.display()))?;
        return Ok(bytes);
    }
    ensure!((active as usize) < paths.len(), "--active {active} but only {} profile(s) given", paths.len());
    let profiles = paths.iter().map(|p| load(p)).collect::<Result<Vec<_>>>()?;
    Ok(codec::encode(&Bundle { active, profiles }))
}

fn profile_label(config: &Config, i: usize) -> String {
    if config.name.is_empty() { format!("Profile {}", i + 1) } else { config.name.clone() }
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
                    println!("stored config: {} bytes, {} profile(s)", bytes.len(), bundle.profiles.len());
                    for (i, config) in bundle.profiles.iter().enumerate() {
                        let marker = if i == bundle.active as usize { "*" } else { " " };
                        println!("{marker} {i}: {} ({} layers)", profile_label(config, i), config.layers.len());
                        if i == bundle.active as usize {
                            summary(config);
                        }
                    }
                }
                Err(e) => println!("{e}"),
            }
        }
        Command::Upload { files, active } => {
            let bytes = load_encoded(&files, active)?;
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
        Command::Pull { output, profile } => {
            let bytes = Device::open(port)?.get_config()?;
            if output.extension().is_some_and(|e| e == "toml") {
                let bundle = codec::decode(&bytes).map_err(|e| anyhow::anyhow!("stored config: {e:?}"))?;
                let index = profile.unwrap_or(bundle.active) as usize;
                let config = bundle
                    .profiles
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("no profile {index} (the device has {})", bundle.profiles.len()))?;
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

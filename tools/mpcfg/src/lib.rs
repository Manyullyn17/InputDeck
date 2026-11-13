//! Macropad OS config tooling shared by the `mpcfg` CLI and the desktop app:
//! the TOML config format, keyboard layouts, and the device connection.

pub mod device;
pub mod document;
pub mod layouts;
pub mod sim;
pub mod to_toml;
pub mod toml_config;

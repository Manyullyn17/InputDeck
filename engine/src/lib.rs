//! Macropad OS transition engine.
//!
//! Pure logic with no hardware dependencies, shared by the firmware and the
//! desktop app. Semantics are defined in `meta-layers.md` at the repo root.

#![cfg_attr(not(test), no_std)]

pub mod config;
pub mod engine;
pub mod hid;
pub mod label;

#[cfg(test)]
mod testutil;

pub use config::*;
pub use engine::{Engine, Event, Instant, Output};
pub use hid::{HidState, KeyboardReport};

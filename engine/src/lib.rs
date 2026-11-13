//! Macropad OS transition engine.
//!
//! Pure logic with no hardware dependencies, shared by the firmware and the
//! desktop app. Semantics are defined in `meta-layers.md` at the repo root.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod codec;
pub mod config;
pub mod engine;
pub mod hid;
pub mod label;
pub mod led;
pub mod notation;
pub mod protocol;

#[cfg(test)]
mod testutil;

pub use config::*;
pub use engine::{Engine, Event, Instant, KeyLight, Output};
pub use hid::{HidState, KeyboardReport};

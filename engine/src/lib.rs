//! Macropad OS transition engine.
//!
//! Pure logic with no hardware dependencies, shared by the firmware and the
//! desktop app. Semantics are defined in `meta-layers.md` at the repo root.

#![cfg_attr(not(test), no_std)]

pub mod config;

pub use config::*;

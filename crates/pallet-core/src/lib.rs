//! Orchestration layer for Pallet.
//!
//! This crate is deliberately free of UI toolkits and platform APIs so that the
//! Tauri app, the picker helper and the CLI can all share it. Platform work
//! lives in `pallet-capture`, `pallet-overlay` and `pallet-hotkey`.

#![warn(missing_docs)]

pub mod error;
pub mod logging;
pub mod paths;

pub use error::{Error, Result};
pub use paths::Paths;

/// The version of the running Pallet build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

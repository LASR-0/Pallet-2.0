//! Persistence for Pallet.
//!
//! Two stores with different jobs. Settings live in a TOML file that users are
//! expected to hand-edit and keep in dotfiles, so loading is forgiving and
//! never fails. The library — colours, palettes, pick history and tags — lives
//! in SQLite, where ordering, search and a growing pick history need real
//! indexes and transactions.
//!
//! Captured frames are never written to disk. A pick records its colour, the
//! time, and optionally which application was underneath.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod model;
pub mod seed;
pub mod store;

pub use config::{Config, Loaded};
pub use error::{Error, Result};
pub use model::{NewColour, Palette, Pick, StoredColour};
pub use store::Store;

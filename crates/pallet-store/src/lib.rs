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
//!
//! A third form exists for leaving the machine: [`share`] writes a library out
//! as a document other installations merge in, rather than as a copy of the
//! database. See that module for why the two are not the same thing.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod model;
pub mod seed;
pub mod share;
pub mod store;

pub use config::{Config, Loaded};
pub use error::{Error, Result};
pub use model::{Member, NewColour, Palette, Pick, StoredColour, Token};
pub use share::{Bundle, BundleColour, BundleMember, BundlePalette, Merged};
pub use store::Store;

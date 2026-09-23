//! Errors from the storage layer.

use std::path::PathBuf;

/// Everything that can go wrong reading or writing Pallet's data.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The SQLite library could not be opened or queried.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Schema migrations could not be applied.
    #[error("could not migrate the database schema: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    /// A settings file could not be written.
    #[error("could not write {path}: {source}")]
    WriteConfig {
        /// The file we failed to write.
        path: PathBuf,
        /// The underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// Settings could not be serialised.
    #[error("could not serialise settings: {0}")]
    SerialiseConfig(#[from] toml::ser::Error),

    /// A referenced row does not exist.
    #[error("no {kind} with id {id}")]
    NotFound {
        /// What was being looked up, e.g. `palette`.
        kind: &'static str,
        /// The identifier that missed.
        id: String,
    },

    /// A shared library file could not be read or written as JSON.
    #[error("that file is not a Pallet library: {0}")]
    Bundle(#[from] serde_json::Error),

    /// The bundle was written by a Pallet newer than this one.
    ///
    /// Worth its own variant rather than a parse failure: the file is not
    /// corrupt and the user has done nothing wrong, so the message has to say
    /// "update Pallet" rather than "this is broken".
    #[error(
        "this library was shared from a newer Pallet (format {found}; this build reads up to {supported})"
    )]
    BundleTooNew {
        /// The format the file declares.
        found: u32,
        /// The newest format this build understands.
        supported: u32,
    },

    /// A colour in a bundle did not carry a readable hex value.
    #[error("the shared library has a colour with an unreadable value: {id} is `{value}`")]
    BadBundleColour {
        /// The colour's identifier, so the offending entry can be found.
        id: String,
        /// What was there instead of `#RRGGBB`.
        value: String,
    },
}

/// Convenience alias for fallible storage operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

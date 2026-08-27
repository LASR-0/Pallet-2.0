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
}

/// Convenience alias for fallible storage operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

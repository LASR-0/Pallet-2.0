//! The error type shared across Pallet's binaries.

use std::path::PathBuf;

/// Everything that can go wrong at the orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The OS gave us no home or config directory to work from.
    #[error("could not determine platform directories for the current user")]
    NoPlatformDirs,

    /// A directory Pallet needs could not be created.
    #[error("could not create {path}: {source}")]
    CreateDir {
        /// The directory we failed to create.
        path: PathBuf,
        /// The underlying OS error.
        #[source]
        source: std::io::Error,
    },
}

/// Convenience alias for fallible core operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

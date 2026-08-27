//! Where Pallet keeps its data on each platform.
//!
//! Settings live in a hand-editable TOML file so they can be managed from
//! dotfiles; the library lives in SQLite next door. Captured frames are never
//! written to disk.
//!
//! | Platform | Config                              | Data                                     |
//! |----------|-------------------------------------|------------------------------------------|
//! | Linux    | `~/.config/pallet/`                 | `~/.local/share/pallet/`                 |
//! | Windows  | `%APPDATA%\Pallet\config\`          | `%APPDATA%\Pallet\data\`                 |
//! | macOS    | `~/Library/Application Support/...` | `~/Library/Application Support/...`      |

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{Error, Result};

/// Resolved on-disk locations for one Pallet installation.
#[derive(Debug, Clone)]
pub struct Paths {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl Paths {
    /// Resolve the platform's standard locations for Pallet.
    ///
    /// This only computes paths; nothing touches the filesystem until
    /// [`Paths::ensure_dirs`] is called.
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "Pallet", "Pallet").ok_or(Error::NoPlatformDirs)?;
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
        })
    }

    /// Build paths rooted at an explicit directory. Used by tests and by the
    /// `PALLET_HOME` override so a run can be fully sandboxed.
    pub fn rooted_at(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
        }
    }

    /// Resolve from the `PALLET_HOME` environment override if it is set,
    /// otherwise fall back to the platform defaults.
    pub fn from_env_or_discover() -> Result<Self> {
        match std::env::var_os("PALLET_HOME") {
            Some(root) if !root.is_empty() => Ok(Self::rooted_at(root)),
            _ => Self::discover(),
        }
    }

    /// The directory holding `config.toml`.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// The directory holding the SQLite library and exports.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The hand-editable settings file.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The SQLite database holding palettes, colours and pick history.
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("pallet.db")
    }

    /// Default destination for user-triggered exports.
    pub fn exports_dir(&self) -> PathBuf {
        self.data_dir.join("exports")
    }

    /// Create every directory Pallet writes to. Safe to call repeatedly.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [&self.config_dir, &self.data_dir, &self.exports_dir()] {
            std::fs::create_dir_all(dir).map_err(|source| Error::CreateDir {
                path: dir.clone(),
                source,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooted_paths_hang_off_the_given_directory() {
        let paths = Paths::rooted_at("/tmp/pallet-test");
        assert_eq!(
            paths.config_file(),
            Path::new("/tmp/pallet-test/config/config.toml")
        );
        assert_eq!(
            paths.database_file(),
            Path::new("/tmp/pallet-test/data/pallet.db")
        );
    }

    #[test]
    fn ensure_dirs_is_idempotent() {
        let root = std::env::temp_dir().join(format!("pallet-{}", std::process::id()));
        let paths = Paths::rooted_at(&root);

        paths.ensure_dirs().expect("first create");
        paths.ensure_dirs().expect("second create should not fail");

        assert!(paths.config_dir().is_dir());
        assert!(paths.exports_dir().is_dir());

        let _ = std::fs::remove_dir_all(&root);
    }
}

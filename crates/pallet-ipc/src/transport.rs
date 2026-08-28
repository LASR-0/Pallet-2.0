//! Where the picker listens, and how to reach it.
//!
//! A Unix socket in the runtime directory: it disappears when the session ends,
//! its permissions restrict it to the owning user, and it needs no port
//! allocation. Windows will use a named pipe at the same abstraction when that
//! platform lands.

use std::path::PathBuf;

use crate::codec::{Error, Result};

/// Where the picker's socket lives.
///
/// Prefers `$XDG_RUNTIME_DIR`, which is user-private and cleared on logout.
/// Falls back to a temporary directory keyed by user id when it is unset, as
/// happens in some minimal sessions and containers.
pub fn socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir).join("pallet").join("picker.sock");
    }

    // Safety: `getuid` is always safe; it reads a process property and cannot
    // fail or touch memory.
    let uid = unsafe { libc_getuid() };
    std::env::temp_dir()
        .join(format!("pallet-{uid}"))
        .join("picker.sock")
}

// Avoiding a `libc` dependency for one call.
unsafe extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

/// Create the socket's parent directory.
pub fn ensure_socket_dir() -> Result<PathBuf> {
    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(Error::Io)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_lives_under_the_runtime_directory_when_there_is_one() {
        // SAFETY: single-threaded test, and the value is restored below.
        let previous = std::env::var_os("XDG_RUNTIME_DIR");
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", "/run/user/4242") };

        assert_eq!(
            socket_path(),
            PathBuf::from("/run/user/4242/pallet/picker.sock")
        );

        match previous {
            Some(v) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", v) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }

    #[test]
    fn the_path_is_always_absolute_and_named_consistently() {
        let path = socket_path();
        assert!(path.is_absolute(), "{path:?}");
        assert!(path.ends_with("picker.sock"), "{path:?}");
    }
}

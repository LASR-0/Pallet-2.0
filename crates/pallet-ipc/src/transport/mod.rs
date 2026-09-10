//! Where the picker listens, and how to reach it.
//!
//! A local, per-user, session-scoped channel: a Unix socket under
//! `$XDG_RUNTIME_DIR` on Linux, a named pipe under `\\.\pipe\` on Windows.
//! Both vanish on their own when the process holding them exits, so neither
//! platform needs an explicit cleanup step after a picker crashes.
//!
//! [`Stream`], [`Listener`] and [`BindError`] are the same shape on every
//! platform, so the CLI, the app and the picker itself never branch on the
//! OS to talk to each other.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{BindError, Listener, Stream, ensure_socket_dir, socket_path};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{BindError, Listener, Stream, ensure_socket_dir, socket_path};

/// When a binary was last written, in seconds since the epoch.
///
/// Zero when that cannot be determined, which callers read as "unknown" and
/// therefore never as a mismatch.
pub fn build_stamp(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// The build stamp of the currently running executable.
pub fn own_build_stamp() -> u64 {
    std::env::current_exe().map_or(0, |p| build_stamp(&p))
}

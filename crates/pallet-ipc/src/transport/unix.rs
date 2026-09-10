//! Unix domain socket transport.

use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

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
pub fn ensure_socket_dir() -> io::Result<PathBuf> {
    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(path)
}

/// A duplex connection to the picker.
#[derive(Debug)]
pub struct Stream(UnixStream);

impl Stream {
    /// Connect to the picker's socket.
    pub fn connect() -> io::Result<Self> {
        UnixStream::connect(socket_path()).map(Stream)
    }

    /// Bound how long a read may block.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(timeout)
    }

    /// Bound how long a write may block.
    pub fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_write_timeout(timeout)
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Why binding the picker's listener failed.
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// Another picker already holds the socket.
    #[error("a picker is already running at {}", .0.display())]
    AlreadyRunning(PathBuf),
    /// Something else went wrong.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The picker's end of the socket.
pub struct Listener(UnixListener);

impl Listener {
    /// Bind the picker's socket, clearing one left behind by a crash.
    ///
    /// A socket file left by a crash would otherwise block binding forever:
    /// only remove it when nothing is listening, so a running picker is never
    /// displaced by a second one starting.
    pub fn bind() -> Result<Self, BindError> {
        let socket = ensure_socket_dir()?;

        if socket.exists() {
            match UnixStream::connect(&socket) {
                Ok(_) => return Err(BindError::AlreadyRunning(socket)),
                Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                    tracing::info!("removing a stale socket from a previous run");
                    std::fs::remove_file(&socket)?;
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(Listener(UnixListener::bind(&socket)?))
    }

    /// Accept connections, one at a time, for as long as the caller keeps
    /// asking.
    pub fn incoming(&self) -> impl Iterator<Item = io::Result<Stream>> + '_ {
        self.0.incoming().map(|r| r.map(Stream))
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Ours to clean up; leaving it behind would make the next start
        // think a picker is still running.
        let _ = std::fs::remove_file(socket_path());
    }
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

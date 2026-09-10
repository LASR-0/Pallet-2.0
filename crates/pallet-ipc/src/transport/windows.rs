//! Windows named-pipe transport, mirroring the Unix socket abstraction.
//!
//! A pipe under `\\.\pipe\` plays the same role `$XDG_RUNTIME_DIR` plays on
//! Linux: named per user so two accounts on the same machine cannot collide,
//! and gone the instant the last handle to it closes — a crashed picker
//! leaves nothing behind to clean up, unlike a stale Unix socket file.
//!
//! Only one instance of the pipe ever exists (`nMaxInstances = 1`), so the
//! picker really does serve one connection at a time: a second client finds
//! the pipe busy and fails immediately rather than queuing, which is why
//! nothing here needs a read timeout the way the Unix side's probes do.

use std::fs::File;
use std::io;
use std::os::windows::io::FromRawHandle;
use std::path::PathBuf;
use std::time::Duration;

use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_PIPE_CONNECTED, GetLastError, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, FILE_SHARE_MODE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::core::PCWSTR;

/// Where the picker listens.
pub fn socket_path() -> PathBuf {
    PathBuf::from(format!(r"\\.\pipe\pallet-{}-picker", user_tag()))
}

/// A per-account tag, so two users on the same machine get different pipes.
fn user_tag() -> String {
    std::env::var("USERNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "user".into())
}

/// No directory to create: the pipe namespace has no filesystem hierarchy.
pub fn ensure_socket_dir() -> io::Result<PathBuf> {
    Ok(socket_path())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_win32(e: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error((e.code().0 as u32 & 0xFFFF) as i32)
}

/// A duplex connection to the picker.
#[derive(Debug)]
pub struct Stream(File);

impl Stream {
    /// Connect to the picker's pipe.
    pub fn connect() -> io::Result<Self> {
        let name = wide(&socket_path().to_string_lossy());
        let handle = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map_err(from_win32)?;

        Ok(Stream(unsafe { File::from_raw_handle(handle.0) }))
    }

    /// No-op here: see the module docs for why a Windows pick never blocks on
    /// a busy picker the way a Unix connection can.
    pub fn set_read_timeout(&mut self, _timeout: Option<Duration>) -> io::Result<()> {
        Ok(())
    }

    /// See [`Stream::set_read_timeout`].
    pub fn set_write_timeout(&mut self, _timeout: Option<Duration>) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for Stream {
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
    /// Another picker already holds the pipe.
    #[error("a picker is already running at {}", .0.display())]
    AlreadyRunning(PathBuf),
    /// Something else went wrong.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The picker's end of the pipe.
///
/// A named pipe instance serves exactly one client and is then spent: each
/// [`Listener::incoming`] iteration creates a fresh instance, waits for it to
/// be connected, and hands it over — the Windows shape of
/// `UnixListener::incoming`'s one-stream-per-client iterator.
#[derive(Debug)]
pub struct Listener {
    name: Vec<u16>,
    /// The instance created by [`Listener::bind`], reused for the first
    /// accept so proving the name is free and actually claiming it happen in
    /// one step, with nothing dropped in between for a second picker to slip
    /// into.
    first: std::cell::RefCell<Option<HANDLE>>,
}

impl Listener {
    /// Claim the picker's pipe name, clearing on its own if the picker that
    /// held it before has since crashed.
    ///
    /// `FILE_FLAG_FIRST_PIPE_INSTANCE` makes creation itself fail when another
    /// process already owns this name — the Windows equivalent of the Unix
    /// side's stale-socket probe, except there is nothing to clean up
    /// afterwards: a crashed picker's instance is gone the moment its process
    /// exits, so binding again just succeeds.
    pub fn bind() -> Result<Self, BindError> {
        let path = socket_path();
        let name = wide(&path.to_string_lossy());

        let handle = create_instance(&name, true).map_err(move |e| {
            if e.raw_os_error() == Some(ERROR_ACCESS_DENIED.0 as i32) {
                BindError::AlreadyRunning(path)
            } else {
                BindError::Io(e)
            }
        })?;

        Ok(Listener {
            name,
            first: std::cell::RefCell::new(Some(handle)),
        })
    }

    /// Accept connections, one at a time, for as long as the caller keeps
    /// asking.
    pub fn incoming(&self) -> impl Iterator<Item = io::Result<Stream>> + '_ {
        std::iter::from_fn(move || Some(self.accept_one()))
    }

    fn accept_one(&self) -> io::Result<Stream> {
        let handle = match self.first.borrow_mut().take() {
            Some(handle) => handle,
            None => create_instance(&self.name, false)?,
        };

        let connected = unsafe { ConnectNamedPipe(handle, None) };
        if let Err(e) = connected {
            // A client that connected in the race between creating the
            // instance and calling this counts as success, not an error.
            if e.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                return Err(from_win32(e));
            }
        }

        Ok(Stream(unsafe { File::from_raw_handle(handle.0) }))
    }
}

fn create_instance(name: &[u16], first: bool) -> io::Result<HANDLE> {
    let mut open_mode = PIPE_ACCESS_DUPLEX;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }

    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            None,
        )
    };

    // `CreateNamedPipeW` reports failure through `GetLastError` and an
    // invalid handle rather than through a `Result`, unlike most of this
    // module's other calls.
    if handle.is_invalid() {
        Err(io::Error::from_raw_os_error(
            unsafe { GetLastError() }.0 as i32,
        ))
    } else {
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pipe_name_is_tagged_with_the_account() {
        let path = socket_path();
        let s = path.to_string_lossy();
        assert!(s.starts_with(r"\\.\pipe\pallet-"), "{s}");
        assert!(s.ends_with("-picker"), "{s}");
    }

    #[test]
    fn the_path_is_named_consistently() {
        assert_eq!(socket_path(), socket_path());
    }
}

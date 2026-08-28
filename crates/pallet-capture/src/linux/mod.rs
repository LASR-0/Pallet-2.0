//! Linux capture backends.

use crate::ScreenCapture;
use crate::error::{Error, Result};

pub mod wayland;
pub mod x11;

/// Choose a backend for the current session.
///
/// Wayland via `wlr-screencopy` is preferred: no permission prompt, one round
/// trip, and the compositor hands back real device pixels. X11 is the fallback.
pub fn open() -> Result<Box<dyn ScreenCapture>> {
    let mut problems = Vec::new();

    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        match wayland::WaylandCapture::new() {
            Ok(backend) => return Ok(Box::new(backend)),
            Err(e) => problems.push(format!("wlr-screencopy: {e}")),
        }
    }

    if std::env::var_os("DISPLAY").is_some() {
        match x11::X11Capture::new() {
            Ok(backend) => return Ok(Box::new(backend)),
            Err(e) => problems.push(format!("x11: {e}")),
        }
    }

    Err(Error::NoBackend(if problems.is_empty() {
        "neither WAYLAND_DISPLAY nor DISPLAY is set".into()
    } else {
        problems.join("; ")
    }))
}

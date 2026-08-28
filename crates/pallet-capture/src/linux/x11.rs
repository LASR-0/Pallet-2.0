//! X11 capture via `XGetImage`.
//!
//! Simpler than Wayland in every respect: X11 lets any client read the root
//! window, so there is no protocol negotiation and no permission model. This
//! is the fallback for X sessions and for Wayland compositors without
//! `wlr-screencopy` running Pallet through Xwayland.

use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat};
use x11rb::rust_connection::RustConnection;

use crate::ScreenCapture;
use crate::error::{Error, Result};
use crate::frame::{Capture, Frame, PixelFormat};
use crate::monitor::{ColorProfile, Monitor, Transform};

/// A live X11 connection.
pub struct X11Capture {
    connection: RustConnection,
    screen_index: usize,
}

impl std::fmt::Debug for X11Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("X11Capture")
            .field("screen", &self.screen_index)
            .finish()
    }
}

impl X11Capture {
    /// Connect to the display named by `DISPLAY`.
    pub fn new() -> Result<Self> {
        let (connection, screen_index) = x11rb::connect(None)
            .map_err(|e| Error::NoBackend(format!("could not reach the X server: {e}")))?;
        Ok(Self {
            connection,
            screen_index,
        })
    }

    fn root_geometry(&self) -> Result<(u32, u32)> {
        let screen = &self.connection.setup().roots[self.screen_index];
        Ok((
            u32::from(screen.width_in_pixels),
            u32::from(screen.height_in_pixels),
        ))
    }

    /// X11 has no per-output capture without RANDR, so the whole root window
    /// is treated as one monitor. Multi-monitor X sessions therefore come back
    /// as a single wide frame, which the loupe handles fine.
    fn root_monitor(&self) -> Result<Monitor> {
        let (width, height) = self.root_geometry()?;
        Ok(Monitor {
            id: "X11".into(),
            name: "X11 root window".into(),
            x: 0,
            y: 0,
            width,
            height,
            scale: 1.0,
            transform: Transform::Normal,
            profile: ColorProfile::Unknown,
        })
    }
}

impl ScreenCapture for X11Capture {
    fn monitors(&mut self) -> Result<Vec<Monitor>> {
        Ok(vec![self.root_monitor()?])
    }

    fn capture_all(&mut self) -> Result<Capture> {
        Ok(Capture {
            frames: vec![self.capture_monitor("X11")?],
        })
    }

    fn capture_monitor(&mut self, _id: &str) -> Result<Frame> {
        let monitor = self.root_monitor()?;
        let root = self.connection.setup().roots[self.screen_index].root;

        let image = self
            .connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                root,
                0,
                0,
                monitor.width as u16,
                monitor.height as u16,
                !0,
            )
            .map_err(|e| Error::Refused(e.to_string()))?
            .reply()
            .map_err(|e| Error::Refused(e.to_string()))?;

        Ok(Frame {
            stride: monitor.width as usize * 4,
            monitor,
            data: image.data,
            // X11 truecolour visuals are little-endian BGRX in memory.
            format: PixelFormat::Bgrx8888,
        })
    }

    fn backend_name(&self) -> &'static str {
        "x11"
    }
}

//! Cross-platform screen capture for Pallet.
//!
//! Everything here works in **physical pixels**. Logical coordinates are a
//! compositor invention that shift with display scaling; the loupe has to
//! address exact device pixels or a picked colour is the wrong colour. The
//! trait boundary enforces that so no backend can quietly leak logical units.
//!
//! Every frame also carries the [`ColorProfile`] of the display it came from,
//! because a pixel on a P3 monitor is not an sRGB pixel and pretending
//! otherwise is unfixable later.
//!
//! Frames live in memory only and are never written to disk.

#![warn(missing_docs)]

pub mod error;
pub mod frame;
pub mod monitor;

#[cfg(target_os = "linux")]
pub mod linux;

pub use error::{Error, Result};
pub use frame::{Capture, Frame, PixelFormat};
pub use monitor::{ColorProfile, Monitor, Transform};

/// A source of screen pixels.
///
/// Implementors hold whatever connection the platform needs, so a picker can
/// build one once at start-up and keep it warm rather than paying connection
/// cost on every pick.
pub trait ScreenCapture: std::fmt::Debug + Send {
    /// The displays currently connected.
    ///
    /// Re-enumerated on each call, because a resident picker outlives any
    /// particular monitor arrangement. **Order carries no meaning** and can
    /// change when a display is unplugged and reconnected; identify a monitor
    /// by [`Monitor::id`], or find one geometrically with
    /// [`Monitor::contains`].
    fn monitors(&mut self) -> Result<Vec<Monitor>>;

    /// Freeze every monitor.
    ///
    /// All frames should come from as close to the same instant as the
    /// platform allows: the loupe presents them as one frozen desktop, and
    /// visible tearing between monitors would give that away.
    ///
    /// Frame order is unspecified, for the same reason as [`Self::monitors`].
    /// An empty [`Capture`] is valid and means every display was disconnected.
    fn capture_all(&mut self) -> Result<Capture>;

    /// Freeze a single monitor by id.
    fn capture_monitor(&mut self, id: &str) -> Result<Frame>;

    /// A short name for the active backend, for diagnostics.
    fn backend_name(&self) -> &'static str;
}

/// Open the best capture backend for the current environment.
///
/// On Linux this prefers `wlr-screencopy` under Wayland, because it needs no
/// permission prompt and costs a single round trip. The portal is the fallback
/// for compositors without it (and is mandatory under Flatpak); X11 is tried
/// last.
pub fn open() -> Result<Box<dyn ScreenCapture>> {
    #[cfg(target_os = "linux")]
    {
        linux::open()
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(Error::NoBackend(format!(
            "{} support lands in a later milestone",
            std::env::consts::OS
        )))
    }
}

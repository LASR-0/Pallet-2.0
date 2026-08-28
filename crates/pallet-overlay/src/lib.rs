//! The picking overlay: a frozen desktop, a loupe, and the interaction that
//! turns a cursor position into a colour.
//!
//! Split three ways so that only the parts that genuinely need hardware
//! require it:
//!
//! * [`session`] is the interaction as a pure state machine — no GPU, no
//!   compositor, fully unit-tested.
//! * The renderer draws that state with `wgpu`.
//! * The platform layer supplies a surface to draw on.

#![warn(missing_docs)]

pub mod error;

#[cfg(target_os = "linux")]
pub mod linux;
pub mod render;
pub mod session;

pub use error::{Error, Result};

/// Freeze the desktop and let the user pick a colour.
///
/// Blocks until the pick is committed or abandoned.
pub fn run(
    capture: pallet_capture::Capture,
    zoom: u32,
    average_size: u32,
) -> Result<session::Outcome> {
    #[cfg(target_os = "linux")]
    {
        linux::run_picker(capture, zoom, average_size)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (capture, zoom, average_size);
        Err(Error::Compositor(format!(
            "{} overlay support lands in a later milestone",
            std::env::consts::OS
        )))
    }
}
pub use render::{LoupeView, Renderer, Screen};
pub use session::{Input, Outcome, Session};

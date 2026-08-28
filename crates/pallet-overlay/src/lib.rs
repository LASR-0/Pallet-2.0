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

/// A warm picker: a compositor connection and a live GPU device, kept between
/// picks so no pick pays the ~220 ms of GPU initialisation.
#[cfg(target_os = "linux")]
pub type Context = linux::PickerContext;

/// Build a reusable picking context. Do this once, at start-up.
#[cfg(target_os = "linux")]
pub fn context() -> Result<Context> {
    linux::PickerContext::new()
}

/// Freeze the desktop and let the user pick a colour.
///
/// Blocks until the pick is committed or abandoned.
#[cfg(target_os = "linux")]
pub fn run(
    context: &Context,
    capture: pallet_capture::Capture,
    zoom: u32,
    average_size: u32,
) -> Result<session::Outcome> {
    linux::run_picker_with(context, capture, zoom, average_size)
}
pub use render::{LoupeView, Renderer, Screen};
pub use session::{Input, Outcome, Session};

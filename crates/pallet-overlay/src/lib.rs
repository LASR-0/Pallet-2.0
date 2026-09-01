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
pub mod hud;

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

/// A palette to gather in one pass.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    /// Colours the caller already holds. Shown as filled slots, not returned.
    pub collected: Vec<pallet_color::Color>,
    /// How many slots the palette has in total.
    pub target: usize,
}

impl Palette {
    /// How many more colours this pass still needs.
    ///
    /// Zero when the caller already holds enough, in which case there is
    /// nothing to gather and the pass is a single pick.
    pub fn remaining(&self) -> usize {
        self.target.saturating_sub(self.collected.len()).max(1)
    }
}

/// Freeze the desktop and let the user pick.
///
/// Blocks until the pick finishes or is abandoned. `palette` gathers a whole
/// set in one pass, keeping the overlay up between colours; `None` is an
/// ordinary single pick and hides the HUD's tray.
#[cfg(target_os = "linux")]
pub fn run(
    context: &Context,
    capture: pallet_capture::Capture,
    zoom: u32,
    average_size: u32,
    keys: LoupeKeys,
    palette: Option<Palette>,
) -> Result<session::Outcome> {
    linux::run_picker_with(context, capture, zoom, average_size, keys, palette)
}

pub use hud::chrome::Tray;
/// The keys the loupe answers to.
#[cfg(target_os = "linux")]
pub use linux::layer::LoupeKeys;
pub use render::{ChromeGpu, LoupeView, Renderer, Screen};
pub use session::Taken;
pub use session::{Input, Outcome, Session};

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
pub mod render;
pub mod session;

pub use error::{Error, Result};
pub use render::{LoupeView, Renderer};
pub use session::{Input, Outcome, Session};

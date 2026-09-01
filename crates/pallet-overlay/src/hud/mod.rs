//! The overlay's chrome: everything drawn over the frozen screen that is not
//! the loupe itself.
//!
//! The loupe is a fragment shader, because magnifying a frozen frame is a
//! per-pixel sampling problem that belongs on the GPU. Its readout pill, the
//! instruction line, the zoom badge and the multi-pick tray are the opposite:
//! small, mostly static, and full of text. Those are rasterised on the CPU
//! into RGBA bitmaps and composited over the shader's output, which avoids a
//! glyph atlas and a second GPU pipeline for four short strings.

pub mod chrome;
pub mod paint;
pub mod text;

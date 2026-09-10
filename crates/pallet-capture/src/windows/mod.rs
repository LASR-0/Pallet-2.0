//! Windows capture backend.

use crate::ScreenCapture;
use crate::error::Result;

pub mod dpi;
pub mod dxgi;

/// Open the DXGI Desktop Duplication backend.
///
/// There is no fallback to choose between: Desktop Duplication has covered
/// every desktop Windows session since Windows 8, unlike Linux's mix of
/// compositors with and without `wlr-screencopy`.
pub fn open() -> Result<Box<dyn ScreenCapture>> {
    Ok(Box::new(dxgi::DxgiCapture::new()?))
}

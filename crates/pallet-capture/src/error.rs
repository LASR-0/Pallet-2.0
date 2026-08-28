//! Errors from screen capture.

/// Why a capture did not happen.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No capture backend works in this environment.
    #[error("no screen capture backend is available: {0}")]
    NoBackend(String),

    /// The compositor or display server refused or failed the request.
    #[error("the display server refused the capture: {0}")]
    Refused(String),

    /// The compositor lacks a protocol Pallet needs.
    #[error("{0} is not supported by this compositor")]
    Unsupported(&'static str),

    /// A monitor id did not match any connected display.
    #[error("no monitor named `{0}`")]
    UnknownMonitor(String),

    /// The pixel format the compositor handed back is not one we decode.
    #[error("unsupported pixel format: {0:?}")]
    UnsupportedFormat(crate::frame::PixelFormat),

    /// A coordinate fell outside every captured monitor.
    #[error("point ({x}, {y}) is not on any captured monitor")]
    OutOfBounds {
        /// Global x, in physical pixels.
        x: i32,
        /// Global y, in physical pixels.
        y: i32,
    },

    /// Something in the OS layer failed.
    #[error("capture failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience alias for fallible capture operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

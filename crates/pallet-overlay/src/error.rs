//! Errors from the overlay.

/// Why the overlay could not run.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No usable GPU adapter or device.
    #[error("no usable GPU: {0}")]
    NoGpu(String),

    /// A draw was requested before any frame was uploaded.
    #[error("no frozen frame has been uploaded")]
    NoFrame,

    /// A frame with no pixels cannot be shown.
    #[error("the captured frame was empty")]
    EmptyFrame,

    /// The compositor refused or failed a request.
    #[error("compositor error: {0}")]
    Compositor(String),

    /// The compositor lacks a protocol the overlay needs.
    #[error("{0} is not supported by this compositor")]
    Unsupported(&'static str),

    /// Capture failed.
    #[error(transparent)]
    Capture(#[from] pallet_capture::Error),
}

/// Convenience alias for fallible overlay operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

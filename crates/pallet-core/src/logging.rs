//! Shared tracing setup.

use tracing_subscriber::EnvFilter;

/// Install a tracing subscriber driven by `PALLET_LOG` (falling back to
/// `RUST_LOG`, then to `info`).
///
/// Calling this more than once in a process is harmless; later calls are
/// ignored because a global subscriber is already installed.
pub fn init(default: &str) {
    let filter = EnvFilter::try_from_env("PALLET_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new(default));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

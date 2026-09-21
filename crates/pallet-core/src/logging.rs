//! Shared tracing setup.

use tracing_subscriber::EnvFilter;

/// Install a tracing subscriber driven by `PALLET_LOG` (falling back to
/// `RUST_LOG`, then to `info`).
///
/// Set `PALLET_LOG_FILE` to a path to write there instead of to stderr.
///
/// That exists for the picker, which is otherwise undiagnosable in the
/// configuration users actually run: the app spawns it with all three standard
/// streams set to null, so anything it reports about a pick is discarded. A
/// bug that only appears when the app launches the picker — rather than when a
/// developer runs it in a terminal — leaves no trace at all without this. The
/// variable is inherited by the spawned picker, so setting it once before
/// starting the app captures both processes.
///
/// Calling this more than once in a process is harmless; later calls are
/// ignored because a global subscriber is already installed.
pub fn init(default: &str) {
    let filter = EnvFilter::try_from_env("PALLET_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new(default));

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false);

    // Appended to, not truncated, so the app and the picker it spawns can
    // share one file and a run does not erase the run that diagnosed it.
    let file = std::env::var_os("PALLET_LOG_FILE").and_then(|path| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| eprintln!("could not open PALLET_LOG_FILE: {e}"))
            .ok()
    });

    match file {
        // No colour codes in a file: these are read in an editor, and escape
        // sequences in the middle of every line make that miserable.
        Some(file) => {
            let _ = builder.with_ansi(false).with_writer(file).try_init();
        }
        // Diagnostics go to stderr, never stdout. `pallet pick` writes the
        // picked hex to stdout and people pipe it: `HEX=$(pallet pick)` must
        // not come back with log lines glued to it.
        None => {
            let _ = builder.with_writer(std::io::stderr).try_init();
        }
    }
}

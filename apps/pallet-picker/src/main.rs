//! The resident overlay helper.
//!
//! Runs hidden with a warm GPU context so that freeze-to-loupe costs about one
//! frame rather than the 100-300 ms a cold `wgpu` initialisation would.
//! Kept in its own process because Tauri's event loop and the overlay's cannot
//! share a main thread, and because an overlay panic must not take the library
//! down with it.

use anyhow::Result;
use pallet_core::logging;

fn main() -> Result<()> {
    logging::init("info");
    anyhow::bail!("the picker helper lands in M4/M5")
}

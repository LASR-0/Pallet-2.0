//! Per-monitor DPI awareness.
//!
//! Every coordinate Pallet works with — DXGI output geometry, cursor
//! position, window placement — must agree on whether it is in real device
//! pixels or DPI-virtualised ones. A process that has not opted in gets the
//! latter from GDI and User32, silently misaligning the loupe on anything but
//! a 100% scaled display, so this runs before any of that is read.

use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

/// Declare the process per-monitor DPI aware.
///
/// Safe to call more than once, and from more than one crate in the same
/// process: Windows refuses a second call (or one after the manifest already
/// set an awareness), which is indistinguishable here from having nothing
/// left to do.
pub fn ensure_aware() {
    // SAFETY: takes no pointers and has no failure mode that leaves process
    // state inconsistent; a refusal just means an earlier call, or the
    // executable's manifest, already won.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

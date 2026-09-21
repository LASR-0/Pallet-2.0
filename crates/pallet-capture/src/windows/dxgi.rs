//! Capture via DXGI Desktop Duplication.
//!
//! The API every Windows screen recorder builds on: `AcquireNextFrame` hands
//! back the whole desktop for one output as a GPU texture, already composited,
//! with no permission prompt and one round trip. Available on every desktop
//! session since Windows 8, so — unlike Linux — there is no fallback backend
//! to choose between.
//!
//! The texture is in the panel's native, un-rotated pixel grid — confirmed by
//! capturing a portrait monitor and finding its buffer 3840×2160, not the
//! 2160×3840 its desktop geometry reports — the same split between a raw
//! framebuffer and a rotated desktop layout the Wayland backend's `Transform`
//! exists for, and [`monitor_from`] leans on exactly that rather than
//! inventing a second way to describe it.
//!
//! One quirk shapes most of this module: Desktop Duplication is a *diff* API.
//! `AcquireNextFrame` only returns once the desktop has actually changed
//! since the last call, and times out with nothing at all if it has not. An
//! idle desktop is the common case, not the exception, so every output keeps
//! the pixels from its last successful acquire and serves those on a
//! timeout — the picker wants "what the screen looks like right now", and an
//! unchanged screen still has an answer to that.

use std::collections::HashMap;

use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_MODE_ROTATION_ROTATE90, DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    DXGI_OUTPUT_DESC, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
    IDXGIResource,
};
use windows::core::Interface;

use crate::ScreenCapture;
use crate::error::{Error, Result};
use crate::frame::{Capture, Frame, PixelFormat};
use crate::monitor::{ColorProfile, Monitor, Transform};

use super::dpi;

/// The pixels DXGI handed back last time, kept for the "nothing changed"
/// case `AcquireNextFrame` reports as a timeout rather than a fresh frame.
struct CachedFrame {
    data: Vec<u8>,
    stride: usize,
}

/// One display, and the duplication session capturing it.
struct Target {
    monitor: Monitor,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    last_frame: Option<CachedFrame>,
}

/// Desktop Duplication capture, one session per connected display.
pub struct DxgiCapture {
    targets: Vec<Target>,
}

impl std::fmt::Debug for DxgiCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DxgiCapture")
            .field("outputs", &self.targets.len())
            .finish()
    }
}

impl DxgiCapture {
    /// Enumerate every output attached to the desktop and open a duplication
    /// session on each.
    pub fn new() -> Result<Self> {
        dpi::ensure_aware();
        Ok(Self {
            targets: enumerate_targets()?,
        })
    }

    fn target_mut(&mut self, id: &str) -> Result<&mut Target> {
        self.targets
            .iter_mut()
            .find(|t| t.monitor.id == id)
            .ok_or_else(|| Error::UnknownMonitor(id.to_string()))
    }
}

impl ScreenCapture for DxgiCapture {
    fn monitors(&mut self) -> Result<Vec<Monitor>> {
        Ok(self.targets.iter().map(|t| t.monitor.clone()).collect())
    }

    fn capture_all(&mut self) -> Result<Capture> {
        let mut frames = Vec::with_capacity(self.targets.len());
        for i in 0..self.targets.len() {
            frames.push(capture_index(self, i)?);
        }
        Ok(Capture { frames })
    }

    fn capture_monitor(&mut self, id: &str) -> Result<Frame> {
        let index = self
            .targets
            .iter()
            .position(|t| t.monitor.id == id)
            .ok_or_else(|| Error::UnknownMonitor(id.to_string()))?;
        capture_index(self, index)
    }

    fn backend_name(&self) -> &'static str {
        "dxgi"
    }
}

/// Capture one output by index, rebuilding every session once and retrying
/// if Desktop Duplication has invalidated itself.
///
/// `DXGI_ERROR_ACCESS_LOST` fires for reasons that have nothing to do with a
/// display actually disappearing — a lock screen, a UAC prompt, a mode
/// change — so the fix is always the same: reopen every duplication session
/// and try once more, rather than trying to tell those cases apart.
fn capture_index(capture: &mut DxgiCapture, index: usize) -> Result<Frame> {
    match capture_from(&mut capture.targets[index]) {
        Ok(frame) => Ok(frame),
        Err(Error::Refused(_)) => {
            let id = capture.targets[index].monitor.id.clone();
            // Dropped before re-enumerating, not just replaced: DXGI allows
            // only one duplication interface per output at a time, and
            // `enumerate_targets` re-running while these old handles are
            // still alive makes it lose exactly the race it exists to
            // recover from — `DuplicateOutput` fails with "the parameter is
            // incorrect" against this same process's own still-open
            // session, silently dropping that output from the new list.
            capture.targets.clear();
            capture.targets = enumerate_targets()?;
            let target = capture.target_mut(&id)?;
            capture_from(target)
        }
        Err(e) => Err(e),
    }
}

fn capture_from(target: &mut Target) -> Result<Frame> {
    let keep = |target: &mut Target, cached: CachedFrame| {
        let frame = Frame {
            monitor: target.monitor.clone(),
            data: cached.data.clone(),
            stride: cached.stride,
            format: PixelFormat::Bgra8888,
        };
        target.last_frame = Some(cached);
        frame
    };

    match acquire(target, true) {
        Ok(cached) => Ok(keep(target, cached)),
        Err(TimedOut) => match &target.last_frame {
            Some(cached) => Ok(Frame {
                monitor: target.monitor.clone(),
                data: cached.data.clone(),
                stride: cached.stride,
                format: PixelFormat::Bgra8888,
            }),
            // Nothing has ever been captured on this output and nothing has
            // painted on it either, so there is no previous frame to serve
            // and no fresh one to be had. Take whatever DXGI offers.
            //
            // That may be the pointer-only frame `acquire` was refusing —
            // which is to say it may be black. It is still the better of the
            // two endings: a blank overlay a second pick fixes, rather than
            // an error where the user asked for a colour. Reaching here at
            // all means an output that has not painted a single pixel in
            // 400ms on the first capture ever taken of it.
            None => {
                tracing::warn!(
                    monitor = %target.monitor.id,
                    "no desktop image yet; the frozen frame may be blank"
                );
                match acquire(target, false) {
                    Ok(cached) => Ok(keep(target, cached)),
                    Err(TimedOut) => Err(Error::Refused(
                        "the desktop did not produce a frame in time".into(),
                    )),
                    Err(Failed(e)) => Err(e),
                }
            }
        },
        Err(Failed(e)) => Err(e),
    }
}

/// [`acquire`]'s outcome, kept separate from [`Error`] because a timeout is
/// routine here, not a failure to report to a caller.
enum AcquireOutcome {
    TimedOut,
    Failed(Error),
}
use AcquireOutcome::{Failed, TimedOut};

impl From<Error> for AcquireOutcome {
    fn from(e: Error) -> Self {
        Failed(e)
    }
}

/// How long [`acquire`] keeps asking for a frame that actually contains a
/// desktop image before giving up and letting the caller fall back.
///
/// Generous because the cost of running out is a black screenshot, and cheap
/// because it is only ever reached on a desktop that is genuinely not
/// painting — in which case the cached frame it falls back to is still a true
/// picture of a screen that has not changed.
const FRESH_DEADLINE: std::time::Duration = std::time::Duration::from_millis(400);

/// Ask Desktop Duplication for the next frame and read it back to system
/// memory.
///
/// # Why this loops
///
/// `AcquireNextFrame` succeeding does not mean it handed over a picture of
/// the desktop. It reports *two* kinds of update through `frame_info`, and
/// only one of them comes with pixels: when the pointer moved but the desktop
/// image did not change, it returns `S_OK` with `LastPresentTime` set to zero
/// and a texture whose contents Microsoft's documentation explicitly says not
/// to process. Copying it out anyway yields a black frame.
///
/// A duplication session that has just been opened answers its very first
/// call that way, which is how this reached a user: every pick worked except
/// the first one after the picker started, which is exactly when the session
/// is new — and it was the monitor showing the *unchanged* half of the
/// desktop that came back black, while a monitor something had just moved on
/// captured fine.
///
/// `require_fresh` is what distinguishes the two callers: the pick wants a
/// real desktop image and can wait a little for one, while the last-resort
/// path in [`capture_from`] has nothing cached to fall back on and would
/// rather have whatever DXGI will give it than fail outright.
fn acquire(
    target: &Target,
    require_fresh: bool,
) -> std::result::Result<CachedFrame, AcquireOutcome> {
    let until = std::time::Instant::now() + FRESH_DEADLINE;

    let resource = loop {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;

        // SAFETY: all three out-parameters are plain stack values the call
        // fills in; `resource` starts `None`, which is what DXGI requires.
        let acquired = unsafe {
            target
                .duplication
                .AcquireNextFrame(500, &mut frame_info, &mut resource)
        };

        if let Err(e) = &acquired {
            if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                return Err(TimedOut);
            }
            if e.code() == DXGI_ERROR_ACCESS_LOST {
                return Err(Error::Refused("duplication session invalidated".into()).into());
            }
            return Err(Error::Refused(format!("AcquireNextFrame: {e}")).into());
        }

        if !require_fresh || frame_info.LastPresentTime != 0 {
            break resource;
        }

        // Pointer-only update: no pixels came with it. Hand the frame back —
        // holding one blocks the next acquire — and ask again.
        // SAFETY: a frame was successfully acquired immediately above.
        unsafe {
            let _ = target.duplication.ReleaseFrame();
        }

        if std::time::Instant::now() >= until {
            // Out of patience rather than out of luck: the desktop really is
            // not painting. `capture_from` serves the last frame this output
            // produced, which is what an unchanged screen still looks like.
            return Err(TimedOut);
        }
    };

    let resource = resource
        .ok_or_else(|| Error::Refused("DXGI reported a frame but gave no resource".into()))?;
    // Desktop Duplication always hands back a `ID3D11Texture2D`; anything
    // else would mean a driver has stopped following the contract DXGI
    // documents for this call.
    let texture: ID3D11Texture2D = resource
        .cast()
        .map_err(|e| Error::Refused(format!("unexpected duplication resource: {e}")))?;

    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: `desc` is a plain out-parameter the call fills in.
    unsafe { texture.GetDesc(&mut desc) };

    let staging_desc = D3D11_TEXTURE2D_DESC {
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
        ..desc
    };

    let mut staging: Option<ID3D11Texture2D> = None;
    // SAFETY: `staging` starts `None` as the call requires; `None` for the
    // subresource data leaves the texture uninitialised, which is fine since
    // it is filled by the `CopyResource` below rather than at creation.
    unsafe {
        target
            .device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging))
    }
    .map_err(|e| Error::Refused(format!("staging texture: {e}")))?;
    let staging = staging.ok_or_else(|| Error::Refused("no staging texture created".into()))?;

    // SAFETY: both textures come from the same device and have matching
    // dimensions and format, since `staging_desc` was derived from `desc`.
    unsafe { target.context.CopyResource(&staging, &texture) };

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    // SAFETY: `mapped` is an out-parameter; the staging texture was just
    // created with `CPU_ACCESS_READ`, which `Map` requires for `MAP_READ`.
    unsafe {
        target
            .context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
    }
    .map_err(|e| Error::Refused(format!("mapping the staging texture: {e}")))?;

    let stride = mapped.RowPitch as usize;
    let height = desc.Height as usize;
    let mut data = vec![0u8; stride * height];
    // SAFETY: `mapped.pData` is valid for `stride * height` bytes for as
    // long as the map above is held, which outlives this copy.
    unsafe {
        std::ptr::copy_nonoverlapping(mapped.pData.cast::<u8>(), data.as_mut_ptr(), data.len());
    }

    // SAFETY: unmapping a subresource that was just mapped on the same
    // context.
    unsafe { target.context.Unmap(&staging, 0) };
    // Releasing the frame is best-effort: a failure here does not change the
    // pixels already copied out above, and the next `AcquireNextFrame` will
    // itself fail loudly if the session is truly broken.
    unsafe {
        let _ = target.duplication.ReleaseFrame();
    }

    Ok(CachedFrame { data, stride })
}

/// Enumerate every output attached to the desktop, opening a Desktop
/// Duplication session on each.
///
/// A `ID3D11Device` is created once per adapter rather than once per output,
/// since `DuplicateOutput` only requires that the device and the output share
/// an adapter — a multi-GPU desktop would otherwise pay for a device on the
/// same card several times over.
fn enumerate_targets() -> Result<Vec<Target>> {
    // SAFETY: no pointers in, and the returned factory owns its own state.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .map_err(|e| Error::NoBackend(format!("DXGI factory: {e}")))?;

    let mut devices: HashMap<u64, (ID3D11Device, ID3D11DeviceContext)> = HashMap::new();
    let mut targets = Vec::new();

    for adapter_index in 0.. {
        // SAFETY: `adapter_index` is bounds-checked by DXGI itself, which
        // reports `DXGI_ERROR_NOT_FOUND` once the index runs off the end.
        let adapter: IDXGIAdapter1 = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(a) => a,
            Err(_) => break,
        };

        let luid = unsafe { adapter.GetDesc1() }
            .map(|d| (d.AdapterLuid.LowPart as u64) | ((d.AdapterLuid.HighPart as u64) << 32))
            .unwrap_or(adapter_index as u64);

        for output_index in 0.. {
            // SAFETY: same bounds-checking contract as `EnumAdapters1`.
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(o) => o,
                Err(_) => break,
            };
            let output1: IDXGIOutput1 = match output.cast() {
                Ok(o) => o,
                Err(_) => continue,
            };

            // SAFETY: no pointers in; the interface owns its own state.
            let desc = match unsafe { output1.GetDesc() } {
                Ok(d) if d.AttachedToDesktop.as_bool() => d,
                _ => continue,
            };

            let (device, context) = match devices.entry(luid) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let device_context =
                        create_device(&adapter).map_err(|e| Error::NoBackend(e.to_string()))?;
                    e.insert(device_context)
                }
            };

            // SAFETY: `device` was created against this exact adapter, which
            // `DuplicateOutput` requires.
            let duplication = match unsafe { output1.DuplicateOutput(&*device) } {
                Ok(d) => d,
                // Another process (Task Manager's screenshot, a remote
                // session, a second Pallet) may already hold this output's
                // one duplication slot; skip it rather than failing every
                // other display too.
                Err(e) => {
                    tracing::warn!("could not duplicate {:?}: {e}", monitor_id(&desc));
                    continue;
                }
            };

            targets.push(Target {
                monitor: monitor_from(&desc, targets.len()),
                device: device.clone(),
                context: context.clone(),
                duplication,
                last_frame: None,
            });
        }
    }

    if targets.is_empty() {
        return Err(Error::NoBackend(
            "no display could be opened for duplication".into(),
        ));
    }

    Ok(targets)
}

fn create_device(
    adapter: &IDXGIAdapter1,
) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;

    // SAFETY: `device`/`context` start `None` as the call requires; an
    // explicit adapter requires `D3D_DRIVER_TYPE_UNKNOWN` per its docs.
    unsafe {
        D3D11CreateDevice(
            &adapter.cast::<windows::Win32::Graphics::Dxgi::IDXGIAdapter>()?,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }

    Ok((
        device.expect("D3D11CreateDevice succeeded without a device"),
        context.expect("D3D11CreateDevice succeeded without a context"),
    ))
}

fn monitor_id(desc: &DXGI_OUTPUT_DESC) -> String {
    String::from_utf16_lossy(&desc.DeviceName)
        .trim_end_matches('\0')
        .to_string()
}

fn monitor_from(desc: &DXGI_OUTPUT_DESC, index: usize) -> Monitor {
    let id = monitor_id(desc);
    let rect: RECT = desc.DesktopCoordinates;
    // `DesktopCoordinates` is in the *desktop's* rotated layout — 2160×3840
    // for a portrait monitor — which is also what a logical position needs.
    let logical_width = (rect.right - rect.left).max(0) as u32;
    let logical_height = (rect.bottom - rect.top).max(0) as u32;

    // DXGI reports output geometry in real device pixels regardless of the
    // caller's DPI awareness, so no scale factor is needed here the way
    // GDI-based enumeration would need one. Rotation is a different matter:
    // confirmed by capturing a real portrait monitor, `AcquireNextFrame`'s
    // texture stays in the panel's native, un-rotated grid — 3840×2160 for
    // that same monitor — so the pixel dimensions and the transform that
    // relates them to the desktop layout both come from `Rotation`, not from
    // `DesktopCoordinates` alone.
    let transform = match desc.Rotation {
        DXGI_MODE_ROTATION_ROTATE90 => Transform::Rotate90,
        DXGI_MODE_ROTATION_ROTATE180 => Transform::Rotate180,
        DXGI_MODE_ROTATION_ROTATE270 => Transform::Rotate270,
        // IDENTITY, and DXGI's own "unspecified" value: assuming unrotated
        // is right far more often than it is wrong, and it is what every
        // monitor without an explicit rotation setting reports.
        _ => Transform::Normal,
    };
    let (pixel_width, pixel_height) = if transform.swaps_axes() {
        (logical_height, logical_width)
    } else {
        (logical_width, logical_height)
    };

    Monitor {
        name: format!("Display {} ({id})", index + 1),
        id,
        logical_x: rect.left,
        logical_y: rect.top,
        logical_width,
        logical_height,
        pixel_width,
        pixel_height,
        transform,
        // Not queried yet: `IDXGIOutput6::GetDesc1().ColorSpace` would tell
        // wide-gamut and HDR displays apart from sRGB ones, the way the
        // Wayland backend's colour management protocol does.
        profile: ColorProfile::Unknown,
    }
}

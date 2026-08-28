//! Wayland capture via `wlr-screencopy`.
//!
//! This is the path `hyprpicker` and `grim` use, and the reason Pallet can
//! freeze the screen without a permission dialog: `zwlr_screencopy_manager_v1`
//! copies an output straight into a shared-memory buffer we own. The
//! xdg-desktop-portal route asks the user every session and costs far more,
//! so it is a fallback rather than the default.
//!
//! Not every compositor implements the protocol. GNOME notably does not, which
//! is why [`WaylandCapture::new`] fails cleanly rather than assuming.

use std::os::fd::AsFd;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer::WlBuffer,
    wl_output::{self, WlOutput},
    wl_registry::WlRegistry,
    wl_shm::{self, WlShm},
    wl_shm_pool::WlShmPool,
};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

use crate::ScreenCapture;
use crate::error::{Error, Result};
use crate::frame::{Capture, Frame, PixelFormat};
use crate::monitor::{ColorProfile, Monitor, Transform};

/// What the compositor told us about one output.
#[derive(Debug, Default, Clone)]
struct OutputInfo {
    name: Option<String>,
    description: Option<String>,
    x: i32,
    y: i32,
    /// Physical size, straight from `wl_output::mode`.
    width: u32,
    height: u32,
    /// Integer scale from `wl_output::scale`.
    scale: i32,
    transform: Transform,
}

/// State the Wayland dispatch loop accumulates into.
#[derive(Debug, Default)]
struct State {
    outputs: Vec<(WlOutput, OutputInfo)>,
    /// One slot per output being copied in the current batch.
    slots: Vec<Slot>,
}

/// Progress of one output's copy. Every output in a batch has one, so all
/// copies can be in flight at once instead of serialised behind each other.
#[derive(Debug, Default, Clone, Copy)]
struct Slot {
    /// Buffer geometry the compositor asked for.
    pending: Option<PendingFrame>,
    /// The compositor finished writing.
    ready: bool,
    /// The compositor gave up.
    failed: bool,
}

#[derive(Debug, Clone, Copy)]
struct PendingFrame {
    format: wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
}

/// A live connection to the compositor, kept warm between picks.
pub struct WaylandCapture {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    screencopy: ZwlrScreencopyManagerV1,
    shm: WlShm,
}

impl std::fmt::Debug for WaylandCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaylandCapture")
            .field("outputs", &self.state.outputs.len())
            .finish()
    }
}

impl WaylandCapture {
    /// Connect and bind the globals we need.
    pub fn new() -> Result<Self> {
        let connection = Connection::connect_to_env()
            .map_err(|e| Error::NoBackend(format!("could not reach the compositor: {e}")))?;

        let (globals, mut queue) = registry_queue_init::<State>(&connection)
            .map_err(|e| Error::NoBackend(format!("registry init failed: {e}")))?;
        let qh = queue.handle();

        let screencopy: ZwlrScreencopyManagerV1 = globals
            .bind(&qh, 1..=3, ())
            .map_err(|_| Error::Unsupported("zwlr_screencopy_manager_v1"))?;
        let shm: WlShm = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| Error::Unsupported("wl_shm"))?;

        let mut state = State::default();

        // Bind every advertised output so geometry events start arriving.
        for global in globals.contents().clone_list() {
            if global.interface == "wl_output" {
                let output: WlOutput =
                    globals
                        .registry()
                        .bind(global.name, global.version.min(4), &qh, ());
                state.outputs.push((output, OutputInfo::default()));
            }
        }

        // Two round trips: one for the binds, one for the geometry bursts.
        queue
            .roundtrip(&mut state)
            .map_err(|e| Error::Refused(e.to_string()))?;
        queue
            .roundtrip(&mut state)
            .map_err(|e| Error::Refused(e.to_string()))?;

        if state.outputs.is_empty() {
            return Err(Error::NoBackend(
                "the compositor advertised no outputs".into(),
            ));
        }

        Ok(Self {
            connection,
            queue,
            state,
            screencopy,
            shm,
        })
    }

    /// Re-read output geometry, in case displays changed since the last pick.
    fn refresh_outputs(&mut self) -> Result<()> {
        self.queue
            .roundtrip(&mut self.state)
            .map_err(|e| Error::Refused(e.to_string()))?;
        Ok(())
    }

    fn monitor_of(info: &OutputInfo) -> Monitor {
        let id = info.name.clone().unwrap_or_else(|| "unknown".into());
        Monitor {
            name: info.description.clone().unwrap_or_else(|| id.clone()),
            id,
            x: info.x,
            y: info.y,
            width: info.width,
            height: info.height,
            // wl_output only reports integer scale. Fractional scaling is a
            // separate protocol and does not change the framebuffer we copy:
            // screencopy always hands back real device pixels, which is
            // exactly what the loupe needs.
            scale: f64::from(info.scale.max(1)),
            transform: info.transform,
            // Wayland has no colour-management protocol in wide deployment
            // yet, so the display's real profile is not knowable here.
            profile: ColorProfile::Unknown,
        }
    }

    /// Copy several outputs in one batch.
    ///
    /// Every request is issued before any is awaited. That matters twice over:
    /// it removes a full compositor round trip per extra monitor, and it puts
    /// the copies close enough together in time that the frozen desktop has no
    /// visible seam between displays.
    fn capture_outputs(&mut self, indices: &[usize]) -> Result<Vec<Frame>> {
        let qh = self.queue.handle();
        self.state.slots = vec![Slot::default(); indices.len()];

        // Phase one: ask for every output at once. `0` excludes the cursor -
        // a picker must read the pixel underneath the pointer, not the pointer.
        let frames: Vec<ZwlrScreencopyFrameV1> = indices
            .iter()
            .enumerate()
            .map(|(slot, &index)| {
                let (output, _) = &self.state.outputs[index];
                self.screencopy.capture_output(0, output, &qh, slot)
            })
            .collect();

        // Phase two: wait for all buffer offers.
        while self
            .state
            .slots
            .iter()
            .any(|s| s.pending.is_none() && !s.failed)
        {
            self.queue
                .blocking_dispatch(&mut self.state)
                .map_err(|e| Error::Refused(e.to_string()))?;
        }

        // Phase three: back every offer with memory and start the copies.
        let mut backing = Vec::with_capacity(indices.len());
        for (slot, frame) in frames.iter().enumerate() {
            let Some(pending) = self.state.slots[slot].pending else {
                return Err(Error::Refused("compositor rejected the capture".into()));
            };
            let len = pending.stride as usize * pending.height as usize;

            let memfd = rustix::fs::memfd_create(
                c"pallet-capture",
                rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
            )
            .map_err(|e| Error::Io(std::io::Error::from(e)))?;
            rustix::fs::ftruncate(&memfd, len as u64)
                .map_err(|e| Error::Io(std::io::Error::from(e)))?;

            let pool: WlShmPool = self.shm.create_pool(memfd.as_fd(), len as i32, &qh, ());
            let buffer: WlBuffer = pool.create_buffer(
                0,
                pending.width as i32,
                pending.height as i32,
                pending.stride as i32,
                pending.format,
                &qh,
                (),
            );

            frame.copy(&buffer);
            backing.push((memfd, pool, buffer, pending));
        }

        // Phase four: wait for all of them to finish.
        while self.state.slots.iter().any(|s| !s.ready && !s.failed) {
            self.queue
                .blocking_dispatch(&mut self.state)
                .map_err(|e| Error::Refused(e.to_string()))?;
        }

        let mut out = Vec::with_capacity(indices.len());
        for (slot, (memfd, pool, buffer, pending)) in backing.into_iter().enumerate() {
            if self.state.slots[slot].failed {
                return Err(Error::Refused("the copy failed".into()));
            }

            // Safety: the compositor has signalled `ready` for this slot, so it
            // has finished writing and will not touch the mapping again.
            let map = unsafe { memmap2::Mmap::map(&memfd) }?;
            let data = map.to_vec();

            buffer.destroy();
            pool.destroy();

            let info = &self.state.outputs[indices[slot]].1;
            let mut monitor = Self::monitor_of(info);
            // Trust the copied buffer's geometry over the mode event: it is
            // what we actually hold, and it accounts for any rotation.
            monitor.width = pending.width;
            monitor.height = pending.height;

            out.push(Frame {
                monitor,
                data,
                stride: pending.stride as usize,
                format: decode_format(pending.format)?,
            });
        }

        for frame in frames {
            frame.destroy();
        }

        Ok(out)
    }
}

impl ScreenCapture for WaylandCapture {
    fn monitors(&mut self) -> Result<Vec<Monitor>> {
        self.refresh_outputs()?;
        Ok(self
            .state
            .outputs
            .iter()
            .map(|(_, info)| Self::monitor_of(info))
            .collect())
    }

    fn capture_all(&mut self) -> Result<Capture> {
        self.refresh_outputs()?;
        let indices: Vec<usize> = (0..self.state.outputs.len()).collect();
        Ok(Capture {
            frames: self.capture_outputs(&indices)?,
        })
    }

    fn capture_monitor(&mut self, id: &str) -> Result<Frame> {
        self.refresh_outputs()?;
        let index = self
            .state
            .outputs
            .iter()
            .position(|(_, info)| info.name.as_deref() == Some(id))
            .ok_or_else(|| Error::UnknownMonitor(id.to_string()))?;
        self.capture_outputs(&[index])?
            .pop()
            .ok_or_else(|| Error::Refused("no frame returned".into()))
    }

    fn backend_name(&self) -> &'static str {
        "wlr-screencopy"
    }
}

impl Drop for WaylandCapture {
    fn drop(&mut self) {
        let _ = self.connection.flush();
    }
}

fn decode_format(format: wl_shm::Format) -> Result<PixelFormat> {
    // Wayland names formats little-endian, so Xrgb8888 is B,G,R,X in memory.
    match format {
        wl_shm::Format::Xrgb8888 => Ok(PixelFormat::Bgrx8888),
        wl_shm::Format::Argb8888 => Ok(PixelFormat::Bgra8888),
        wl_shm::Format::Xbgr8888 => Ok(PixelFormat::Rgbx8888),
        wl_shm::Format::Abgr8888 => Ok(PixelFormat::Rgba8888),
        other => Err(Error::Refused(format!(
            "unsupported wl_shm format {other:?}"
        ))),
    }
}

// ---- dispatch ----

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Hot-plug is handled by re-reading geometry before each capture.
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        output: &WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some((_, info)) = state.outputs.iter_mut().find(|(o, _)| o == output) else {
            return;
        };

        match event {
            wl_output::Event::Geometry {
                x, y, transform, ..
            } => {
                info.x = x;
                info.y = y;
                if let wayland_client::WEnum::Value(t) = transform {
                    info.transform = match t {
                        wl_output::Transform::Normal => Transform::Normal,
                        wl_output::Transform::_90 => Transform::Rotate90,
                        wl_output::Transform::_180 => Transform::Rotate180,
                        wl_output::Transform::_270 => Transform::Rotate270,
                        wl_output::Transform::Flipped => Transform::Flipped(0),
                        wl_output::Transform::Flipped90 => Transform::Flipped(90),
                        wl_output::Transform::Flipped180 => Transform::Flipped(180),
                        wl_output::Transform::Flipped270 => Transform::Flipped(270),
                        _ => Transform::Normal,
                    };
                }
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                ..
            } => {
                // Only the current mode describes the framebuffer.
                if let wayland_client::WEnum::Value(f) = flags
                    && f.contains(wl_output::Mode::Current)
                {
                    info.width = width.max(0) as u32;
                    info.height = height.max(0) as u32;
                }
            }
            wl_output::Event::Scale { factor } => info.scale = factor,
            wl_output::Event::Name { name } => info.name = Some(name),
            wl_output::Event::Description { description } => info.description = Some(description),
            _ => {}
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        &slot: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(entry) = state.slots.get_mut(slot) else {
            return;
        };

        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format: wayland_client::WEnum::Value(format),
                width,
                height,
                stride,
            } => {
                // Compositors may offer several; the first shm format is the
                // one they prefer.
                entry.pending.get_or_insert(PendingFrame {
                    format,
                    width,
                    height,
                    stride,
                });
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => entry.ready = true,
            zwlr_screencopy_frame_v1::Event::Failed => entry.failed = true,
            _ => {}
        }
    }
}

// These carry no events Pallet needs to act on.
macro_rules! ignore_events {
    ($($iface:ty),* $(,)?) => {$(
        impl Dispatch<$iface, ()> for State {
            fn event(
                _: &mut Self,
                _: &$iface,
                _: <$iface as wayland_client::Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignore_events!(WlShm, WlShmPool, WlBuffer, ZwlrScreencopyManagerV1);

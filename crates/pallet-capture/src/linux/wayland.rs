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

use std::collections::BTreeMap;
use std::os::fd::AsFd;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer::WlBuffer,
    wl_output::{self, WlOutput},
    wl_registry::{self, WlRegistry},
    wl_shm::{self, WlShm},
    wl_shm_pool::WlShmPool,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1::ZxdgOutputManagerV1,
    zxdg_output_v1::{self, ZxdgOutputV1},
};
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
    /// Physical mode size, straight from `wl_output::mode`.
    pixel_width: u32,
    pixel_height: u32,
    /// Logical geometry from `xdg_output`, which is the only source that
    /// describes fractional scaling correctly. `wl_output::scale` reports an
    /// integer (2 for a 1.5 scale) and `wl_output::geometry` gives a logical
    /// position with no matching logical size, so neither is sufficient.
    logical_x: i32,
    logical_y: i32,
    logical_width: u32,
    logical_height: u32,
    transform: Transform,
}

/// One connected output and everything we know about it.
#[derive(Debug)]
struct OutputEntry {
    output: WlOutput,
    xdg: Option<ZxdgOutputV1>,
    info: OutputInfo,
}

/// State the Wayland dispatch loop accumulates into.
///
/// Outputs are keyed by their registry global name, which the compositor
/// guarantees is stable for as long as the global exists. Keying by position
/// in a list would be a latent bug: unplugging a monitor shifts every later
/// index, silently routing one output's geometry onto another.
///
/// A `BTreeMap` rather than a `HashMap` so enumeration order is stable across
/// runs, which keeps `monitors()` output predictable.
#[derive(Debug, Default)]
struct State {
    outputs: BTreeMap<u32, OutputEntry>,
    /// Kept so newly hot-plugged outputs can be given an xdg_output too.
    xdg_manager: Option<ZxdgOutputManagerV1>,
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

impl State {
    /// Bind a `wl_output` global and start tracking it.
    ///
    /// Used both for the outputs present at start-up and for any that appear
    /// later, so the two paths cannot drift apart.
    fn add_output(
        &mut self,
        registry: &WlRegistry,
        name: u32,
        version: u32,
        qh: &QueueHandle<Self>,
    ) {
        if self.outputs.contains_key(&name) {
            return;
        }

        let output: WlOutput = registry.bind(name, version, qh, name);
        let xdg = self
            .xdg_manager
            .as_ref()
            .map(|manager| manager.get_xdg_output(&output, qh, name));

        self.outputs.insert(
            name,
            OutputEntry {
                output,
                xdg,
                info: OutputInfo::default(),
            },
        );
        tracing::debug!(global = name, "output connected");
    }

    /// Stop tracking an output the compositor has withdrawn.
    fn remove_output(&mut self, name: u32) {
        let Some(entry) = self.outputs.remove(&name) else {
            return;
        };

        if let Some(xdg) = entry.xdg {
            xdg.destroy();
        }
        // `release` exists only from wl_output v3. Below that, dropping the
        // proxy is all a client can do.
        if entry.output.version() >= 3 {
            entry.output.release();
        }
        tracing::debug!(global = name, "output disconnected");
    }
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

        // Optional: without it, logical geometry falls back to physical, which
        // is correct on unscaled desktops.
        let xdg_manager: Option<ZxdgOutputManagerV1> = globals.bind(&qh, 1..=3, ()).ok();
        if xdg_manager.is_none() {
            tracing::warn!(
                "compositor has no xdg_output; fractional scaling will not be accounted for"
            );
        }

        let mut state = State {
            xdg_manager,
            ..State::default()
        };

        // Bind every advertised output so geometry events start arriving.
        for global in globals.contents().clone_list() {
            if global.interface == "wl_output" {
                state.add_output(globals.registry(), global.name, global.version.min(4), &qh);
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

    /// Re-sync the output list before a capture.
    ///
    /// Two round trips, not one: the first delivers registry `global` and
    /// `global_remove` events for any monitor plugged or unplugged since the
    /// last pick, and the second delivers the geometry burst for anything the
    /// first round trip newly bound. The picker is a long-lived process, so
    /// docking a laptop between picks is routine rather than exceptional.
    fn refresh_outputs(&mut self) -> Result<()> {
        for _ in 0..2 {
            self.queue
                .roundtrip(&mut self.state)
                .map_err(|e| Error::Refused(e.to_string()))?;
        }
        Ok(())
    }

    fn monitor_of(info: &OutputInfo) -> Monitor {
        let id = info.name.clone().unwrap_or_else(|| "unknown".into());

        // Compositors without xdg_output leave the logical size at zero. Fall
        // back to the physical size, which is correct at scale 1 and is the
        // best guess available.
        let logical_width = if info.logical_width == 0 {
            info.pixel_width
        } else {
            info.logical_width
        };
        let logical_height = if info.logical_height == 0 {
            info.pixel_height
        } else {
            info.logical_height
        };

        Monitor {
            name: info.description.clone().unwrap_or_else(|| id.clone()),
            id,
            logical_x: info.logical_x,
            logical_y: info.logical_y,
            logical_width,
            logical_height,
            pixel_width: info.pixel_width,
            pixel_height: info.pixel_height,
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
    fn capture_outputs(&mut self, names: &[u32]) -> Result<Vec<Frame>> {
        let qh = self.queue.handle();
        self.state.slots = vec![Slot::default(); names.len()];

        // Phase one: ask for every output at once. `0` excludes the cursor -
        // a picker must read the pixel underneath the pointer, not the pointer.
        let frames: Vec<ZwlrScreencopyFrameV1> = names
            .iter()
            .enumerate()
            .map(|(slot, name)| {
                let entry = self
                    .state
                    .outputs
                    .get(name)
                    .expect("caller passes names taken from the live output map");
                self.screencopy.capture_output(0, &entry.output, &qh, slot)
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
        let mut backing = Vec::with_capacity(names.len());
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

        let mut out = Vec::with_capacity(names.len());
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

            let info = &self.state.outputs[&names[slot]].info;
            let mut monitor = Self::monitor_of(info);
            // Trust the copied buffer's geometry over the mode event: it is
            // what we actually hold, and it accounts for any rotation.
            monitor.pixel_width = pending.width;
            monitor.pixel_height = pending.height;

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
            .values()
            .map(|entry| Self::monitor_of(&entry.info))
            .collect())
    }

    fn capture_all(&mut self) -> Result<Capture> {
        self.refresh_outputs()?;
        let names: Vec<u32> = self.state.outputs.keys().copied().collect();
        if names.is_empty() {
            // Every display was unplugged. An empty capture is honest; the
            // caller gets OutOfBounds for any point it asks about.
            return Ok(Capture::default());
        }
        Ok(Capture {
            frames: self.capture_outputs(&names)?,
        })
    }

    fn capture_monitor(&mut self, id: &str) -> Result<Frame> {
        self.refresh_outputs()?;
        let name = self
            .state
            .outputs
            .iter()
            .find(|(_, entry)| entry.info.name.as_deref() == Some(id))
            .map(|(name, _)| *name)
            .ok_or_else(|| Error::UnknownMonitor(id.to_string()))?;
        self.capture_outputs(&[name])?
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
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // The picker is resident for the whole session, so monitors really do
        // come and go underneath it: docking a laptop, switching a display
        // off, a KVM changing inputs.
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_output" => {
                state.add_output(registry, name, version.min(4), qh);
            }
            wl_registry::Event::GlobalRemove { name } => state.remove_output(name),
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        &name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(info) = state.outputs.get_mut(&name).map(|e| &mut e.info) else {
            return;
        };

        match event {
            // Position deliberately ignored here: xdg_output provides it
            // alongside a matching logical size, and mixing the two sources is
            // what puts the seam in the wrong place.
            wl_output::Event::Geometry {
                transform: wayland_client::WEnum::Value(t),
                ..
            } => {
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
                    info.pixel_width = width.max(0) as u32;
                    info.pixel_height = height.max(0) as u32;
                }
            }
            wl_output::Event::Name { name } => info.name = Some(name),
            wl_output::Event::Description { description } => info.description = Some(description),
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, u32> for State {
    fn event(
        state: &mut Self,
        _: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        &name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(info) = state.outputs.get_mut(&name).map(|e| &mut e.info) else {
            return;
        };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                info.logical_x = x;
                info.logical_y = y;
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                info.logical_width = width.max(0) as u32;
                info.logical_height = height.max(0) as u32;
            }
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

ignore_events!(
    WlShm,
    WlShmPool,
    WlBuffer,
    ZwlrScreencopyManagerV1,
    ZxdgOutputManagerV1,
);

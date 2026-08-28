//! The picking overlay as a `wlr-layer-shell` surface.
//!
//! `winit` has no layer-shell support, so the surface is built directly on
//! `smithay-client-toolkit`. That is not incidental complexity: only a layer
//! surface on the `Overlay` layer can cover everything including panels and
//! fullscreen windows, and only `KeyboardInteractivity::Exclusive` can take
//! every key — including Escape — away from whatever was focused. A normal
//! toplevel would leave the picker fighting the window manager for both.
//!
//! One surface is created per captured monitor so the whole desktop freezes at
//! once, and each is handed to `wgpu` through `raw-window-handle`.

use std::ptr::NonNull;

use pallet_capture::Capture;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, FrameCallbackData};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers, RepeatInfo,
};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_client::backend::ObjectId;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::error::{Error, Result};
use crate::render::{LoupeView, Renderer, Screen};
use crate::session::{Input, Outcome, Session};

/// How large the loupe is drawn, in physical pixels.
const LOUPE_RADIUS: f32 = 150.0;

/// One monitor's overlay: a layer surface, its GPU surface, and its pixels.
struct Overlay {
    layer: LayerSurface,
    /// The surface this overlay draws on, for routing input events back to it.
    /// A plain field searched linearly rather than a map key: Wayland proxies
    /// and their ids both have interior mutability, so neither is a sound hash
    /// key, and a desktop has a handful of monitors at most.
    surface_id: ObjectId,
    surface: wgpu::Surface<'static>,
    screen: Screen,
    /// Index into the capture's frames.
    frame_index: usize,
    configured: bool,
    width: u32,
    height: u32,
    /// State has changed and this overlay owes the screen a new frame.
    needs_redraw: bool,
    /// A frame callback is outstanding, so the compositor has not yet asked
    /// for the next frame.
    awaiting_frame: bool,
}

/// The running picker.
struct Picker {
    registry: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,

    renderer: Renderer,
    session: Session,

    overlays: Vec<Overlay>,
    /// Which overlay the pointer is currently over.
    pointer_on: Option<usize>,

    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    shift_held: bool,
    exit: bool,
}

/// Freeze the desktop and let the user pick a colour.
///
/// Blocks until the user commits or cancels.
pub fn run_picker(capture: Capture, zoom: u32, average_size: u32) -> Result<Outcome> {
    if capture.frames.is_empty() {
        return Err(Error::NoFrame);
    }

    let connection = Connection::connect_to_env()
        .map_err(|e| Error::Compositor(format!("could not reach the compositor: {e}")))?;
    let (globals, mut queue) = registry_queue_init::<Picker>(&connection)
        .map_err(|e| Error::Compositor(format!("registry init failed: {e}")))?;
    let qh: QueueHandle<Picker> = queue.handle();

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|_| Error::Unsupported("wl_compositor"))?;
    let layer_shell =
        LayerShell::bind(&globals, &qh).map_err(|_| Error::Unsupported("zwlr_layer_shell_v1"))?;
    let output_state = OutputState::new(&globals, &qh);

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

    let display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(
            connection
                .backend()
                .display_ptr()
                .cast::<std::ffi::c_void>(),
        )
        .ok_or_else(|| Error::Compositor("null display pointer".into()))?,
    ));

    // Surfaces are built before the GPU device, because the adapter must be one
    // that can actually present to them. Picking an adapter first risks a
    // headless or otherwise incompatible device that fails only at present
    // time, on someone else's machine.
    let mut pending = Vec::with_capacity(capture.frames.len());
    for (index, frame) in capture.frames.iter().enumerate() {
        let surface = compositor.create_surface(&qh);

        let output = output_state.outputs().find(|o| {
            output_state.info(o).and_then(|i| i.name).as_deref() == Some(frame.monitor.id.as_str())
        });

        let layer = layer_shell.create_layer_surface(
            &qh,
            surface.clone(),
            Layer::Overlay,
            Some("pallet-picker"),
            output.as_ref(),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        // Exclusive keyboard so Escape reaches the picker rather than whatever
        // application happened to be focused.
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.set_exclusive_zone(-1);
        layer.commit();

        let window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface.id().as_ptr().cast::<std::ffi::c_void>())
                .ok_or_else(|| Error::Compositor("null surface pointer".into()))?,
        ));

        // Safety: the connection and the layer surface both outlive the wgpu
        // surface; all three are owned by `Picker` until the session ends.
        let gpu_surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(display_handle),
                raw_window_handle: window_handle,
            })
        }
        .map_err(|e| Error::Compositor(format!("could not create a GPU surface: {e}")))?;

        pending.push((index, surface, layer, gpu_surface));
    }

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: pending.first().map(|(_, _, _, s)| s),
        ..Default::default()
    }))
    .map_err(|e| Error::NoGpu(e.to_string()))?;

    let (device, gpu_queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("pallet-picker"),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .map_err(|e| Error::NoGpu(e.to_string()))?;

    let renderer = Renderer::from_device(device, gpu_queue);

    // Start at the centre of the first monitor; the first pointer event
    // corrects this before anything is drawn.
    let cursor = capture
        .frames
        .first()
        .map(|f| {
            (
                f.monitor.logical_x + f.monitor.logical_width as i32 / 2,
                f.monitor.logical_y + f.monitor.logical_height as i32 / 2,
            )
        })
        .unwrap_or((0, 0));

    let session = Session::new(capture, cursor, zoom, average_size);

    let mut overlays = Vec::with_capacity(pending.len());
    for (frame_index, surface, layer, gpu_surface) in pending {
        let screen = renderer.create_screen(&session.capture().frames[frame_index])?;
        overlays.push(Overlay {
            surface_id: surface.id(),
            layer,
            surface: gpu_surface,
            screen,
            frame_index,
            configured: false,
            width: 0,
            height: 0,
            needs_redraw: true,
            awaiting_frame: false,
        });
    }

    let mut picker = Picker {
        registry: RegistryState::new(&globals),
        output_state,
        seat_state: SeatState::new(&globals, &qh),
        renderer,
        session,
        overlays,
        pointer_on: None,
        keyboard: None,
        pointer: None,
        shift_held: false,
        exit: false,
    };

    while !picker.exit {
        queue
            .blocking_dispatch(&mut picker)
            .map_err(|e| Error::Compositor(e.to_string()))?;
    }

    Ok(picker
        .session
        .outcome()
        .cloned()
        .unwrap_or(Outcome::Cancelled))
}

impl Picker {
    /// Which overlay owns a surface.
    fn overlay_for(&self, id: &ObjectId) -> Option<usize> {
        self.overlays.iter().position(|o| &o.surface_id == id)
    }

    /// Configure a surface for its assigned size and draw it.
    /// Note that an overlay's content has changed.
    ///
    /// Input is coalesced rather than drawn immediately. A mouse can emit
    /// motion events far faster than the display refreshes; drawing each one
    /// fills the presentation queue, and every queued frame then shows a
    /// cursor position that is already out of date. That queueing *is* the
    /// input lag. Marking dirty and drawing once per frame callback means the
    /// frame that reaches the screen always reflects the newest position.
    fn mark_dirty(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let Some(overlay) = self.overlays.get_mut(index) else {
            return;
        };
        overlay.needs_redraw = true;
        if !overlay.awaiting_frame {
            self.redraw(index, qh);
        }
    }

    /// Mark every overlay dirty. Used for changes that affect all of them,
    /// such as a zoom step.
    fn mark_all_dirty(&mut self, qh: &QueueHandle<Self>) {
        for index in 0..self.overlays.len() {
            self.mark_dirty(index, qh);
        }
    }

    fn redraw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let __t = std::time::Instant::now();
        let Some(overlay) = self.overlays.get(index) else {
            return;
        };
        if !overlay.configured || overlay.width == 0 || overlay.height == 0 {
            return;
        }

        // Skip a frame the compositor cannot give us rather than stalling the
        // picker; another frame callback will arrive.
        // Ask to be told when the compositor is ready for the next frame,
        // before presenting this one. Marking the wait here rather than after
        // a successful present matters: if acquiring the texture below fails,
        // the callback is still outstanding, and requesting a second one would
        // leave two in flight.
        let wl_surface = overlay.layer.wl_surface().clone();
        wl_surface.frame(qh, FrameCallbackData(wl_surface.clone()));
        if let Some(overlay) = self.overlays.get_mut(index) {
            overlay.awaiting_frame = true;
        }
        let Some(overlay) = self.overlays.get(index) else {
            return;
        };

        let frame = match overlay.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                tracing::debug!("skipped a frame: {other:?}");
                return;
            }
        };

        // The loupe is drawn only on the monitor the pointer is over; a radius
        // of zero leaves the others as a plain frozen image.
        let radius = if self.pointer_on == Some(index) {
            LOUPE_RADIUS
        } else {
            0.0
        };

        let cursor = self.local_cursor(index).unwrap_or((0, 0));
        let view = LoupeView {
            cursor,
            zoom: self.session.zoom(),
            radius,
            sample: self.session.sample_size(),
            grid: true,
            picked: self
                .session
                .current_color()
                .unwrap_or(pallet_color::Color::new(0, 0, 0)),
        };

        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer.draw(&overlay.screen, &target, view);
        self.renderer.queue().present(frame);
        tracing::debug!(index, us = __t.elapsed().as_micros(), "redraw");

        if let Some(overlay) = self.overlays.get_mut(index) {
            overlay.needs_redraw = false;
        }
    }

    /// The cursor in one overlay's physical pixels, if it is on that monitor.
    fn local_cursor(&self, index: usize) -> Option<(u32, u32)> {
        let overlay = self.overlays.get(index)?;
        let frame = self.session.capture().frames.get(overlay.frame_index)?;
        let (x, y) = self.session.cursor();
        frame.monitor.to_pixel(x, y)
    }

    /// Reconfigure a surface's swapchain after a configure event.
    fn configure_surface(&mut self, index: usize, width: u32, height: u32) {
        let Some(overlay) = self.overlays.get_mut(index) else {
            return;
        };
        overlay.width = width;
        overlay.height = height;
        overlay.configured = true;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            // Must match the renderer's non-sRGB format, or the compositor
            // would apply a transfer function and every colour would shift.
            format: crate::render::FORMAT,
            // `Auto` resolves to sRGB for an 8-bit format, meaning the bytes
            // are handed to the compositor already encoded and displayed
            // as-is. Anything else would re-encode and shift every colour.
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            // One frame in flight. Deeper queues trade latency for throughput,
            // which is the wrong trade for a cursor-tracking loupe.
            desired_maximum_frame_latency: 1,
        };
        overlay.surface.configure(self.renderer.device(), &config);
    }
}

// ---- sctk handlers ----

impl CompositorHandler for Picker {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
        // The overlay works in physical pixels throughout, so a scale change
        // does not alter what it draws.
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let Some(index) = self.overlay_for(&surface.id()) else {
            return;
        };
        if let Some(overlay) = self.overlays.get_mut(index) {
            overlay.awaiting_frame = false;
            if !overlay.needs_redraw {
                return;
            }
        }
        self.redraw(index, qh);
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Picker {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        // The compositor took the surface away; treat it as a cancel rather
        // than returning a colour the user never confirmed.
        self.session.apply(Input::Cancel);
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(index) = self
            .overlays
            .iter()
            .position(|o| o.layer.wl_surface() == layer.wl_surface())
        else {
            return;
        };

        let (w, h) = configure.new_size;
        tracing::debug!(index, w, h, "layer surface configured");
        self.configure_surface(index, w.max(1), h.max(1));
        if let Some(overlay) = self.overlays.get_mut(index) {
            overlay.awaiting_frame = false;
        }
        self.mark_dirty(index, qh);
    }
}

impl SeatHandler for Picker {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Keyboard if self.keyboard.is_none() => {
                self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
            }
            Capability::Pointer if self.pointer.is_none() => {
                self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Keyboard => {
                if let Some(k) = self.keyboard.take() {
                    k.release();
                }
            }
            Capability::Pointer => {
                if let Some(p) = self.pointer.take() {
                    p.release();
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Picker {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let input = match event.keysym {
            Keysym::Escape | Keysym::q => Some(Input::Cancel),
            Keysym::Return | Keysym::KP_Enter | Keysym::space => Some(Input::Commit),
            Keysym::Left => Some(Input::Nudge { dx: -1, dy: 0 }),
            Keysym::Right => Some(Input::Nudge { dx: 1, dy: 0 }),
            Keysym::Up => Some(Input::Nudge { dx: 0, dy: -1 }),
            Keysym::Down => Some(Input::Nudge { dx: 0, dy: 1 }),
            Keysym::plus | Keysym::equal | Keysym::KP_Add => Some(Input::ZoomIn),
            Keysym::minus | Keysym::KP_Subtract => Some(Input::ZoomOut),
            _ => None,
        };

        if let Some(input) = input {
            self.session.apply(input);
            if self.session.is_finished() {
                self.exit = true;
            } else {
                self.mark_all_dirty(qh);
            }
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn repeat_key(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        // Holding an arrow key should keep nudging.
        self.press_key(conn, qh, keyboard, serial, event);
    }

    #[allow(clippy::too_many_arguments)]
    fn update_modifiers(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        // Shift switches to averaging a square, per the prototype.
        if modifiers.shift != self.shift_held {
            self.shift_held = modifiers.shift;
            self.session.apply(Input::Averaging(self.shift_held));
            self.mark_all_dirty(qh);
        }
    }

    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: RepeatInfo,
    ) {
    }
}

impl PointerHandler for Picker {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        // Only the monitor showing the loupe changes as the pointer moves; the
        // others hold a static frozen image. Redrawing them too would double
        // the GPU work on the hottest path for no visible difference.
        let was_on = self.pointer_on;
        let mut dirty = false;

        for event in events {
            let Some(index) = self.overlay_for(&event.surface.id()) else {
                continue;
            };
            let Some(frame) = self
                .session
                .capture()
                .frames
                .get(self.overlays[index].frame_index)
            else {
                continue;
            };
            let monitor = &frame.monitor;

            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    let (sx, sy) = event.position;
                    self.pointer_on = Some(index);
                    dirty = true;

                    // Hyprland sends enter with (-1, -1) when it has no
                    // meaningful position yet. Taking that literally would
                    // clamp the cursor to the monitor's top-left corner and
                    // show the wrong colour until the pointer next moves.
                    if sx < 0.0 || sy < 0.0 {
                        continue;
                    }

                    // Surface-local logical position, offset into the desktop.
                    self.session.apply(Input::PointerTo {
                        x: monitor.logical_x + sx as i32,
                        y: monitor.logical_y + sy as i32,
                    });
                }
                PointerEventKind::Leave { .. } => {
                    if self.pointer_on == Some(index) {
                        self.pointer_on = None;
                        dirty = true;
                    }
                }
                PointerEventKind::Press { button, .. } => {
                    // Left commits, anything else cancels.
                    const BTN_LEFT: u32 = 0x110;
                    self.session.apply(if button == BTN_LEFT {
                        Input::Commit
                    } else {
                        Input::Cancel
                    });
                }
                PointerEventKind::Axis { vertical, .. } => {
                    if vertical.discrete != 0 {
                        self.session.apply(if vertical.discrete < 0 {
                            Input::ZoomIn
                        } else {
                            Input::ZoomOut
                        });
                        dirty = true;
                    }
                }
                PointerEventKind::Release { .. } => {}
            }
        }

        if self.session.is_finished() {
            self.exit = true;
        } else if dirty {
            if let Some(index) = self.pointer_on {
                self.mark_dirty(index, qh);
            }
            // If the pointer crossed to another monitor, the one it left must
            // be redrawn once to erase the loupe.
            if let Some(previous) = was_on
                && was_on != self.pointer_on
            {
                self.mark_dirty(previous, qh);
            }
        }
    }
}

impl OutputHandler for Picker {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ProvidesRegistryState for Picker {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(Picker);
smithay_client_toolkit::delegate_dispatch2!(Picker);

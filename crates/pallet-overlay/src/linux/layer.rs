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

use crate::Palette;
use crate::error::{Error, Result};
use crate::hud::chrome::{Chrome, HudState, Tray};
use crate::render::{ChromeGpu, LoupeView, Renderer, Screen};
use crate::session::{Input, Outcome, Session};

/// The keys the loupe answers to, as configured.
#[derive(Debug, Clone)]
pub struct LoupeKeys {
    /// Take the colour.
    pub commit: String,
    /// Take it and keep it.
    pub save: String,
    /// Abandon the pick.
    pub cancel: String,
}

impl Default for LoupeKeys {
    fn default() -> Self {
        Self {
            commit: "Return".into(),
            save: "S".into(),
            cancel: "Escape".into(),
        }
    }
}

/// A comparable name for a key press.
///
/// Named keys come from the keysym so they are layout-independent; everything
/// else comes from the text the key produces, so a letter matches whatever the
/// user's layout puts under it.
fn key_name(event: &KeyEvent) -> String {
    match event.keysym {
        Keysym::Return | Keysym::KP_Enter => "RETURN".into(),
        Keysym::Escape => "ESCAPE".into(),
        Keysym::space => "SPACE".into(),
        Keysym::Tab => "TAB".into(),
        Keysym::BackSpace => "BACKSPACE".into(),
        Keysym::Left => "LEFT".into(),
        Keysym::Right => "RIGHT".into(),
        Keysym::Up => "UP".into(),
        Keysym::Down => "DOWN".into(),
        _ => event
            .utf8
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_uppercase(),
    }
}

/// Whether a key name matches a configured binding.
///
/// Loupe bindings are a single key: the overlay holds an exclusive keyboard
/// grab, and a modifier combination there would fight Shift, which already
/// means "average this area". Any modifiers written in the binding are
/// therefore ignored rather than silently making it unmatchable.
fn name_binds(name: &str, binding: &str) -> bool {
    !name.is_empty()
        && binding
            .split(['+', '-'])
            .next_back()
            .is_some_and(|k| k.trim().eq_ignore_ascii_case(name))
}

/// Whether a press matches a configured binding.
fn binds(event: &KeyEvent, binding: &str) -> bool {
    name_binds(&key_name(event), binding)
}

#[cfg(test)]
mod tests {
    use super::name_binds;

    #[test]
    fn a_binding_matches_its_key_whatever_the_case() {
        assert!(name_binds("RETURN", "Return"));
        assert!(name_binds("K", "k"));
        assert!(name_binds("ESCAPE", "Escape"));
    }

    #[test]
    fn a_remapped_key_stops_matching_the_old_one() {
        // The whole point of remapping: once commit is K, Return must not
        // still commit.
        assert!(!name_binds("RETURN", "K"));
        assert!(name_binds("K", "K"));
    }

    #[test]
    fn modifiers_in_a_binding_are_ignored_rather_than_breaking_it() {
        // Shift already means "average" inside the loupe, so a binding written
        // with modifiers still matches on its final key instead of never
        // matching at all.
        assert!(name_binds("K", "CTRL+K"));
        assert!(name_binds("RETURN", "SHIFT+Return"));
    }

    #[test]
    fn an_empty_press_matches_nothing() {
        assert!(!name_binds("", "K"));
        assert!(!name_binds("", ""));
    }

    #[test]
    fn a_wheel_notch_reported_as_discrete_is_one_zoom_step() {
        let mut carry = 0.0;
        assert_eq!(super::scroll_steps(-1, 0.0, &mut carry), 1, "up zooms in");
        assert_eq!(
            super::scroll_steps(1, 0.0, &mut carry),
            -1,
            "down zooms out"
        );
    }

    #[test]
    fn high_resolution_scroll_still_zooms() {
        // The bug: compositors speaking `wl_pointer` v8 leave `discrete` at
        // zero and report the movement in `absolute`, so the wheel did nothing.
        let mut carry = 0.0;
        assert_eq!(super::scroll_steps(0, -15.0, &mut carry), 1);
        assert_eq!(super::scroll_steps(0, 15.0, &mut carry), -1);
    }

    #[test]
    fn a_partial_scroll_is_carried_rather_than_lost() {
        // A trackpad delivers a notch in fragments; dropping them would make
        // slow scrolling do nothing at all.
        let mut carry = 0.0;
        assert_eq!(super::scroll_steps(0, -5.0, &mut carry), 0);
        assert_eq!(super::scroll_steps(0, -5.0, &mut carry), 0);
        assert_eq!(
            super::scroll_steps(0, -5.0, &mut carry),
            1,
            "three fifths make one"
        );
        assert_eq!(
            super::scroll_steps(0, -5.0, &mut carry),
            0,
            "and the count restarts"
        );
    }

    #[test]
    fn a_discrete_notch_discards_any_part_spent_accumulation() {
        // Counting both would zoom twice for one movement.
        let mut carry = 0.0;
        super::scroll_steps(0, -10.0, &mut carry);
        assert!(carry != 0.0);
        assert_eq!(super::scroll_steps(-1, -15.0, &mut carry), 1);
        assert_eq!(carry, 0.0);
    }

    #[test]
    fn a_flick_of_the_wheel_is_capped() {
        // Zoom is exponential: 2x to 64x is five steps, so an uncapped flick
        // would jump the whole range at once.
        let mut carry = 0.0;
        assert_eq!(super::scroll_steps(0, -1000.0, &mut carry), 4);
        assert_eq!(super::scroll_steps(-40, 0.0, &mut carry), 4);
    }

    #[test]
    fn a_scroll_of_nothing_does_nothing() {
        let mut carry = 0.0;
        assert_eq!(super::scroll_steps(0, 0.0, &mut carry), 0);
        assert_eq!(carry, 0.0);
    }
}

/// How much continuous scroll makes one zoom step.
///
/// `wl_pointer` reports high-resolution scroll in the same units as a notch,
/// and libinput's notch is 15. Matching it means one click of a wheel is one
/// step, and a trackpad swipe is proportional.
const SCROLL_NOTCH: f32 = 15.0;

/// How many zoom steps one scroll event is worth, positive to zoom in.
///
/// `discrete` counts whole wheel notches, but a compositor speaking
/// `wl_pointer` v8 reports high-resolution scroll and leaves it at zero — the
/// reason the wheel did nothing at all. When it does, the continuous value is
/// accumulated in `carry` and spent a notch at a time, so a trackpad swipe
/// zooms proportionally and a wheel click still moves exactly one step.
///
/// Wayland measures vertical scroll downward, while zooming in is upward, so
/// the sign is flipped.
fn scroll_steps(discrete: i32, absolute: f64, carry: &mut f32) -> i32 {
    let steps = if discrete != 0 {
        // A compositor that reports notches is authoritative; anything part
        // way through the accumulator would double-count.
        *carry = 0.0;
        -discrete
    } else if absolute != 0.0 {
        *carry -= absolute as f32;
        let whole = (*carry / SCROLL_NOTCH).trunc();
        *carry -= whole * SCROLL_NOTCH;
        whole as i32
    } else {
        0
    };

    // A flick of a free-spinning wheel can deliver a lot at once, and zoom is
    // exponential, so clamp it to something a user can still follow.
    steps.clamp(-4, 4)
}

/// The instruction pill's text, built from the keys actually in force.
///
/// The design's copy reads "Click to sample · Scroll to zoom · Space locks the
/// loupe · Esc cancels". Two of those are wrong for this picker — there is no
/// loupe lock, and the cancel key is remappable — and an instruction pill that
/// lies is worse than one that deviates by a word, so the middle items are
/// generated from the live bindings and the shape of the line is kept.
fn instructions(keys: &LoupeKeys, average: u32, palette: bool) -> String {
    let last = if palette {
        // Backing out of a palette pass keeps what it gathered, so calling it
        // "cancel" would read as a threat to discard the work.
        format!("{} finishes", pretty_key(&keys.cancel))
    } else {
        format!("{} cancels", pretty_key(&keys.cancel))
    };
    format!("Click to sample · Scroll to zoom · Shift averages {average}×{average} · {last}")
}

/// A key name as a reader would write it.
fn pretty_key(name: &str) -> String {
    match name.to_ascii_uppercase().as_str() {
        "ESCAPE" | "ESC" => "Esc".into(),
        "RETURN" | "ENTER" => "Enter".into(),
        "SPACE" => "Space".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()
                }
                None => String::new(),
            }
        }
    }
}

/// One monitor's overlay: a layer surface, its GPU surface, and its pixels.
struct Overlay {
    // Declaration order is drop order, and it matters here: the wgpu surface
    // borrows the wl_surface that the layer surface owns, so it must be
    // dropped first. The reverse would tear down the Wayland object while
    // wgpu still held a pointer to it.
    surface: wgpu::Surface<'static>,
    layer: LayerSurface,
    /// The surface this overlay draws on, for routing input events back to it.
    /// A plain field searched linearly rather than a map key: Wayland proxies
    /// and their ids both have interior mutability, so neither is a sound hash
    /// key, and a desktop has a handful of monitors at most.
    surface_id: ObjectId,
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

    /// The HUD's panels, and the textures they are composited from.
    chrome: Chrome,
    chrome_gpu: ChromeGpu,
    /// The palette being gathered, or `None` for a single pick.
    ///
    /// Holds only the colours the caller already had; the ones taken during
    /// this pass live in the session, so the tray fills as the user works.
    palette: Option<Palette>,
    /// When the overlay opened, which drives the tray's pulse.
    started: std::time::Instant,

    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    shift_held: bool,
    /// Continuous scroll not yet spent on a zoom step.
    scroll: f32,
    /// What the commit, save and cancel keys are bound to.
    keys: LoupeKeys,
    exit: bool,
}
/// A warm picker: a compositor connection and a live GPU device, kept between
/// picks.
///
/// Building the GPU context costs about 220 ms — measured, and roughly 80% of a
/// cold pick against 31 ms to capture the screen. Keeping it alive is the
/// difference between a hotkey that feels instant and one that feels broken,
/// and is the reason the picker is a resident process at all.
pub struct PickerContext {
    connection: Connection,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl std::fmt::Debug for PickerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PickerContext").finish_non_exhaustive()
    }
}

fn display_handle_of(connection: &Connection) -> Result<RawDisplayHandle> {
    Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(
            connection
                .backend()
                .display_ptr()
                .cast::<std::ffi::c_void>(),
        )
        .ok_or_else(|| Error::Compositor("null display pointer".into()))?,
    )))
}

fn window_handle_of(surface: &wl_surface::WlSurface) -> Result<RawWindowHandle> {
    Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(surface.id().as_ptr().cast::<std::ffi::c_void>())
            .ok_or_else(|| Error::Compositor("null surface pointer".into()))?,
    )))
}

impl PickerContext {
    /// Connect to the compositor and build a GPU device. Done once.
    pub fn new() -> Result<Self> {
        let connection = Connection::connect_to_env()
            .map_err(|e| Error::Compositor(format!("could not reach the compositor: {e}")))?;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

        // The adapter must be able to present to a real Wayland surface, so a
        // throwaway one is created to choose against. Requesting an adapter
        // with no surface risks a device that only fails at present time.
        let (globals, queue) = registry_queue_init::<Picker>(&connection)
            .map_err(|e| Error::Compositor(format!("registry init failed: {e}")))?;
        let qh: QueueHandle<Picker> = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|_| Error::Unsupported("wl_compositor"))?;
        // Fail here rather than on the first pick if the compositor cannot do
        // layer shell at all.
        LayerShell::bind(&globals, &qh).map_err(|_| Error::Unsupported("zwlr_layer_shell_v1"))?;

        let probe = compositor.create_surface(&qh);
        let probe_surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(display_handle_of(&connection)?),
                raw_window_handle: window_handle_of(&probe)?,
            })
        }
        .map_err(|e| Error::Compositor(format!("could not create a probe surface: {e}")))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&probe_surface),
            ..Default::default()
        }))
        .map_err(|e| Error::NoGpu(e.to_string()))?;

        let (device, gpu_queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("pallet-picker"),
                required_limits: wgpu::Limits::downlevel_defaults(),
                ..Default::default()
            }))
            .map_err(|e| Error::NoGpu(e.to_string()))?;

        drop(probe_surface);
        probe.destroy();
        let _ = queue.flush();

        Ok(Self {
            connection,
            instance,
            adapter,
            device,
            queue: gpu_queue,
        })
    }

    /// Which GPU the picker is drawing with, for diagnostics.
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }
}

/// Freeze the desktop and let the user pick a colour on a warm context.
///
/// Blocks until the user commits or cancels.
pub fn run_picker_with(
    context: &PickerContext,
    capture: Capture,
    zoom: u32,
    average_size: u32,
    keys: LoupeKeys,
    palette: Option<Palette>,
) -> Result<Outcome> {
    if capture.frames.is_empty() {
        return Err(Error::NoFrame);
    }

    // The HUD is laid out in design pixels. Every monitor in a capture can have
    // its own scale, but the HUD only ever draws on the one the pointer is
    // over, so the largest is the right choice: sized for the densest display,
    // the panels stay crisp when the pointer crosses onto it.
    let scale = capture
        .frames
        .iter()
        .map(|f| f.monitor.scale_x() as f32)
        .fold(1.0f32, f32::max);
    let opened = std::time::Instant::now();

    let connection = &context.connection;
    let instance = &context.instance;

    // A fresh event queue per pick: surfaces are created and torn down each
    // time, and a queue outliving them would accumulate dead state.
    let (globals, mut queue) = registry_queue_init::<Picker>(connection)
        .map_err(|e| Error::Compositor(format!("registry init failed: {e}")))?;
    let qh: QueueHandle<Picker> = queue.handle();

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|_| Error::Unsupported("wl_compositor"))?;
    let layer_shell =
        LayerShell::bind(&globals, &qh).map_err(|_| Error::Unsupported("zwlr_layer_shell_v1"))?;
    let output_state = OutputState::new(&globals, &qh);

    let display_handle = display_handle_of(connection)?;

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

        let window_handle = window_handle_of(&surface)?;

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

    // The expensive part was paid once, when the context was built.
    let renderer = Renderer::from_device(context.device.clone(), context.queue.clone());

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

    // A palette pass gathers only what the caller still needs, so resuming a
    // half-built palette asks for the remaining slots rather than starting the
    // count again.
    let wanted = palette.as_ref().map_or(1, Palette::remaining);
    let session = Session::for_palette(capture, cursor, zoom, average_size, wanted);

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
        chrome: Chrome::new(scale, instructions(&keys, average_size, palette.is_some())),
        chrome_gpu: ChromeGpu::new(),
        palette,
        started: std::time::Instant::now(),
        keyboard: None,
        pointer: None,
        shift_held: false,
        scroll: 0.0,
        keys,
        exit: false,
    };

    tracing::info!(
        setup_ms = opened.elapsed().as_millis(),
        monitors = picker.overlays.len(),
        "overlay ready"
    );

    while !picker.exit {
        queue
            .blocking_dispatch(&mut picker)
            .map_err(|e| Error::Compositor(e.to_string()))?;
    }

    let outcome = picker
        .session
        .outcome()
        .cloned()
        .unwrap_or(Outcome::Cancelled);

    // Take the overlay down explicitly.
    //
    // Relying on `Drop` is not enough: the destroy requests it queues are only
    // buffered, and something has to push them to the compositor. When the
    // picker was a short-lived process this was invisible, because exiting
    // closed the connection and the compositor cleaned up. A resident picker
    // outlives the pick, so without this the frozen screen stays on top of
    // everything with the keyboard still grabbed.
    for overlay in picker.overlays.drain(..) {
        // wgpu first: it borrows the wl_surface underneath.
        drop(overlay.surface);
        // Attaching a null buffer unmaps the surface immediately, so the
        // screen is released before the round trip rather than after it.
        let wl_surface = overlay.layer.wl_surface().clone();
        wl_surface.attach(None, 0, 0);
        wl_surface.commit();
        drop(overlay.layer);
    }

    queue
        .roundtrip(&mut picker)
        .map_err(|e| Error::Compositor(format!("tearing down the overlay: {e}")))?;

    Ok(outcome)
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

        // The loupe and the HUD are drawn only on the monitor the pointer is
        // over; a radius of zero leaves the others as a plain frozen image.
        let here = self.pointer_on == Some(index);
        let scale = self
            .session
            .capture()
            .frames
            .get(overlay.frame_index)
            .map_or(1.0, |f| f.monitor.scale_x() as f32);

        let cursor = self.local_cursor(index).unwrap_or((0, 0));
        let view = LoupeView {
            cursor,
            zoom: self.session.zoom(),
            radius: if here {
                crate::hud::chrome::LOUPE_DIAMETER / 2.0 * scale
            } else {
                0.0
            },
            sample: self.session.sample_size(),
            grid: true,
            scale,
            vignette: crate::render::VIGNETTE,
        };

        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        if here {
            let state = HudState {
                colour: self
                    .session
                    .current_color()
                    .unwrap_or(pallet_color::Color::new(0, 0, 0)),
                zoom: self.session.zoom(),
                sample: self.session.sample_size(),
                tray: self.palette.as_ref().map(|p| Tray {
                    collected: p
                        .collected
                        .iter()
                        .copied()
                        .chain(self.session.taken().iter().map(|t| t.color))
                        .collect(),
                    target: p.target,
                }),
                clock: self.started.elapsed().as_secs_f32(),
            };
            let centre = (cursor.0 as f32 + 0.5, cursor.1 as f32 + 0.5);
            let layers = self
                .chrome
                .layers(&state, (overlay.width, overlay.height), centre);
            self.renderer.draw_hud(
                &overlay.screen,
                &target,
                view,
                &mut self.chrome_gpu,
                &layers,
            );
        } else {
            self.renderer.draw(&overlay.screen, &target, view);
        }
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
        // The tray's next slot pulses, so while a palette is being built the
        // overlay animates and must keep drawing even when nothing is moving.
        // A single pick stays entirely event-driven and idles at zero cost.
        let animating = self.palette.is_some() && self.pointer_on == Some(index);
        if let Some(overlay) = self.overlays.get_mut(index) {
            overlay.awaiting_frame = false;
            if !overlay.needs_redraw && !animating {
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
        let input = if binds(&event, &self.keys.cancel) {
            Some(Input::Cancel)
        } else if binds(&event, &self.keys.commit) {
            Some(Input::Commit)
        } else if binds(&event, &self.keys.save) {
            Some(Input::CommitAndSave)
        } else {
            match event.keysym {
                Keysym::Left => Some(Input::Nudge { dx: -1, dy: 0 }),
                Keysym::Right => Some(Input::Nudge { dx: 1, dy: 0 }),
                Keysym::Up => Some(Input::Nudge { dx: 0, dy: -1 }),
                Keysym::Down => Some(Input::Nudge { dx: 0, dy: 1 }),
                Keysym::plus | Keysym::equal | Keysym::KP_Add => Some(Input::ZoomIn),
                Keysym::minus | Keysym::KP_Subtract => Some(Input::ZoomOut),
                _ => None,
            }
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
                    let steps =
                        scroll_steps(vertical.discrete, vertical.absolute, &mut self.scroll);
                    for _ in 0..steps.unsigned_abs() {
                        self.session.apply(if steps > 0 {
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

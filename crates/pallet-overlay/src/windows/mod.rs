//! The picking overlay as a `winit` window per monitor.
//!
//! Windows has none of Wayland's restrictions on grabbing input, so this is
//! most of what the Linux backend's `smithay-client-toolkit` plumbing exists
//! to work around, for free: a topmost, borderless, fullscreen window per
//! monitor receives every key while it has focus, and `winit` hands `wgpu` a
//! surface directly rather than needing raw compositor handles assembled by
//! hand.
//!
//! `winit`'s 0.30 API runs one [`ApplicationHandler`] per call to
//! [`EventLoopExtRunOnDemand::run_app_on_demand`], which is exactly the shape
//! a resident picker needs: the event loop — and the GPU device built against
//! it — are created once in [`PickerContext::new`], and each pick reruns the
//! same loop to completion and returns, leaving it ready for the next one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use pallet_capture::Capture;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFOEXW};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::monitor::MonitorHandle;
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::platform::windows::MonitorHandleExtWindows;
use winit::window::{Fullscreen, Window, WindowId, WindowLevel};

use crate::Palette;
use crate::error::{Error, Result};
use crate::hud::chrome::{Chrome, HudState, Tray};
use crate::keys::{LoupeKeys, instructions, name_binds, scroll_steps};
use crate::render::{ChromeGpu, LoupeView, Renderer, Screen};
use crate::session::{Input, Outcome, Session};

/// A comparable name for a key press, in the same vocabulary
/// [`crate::keys::name_binds`] expects.
fn key_name(event: &winit::event::KeyEvent) -> String {
    match &event.logical_key {
        Key::Named(NamedKey::Enter) => "RETURN".into(),
        Key::Named(NamedKey::Escape) => "ESCAPE".into(),
        Key::Named(NamedKey::Space) => "SPACE".into(),
        Key::Named(NamedKey::Tab) => "TAB".into(),
        Key::Named(NamedKey::Backspace) => "BACKSPACE".into(),
        Key::Named(NamedKey::ArrowLeft) => "LEFT".into(),
        Key::Named(NamedKey::ArrowRight) => "RIGHT".into(),
        Key::Named(NamedKey::ArrowUp) => "UP".into(),
        Key::Named(NamedKey::ArrowDown) => "DOWN".into(),
        Key::Character(s) => s.to_uppercase(),
        _ => String::new(),
    }
}

/// Whether a press matches a configured binding.
fn binds(event: &winit::event::KeyEvent, binding: &str) -> bool {
    name_binds(&key_name(event), binding)
}

/// The `\\.\DISPLAY1`-style device name behind a `winit` monitor handle.
///
/// Matched against [`pallet_capture::Monitor::id`], which DXGI derives from
/// the same GDI device name — the only identifier both sides of this crate's
/// platform split agree on. Comparing geometry instead is the tempting
/// shortcut, and the wrong one: a mismatch there — DPI rounding, a rotated
/// panel, whichever side samples first during a mode change — silently
/// hands one monitor's frozen pixels to another monitor's window rather than
/// failing loudly.
fn device_name(handle: &MonitorHandle) -> Option<String> {
    let mut info = MONITORINFOEXW {
        monitorInfo: windows::Win32::Graphics::Gdi::MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    let hmonitor = windows::Win32::Graphics::Gdi::HMONITOR(handle.hmonitor() as *mut _);
    // SAFETY: `info` is sized and zeroed per `GetMonitorInfoW`'s contract,
    // and `hmonitor` came from `winit`'s own enumeration, so it names a
    // monitor that exists for the duration of this call.
    if !unsafe { GetMonitorInfoW(hmonitor, &mut info.monitorInfo) }.as_bool() {
        return None;
    }

    let len = info
        .szDevice
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(info.szDevice.len());
    Some(String::from_utf16_lossy(&info.szDevice[..len]))
}

/// A warm picker: an event loop and a live GPU device, kept between picks.
///
/// Building the GPU context costs about 220 ms — measured on Linux, and
/// expected to be the same order here — against tens of milliseconds to
/// capture the screen. Keeping it alive is the difference between a hotkey
/// that feels instant and one that feels broken, and is the reason the
/// picker is a resident process at all.
pub struct PickerContext {
    event_loop: RefCell<EventLoop<()>>,
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

/// Opens one hidden window just long enough to pick a GPU adapter that can
/// actually present, the same reason the Linux backend opens a throwaway
/// surface before requesting an adapter.
#[derive(Default)]
struct Probe {
    window: Option<Arc<Window>>,
}

impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_visible(false)
            .with_inner_size(PhysicalSize::new(1u32, 1u32));
        self.window = event_loop.create_window(attrs).ok().map(Arc::new);
        event_loop.exit();
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}

impl PickerContext {
    /// Open a window session and build a GPU device. Done once.
    pub fn new() -> Result<Self> {
        let mut event_loop = EventLoop::new()
            .map_err(|e| Error::Compositor(format!("could not open a window session: {e}")))?;

        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

        let mut probe = Probe::default();
        event_loop
            .run_app_on_demand(&mut probe)
            .map_err(|e| Error::Compositor(format!("probe window: {e}")))?;
        let window = probe
            .window
            .ok_or_else(|| Error::Compositor("could not open a probe window".into()))?;

        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| Error::Compositor(format!("could not create a probe surface: {e}")))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| Error::NoGpu(e.to_string()))?;

        // `downlevel_defaults()` caps texture dimensions at 2048px, which
        // any monitor at 4K or wider already exceeds; the adapter's own
        // limits are whatever this GPU actually supports.
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pallet-picker"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| Error::NoGpu(e.to_string()))?;

        drop(surface);
        drop(window);

        Ok(Self {
            event_loop: RefCell::new(event_loop),
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// Which GPU the picker is drawing with, for diagnostics.
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }
}

/// One monitor's overlay window and the pixels frozen on it.
struct Overlay {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    screen: Screen,
    /// Index into the capture's frames.
    frame_index: usize,
    configured: bool,
    width: u32,
    height: u32,
}

/// The running picker, driven by `winit`'s event loop for one pick.
struct PickerApp<'a> {
    context: &'a PickerContext,
    renderer: Renderer,
    session: Session,

    overlays: Vec<Overlay>,
    window_index: HashMap<WindowId, usize>,
    /// Which overlay the pointer is currently over.
    pointer_on: Option<usize>,

    chrome: Chrome,
    chrome_gpu: ChromeGpu,
    palette: Option<Palette>,
    started: std::time::Instant,
    opened: std::time::Instant,

    shift_held: bool,
    /// Continuous scroll not yet spent on a zoom step.
    scroll: f32,
    keys: LoupeKeys,
    windows_created: bool,
}

impl PickerApp<'_> {
    /// Which overlay owns a window.
    fn overlay_for(&self, id: WindowId) -> Option<usize> {
        self.window_index.get(&id).copied()
    }

    fn mark_dirty(&self, index: usize) {
        if let Some(overlay) = self.overlays.get(index) {
            overlay.window.request_redraw();
        }
    }

    fn mark_all_dirty(&self) {
        for index in 0..self.overlays.len() {
            self.mark_dirty(index);
        }
    }

    /// The cursor in one overlay's physical pixels, if it is on that monitor.
    fn local_cursor(&self, index: usize) -> Option<(u32, u32)> {
        let overlay = self.overlays.get(index)?;
        let frame = self.session.capture().frames.get(overlay.frame_index)?;
        let (x, y) = self.session.cursor();
        frame.monitor.to_pixel(x, y)
    }

    fn configure_surface(&mut self, index: usize, width: u32, height: u32) {
        let Some(overlay) = self.overlays.get_mut(index) else {
            return;
        };
        overlay.width = width;
        overlay.height = height;
        overlay.configured = width > 0 && height > 0;
        if !overlay.configured {
            return;
        }

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: crate::render::FORMAT,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        overlay.surface.configure(self.renderer.device(), &config);
    }

    fn redraw(&mut self, index: usize) {
        let Some(overlay) = self.overlays.get(index) else {
            return;
        };
        if !overlay.configured || overlay.width == 0 || overlay.height == 0 {
            return;
        }

        let frame = match overlay.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                tracing::debug!("skipped a frame: {other:?}");
                return;
            }
        };

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
    }

    /// Take a key press and turn it into a session input, the same table the
    /// Linux backend's keyboard handler uses.
    fn handle_key(&mut self, event: &winit::event::KeyEvent) {
        let input = if binds(event, &self.keys.cancel) {
            Some(Input::Cancel)
        } else if binds(event, &self.keys.commit) {
            Some(Input::Commit)
        } else if binds(event, &self.keys.save) {
            Some(Input::CommitAndSave)
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowLeft) => Some(Input::Nudge { dx: -1, dy: 0 }),
                Key::Named(NamedKey::ArrowRight) => Some(Input::Nudge { dx: 1, dy: 0 }),
                Key::Named(NamedKey::ArrowUp) => Some(Input::Nudge { dx: 0, dy: -1 }),
                Key::Named(NamedKey::ArrowDown) => Some(Input::Nudge { dx: 0, dy: 1 }),
                Key::Character(s) if s == "+" || s == "=" => Some(Input::ZoomIn),
                Key::Character(s) if s == "-" => Some(Input::ZoomOut),
                _ => None,
            }
        };

        if let Some(input) = input {
            self.session.apply(input);
            self.mark_all_dirty();
        }
    }
}

impl ApplicationHandler for PickerApp<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.windows_created {
            return;
        }
        self.windows_created = true;

        let monitors: Vec<(MonitorHandle, Option<String>)> = event_loop
            .available_monitors()
            .map(|h| {
                let name = device_name(&h);
                (h, name)
            })
            .collect();
        let frames: Vec<_> = self.session.capture().frames.to_vec();

        for (frame_index, frame) in frames.iter().enumerate() {
            let m = &frame.monitor;
            let handle = monitors
                .iter()
                .find(|(_, name)| name.as_deref() == Some(m.id.as_str()))
                .map(|(h, _)| h);

            let mut attrs = Window::default_attributes()
                .with_title("Pallet")
                .with_decorations(false)
                .with_resizable(false)
                .with_visible(true);
            attrs = match handle {
                Some(h) => attrs.with_fullscreen(Some(Fullscreen::Borderless(Some(h.clone())))),
                // A monitor DXGI captured but `winit` cannot find by device
                // name is placed by geometry instead of giving up on it
                // entirely — degraded, since a manually sized and positioned
                // window skips whatever `Fullscreen::Borderless` does to
                // guarantee full monitor coverage, but still usable.
                None => {
                    tracing::warn!(
                        monitor = %m.id,
                        "winit did not report this display; placing its overlay by geometry"
                    );
                    attrs
                        .with_position(PhysicalPosition::new(m.logical_x, m.logical_y))
                        .with_inner_size(PhysicalSize::new(
                            m.pixel_width.max(1),
                            m.pixel_height.max(1),
                        ))
                }
            };

            let window = match event_loop.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    tracing::warn!("could not open an overlay window: {e}");
                    continue;
                }
            };
            window.set_window_level(WindowLevel::AlwaysOnTop);

            let surface = match self.context.instance.create_surface(Arc::clone(&window)) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("could not create a GPU surface: {e}");
                    continue;
                }
            };

            let screen = match self.renderer.create_screen(frame) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("could not upload a frame: {e}");
                    continue;
                }
            };

            let size = window.inner_size();
            self.window_index.insert(window.id(), self.overlays.len());
            self.overlays.push(Overlay {
                window,
                surface,
                screen,
                frame_index,
                configured: false,
                width: 0,
                height: 0,
            });
            let index = self.overlays.len() - 1;
            self.configure_surface(index, size.width, size.height);
            // `winit` does not promise an initial `RedrawRequested` just
            // because a window was created visible — without asking
            // explicitly, a monitor the pointer never happens to cross stays
            // black until something else on it triggers a redraw.
            self.mark_dirty(index);
        }

        // The first pointer event corrects the seeded cursor; focus must land
        // somewhere before that happens so the very first key press works.
        if let Some(first) = self.overlays.first() {
            first.window.focus_window();
        }

        tracing::info!(
            setup_ms = self.opened.elapsed().as_millis(),
            monitors = self.overlays.len(),
            "overlay ready"
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(index) = self.overlay_for(window_id) else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                self.session.apply(Input::Cancel);
            }
            WindowEvent::Resized(size) => {
                self.configure_surface(index, size.width, size.height);
                self.mark_dirty(index);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(frame) = self
                    .session
                    .capture()
                    .frames
                    .get(self.overlays[index].frame_index)
                {
                    let (mx, my) = (frame.monitor.logical_x, frame.monitor.logical_y);
                    let was_on = self.pointer_on;
                    self.pointer_on = Some(index);
                    self.session.apply(Input::PointerTo {
                        x: mx + position.x as i32,
                        y: my + position.y as i32,
                    });
                    self.mark_dirty(index);
                    // The monitor the pointer left must be redrawn once to
                    // erase the loupe from it.
                    if let Some(previous) = was_on
                        && was_on != self.pointer_on
                    {
                        self.mark_dirty(previous);
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if self.pointer_on == Some(index) {
                    self.pointer_on = None;
                    self.mark_dirty(index);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                // Left commits, anything else cancels.
                self.session.apply(if button == MouseButton::Left {
                    Input::Commit
                } else {
                    Input::Cancel
                });
                self.mark_all_dirty();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // A forward/away-from-the-user notch is positive here, the
                // same sense as "scroll up to zoom in"; `scroll_steps` was
                // written for `wl_pointer`'s opposite, downward-positive
                // convention, hence the sign flip on both branches. Unverified
                // on real hardware — flip if a release build zooms backwards.
                let (discrete, absolute) = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (-y.round() as i32, 0.0),
                    MouseScrollDelta::PixelDelta(p) => (0, -p.y),
                };
                let steps = scroll_steps(discrete, absolute, &mut self.scroll);
                for _ in 0..steps.unsigned_abs() {
                    self.session.apply(if steps > 0 {
                        Input::ZoomIn
                    } else {
                        Input::ZoomOut
                    });
                }
                if steps != 0 {
                    self.mark_all_dirty();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.handle_key(&event);
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                // Shift switches to averaging a square, per the prototype.
                let shift = mods.state().shift_key();
                if shift != self.shift_held {
                    self.shift_held = shift;
                    self.session.apply(Input::Averaging(shift));
                    self.mark_all_dirty();
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw(index);
            }
            _ => {}
        }

        if self.session.is_finished() {
            // Hidden before the loop exits rather than left for `app`'s
            // `Drop`: a window mid-teardown is not somewhere the compositor
            // guarantees a redraw would still land, and a stale, click-through
            // frozen desktop lingering on screen for even one more frame reads
            // as the overlay failing to close, not as it about to. See
            // `run_picker_with` for the mouse-capture half of that same
            // symptom — this window's captured button-down until its
            // corresponding up.
            for overlay in &self.overlays {
                overlay.window.set_visible(false);
            }
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Nothing left to animate once the pick is over, and requesting a
        // redraw here would just race the teardown in `window_event` above.
        if self.session.is_finished() {
            return;
        }

        // The tray's next slot pulses, so while a palette is being built the
        // overlay must keep animating even when nothing is moving. A single
        // pick stays entirely event-driven and idles at zero cost.
        if self.palette.is_some()
            && let Some(index) = self.pointer_on
        {
            self.mark_dirty(index);
        }
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

    // The HUD is laid out in design pixels. Every monitor in a capture can
    // have its own scale, but the HUD only ever draws on the one the pointer
    // is over, so the largest is the right choice: sized for the densest
    // display, the panels stay crisp when the pointer crosses onto it.
    let scale = capture
        .frames
        .iter()
        .map(|f| f.monitor.scale_x() as f32)
        .fold(1.0f32, f32::max);

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
    // half-built palette asks for the remaining slots rather than starting
    // the count again.
    let wanted = palette.as_ref().map_or(1, Palette::remaining);
    let session = Session::for_palette(capture, cursor, zoom, average_size, wanted);

    let mut app = PickerApp {
        context,
        renderer,
        session,
        overlays: Vec::new(),
        window_index: HashMap::new(),
        pointer_on: None,
        chrome: Chrome::new(scale, instructions(&keys, average_size, palette.is_some())),
        chrome_gpu: ChromeGpu::new(),
        palette,
        started: std::time::Instant::now(),
        opened: std::time::Instant::now(),
        shift_held: false,
        scroll: 0.0,
        keys,
        windows_created: false,
    };

    context
        .event_loop
        .borrow_mut()
        .run_app_on_demand(&mut app)
        .map_err(|e| Error::Compositor(format!("overlay event loop: {e}")))?;

    // The click that just finished the session left its window holding the
    // mouse button's capture — set automatically on button-down, and never
    // released because that window is destroyed below before the matching
    // button-up arrives. Windows will not deliver input to *any* window in
    // this process again until that capture is released, which is what made
    // this hang rather than merely leave a dangling window: the whole app
    // reads as "Not Responding" and needs Task Manager to end it. Releasing
    // it here, unconditionally and regardless of which window (if any)
    // holds it, is the documented fix and cheap enough to always run.
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
    }

    // Every overlay window is owned by `app` and closes as it drops here;
    // `winit` on Windows tears down the native window synchronously, unlike
    // Wayland's buffered destroy requests that need an explicit round trip.
    Ok(app.session.outcome().cloned().unwrap_or(Outcome::Cancelled))
}

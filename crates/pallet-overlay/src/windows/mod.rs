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
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongW,
    WS_EX_LAYERED,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::{Window, WindowId, WindowLevel};

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

/// The raw `HWND` behind a `winit` window, for the Win32 calls `winit`
/// doesn't expose a safe wrapper for.
fn hwnd_of(window: &Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}

/// Make a window fully transparent without touching `WS_VISIBLE`.
///
/// A window has to keep receiving `WM_PAINT` for `winit` to ever deliver
/// `RedrawRequested` — Win32 simply never paints a window created (or later
/// made) invisible, so hiding it the obvious way (`Window::set_visible`)
/// would freeze the overlay forever, one render short of ever showing
/// itself. A layered window sidesteps that: it stays visible and paintable,
/// but composites as fully transparent, so nothing appears on screen until
/// [`reveal`] flips it opaque once the frozen desktop is actually on it.
fn hide_via_transparency(window: &Window) {
    let Some(hwnd) = hwnd_of(window) else { return };
    // SAFETY: `hwnd` is a live top-level window owned by this process for as
    // long as `window` is alive.
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED.0 as i32);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
    }
}

/// Undo [`hide_via_transparency`], snapping the window to fully opaque.
///
/// The constant alpha goes to 255 *and* `WS_EX_LAYERED` comes back off. Only
/// the first is needed to make the window visible, but leaving the style on
/// keeps the window composited through DWM's layered path for the rest of the
/// pick, which is not a combination a flip-model DXGI swapchain is meant to
/// present into. Dropping it puts the window back on the ordinary path the
/// swapchain expects, and costs nothing — transparency has done its job by
/// the time this runs.
fn reveal(window: &Window) {
    let Some(hwnd) = hwnd_of(window) else { return };
    // SAFETY: same as `hide_via_transparency`.
    unsafe {
        // Opaque first: a stale constant alpha of 0 must not outlive the
        // style that gives it meaning.
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_LAYERED.0 as i32));
    }
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
    /// Whether the window has been revealed yet. Kept transparent until its
    /// first frame is presented so Windows never composites a blank default
    /// frame while the surface is still being configured.
    shown: bool,
    /// Set whenever this overlay is asked to redraw, cleared only once a
    /// frame is actually presented. `request_redraw` alone is fire-and-
    /// forget: a monitor the pointer never crosses gets exactly one
    /// `RedrawRequested`, from the initial `mark_dirty` in `resumed`, and if
    /// that one skips a frame (`get_current_texture` returning anything but
    /// success) nothing ever asks it to try again — it was one bad frame
    /// away from staying stuck forever, rescued only by whichever monitor
    /// happens to be under the cursor getting a continuous, self-healing
    /// stream of redraws from `CursorMoved`. This flag makes every overlay
    /// get that same persistent retry, driven from `about_to_wait`.
    needs_redraw: bool,
    /// When this overlay stops re-presenting after [`reveal`], or `None`
    /// once it has.
    ///
    /// Every frame presented before the reveal went into a window DXGI had
    /// every reason to call occluded, and an occluded present is discarded
    /// rather than composited — see `PickerApp::present_frame`. These are
    /// the frames that re-present the same content once the window is
    /// genuinely visible, so what DWM composites is the frozen desktop
    /// rather than whatever the redirection surface happened to hold.
    warmup_until: Option<std::time::Instant>,
    /// When the next warmup frame is due. See [`WARMUP_INTERVAL`].
    warmup_next: std::time::Instant,
}

/// How long an overlay keeps re-presenting after being revealed.
///
/// This was a count of frames before it was a span of time, and the
/// difference is the whole point. A brand-new `HWND` takes DWM a moment to
/// take up, and until it has, presenting into it achieves nothing no matter
/// how many times it is done — which is exactly why two frames was enough on
/// every pick after the first (warm windows) and not enough on the first
/// (cold ones). Only the monitor under the pointer was ever reliably rescued,
/// and only because `CursorMoved` happens to re-present it for as long as the
/// hand on the mouse keeps moving. Hold still on the first pick and it went
/// black like all the rest.
const WARMUP_SPAN: std::time::Duration = std::time::Duration::from_millis(200);

/// How far apart warmup frames are spaced.
///
/// Presenting as fast as the loop can turn would put every warmup frame
/// inside a single composition pass and prove nothing; roughly one display
/// refresh apart spreads them across passes, which is the only thing that
/// gives DWM a chance to have caught up in between. Paced from
/// `about_to_wait` with `ControlFlow::WaitUntil` rather than by blocking on
/// each present, so the overlays still reveal in parallel instead of one
/// monitor's pacing delaying the next one's first frame.
const WARMUP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

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

    fn mark_dirty(&mut self, index: usize) {
        if let Some(overlay) = self.overlays.get_mut(index) {
            overlay.needs_redraw = true;
            overlay.window.request_redraw();
        }
    }

    fn mark_all_dirty(&mut self) {
        for index in 0..self.overlays.len() {
            self.mark_dirty(index);
        }
    }

    /// The cursor in one overlay's *displayed* pixels, if it is on that
    /// monitor.
    ///
    /// Displayed rather than raw-framebuffer coordinates because this positions
    /// the loupe and the HUD on a surface that is itself in the desktop's
    /// rotated layout; the two only coincide on an unrotated display.
    fn local_cursor(&self, index: usize) -> Option<(u32, u32)> {
        let overlay = self.overlays.get(index)?;
        let frame = self.session.capture().frames.get(overlay.frame_index)?;
        let (x, y) = self.session.cursor();
        frame.monitor.to_displayed(x, y)
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

        // Asked for rather than assumed. This surface shows a frozen desktop,
        // so there is no animation to tear and nothing to gain from pacing to
        // the display, and `Mailbox` is worth having: `Fifo` blocks each
        // present on a vblank, and since every overlay presents from the same
        // thread, one monitor's wait is added to the next monitor's
        // time-to-first-frame. But *which* backend this picker ended up on is
        // not something it chooses — `new_without_display_handle_from_env`
        // leaves that open — and configuring a surface with a present mode it
        // does not advertise is a hard failure, not a downgrade. `Fifo` is the
        // one mode every backend must support, so it is the floor.
        let modes = overlay.surface.get_capabilities(&self.context.adapter);
        let present_mode = if modes.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: crate::render::FORMAT,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        overlay.surface.configure(self.renderer.device(), &config);
        tracing::debug!(?present_mode, width, height, "surface configured");
    }

    /// Draw this overlay's frame and put it on screen, reporting whether a
    /// frame was actually submitted.
    ///
    /// # Why the return value is not the whole truth
    ///
    /// `false` means no frame was submitted at all, which is the caller's cue
    /// to leave the overlay dirty and try again. `true` means `present` was
    /// *called* — which is weaker than it sounds, and is the reason this is
    /// split out of [`Self::redraw`] at all.
    ///
    /// `wgpu`'s DX12 backend presents with `IDXGISwapChain3::Present` and
    /// treats the result as a plain `Result`. `DXGI_STATUS_OCCLUDED` — "this
    /// window is not visible, so the frame was dropped rather than
    /// composited" — is a *success* HRESULT, so it arrives here as `Ok`,
    /// indistinguishable from a frame that reached the screen. A window held
    /// at `LWA_ALPHA` 0 by [`hide_via_transparency`] is about as occluded as
    /// a window gets, so every frame drawn before the reveal should be
    /// assumed thrown away.
    ///
    /// That is the whole of the "away monitors render half black, half white
    /// after the first pick" bug: the monitor under the pointer gets a
    /// continuous stream of presents from `CursorMoved` and so re-presents
    /// itself into visibility within a frame, while every other monitor
    /// presented exactly once — before the reveal, into the void — and then
    /// sat on the uninitialized redirection surface that composite left
    /// behind. `needs_redraw` could not rescue it because, as far as this
    /// code could tell, the present had succeeded. Hence
    /// [`Overlay::warmup`]: the frames that go up *after* the window is
    /// something DWM will composite.
    fn present_frame(&mut self, index: usize) -> bool {
        let Some(overlay) = self.overlays.get(index) else {
            return false;
        };
        if !overlay.configured || overlay.width == 0 || overlay.height == 0 {
            return false;
        }

        let frame = match overlay.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                tracing::debug!("skipped a frame: {other:?}");
                return false;
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
        true
    }

    /// Put this overlay's current frame on screen, revealing the window on
    /// the way if this is its first.
    fn redraw(&mut self, index: usize) {
        if !self.present_frame(index) {
            // See `Overlay::needs_redraw`: a frame that never got submitted
            // leaves the overlay dirty so `about_to_wait` tries again,
            // rather than stranding it on whatever it last managed to show.
            return;
        }
        // Submitted — but not necessarily composited, and while the window
        // is still transparent, almost certainly not. See `present_frame`.
        self.overlays[index].needs_redraw = false;

        // Only revealed now, after the frozen desktop is actually on the
        // surface — revealing it any earlier let Windows composite a blank
        // default frame for the gap between window creation and this point.
        if !self.overlays[index].shown {
            self.overlays[index].shown = true;
            reveal(&self.overlays[index].window);

            // The frame above went up while this window was transparent and
            // was therefore very likely dropped as occluded, so the first
            // thing to do with a now-visible window is draw it again —
            // immediately and in this same call, not a loop turn later, so
            // there is no composition pass where the window is opaque but
            // has nothing of ours on it. `WARMUP_SPAN` covers the rest, for
            // as long as it takes DWM to take up a window this new.
            let _ = self.present_frame(index);
            let now = std::time::Instant::now();
            self.overlays[index].warmup_until = Some(now + WARMUP_SPAN);
            self.overlays[index].warmup_next = now + WARMUP_INTERVAL;

            // Focus couldn't usefully land on a window nobody can see;
            // give it here instead, on the same window `resumed` picked.
            if index == 0 {
                self.overlays[index].window.focus_window();
            }
        }
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

        let frames: Vec<_> = self.session.capture().frames.to_vec();

        for (frame_index, frame) in frames.iter().enumerate() {
            let m = &frame.monitor;

            // Positioned and sized by geometry rather than through
            // `Fullscreen::Borderless`: entering borderless fullscreen on a
            // monitor that isn't the one currently under the cursor is a
            // known Windows quirk where the OS defers the transition until
            // that monitor gets some input, which is exactly the "second
            // monitor needs to be hovered before it appears" symptom. A
            // plain top-level window sized to the monitor's own rect sidesteps
            // that negotiation (and its transition animation) entirely.
            //
            // `logical_x/y/width/height` are the desktop's upright layout —
            // 1440x2560 for a portrait monitor — which is what a window on
            // screen needs; `pixel_width/height` is the *raw* framebuffer
            // behind the rotation (2560x1440 for that same monitor) and
            // would size the window sideways.
            //
            // Created visible, then made transparent (not hidden) right
            // below — see `hide_via_transparency` for why an actually
            // hidden window would never be able to show itself again.
            let attrs = Window::default_attributes()
                .with_title("Pallet")
                .with_decorations(false)
                .with_resizable(false)
                .with_visible(true)
                .with_position(PhysicalPosition::new(m.logical_x, m.logical_y))
                .with_inner_size(PhysicalSize::new(
                    m.logical_width.max(1),
                    m.logical_height.max(1),
                ));

            // Timed per stage rather than per monitor: "a second and a bit
            // before the first surface exists" is not an actionable
            // measurement, and guessing which of these three it was is how
            // you end up optimising the one that was already fast.
            let began = std::time::Instant::now();
            let window = match event_loop.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    tracing::warn!("could not open an overlay window: {e}");
                    continue;
                }
            };
            window.set_window_level(WindowLevel::AlwaysOnTop);
            hide_via_transparency(&window);
            let window_ms = began.elapsed().as_millis();

            let began = std::time::Instant::now();
            let surface = match self.context.instance.create_surface(Arc::clone(&window)) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("could not create a GPU surface: {e}");
                    continue;
                }
            };
            let surface_ms = began.elapsed().as_millis();

            let began = std::time::Instant::now();
            let screen = match self.renderer.create_screen(frame) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("could not upload a frame: {e}");
                    continue;
                }
            };
            let screen_ms = began.elapsed().as_millis();
            tracing::debug!(
                monitor = frame_index,
                transform = ?frame.monitor.transform,
                window_ms,
                surface_ms,
                screen_ms,
                "overlay built"
            );

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
                shown: false,
                needs_redraw: true,
                warmup_until: None,
                warmup_next: std::time::Instant::now(),
            });
            let index = self.overlays.len() - 1;
            self.configure_surface(index, size.width, size.height);
            // `winit` does not promise an initial `RedrawRequested` on its
            // own, and the window stays transparent (and unfocused, see
            // `redraw`) until one lands — without asking explicitly, a
            // monitor the pointer never happens to cross would never reveal
            // its overlay.
            self.mark_dirty(index);
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Nothing left to animate once the pick is over, and requesting a
        // redraw here would just race the teardown in `window_event` above.
        if self.session.is_finished() {
            return;
        }

        // Pace each freshly revealed overlay's warmup frames. See
        // `WARMUP_SPAN`: these are what get the frozen desktop onto a monitor
        // that has no pointer on it to redraw it, whether that is a second
        // monitor or — on the first pick, with a hand that never moved — the
        // one the pointer is already sitting on.
        let now = std::time::Instant::now();
        let mut wake: Option<std::time::Instant> = None;
        for index in 0..self.overlays.len() {
            let Some(until) = self.overlays[index].warmup_until else {
                continue;
            };
            if now >= until {
                self.overlays[index].warmup_until = None;
                continue;
            }
            if now >= self.overlays[index].warmup_next {
                self.overlays[index].warmup_next = now + WARMUP_INTERVAL;
                self.mark_dirty(index);
            }
            let due = self.overlays[index].warmup_next.min(until);
            wake = Some(wake.map_or(due, |w| w.min(due)));
        }
        // `Wait` once the last warmup is spent: a pick that is just sitting
        // there should cost nothing, which is the whole reason the overlay is
        // event-driven. `request_redraw` wakes the loop regardless of this,
        // so the palette pulse below is unaffected either way.
        event_loop.set_control_flow(match wake {
            Some(wake) => ControlFlow::WaitUntil(wake),
            None => ControlFlow::Wait,
        });

        // Retried here rather than left to `CursorMoved`, `Resized`, and the
        // rest of `window_event`: those cover redrawing an overlay whose
        // content needs to change, not recovering one whose last attempt
        // didn't land. Without this, only the monitor under the pointer got
        // that recovery, for free, from its own continuous stream of
        // `CursorMoved`-driven redraws — see `Overlay::needs_redraw`.
        for index in 0..self.overlays.len() {
            if self.overlays[index].needs_redraw {
                self.mark_dirty(index);
            }
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
    let outcome = app.session.outcome().cloned().unwrap_or(Outcome::Cancelled);
    drop(app);

    // Dropping the overlays above only *marks* their textures and surfaces for
    // destruction: `wgpu` defers the actual release until the device is next
    // polled or something else is submitted to it. A resident picker does
    // neither between picks — it goes straight back to blocking on its socket
    // — so without this, two 4K textures per pick stay held until the *next*
    // pick happens to submit something, which is the one moment they are most
    // in the way.
    //
    // Housekeeping, not a fix for anything measured. It was added chasing a
    // suspected slowdown across successive picks that turned out not to exist:
    // resident size after eight picks was lower than at rest, and `setup_ms`
    // showed no trend across them, only the same spread that `capture_ms`
    // shows on code this does not touch. Kept because reclaiming promptly is
    // the right shape for a process that lives for days between picks, and
    // because it is free — the overlay is already off screen and the colour
    // already in hand, so the only thing still waiting is the next pick.
    if let Err(e) = context.device.poll(wgpu::PollType::wait_indefinitely()) {
        tracing::warn!("could not reclaim the last pick's GPU memory: {e}");
    }

    Ok(outcome)
}

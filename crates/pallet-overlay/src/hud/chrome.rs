//! The HUD's panels, laid out from `Prototype/Pallet Pick.dc.html`.
//!
//! Every metric here is transcribed from that file's inline CSS in design
//! pixels, then multiplied by the display scale. Keeping the numbers literal
//! makes the source easy to diff against the design when it changes.
//!
//! Each panel is drawn into its own small bitmap with a transparent margin for
//! its shadow, and the GPU composites those over the frozen screen. Panels are
//! cached on the state they were drawn from, so moving the cursor across a
//! run of identical pixels re-rasterises nothing.

use pallet_color::Color;

use super::paint::{Bitmap, Rect, Rgba, Shadow};
use super::text::{self, Face, Run};

/// `rgba(16,17,21,.9)` — the readout pill.
const PANEL_STRONG: Rgba = Rgba(16, 17, 21, 230);
/// `rgba(16,17,21,.82)` — the instruction pill, zoom badge and tray.
const PANEL: Rgba = Rgba(16, 17, 21, 209);

/// The loupe's diameter in design pixels: 11 cells of 16px.
pub const LOUPE_DIAMETER: f32 = 176.0;

/// What the HUD is showing.
#[derive(Debug, Clone, PartialEq)]
pub struct HudState {
    /// The colour under the cursor.
    pub colour: Color,
    /// Current magnification, for the badge.
    pub zoom: u32,
    /// The width of the averaged sample square, or 1 for a single pixel.
    pub sample: u32,
    /// Multi-pick progress, or `None` for an ordinary single pick.
    ///
    /// The design shows the tray unconditionally, but the picker is also the
    /// tool for grabbing one colour, and a "Palette 1 / 5" tray on a single
    /// pick would promise a flow the user did not ask for.
    pub tray: Option<Tray>,
    /// Animation phase in seconds, for the tray's pulsing next slot.
    pub clock: f32,
}

/// Progress through a multi-pick palette, as the tray shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tray {
    /// Filled slots, in order: colours the caller already held followed by the
    /// ones taken in this pass.
    pub collected: Vec<Color>,
    /// How many slots the tray shows.
    pub target: usize,
}

/// A rasterised panel and the transparent margin its shadow needs.
///
/// The margin is tracked separately because the design positions panels by
/// their visible box — `top:22px`, `bottom:22px` — and a shadow that spills
/// 30px past the panel would otherwise push it that far out of place.
#[derive(Debug, Clone)]
struct Panel {
    bitmap: Bitmap,
    pad: f32,
    /// Bumped every time the pixels are redrawn.
    ///
    /// The overlay redraws on every pointer move, but most panels are
    /// identical from one frame to the next. Without this the GPU would
    /// re-upload all of them every frame — about 280 KB of texture writes to
    /// change a six-character hex string.
    version: u64,
}

impl Panel {
    /// The visible box's width, excluding the shadow margin.
    fn width(&self) -> f32 {
        self.bitmap.width as f32 - self.pad * 2.0
    }

    /// The visible box's height.
    fn height(&self) -> f32 {
        self.bitmap.height as f32 - self.pad * 2.0
    }
}

/// One rasterised panel and where it goes, in physical pixels on the screen
/// the cursor is currently over.
///
/// Borrows from the [`Chrome`] that built it: these are produced every frame
/// and copying a few hundred kilobytes of pixels each time is most of what
/// made the HUD expensive to draw.
#[derive(Debug)]
pub struct Layer<'a> {
    /// The panel's pixels, straight alpha.
    pub bitmap: &'a Bitmap,
    /// Left edge on screen.
    pub x: i32,
    /// Top edge on screen.
    pub y: i32,
    /// Identifies this panel's contents; unchanged means no re-upload needed.
    pub version: u64,
}

/// Builds and caches the HUD's panels.
#[derive(Debug)]
pub struct Chrome {
    scale: f32,
    instructions: String,
    cached_instructions: Option<Panel>,
    /// The readout's pill, shadow and background, without the colour on it.
    ///
    /// Rasterising this costs a Gaussian blur, and it is identical for every
    /// colour, so it is drawn once and the swatch and text are stamped onto a
    /// copy of it.
    readout_base: Option<Panel>,
    cached_readout: Option<(Color, Panel)>,
    /// The tray's panel and shadow, keyed on how many slots it has.
    tray_base: Option<(usize, Panel)>,
    cached_badge: Option<(u32, Panel)>,
    cached_tray: Option<(Tray, u32, Panel)>,
    next_version: u64,
}

impl Chrome {
    /// A HUD drawn at `scale` physical pixels per design pixel, labelled with
    /// the keys the picker actually answers to.
    pub fn new(scale: f32, instructions: String) -> Self {
        Self {
            scale: scale.max(0.5),
            instructions,
            cached_instructions: None,
            readout_base: None,
            cached_readout: None,
            tray_base: None,
            cached_badge: None,
            cached_tray: None,
            next_version: 0,
        }
    }

    /// A version number no previous panel has used.
    fn bump(&mut self) -> u64 {
        self.next_version += 1;
        self.next_version
    }

    fn px(&self, design: f32) -> f32 {
        design * self.scale
    }

    /// Every panel to draw this frame, positioned for a screen of the given
    /// size with the loupe centred at `loupe`.
    pub fn layers(
        &mut self,
        state: &HudState,
        screen: (u32, u32),
        loupe: (f32, f32),
    ) -> Vec<Layer<'_>> {
        // Rasterise first, place second. Splitting the two is what lets the
        // returned layers borrow the cached panels: placement needs every
        // panel borrowed at once, which cannot overlap with the `&mut self`
        // that redrawing one of them takes.
        self.refresh(state);
        self.place_all(state, screen, loupe)
    }

    /// Redraw whatever this frame changed, and nothing else.
    fn refresh(&mut self, state: &HudState) {
        if self.readout_base.is_none() {
            let version = self.bump();
            self.readout_base = Some(self.readout_background(version));
        }
        if self
            .cached_readout
            .as_ref()
            .is_none_or(|(c, _)| *c != state.colour)
        {
            let version = self.bump();
            let panel = self.readout(state.colour, version);
            self.cached_readout = Some((state.colour, panel));
        }

        if self.cached_instructions.is_none() {
            let version = self.bump();
            self.cached_instructions = Some(self.instruction_pill(version));
        }

        if self
            .cached_badge
            .as_ref()
            .is_none_or(|(z, _)| *z != state.zoom)
        {
            let version = self.bump();
            let panel = self.zoom_badge(state.zoom, version);
            self.cached_badge = Some((state.zoom, panel));
        }

        if let Some(tray) = &state.tray {
            let phase = pulse_step(state.clock);
            let stale = self
                .cached_tray
                .as_ref()
                .is_none_or(|(t, p, _)| t != tray || *p != phase);
            if stale {
                if self
                    .tray_base
                    .as_ref()
                    .is_none_or(|(t, _)| *t != tray.target)
                {
                    let version = self.bump();
                    let base = self.tray_background(tray.target, version);
                    self.tray_base = Some((tray.target, base));
                }
                let version = self.bump();
                let panel = self.tray(tray, phase, version);
                self.cached_tray = Some((tray.clone(), phase, panel));
            }
        }
    }

    fn place_all(&self, state: &HudState, screen: (u32, u32), loupe: (f32, f32)) -> Vec<Layer<'_>> {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let mut layers = Vec::with_capacity(4);

        // Readout, hanging below the loupe. The design centres the whole
        // circle-plus-pill column on the cursor, which would leave the
        // magnified centre cell sitting above the crosshair; centring the
        // circle instead keeps the pixel under the crosshair the pixel that
        // gets picked, and the pill still trails it by the same 10px.
        if let Some((_, panel)) = &self.cached_readout {
            let top = loupe.1 + self.px(LOUPE_DIAMETER / 2.0) + self.px(10.0);
            layers.push(place(panel, loupe.0 - panel.width() / 2.0, top, sw, sh));
        }

        // Instruction pill, `left:50%; top:22px`.
        if let Some(panel) = &self.cached_instructions {
            layers.push(place(
                panel,
                sw / 2.0 - panel.width() / 2.0,
                self.px(22.0),
                sw,
                sh,
            ));
        }

        // Zoom badge, `right:20px; top:22px`.
        if let Some((_, panel)) = &self.cached_badge {
            layers.push(place(
                panel,
                sw - self.px(20.0) - panel.width(),
                self.px(22.0),
                sw,
                sh,
            ));
        }

        // Tray, `left:50%; bottom:22px`. A 25-slot palette on a narrow display
        // would be wider than the screen, and a tray running off both edges
        // tells the user less than no tray at all.
        if state.tray.is_some()
            && let Some((_, _, panel)) = &self.cached_tray
            && panel.width() <= sw
        {
            let top = sh - self.px(22.0) - panel.height();
            layers.push(place(panel, sw / 2.0 - panel.width() / 2.0, top, sw, sh));
        }

        layers
    }

    /// The readout's geometry, which never depends on the colour.
    ///
    /// The pill is sized for the widest RGB triple it can ever show rather
    /// than for the one currently on it. The design lets the pill shrink-wrap
    /// its text, but at 60 frames a second that means a pill that visibly
    /// breathes as the cursor moves, and a texture the GPU has to reallocate
    /// every time a channel crosses a power of ten.
    fn readout_metrics(&self) -> (f32, f32, f32) {
        let widest = Run::new("255 255 255", Face::Mono, self.px(11.0), Rgba(0, 0, 0, 0));
        let hex = Run::new("#000000", Face::MonoMedium, self.px(12.0), Rgba(0, 0, 0, 0))
            .tracking_em(0.04);
        let swatch = self.px(18.0);
        let h = swatch + self.px(5.0) * 2.0;
        let w = self.px(5.0)
            + swatch
            + self.px(8.0)
            + hex.width()
            + self.px(8.0)
            + widest.width()
            + self.px(10.0);
        (w, h, swatch)
    }

    fn readout_shadow(&self) -> Shadow {
        Shadow {
            dx: 0.0,
            dy: self.px(6.0),
            blur: self.px(18.0),
            spread: self.px(-6.0),
            colour: Rgba(0, 0, 0, 153),
        }
    }

    /// `padding:5px 6px 5px 5px; gap:8px; radius:999px; background:rgba(16,17,21,.9)`
    ///
    /// Just the pill and its shadow — the expensive, unchanging half.
    fn readout_background(&self, version: u64) -> Panel {
        let (w, h, _) = self.readout_metrics();
        let shadow = self.readout_shadow();
        let (mut bmp, pad) = canvas(w, h, shadow);
        let rect = Rect::new(pad, pad, w, h);
        bmp.shadow(rect, h / 2.0, shadow);
        bmp.rounded_rect(rect, h / 2.0, PANEL_STRONG);
        Panel {
            bitmap: bmp,
            pad,
            version,
        }
    }

    /// The swatch and text for one colour, stamped onto a copy of the pill.
    fn readout(&self, colour: Color, version: u64) -> Panel {
        let (w, h, swatch) = self.readout_metrics();
        let mut panel = self
            .readout_base
            .clone()
            .unwrap_or_else(|| self.readout_background(version));
        panel.version = version;
        let pad = panel.pad;
        let bmp = &mut panel.bitmap;

        // The swatch: a filled circle inside a 1px inner ring, so a colour
        // close to the panel's own black still reads as a disc.
        let cx = pad + self.px(5.0) + swatch / 2.0;
        let cy = pad + h / 2.0;
        bmp.circle(
            cx,
            cy,
            swatch / 2.0,
            Rgba(colour.r, colour.g, colour.b, 255),
        );
        let ring = Rect::new(cx - swatch / 2.0, cy - swatch / 2.0, swatch, swatch);
        bmp.inset_stroke(ring, swatch / 2.0, self.px(1.0), Rgba(255, 255, 255, 71));

        let hex = colour.to_hex().to_uppercase();
        let rgb = format!("{} {} {}", colour.r, colour.g, colour.b);
        let hex_run = Run::new(
            &hex,
            Face::MonoMedium,
            self.px(12.0),
            Rgba(255, 255, 255, 255),
        )
        .tracking_em(0.04);
        let rgb_run = Run::new(&rgb, Face::Mono, self.px(11.0), Rgba(255, 255, 255, 115));

        let mut pen = pad + self.px(5.0) + swatch + self.px(8.0);
        text::draw(bmp, &hex_run, pen, baseline(cy, &hex_run));
        pen += hex_run.width() + self.px(8.0);
        text::draw(bmp, &rgb_run, pen, baseline(cy, &rgb_run));
        debug_assert!(pen + rgb_run.width() <= pad + w, "the pill is too narrow");
        panel
    }

    /// `padding:8px 16px; gap:14px; radius:999px; background:rgba(16,17,21,.82)`
    ///
    /// The design writes the first instruction in white and the rest at 62%,
    /// with the separators dimmer again.
    fn instruction_pill(&self, version: u64) -> Panel {
        let size = self.px(11.5);
        let bright = Rgba(255, 255, 255, 255);
        let dim = Rgba(255, 255, 255, 158);
        let faint = Rgba(255, 255, 255, 47);

        let parts: Vec<&str> = self.instructions.split(" · ").collect();
        let gap = self.px(14.0);
        let dot = Run::new("·", Face::Sans, size, faint);

        let runs: Vec<Run<'_>> = parts
            .iter()
            .enumerate()
            .map(|(i, p)| Run::new(p, Face::Sans, size, if i == 0 { bright } else { dim }))
            .collect();

        let mut w = self.px(16.0) * 2.0;
        for (i, run) in runs.iter().enumerate() {
            if i > 0 {
                w += gap + dot.width() + gap;
            }
            w += run.width();
        }
        let h = size + self.px(16.0);

        let shadow = Shadow {
            dx: 0.0,
            dy: self.px(8.0),
            blur: self.px(24.0),
            spread: self.px(-10.0),
            colour: Rgba(0, 0, 0, 179),
        };
        let (mut bmp, pad) = canvas(w, h, shadow);
        let rect = Rect::new(pad, pad, w, h);
        bmp.shadow(rect, h / 2.0, shadow);
        bmp.rounded_rect(rect, h / 2.0, PANEL);

        let cy = pad + h / 2.0;
        let mut pen = pad + self.px(16.0);
        for (i, run) in runs.iter().enumerate() {
            if i > 0 {
                pen += gap;
                text::draw(&mut bmp, &dot, pen, baseline(cy, &dot));
                pen += dot.width() + gap;
            }
            text::draw(&mut bmp, run, pen, baseline(cy, run));
            pen += run.width();
        }
        Panel {
            bitmap: bmp,
            pad,
            version,
        }
    }

    /// `padding:7px 11px; radius:8px; background:rgba(16,17,21,.82)`
    fn zoom_badge(&self, zoom: u32, version: u64) -> Panel {
        let label = format!("{zoom}×");
        let run = Run::new(
            &label,
            Face::MonoMedium,
            self.px(11.0),
            Rgba(255, 255, 255, 191),
        )
        .tracking_em(0.05);

        let w = run.width() + self.px(11.0) * 2.0;
        let h = self.px(11.0) + self.px(7.0) * 2.0;
        let mut bmp = Bitmap::new(w.ceil() as u32, h.ceil() as u32);
        let rect = Rect::new(0.0, 0.0, w, h);
        bmp.rounded_rect(rect, self.px(8.0), PANEL);
        text::draw_centred(&mut bmp, &run, w / 2.0, baseline(h / 2.0, &run));
        Panel {
            bitmap: bmp,
            pad: 0.0,
            version,
        }
    }

    /// The tray's geometry, which depends only on how many slots it has.
    ///
    /// The label is measured at the widest it can reach for *this* palette —
    /// `PALETTE 5 / 5` for five slots — rather than at its current value, so
    /// the tray neither jumps sideways as it fills nor carries slack for
    /// counts it will never show.
    fn tray_metrics(&self, target: usize) -> (f32, f32, f32, f32) {
        let slot = self.px(30.0);
        let slot_gap = self.px(6.0);
        let slots = target.max(1);
        let slots_w = slot * slots as f32 + slot_gap * (slots - 1) as f32;
        let widest = Self::tray_label(slots, slots);
        let label = self.tray_label_run(&widest);
        let tail = self.tray_tail_run();
        let w = self.px(14.0) * 2.0
            + label.width()
            + self.px(12.0)
            + slots_w
            + self.px(12.0)
            + tail.width();
        (w, slot + self.px(10.0) * 2.0, slot, slot_gap)
    }

    /// `Palette 3 / 5`, uppercased by the design's `text-transform`.
    fn tray_label(taken: usize, target: usize) -> String {
        format!("PALETTE {taken} / {target}")
    }

    fn tray_label_run<'a>(&self, text: &'a str) -> Run<'a> {
        Run::new(text, Face::Mono, self.px(10.0), Rgba(255, 255, 255, 115)).tracking_em(0.1)
    }

    /// The hint at the end of the tray.
    ///
    /// The design reads "Enter to save", but Enter is the commit key here and
    /// takes a colour, so saying so would be an instruction to do the wrong
    /// thing. The pass ends on its own when the last slot fills; this names
    /// the way out before then.
    fn tray_tail_run(&self) -> Run<'static> {
        Run::new(
            "Esc to finish",
            Face::Sans,
            self.px(11.0),
            Rgba(255, 255, 255, 128),
        )
    }

    fn tray_shadow(&self) -> Shadow {
        Shadow {
            dx: 0.0,
            dy: self.px(10.0),
            blur: self.px(30.0),
            spread: self.px(-12.0),
            colour: Rgba(0, 0, 0, 179),
        }
    }

    /// `padding:10px 14px; gap:12px; radius:14px; background:rgba(16,17,21,.82)`
    ///
    /// The panel and its shadow, which do not change as the palette fills.
    /// Kept separate because the next slot pulses, and re-blurring a
    /// 30px shadow several times a second to animate one dashed square is
    /// most of what the tray would otherwise cost.
    fn tray_background(&self, target: usize, version: u64) -> Panel {
        let (w, h, _, _) = self.tray_metrics(target);
        let shadow = self.tray_shadow();
        let (mut bmp, pad) = canvas(w, h, shadow);
        let rect = Rect::new(pad, pad, w, h);
        bmp.shadow(rect, self.px(14.0), shadow);
        bmp.rounded_rect(rect, self.px(14.0), PANEL);
        Panel {
            bitmap: bmp,
            pad,
            version,
        }
    }

    /// The label, slots and hint, stamped onto a copy of the tray's panel.
    fn tray(&self, tray: &Tray, phase: u32, version: u64) -> Panel {
        let (w, h, slot, slot_gap) = self.tray_metrics(tray.target);
        let mut panel = self
            .tray_base
            .as_ref()
            .filter(|(t, _)| *t == tray.target)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| self.tray_background(tray.target, version));
        panel.version = version;
        let pad = panel.pad;
        let bmp = &mut panel.bitmap;

        let slots = tray.target.max(1);
        let label = Self::tray_label(tray.collected.len(), tray.target);
        let label_run = self.tray_label_run(&label);
        let tail_run = self.tray_tail_run();

        let cy = pad + h / 2.0;
        let mut pen = pad + self.px(14.0);
        text::draw(bmp, &label_run, pen, baseline(cy, &label_run));
        // Measured at its widest, so the slots do not shift as the count grows.
        let widest = Self::tray_label(slots, slots);
        pen += self.tray_label_run(&widest).width() + self.px(12.0);

        let radius = self.px(7.0);
        let top = pad + self.px(10.0);
        for i in 0..slots {
            let cell = Rect::new(pen, top, slot, slot);
            match tray.collected.get(i) {
                // Filled: the colour behind a 1px inner ring.
                Some(c) => {
                    bmp.rounded_rect(cell, radius, Rgba(c.r, c.g, c.b, 255));
                    bmp.inset_stroke(cell, radius, self.px(1.0), Rgba(255, 255, 255, 51));
                }
                // The next slot pulses; the ones after it wait quietly.
                None => {
                    let alpha = if i == tray.collected.len() {
                        (89.0 * pulse_opacity(phase)).round() as u8
                    } else {
                        46
                    };
                    bmp.dashed_rounded_rect(
                        cell,
                        radius,
                        self.px(1.0),
                        self.px(3.0),
                        self.px(3.0),
                        Rgba(255, 255, 255, alpha),
                    );
                }
            }
            pen += slot + slot_gap;
        }

        pen += self.px(12.0) - slot_gap;
        text::draw(bmp, &tail_run, pen, baseline(cy, &tail_run));
        debug_assert!(pen + tail_run.width() <= pad + w, "the tray is too narrow");
        panel
    }
}

/// A canvas big enough for a panel and the shadow that spills out of it.
fn canvas(w: f32, h: f32, shadow: Shadow) -> (Bitmap, f32) {
    let reach =
        shadow.blur / 2.0 + shadow.dx.abs().max(shadow.dy.abs()) + shadow.spread.abs() + 2.0;
    let pad = reach.ceil();
    (
        Bitmap::new((w + pad * 2.0).ceil() as u32, (h + pad * 2.0).ceil() as u32),
        pad,
    )
}

/// The baseline that centres a run's cap height on `cy`.
///
/// The design sets every HUD string at `line-height:1` inside a flex row with
/// `align-items:center`, which optically centres what is drawn rather than the
/// font's full ascent-to-descent box.
fn baseline(cy: f32, run: &Run<'_>) -> f32 {
    cy + run.cap_height() / 2.0
}

/// Place a panel by its visible box and clamp it on screen.
///
/// `x` and `y` are where the design puts the panel itself; the shadow margin
/// is subtracted here so the bitmap lands with its visible box on that spot.
/// Clamping keeps the panel legible when the cursor is at a screen edge, and
/// allows the shadow to fall off the edge rather than shoving the panel in.
fn place(panel: &Panel, x: f32, y: f32, screen_w: f32, screen_h: f32) -> Layer<'_> {
    let x = x.clamp(0.0, (screen_w - panel.width()).max(0.0)) - panel.pad;
    let y = y.clamp(0.0, (screen_h - panel.height()).max(0.0)) - panel.pad;
    Layer {
        bitmap: &panel.bitmap,
        x: x.round() as i32,
        y: y.round() as i32,
        version: panel.version,
    }
}

/// `pl-pulse 2.4s ease-in-out infinite`, quantised.
///
/// Quantising to a step index is what makes the cache work: the tray only
/// re-rasterises when the pulse visibly moves, not on every frame.
fn pulse_step(clock: f32) -> u32 {
    const STEPS: f32 = 24.0;
    ((clock / 2.4).rem_euclid(1.0) * STEPS) as u32 % STEPS as u32
}

/// Opacity for a pulse step, easing between `.55` and `1`.
fn pulse_opacity(step: u32) -> f32 {
    const STEPS: f32 = 24.0;
    let t = step as f32 / STEPS;
    // `ease-in-out` over a 0 -> 1 -> 0 triangle is a raised cosine.
    let eased = 0.5 - 0.5 * (t * std::f32::consts::TAU).cos();
    0.55 + 0.45 * eased
}

/// How many pixels a panel actually inked, for tests.
#[cfg(test)]
fn ink(bitmap: &Bitmap) -> u32 {
    (0..bitmap.height)
        .flat_map(|y| (0..bitmap.width).map(move |x| (x, y)))
        .filter(|(x, y)| bitmap.pixel(*x, *y).3 > 0)
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome() -> Chrome {
        Chrome::new(1.0, "Click to sample · Scroll to zoom · Esc cancels".into())
    }

    /// Where a layer landed, copied out so the borrow on `Chrome` ends.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Placed {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        version: u64,
        ink: u32,
    }

    fn placed(
        chrome: &mut Chrome,
        state: &HudState,
        screen: (u32, u32),
        loupe: (f32, f32),
    ) -> Vec<Placed> {
        chrome
            .layers(state, screen, loupe)
            .iter()
            .map(|l| Placed {
                x: l.x,
                y: l.y,
                w: l.bitmap.width,
                h: l.bitmap.height,
                version: l.version,
                ink: ink(l.bitmap),
            })
            .collect()
    }

    fn state() -> HudState {
        HudState {
            colour: Color::new(0xA5, 0x23, 0x6E),
            zoom: 16,
            sample: 1,
            tray: None,
            clock: 0.0,
        }
    }

    #[test]
    fn a_single_pick_shows_three_panels_and_no_tray() {
        let layers = placed(&mut chrome(), &state(), (1920, 1080), (960.0, 540.0));
        assert_eq!(layers.len(), 3, "readout, instructions, zoom badge");
    }

    #[test]
    fn a_multi_pick_adds_the_tray() {
        let mut s = state();
        s.tray = Some(Tray {
            collected: vec![Color::new(240, 169, 160), Color::new(201, 115, 106)],
            target: 5,
        });
        let layers = placed(&mut chrome(), &s, (1920, 1080), (960.0, 540.0));
        assert_eq!(layers.len(), 4);

        let tray = &layers[3];
        assert!(
            tray.y > 900,
            "the tray sits at the bottom, got y={}",
            tray.y
        );
    }

    #[test]
    fn the_readout_hangs_below_the_loupe_and_stays_centred_on_it() {
        let mut chrome = chrome();
        let layers = placed(&mut chrome, &state(), (1920, 1080), (960.0, 540.0));
        let readout = &layers[0];
        let centre = readout.x + readout.w as i32 / 2;
        assert!((centre - 960).abs() <= 1, "off centre by {}", centre - 960);

        // The design's 10px gap, measured from the loupe's rim to the pill's
        // own top edge — not to the top of the shadow's margin.
        let pad = chrome.cached_readout.as_ref().unwrap().1.pad;
        let top = readout.y as f32 + pad;
        assert_eq!(top, 540.0 + 88.0 + 10.0, "the gap below the loupe drifted");
    }

    #[test]
    fn a_shadow_margin_does_not_shift_the_panel_it_belongs_to() {
        // Every shadowed panel is rasterised into a larger bitmap so its blur
        // has somewhere to go. Positioning that bitmap instead of the panel
        // pushed all of them tens of pixels out of place.
        let mut chrome = chrome();
        let mut s = state();
        s.tray = Some(Tray {
            collected: vec![Color::new(1, 2, 3)],
            target: 5,
        });
        let layers = placed(&mut chrome, &s, (1920, 1080), (960.0, 540.0));

        let pill = chrome.cached_instructions.as_ref().unwrap();
        assert!(pill.pad > 0.0, "the instruction pill has a shadow to spill");
        assert_eq!(layers[1].y as f32 + pill.pad, 22.0, "`top:22px`");

        let tray = &chrome.cached_tray.as_ref().unwrap().2;
        let bottom = layers[3].y as f32 + tray.pad + tray.height();
        assert_eq!(bottom, 1080.0 - 22.0, "`bottom:22px`");
    }

    #[test]
    fn panels_are_clamped_onto_the_screen_at_its_edges() {
        // The loupe near a corner must not push the readout off the display.
        // The shadow margin may hang over the edge — it is transparent — but
        // the panel itself has to stay visible.
        let mut chrome = chrome();
        let layers = placed(&mut chrome, &state(), (1920, 1080), (4.0, 1076.0));
        let pad = chrome.cached_readout.as_ref().unwrap().1.pad;
        let readout = &layers[0];
        let (x, y) = (readout.x as f32 + pad, readout.y as f32 + pad);
        let w = readout.w as f32 - pad * 2.0;
        let h = readout.h as f32 - pad * 2.0;
        assert!(x >= 0.0 && y >= 0.0, "readout went off the top left");
        assert!(x + w <= 1920.0 && y + h <= 1080.0, "readout ran off screen");
    }

    #[test]
    fn the_badge_sits_20px_in_from_the_right_edge() {
        // `right:20px; top:22px`, and the design gives it no shadow to offset.
        let layers = placed(&mut chrome(), &state(), (1920, 1080), (960.0, 540.0));
        let badge = &layers[2];
        assert_eq!(badge.x + badge.w as i32, 1900);
        assert_eq!(badge.y, 22);
    }

    #[test]
    fn re_rasterising_is_skipped_when_nothing_changed() {
        let mut chrome = chrome();
        let s = state();
        chrome.layers(&s, (1920, 1080), (960.0, 540.0));
        let first = chrome
            .cached_readout
            .as_ref()
            .unwrap()
            .1
            .bitmap
            .pixels
            .clone();

        // A different loupe position with the same colour must reuse the pill.
        chrome.layers(&s, (1920, 1080), (300.0, 300.0));
        assert_eq!(
            chrome.cached_readout.as_ref().unwrap().1.bitmap.pixels,
            first
        );
    }

    #[test]
    fn a_new_colour_redraws_the_readout() {
        let mut chrome = chrome();
        let mut s = state();
        chrome.layers(&s, (1920, 1080), (960.0, 540.0));
        let before = chrome
            .cached_readout
            .as_ref()
            .unwrap()
            .1
            .bitmap
            .pixels
            .clone();

        s.colour = Color::new(0x11, 0x99, 0x44);
        chrome.layers(&s, (1920, 1080), (960.0, 540.0));
        assert_ne!(
            chrome.cached_readout.as_ref().unwrap().1.bitmap.pixels,
            before
        );
    }

    #[test]
    fn scaling_grows_every_panel() {
        let one = placed(
            &mut Chrome::new(1.0, "Click to sample".into()),
            &state(),
            (1920, 1080),
            (960.0, 540.0),
        );
        let two = placed(
            &mut Chrome::new(2.0, "Click to sample".into()),
            &state(),
            (3840, 2160),
            (1920.0, 1080.0),
        );
        for (a, b) in one.iter().zip(&two) {
            let ratio = b.w as f32 / a.w as f32;
            assert!((ratio - 2.0).abs() < 0.15, "panel scaled by {ratio}");
        }
    }

    #[test]
    fn a_tray_too_wide_for_the_screen_is_dropped_rather_than_clipped() {
        let mut s = state();
        s.tray = Some(Tray {
            collected: vec![],
            target: 25,
        });
        let wide = placed(&mut chrome(), &s, (1920, 1080), (960.0, 540.0));
        assert_eq!(wide.len(), 4, "a 25-slot tray fits on a 1920px display");

        let narrow = placed(&mut chrome(), &s, (800, 600), (400.0, 300.0));
        assert_eq!(narrow.len(), 3, "but not on an 800px one");
    }

    #[test]
    fn the_pulse_stays_within_the_opacity_the_design_animates() {
        for step in 0..24 {
            let o = pulse_opacity(step);
            assert!((0.549..=1.001).contains(&o), "step {step} gave {o}");
        }
        assert!(
            pulse_opacity(0) < pulse_opacity(12),
            "the pulse should move"
        );
    }

    #[test]
    fn every_panel_actually_draws_something() {
        let mut s = state();
        s.tray = Some(Tray {
            collected: vec![Color::new(1, 2, 3)],
            target: 5,
        });
        for layer in placed(&mut chrome(), &s, (1920, 1080), (960.0, 540.0)) {
            assert!(layer.ink > 100, "{layer:?} came out blank");
        }
    }
}

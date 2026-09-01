//! Text for the overlay's chrome.
//!
//! The HUD shows four short strings — a hex readout, an RGB triple, one line
//! of instructions and a zoom badge — so this is deliberately the smallest
//! thing that draws them well: rasterise each glyph with `fontdue`, advance by
//! the font's own metrics, and blend the coverage mask into a [`Bitmap`].
//!
//! No glyph cache. The readout re-rasterises only when the colour under the
//! cursor changes, and a dozen glyphs at 12px is microseconds.

use std::sync::LazyLock;

use fontdue::{Font, FontSettings};

use super::paint::{Bitmap, Rgba};

/// The design's two typefaces.
///
/// `Prototype/Pallet Pick.dc.html` sets the readout, RGB triple and zoom badge
/// in IBM Plex Mono, and the instruction pill in Instrument Sans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// IBM Plex Mono 400 — the dimmer RGB triple.
    Mono,
    /// IBM Plex Mono 500 — the hex readout and zoom badge.
    MonoMedium,
    /// Instrument Sans — the instruction pill.
    Sans,
}

/// Fonts are embedded rather than loaded from disk: the picker must draw its
/// first frame the instant the hotkey fires, and a missing font file at that
/// moment would be a blank HUD over a frozen screen.
static MONO: LazyLock<Font> = LazyLock::new(|| {
    load(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/fonts/ttf/plexmono-400.ttf"
    )))
});
static MONO_MEDIUM: LazyLock<Font> = LazyLock::new(|| {
    load(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/fonts/ttf/plexmono-500.ttf"
    )))
});
static SANS: LazyLock<Font> = LazyLock::new(|| {
    load(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/fonts/ttf/instrumentsans.ttf"
    )))
});

fn load(bytes: &[u8]) -> Font {
    // These files ship inside the binary, so a parse failure is a build
    // mistake rather than a runtime condition worth handling.
    Font::from_bytes(bytes, FontSettings::default()).expect("embedded font is valid")
}

impl Face {
    fn font(self) -> &'static Font {
        match self {
            Self::Mono => &MONO,
            Self::MonoMedium => &MONO_MEDIUM,
            Self::Sans => &SANS,
        }
    }
}

/// One run of text at one size.
#[derive(Debug, Clone, Copy)]
pub struct Run<'a> {
    /// The string to draw.
    pub text: &'a str,
    /// Typeface.
    pub face: Face,
    /// Size in pixels.
    pub size: f32,
    /// Extra space between glyphs, in pixels.
    ///
    /// The design specifies tracking in ems (`letter-spacing:.04em`); callers
    /// multiply by the size themselves so this stays a plain pixel value.
    pub tracking: f32,
    /// Colour, including the alpha the design asks for.
    pub colour: Rgba,
}

impl<'a> Run<'a> {
    /// A run with no extra tracking.
    pub fn new(text: &'a str, face: Face, size: f32, colour: Rgba) -> Self {
        Self {
            text,
            face,
            size,
            tracking: 0.0,
            colour,
        }
    }

    /// Set tracking as a fraction of the em, matching CSS `letter-spacing`.
    pub fn tracking_em(mut self, em: f32) -> Self {
        self.tracking = em * self.size;
        self
    }

    /// How wide this run draws, in pixels.
    ///
    /// Trailing tracking is excluded — CSS adds it after the last glyph, which
    /// would visibly bias a centred pill to the left.
    pub fn width(&self) -> f32 {
        let font = self.face.font();
        let mut w = 0.0;
        let mut previous = None;
        for ch in self.text.chars() {
            if let Some(p) = previous {
                w += font.horizontal_kern(p, ch, self.size).unwrap_or(0.0) + self.tracking;
            }
            w += font.metrics(ch, self.size).advance_width;
            previous = Some(ch);
        }
        w
    }

    /// Distance from the top of the em box down to the baseline.
    ///
    /// The design positions text by centring it in a pill, so callers need the
    /// ascent to convert a box into a baseline.
    pub fn ascent(&self) -> f32 {
        self.face
            .font()
            .horizontal_line_metrics(self.size)
            .map_or(self.size * 0.8, |m| m.ascent)
    }

    /// The cap height, used to centre a line optically.
    ///
    /// Centring on the full ascent-to-descent box leaves short strings like
    /// `16×` sitting visibly high, because nothing in them reaches the
    /// descender. Centring on what is actually drawn looks right instead.
    pub fn cap_height(&self) -> f32 {
        let font = self.face.font();
        let m = font.metrics('H', self.size);
        if m.height > 0 {
            m.height as f32
        } else {
            self.size * 0.7
        }
    }
}

/// Draw a run with its left edge at `x` and its baseline at `y`.
pub fn draw(bitmap: &mut Bitmap, run: &Run<'_>, x: f32, y: f32) {
    let font = run.face.font();
    let mut pen = x;
    let mut previous = None;

    for ch in run.text.chars() {
        if let Some(p) = previous {
            pen += font.horizontal_kern(p, ch, run.size).unwrap_or(0.0) + run.tracking;
        }
        let (metrics, mask) = font.rasterize(ch, run.size);

        // fontdue measures from the baseline upward; the canvas measures from
        // the top down, so the glyph's top edge is the baseline minus its
        // height above it.
        let left = pen + metrics.xmin as f32;
        let gx = left.floor() as i32;
        let gy = (y - (metrics.height as f32 + metrics.ymin as f32)).round() as i32;

        // Positioning glyphs on whole pixels would quantise every advance,
        // and at 11px that shows: a run reads as "Shif t averages" because
        // half a pixel of error lands between two letters instead of being
        // spread across them. Shifting the coverage mask by the fractional
        // part puts each glyph where its advance actually asked for.
        let frac = left - gx as f32;
        for row in 0..metrics.height {
            let scanline = &mask[row * metrics.width..(row + 1) * metrics.width];
            for col in 0..=metrics.width {
                let right = scanline.get(col).map_or(0.0, |v| f32::from(*v) / 255.0);
                let carried = col
                    .checked_sub(1)
                    .and_then(|c| scanline.get(c))
                    .map_or(0.0, |v| f32::from(*v) / 255.0);
                let coverage = right * (1.0 - frac) + carried * frac;
                if coverage > 0.0 {
                    bitmap.blend(gx + col as i32, gy + row as i32, run.colour, coverage);
                }
            }
        }

        pen += metrics.advance_width;
        previous = Some(ch);
    }
}

/// Draw a run centred horizontally on `cx`, with its baseline at `y`.
pub fn draw_centred(bitmap: &mut Bitmap, run: &Run<'_>, cx: f32, y: f32) {
    draw(bitmap, run, cx - run.width() / 2.0, y);
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgba = Rgba(255, 255, 255, 255);

    #[test]
    fn every_embedded_font_parses() {
        for face in [Face::Mono, Face::MonoMedium, Face::Sans] {
            assert!(face.font().units_per_em() > 0.0, "{face:?} failed to load");
        }
    }

    #[test]
    fn the_mono_faces_are_actually_monospaced() {
        // The hex readout sits above a fixed-width pill; a proportional font
        // here would make the pill jitter as the cursor moves.
        for face in [Face::Mono, Face::MonoMedium] {
            let font = face.font();
            let i = font.metrics('i', 12.0).advance_width;
            let w = font.metrics('W', 12.0).advance_width;
            assert!((i - w).abs() < 0.01, "{face:?}: 'i' {i} vs 'W' {w}");
        }
    }

    #[test]
    fn width_scales_with_the_length_of_the_string() {
        let one = Run::new("#FFFFFF", Face::MonoMedium, 12.0, WHITE);
        let two = Run::new("#FFFFFF#FFFFFF", Face::MonoMedium, 12.0, WHITE);
        assert!(
            (two.width() - one.width() * 2.0).abs() < 0.5,
            "{} vs {}",
            two.width(),
            one.width()
        );
    }

    #[test]
    fn tracking_widens_a_run_but_not_past_the_last_glyph() {
        let plain = Run::new("16×", Face::MonoMedium, 11.0, WHITE);
        let tracked = plain.tracking_em(0.05);
        // Two gaps between three glyphs, not three.
        let expected = plain.width() + 0.05 * 11.0 * 2.0;
        assert!((tracked.width() - expected).abs() < 0.01);
    }

    #[test]
    fn drawing_puts_ink_on_the_canvas_within_its_measured_width() {
        let mut bmp = Bitmap::new(120, 30);
        let run = Run::new("#A5236E", Face::MonoMedium, 12.0, WHITE);
        draw(&mut bmp, &run, 4.0, 20.0);

        let inked = |x: u32| (0..30).any(|y| bmp.pixel(x, y).3 > 0);
        assert!((4..4 + run.width() as u32).any(inked), "nothing was drawn");
        assert!(!inked(0), "ink escaped to the left of the origin");
        assert!(
            !inked(4 + run.width().ceil() as u32 + 2),
            "ink ran past the measured width"
        );
    }

    #[test]
    fn glyphs_land_on_subpixel_positions() {
        // Two runs half a pixel apart must not rasterise identically, or the
        // advances are being quantised and the spacing goes ragged.
        let mut a = Bitmap::new(60, 24);
        let mut b = Bitmap::new(60, 24);
        let run = Run::new("nn", Face::Sans, 11.5, WHITE);
        draw(&mut a, &run, 4.0, 16.0);
        draw(&mut b, &run, 4.5, 16.0);
        assert_ne!(a.pixels, b.pixels, "half a pixel made no difference");
    }

    #[test]
    fn a_baseline_keeps_glyphs_above_it() {
        // Descenderless text must sit entirely above the baseline, or the
        // pills will look bottom-heavy.
        let mut bmp = Bitmap::new(80, 40);
        draw(
            &mut bmp,
            &Run::new("HEX", Face::Sans, 12.0, WHITE),
            4.0,
            24.0,
        );
        let below = (26..40).any(|y| (0..80).any(|x| bmp.pixel(x, y).3 > 0));
        assert!(!below, "ink fell below the baseline");
    }

    #[test]
    fn centring_balances_the_margins() {
        let mut bmp = Bitmap::new(101, 24);
        draw_centred(
            &mut bmp,
            &Run::new("Esc cancels", Face::Sans, 11.5, WHITE),
            50.5,
            16.0,
        );

        let first = (0..101).find(|x| (0..24).any(|y| bmp.pixel(*x, y).3 > 0));
        let last = (0..101)
            .rev()
            .find(|x| (0..24).any(|y| bmp.pixel(*x, y).3 > 0));
        let (first, last) = (first.expect("no ink") as i32, last.expect("no ink") as i32);
        assert!(
            (first - (100 - last)).abs() <= 2,
            "left {first}, right {}",
            100 - last
        );
    }
}

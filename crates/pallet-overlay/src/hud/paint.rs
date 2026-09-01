//! A tiny software canvas for the overlay's chrome.
//!
//! The loupe itself is a fragment shader, because magnifying a frozen frame is
//! a per-pixel sampling problem. Its readout, the instruction pill and the
//! multi-pick tray are not: they are small, they change rarely, and drawing
//! them on the CPU avoids building a glyph atlas and a second GPU pipeline for
//! four short strings.
//!
//! Everything here composites straight-alpha RGBA over what is already there.

/// An RGBA image, row-major, 4 bytes per pixel.
#[derive(Clone)]
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, straight (not premultiplied) alpha.
    pub pixels: Vec<u8>,
}

impl std::fmt::Debug for Bitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bitmap")
            .field("size", &(self.width, self.height))
            .finish()
    }
}

/// An 8-bit RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Bitmap {
    /// A fully transparent canvas.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    /// The colour at a pixel, or transparent black outside the canvas.
    pub fn pixel(&self, x: u32, y: u32) -> Rgba {
        if x >= self.width || y >= self.height {
            return Rgba(0, 0, 0, 0);
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Rgba(
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        )
    }

    /// Composite one pixel of `colour` with the given coverage, 0.0 to 1.0.
    ///
    /// Coverage is separate from the colour's own alpha so a glyph's
    /// antialiasing and a panel's translucency multiply rather than fight.
    pub fn blend(&mut self, x: i32, y: i32, colour: Rgba, coverage: f32) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let a = f32::from(colour.3) / 255.0 * coverage.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }

        let i = (y as usize * self.width as usize + x as usize) * 4;
        let dst_a = f32::from(self.pixels[i + 3]) / 255.0;
        let out_a = a + dst_a * (1.0 - a);
        if out_a <= 0.0 {
            return;
        }

        for c in 0..3 {
            let src = f32::from([colour.0, colour.1, colour.2][c]) / 255.0;
            let dst = f32::from(self.pixels[i + c]) / 255.0;
            let out = (src * a + dst * dst_a * (1.0 - a)) / out_a;
            self.pixels[i + c] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        self.pixels[i + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    /// Fill a rounded rectangle.
    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, colour: Rgba) {
        let r = clamp_radius(rect, radius);
        for y in span(rect.y - 1.0, rect.y + rect.h + 1.0) {
            for x in span(rect.x - 1.0, rect.x + rect.w + 1.0) {
                self.blend(x, y, colour, rounded_coverage(rect, r, x, y));
            }
        }
    }

    /// Fill a circle.
    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, colour: Rgba) {
        let x0 = (cx - radius - 1.0).floor() as i32;
        let y0 = (cy - radius - 1.0).floor() as i32;
        let x1 = (cx + radius + 1.0).ceil() as i32;
        let y1 = (cy + radius + 1.0).ceil() as i32;

        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                self.blend(x, y, colour, (radius + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }
}

/// A rectangle in canvas coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// Build a rectangle.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

/// A CSS-style drop shadow.
///
/// Mirrors `box-shadow: <dx> <dy> <blur> <spread> <colour>` so the values from
/// `Prototype/Pallet Pick.dc.html` can be transcribed rather than reinterpreted.
#[derive(Debug, Clone, Copy)]
pub struct Shadow {
    /// Horizontal offset.
    pub dx: f32,
    /// Vertical offset.
    pub dy: f32,
    /// CSS blur radius. The Gaussian's sigma is half this.
    pub blur: f32,
    /// Positive grows the silhouette, negative shrinks it.
    pub spread: f32,
    /// Shadow colour, alpha included.
    pub colour: Rgba,
}

/// A single-channel coverage buffer.
///
/// Anything built from overlapping pieces — a dashed border stamped along a
/// path, a shadow silhouette about to be blurred — accumulates here first and
/// is blended onto a [`Bitmap`] once. Blending each piece directly would let a
/// translucent stroke composite with itself wherever the pieces overlap, so a
/// 35%-white dash would read as 58% white at every join.
#[derive(Clone)]
pub struct Mask {
    width: u32,
    height: u32,
    values: Vec<f32>,
}

impl std::fmt::Debug for Mask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mask")
            .field("size", &(self.width, self.height))
            .finish()
    }
}

impl Mask {
    /// An empty mask.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            values: vec![0.0; width as usize * height as usize],
        }
    }

    /// Coverage at a pixel, or zero outside.
    pub fn get(&self, x: u32, y: u32) -> f32 {
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        self.values[y as usize * self.width as usize + x as usize]
    }

    /// Raise the coverage at a pixel, never lowering it.
    fn raise(&mut self, x: i32, y: i32, v: f32) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height || v <= 0.0 {
            return;
        }
        let i = y as usize * self.width as usize + x as usize;
        self.values[i] = self.values[i].max(v.min(1.0));
    }

    /// Add a filled rounded rectangle.
    pub fn rounded_rect(&mut self, rect: Rect, radius: f32) {
        let r = clamp_radius(rect, radius);
        for y in span(rect.y - 1.0, rect.y + rect.h + 1.0) {
            for x in span(rect.x - 1.0, rect.x + rect.w + 1.0) {
                self.raise(x, y, rounded_coverage(rect, r, x, y));
            }
        }
    }

    /// Add a filled circle.
    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32) {
        for y in span(cy - radius - 1.0, cy + radius + 1.0) {
            for x in span(cx - radius - 1.0, cx + radius + 1.0) {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                self.raise(x, y, (radius + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Blur with an approximate Gaussian of the given sigma.
    ///
    /// Three box passes, which is the standard cheap approximation and is
    /// indistinguishable from a true Gaussian at the sizes the HUD uses.
    pub fn blur(&mut self, sigma: f32) {
        if sigma <= 0.0 {
            return;
        }
        for radius in box_radii(sigma) {
            self.box_pass(radius, true);
            self.box_pass(radius, false);
        }
    }

    fn box_pass(&mut self, radius: usize, horizontal: bool) {
        if radius == 0 {
            return;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        let scale = 1.0 / (radius * 2 + 1) as f32;
        let mut line = vec![0.0f32; inner];

        for o in 0..outer {
            let at = |i: usize| if horizontal { o * w + i } else { i * w + o };
            for (i, slot) in line.iter_mut().enumerate() {
                *slot = self.values[at(i)];
            }
            // Edges clamp rather than fade to zero, so a shadow does not
            // develop a bright seam where its buffer runs out.
            let sample = |k: i32| line[(k.clamp(0, inner as i32 - 1)) as usize];
            let mut sum: f32 = (-(radius as i32)..=(radius as i32)).map(sample).sum();
            self.values[at(0)] = sum * scale;
            for i in 1..inner {
                sum += sample(i as i32 + radius as i32) - sample(i as i32 - 1 - radius as i32);
                self.values[at(i)] = sum * scale;
            }
        }
    }
}

/// Box radii whose triple convolution approximates a Gaussian of `sigma`.
fn box_radii(sigma: f32) -> [usize; 3] {
    let n = 3.0f32;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut lower = ideal.floor() as i32;
    if lower % 2 == 0 {
        lower -= 1;
    }
    let lower = lower.max(1);
    let upper = lower + 2;
    let m =
        ((12.0 * sigma * sigma - (n * (lower * lower) as f32) - (4.0 * n * lower as f32) - 3.0 * n)
            / (-4.0 * lower as f32 - 4.0))
            .round() as i32;

    let mut out = [0usize; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        let size = if (i as i32) < m { lower } else { upper };
        *slot = ((size - 1) / 2).max(0) as usize;
    }
    out
}

fn clamp_radius(rect: Rect, radius: f32) -> f32 {
    radius.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0)
}

fn span(from: f32, to: f32) -> std::ops::Range<i32> {
    from.floor() as i32..to.ceil() as i32
}

/// Antialiased coverage of a rounded rectangle at one pixel centre.
fn rounded_coverage(rect: Rect, radius: f32, x: i32, y: i32) -> f32 {
    let px = x as f32 + 0.5;
    let py = y as f32 + 0.5;
    let dx = (rect.x + radius - px)
        .max(px - (rect.x + rect.w - radius))
        .max(0.0);
    let dy = (rect.y + radius - py)
        .max(py - (rect.y + rect.h - radius))
        .max(0.0);

    if radius <= 0.0 {
        // Square corners: the two axes are independent, so coverage is the
        // product of how far inside each edge the pixel sits.
        let ix = (px - rect.x + 0.5)
            .min(rect.x + rect.w - px + 0.5)
            .clamp(0.0, 1.0);
        let iy = (py - rect.y + 0.5)
            .min(rect.y + rect.h - py + 0.5)
            .clamp(0.0, 1.0);
        return ix * iy;
    }

    let outside = (dx * dx + dy * dy).sqrt() - radius;
    (0.5 - outside).clamp(0.0, 1.0)
}

impl Bitmap {
    /// Blend a colour through a coverage mask aligned to the canvas origin.
    pub fn blend_mask(&mut self, mask: &Mask, colour: Rgba) {
        self.blend_mask_at(mask, 0, 0, colour);
    }

    /// Blend a colour through a coverage mask placed at `(ox, oy)`.
    ///
    /// A mask only has to cover the shape it describes. The tray's dashed
    /// slots are 30px squares on a panel several hundred pixels wide, and
    /// giving each one a full-panel mask meant allocating and then scanning
    /// two hundred kilobytes to draw a border.
    pub fn blend_mask_at(&mut self, mask: &Mask, ox: i32, oy: i32, colour: Rgba) {
        for y in 0..mask.height {
            for x in 0..mask.width {
                self.blend(ox + x as i32, oy + y as i32, colour, mask.get(x, y));
            }
        }
    }

    /// Draw a CSS-style drop shadow for a rounded rectangle.
    ///
    /// Call before filling the shape: the shadow is drawn underneath, and the
    /// shape's own fill then covers the part of it that shows through.
    pub fn shadow(&mut self, rect: Rect, radius: f32, shadow: Shadow) {
        let silhouette = Rect::new(
            rect.x + shadow.dx - shadow.spread,
            rect.y + shadow.dy - shadow.spread,
            rect.w + shadow.spread * 2.0,
            rect.h + shadow.spread * 2.0,
        );
        if silhouette.w <= 0.0 || silhouette.h <= 0.0 {
            return;
        }

        let mut mask = Mask::new(self.width, self.height);
        mask.rounded_rect(silhouette, clamp_radius(silhouette, radius + shadow.spread));
        // CSS blur radius spans the whole transition, so its sigma is half.
        mask.blur(shadow.blur / 2.0);
        self.blend_mask(&mask, shadow.colour);
    }

    /// Draw a stroke just inside a rounded rectangle's edge.
    ///
    /// This is CSS `box-shadow: inset 0 0 0 <width>` — the ring the design puts
    /// around every swatch and filled tray slot — not an outside border.
    pub fn inset_stroke(&mut self, rect: Rect, radius: f32, width: f32, colour: Rgba) {
        let r = clamp_radius(rect, radius);
        let inner = Rect::new(
            rect.x + width,
            rect.y + width,
            (rect.w - width * 2.0).max(0.0),
            (rect.h - width * 2.0).max(0.0),
        );
        let inner_r = (r - width).max(0.0);

        for y in span(rect.y - 1.0, rect.y + rect.h + 1.0) {
            for x in span(rect.x - 1.0, rect.x + rect.w + 1.0) {
                let outside = rounded_coverage(rect, r, x, y);
                let hole = rounded_coverage(inner, inner_r, x, y);
                self.blend(x, y, colour, (outside - hole).clamp(0.0, 1.0));
            }
        }
    }

    /// Draw a dashed border around a rounded rectangle.
    ///
    /// Used for the tray's not-yet-filled slots. The dash is stamped along the
    /// path into a mask rather than drawn directly, so overlapping stamps do
    /// not composite into a brighter line than the design asks for.
    pub fn dashed_rounded_rect(
        &mut self,
        rect: Rect,
        radius: f32,
        width: f32,
        dash: f32,
        gap: f32,
        colour: Rgba,
    ) {
        let r = clamp_radius(rect, radius);
        let period = dash + gap;
        if period <= 0.0 {
            return;
        }

        // The mask covers the border and nothing else, in its own
        // coordinates, and is blended back at this offset.
        let margin = width.ceil() + 2.0;
        let ox = (rect.x - margin).floor() as i32;
        let oy = (rect.y - margin).floor() as i32;
        let mut mask = Mask::new(
            (rect.w + margin * 2.0).ceil() as u32,
            (rect.h + margin * 2.0).ceil() as u32,
        );

        // Half a pixel in from the edge, so a 1px border lands on the boundary
        // the way a CSS border does rather than straddling it.
        let inset = width / 2.0;
        let path = rounded_path(
            Rect::new(
                rect.x - ox as f32 + inset,
                rect.y - oy as f32 + inset,
                rect.w - inset * 2.0,
                rect.h - inset * 2.0,
            ),
            (r - inset).max(0.0),
        );

        let total = path.length();
        let step = 0.15;
        let mut travelled = 0.0;
        while travelled < total {
            if travelled % period < dash {
                let (x, y) = path.at(travelled);
                mask.circle(x, y, width / 2.0);
            }
            travelled += step;
        }
        self.blend_mask_at(&mask, ox, oy, colour);
    }
}

/// The outline of a rounded rectangle, parametrised by arc length.
struct RoundedPath {
    rect: Rect,
    radius: f32,
    straight_h: f32,
    straight_v: f32,
    quarter: f32,
}

fn rounded_path(rect: Rect, radius: f32) -> RoundedPath {
    let radius = clamp_radius(rect, radius);
    RoundedPath {
        rect,
        radius,
        straight_h: (rect.w - radius * 2.0).max(0.0),
        straight_v: (rect.h - radius * 2.0).max(0.0),
        quarter: std::f32::consts::FRAC_PI_2 * radius,
    }
}

impl RoundedPath {
    fn length(&self) -> f32 {
        (self.straight_h + self.straight_v + self.quarter * 2.0) * 2.0
    }

    /// The point at arc length `t`, walking clockwise from the top-left corner.
    fn at(&self, t: f32) -> (f32, f32) {
        let (x, y, w, h, r) = (
            self.rect.x,
            self.rect.y,
            self.rect.w,
            self.rect.h,
            self.radius,
        );
        let mut t = t.rem_euclid(self.length());

        let arc = |cx: f32, cy: f32, from: f32, along: f32| {
            let angle = from + along / r.max(f32::EPSILON);
            (cx + r * angle.cos(), cy + r * angle.sin())
        };
        let pi = std::f32::consts::PI;

        for (len, place) in [
            (self.straight_h, 0),
            (self.quarter, 1),
            (self.straight_v, 2),
            (self.quarter, 3),
            (self.straight_h, 4),
            (self.quarter, 5),
            (self.straight_v, 6),
            (self.quarter, 7),
        ] {
            if t <= len || len == 0.0 {
                return match place {
                    0 => (x + r + t, y),
                    1 => arc(x + w - r, y + r, -pi / 2.0, t),
                    2 => (x + w, y + r + t),
                    3 => arc(x + w - r, y + h - r, 0.0, t),
                    4 => (x + w - r - t, y + h),
                    5 => arc(x + r, y + h - r, pi / 2.0, t),
                    6 => (x, y + h - r - t),
                    _ => arc(x + r, y + r, pi, t),
                };
            }
            t -= len;
        }
        (x + r, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_canvas_is_fully_transparent() {
        let bmp = Bitmap::new(4, 3);
        assert_eq!(bmp.pixels.len(), 4 * 3 * 4);
        assert!(bmp.pixels.iter().all(|b| *b == 0));
    }

    #[test]
    fn drawing_outside_the_canvas_is_ignored_rather_than_panicking() {
        let mut bmp = Bitmap::new(4, 4);
        for (x, y) in [(-5, 0), (0, -5), (99, 0), (0, 99)] {
            bmp.blend(x, y, Rgba(255, 0, 0, 255), 1.0);
        }
        assert!(bmp.pixels.iter().all(|b| *b == 0));
    }

    #[test]
    fn an_opaque_fill_lands_exactly() {
        let mut bmp = Bitmap::new(8, 8);
        bmp.rounded_rect(Rect::new(2.0, 2.0, 4.0, 4.0), 0.0, Rgba(255, 128, 0, 255));
        assert_eq!(bmp.pixel(4, 4), Rgba(255, 128, 0, 255));
        assert_eq!(bmp.pixel(0, 0), Rgba(0, 0, 0, 0), "outside stays clear");
    }

    #[test]
    fn translucent_layers_composite_rather_than_replace() {
        // The pills are ~85% black over the frozen screen; stacking must
        // darken toward the colour rather than snapping to it.
        let mut bmp = Bitmap::new(4, 4);
        bmp.blend(1, 1, Rgba(255, 255, 255, 255), 1.0);
        bmp.blend(1, 1, Rgba(0, 0, 0, 128), 1.0);
        let p = bmp.pixel(1, 1);
        assert_eq!(p.3, 255, "still opaque");
        assert!(p.0 > 100 && p.0 < 160, "expected a mid grey, got {p:?}");
    }

    #[test]
    fn coverage_and_alpha_multiply() {
        let mut bmp = Bitmap::new(4, 4);
        bmp.blend(1, 1, Rgba(255, 0, 0, 128), 0.5);
        let a = bmp.pixel(1, 1).3;
        assert!((60..=68).contains(&a), "expected about 64, got {a}");
    }

    #[test]
    fn a_rounded_corner_is_softer_than_the_middle_of_an_edge() {
        let mut bmp = Bitmap::new(40, 40);
        bmp.rounded_rect(
            Rect::new(4.0, 4.0, 32.0, 32.0),
            10.0,
            Rgba(255, 255, 255, 255),
        );
        let corner = bmp.pixel(5, 5).3;
        let edge = bmp.pixel(20, 5).3;
        assert!(
            edge > corner,
            "corner {corner} should be lighter than edge {edge}"
        );
        assert_eq!(bmp.pixel(20, 20).3, 255, "the middle is solid");
    }

    #[test]
    fn a_circle_is_round_not_square() {
        let mut bmp = Bitmap::new(32, 32);
        bmp.circle(16.0, 16.0, 12.0, Rgba(255, 255, 255, 255));
        assert_eq!(bmp.pixel(16, 16).3, 255, "centre is filled");
        assert_eq!(bmp.pixel(1, 1).3, 0, "corners are not");
        assert_eq!(bmp.pixel(16, 5).3, 255, "top of the circle is filled");
    }

    #[test]
    fn a_blur_conserves_what_it_spreads() {
        // A shadow that gains or loses energy would read as the wrong
        // darkness once it is composited under a pill.
        let mut mask = Mask::new(64, 64);
        mask.rounded_rect(Rect::new(20.0, 20.0, 24.0, 24.0), 4.0);
        let before: f32 = (0..64)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .map(|(x, y)| mask.get(x, y))
            .sum();

        let mut blurred = mask.clone();
        blurred.blur(4.0);
        let after: f32 = (0..64)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .map(|(x, y)| blurred.get(x, y))
            .sum();

        assert!(
            (after - before).abs() / before < 0.02,
            "{before} -> {after}"
        );
        assert!(blurred.get(32, 32) < 1.0, "the centre softened");
        assert!(
            blurred.get(16, 32) > 0.0,
            "coverage spread outside the shape"
        );
    }

    #[test]
    fn a_shadow_sits_where_css_would_put_it() {
        // `0 6px 18px -6px`: pushed down 6, shrunk by 6, softened by 18.
        let mut bmp = Bitmap::new(120, 120);
        bmp.shadow(
            Rect::new(30.0, 30.0, 60.0, 40.0),
            14.0,
            Shadow {
                dx: 0.0,
                dy: 6.0,
                blur: 18.0,
                spread: -6.0,
                colour: Rgba(0, 0, 0, 255),
            },
        );
        let below = bmp.pixel(60, 78).3;
        let above = bmp.pixel(60, 22).3;
        assert!(
            below > above,
            "shadow should fall downward: {below} vs {above}"
        );
        assert!(above > 0, "blur still reaches above the shape");
        assert!(
            bmp.pixel(60, 50).3 > 0,
            "the shape's own footprint is shaded"
        );
    }

    #[test]
    fn an_inset_stroke_hugs_the_edge_and_leaves_the_middle_clear() {
        let mut bmp = Bitmap::new(40, 40);
        bmp.inset_stroke(
            Rect::new(5.0, 5.0, 30.0, 30.0),
            7.0,
            1.0,
            Rgba(255, 255, 255, 255),
        );
        assert!(bmp.pixel(20, 5).3 > 200, "top edge is stroked");
        assert_eq!(bmp.pixel(20, 20).3, 0, "the middle stays clear");
        assert_eq!(bmp.pixel(20, 10).3, 0, "the stroke is only a pixel wide");
    }

    #[test]
    fn overlapping_dash_stamps_do_not_accumulate() {
        // Stamping the dash directly would let overlapping stamps composite
        // with each other; masking first is what keeps 35% at 35%.
        let mut bmp = Bitmap::new(40, 40);
        bmp.dashed_rounded_rect(
            Rect::new(5.0, 5.0, 30.0, 30.0),
            7.0,
            1.0,
            3.0,
            3.0,
            Rgba(255, 255, 255, 89),
        );
        let peak = (0..40)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .map(|(x, y)| bmp.pixel(x, y).3)
            .max()
            .unwrap();
        // Stamping lands a hair under nominal — a 1px stamp centred between
        // two pixel centres covers neither fully — but it must never land
        // over, which is what compositing the stamps directly would do.
        assert!(peak <= 89, "dashes accumulated past their alpha: {peak}");
        assert!(peak >= 80, "dashes came out faint: {peak}");
    }

    #[test]
    fn a_dashed_border_actually_has_gaps() {
        let mut bmp = Bitmap::new(40, 40);
        bmp.dashed_rounded_rect(
            Rect::new(5.0, 5.0, 30.0, 30.0),
            7.0,
            1.0,
            3.0,
            3.0,
            Rgba(255, 255, 255, 255),
        );
        // Along the top edge, between the corner radii.
        let top: Vec<u8> = (13..27).map(|x| bmp.pixel(x, 5).3).collect();
        assert!(top.iter().any(|a| *a > 200), "no dashes: {top:?}");
        assert!(top.iter().any(|a| *a < 40), "no gaps: {top:?}");
    }

    #[test]
    fn the_rounded_path_closes_on_itself() {
        let path = rounded_path(Rect::new(0.0, 0.0, 30.0, 30.0), 7.0);
        let (sx, sy) = path.at(0.0);
        let (ex, ey) = path.at(path.length());
        assert!(
            (sx - ex).abs() < 0.01 && (sy - ey).abs() < 0.01,
            "({sx},{sy}) vs ({ex},{ey})"
        );

        // Every point on the path lies on the rectangle's boundary.
        let steps = 200;
        for i in 0..steps {
            let (x, y) = path.at(path.length() * i as f32 / steps as f32);
            assert!(
                (-0.01..=30.01).contains(&x) && (-0.01..=30.01).contains(&y),
                "path left the rect at ({x},{y})"
            );
        }
    }
}

//! Connected displays.
//!
//! Two coordinate systems meet here, and conflating them is the classic
//! screen-picker bug:
//!
//! * **Logical** coordinates are the compositor's global desktop layout. This
//!   is where the pointer lives and how monitors are positioned relative to
//!   each other.
//! * **Physical** pixels are what a captured framebuffer actually contains, and
//!   what the loupe must address to read the colour a user is pointing at.
//!
//! At scale 1 the two are identical, which is exactly why mixing them survives
//! testing on an unscaled desktop and breaks the moment anyone enables
//! fractional scaling. Every field below says which system it is in.

/// The colour space a display's pixels are in.
///
/// A captured pixel is in the *display's* space, not necessarily sRGB. On a
/// P3 or HDR monitor the raw buffer value is not the hex a designer wants, so
/// this travels with every frame rather than being assumed. Discarding it here
/// is the single most common way a colour picker ends up quietly wrong.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ColorProfile {
    /// Plain sRGB. The overwhelmingly common case.
    #[default]
    Srgb,
    /// Display P3, typical of Apple hardware and newer laptops.
    DisplayP3,
    /// Rec. 2020, usually alongside HDR.
    Rec2020,
    /// Known to be something else, named by the platform.
    Other(String),
    /// The platform did not tell us. Treated as sRGB, but flagged so the UI
    /// can say the value is unverified rather than implying certainty.
    Unknown,
}

impl ColorProfile {
    /// The tag stored alongside a colour in the library. `None` means sRGB,
    /// which keeps the common case out of the database entirely.
    pub fn tag(&self) -> Option<String> {
        match self {
            ColorProfile::Srgb => None,
            ColorProfile::DisplayP3 => Some("display-p3".into()),
            ColorProfile::Rec2020 => Some("rec2020".into()),
            ColorProfile::Other(name) => Some(name.clone()),
            ColorProfile::Unknown => Some("unknown".into()),
        }
    }
}

/// How a display is rotated relative to its framebuffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transform {
    /// Not rotated.
    #[default]
    Normal,
    /// Rotated 90 degrees counter-clockwise.
    Rotate90,
    /// Rotated 180 degrees.
    Rotate180,
    /// Rotated 270 degrees counter-clockwise.
    Rotate270,
    /// Mirrored, with an optional rotation on top.
    Flipped(u16),
}

/// A connected display.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// Stable identifier, e.g. `DP-1`.
    pub id: String,
    /// Human-readable description, when the platform offers one.
    pub name: String,

    /// Left edge in the compositor's **logical** desktop layout.
    pub logical_x: i32,
    /// Top edge in the compositor's **logical** desktop layout.
    pub logical_y: i32,
    /// Width in **logical** units.
    pub logical_width: u32,
    /// Height in **logical** units.
    pub logical_height: u32,

    /// Framebuffer width in **physical** pixels.
    pub pixel_width: u32,
    /// Framebuffer height in **physical** pixels.
    pub pixel_height: u32,

    /// Rotation applied to the output.
    pub transform: Transform,
    /// The colour space of this display's pixels.
    pub profile: ColorProfile,
}

impl Monitor {
    /// Physical pixels per logical unit, horizontally.
    ///
    /// Derived from the two geometries rather than read from `wl_output.scale`,
    /// which reports only an integer (2 for a 1.5 scale) and so cannot describe
    /// fractional scaling at all.
    pub fn scale_x(&self) -> f64 {
        if self.logical_width == 0 {
            1.0
        } else {
            f64::from(self.pixel_width) / f64::from(self.logical_width)
        }
    }

    /// Physical pixels per logical unit, vertically.
    pub fn scale_y(&self) -> f64 {
        if self.logical_height == 0 {
            1.0
        } else {
            f64::from(self.pixel_height) / f64::from(self.logical_height)
        }
    }

    /// Whether a **logical** desktop point lands on this display.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.logical_x
            && y >= self.logical_y
            && x < self.logical_x.saturating_add(self.logical_width as i32)
            && y < self.logical_y.saturating_add(self.logical_height as i32)
    }

    /// Map a **logical** desktop point to a **physical** pixel in this
    /// display's framebuffer.
    ///
    /// The result is clamped to the framebuffer, because rounding at the far
    /// edge of a fractionally scaled display can otherwise land one pixel past
    /// the end.
    pub fn to_pixel(&self, x: i32, y: i32) -> Option<(u32, u32)> {
        if !self.contains(x, y) {
            return None;
        }

        let px = (f64::from(x - self.logical_x) * self.scale_x()).floor();
        let py = (f64::from(y - self.logical_y) * self.scale_y()).floor();

        Some((
            (px.max(0.0) as u32).min(self.pixel_width.saturating_sub(1)),
            (py.max(0.0) as u32).min(self.pixel_height.saturating_sub(1)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unscaled(id: &str, x: i32) -> Monitor {
        Monitor {
            id: id.into(),
            name: id.into(),
            logical_x: x,
            logical_y: 0,
            logical_width: 1920,
            logical_height: 1080,
            pixel_width: 1920,
            pixel_height: 1080,
            transform: Transform::Normal,
            profile: ColorProfile::Srgb,
        }
    }

    /// 1920x1080 panel driven at a 1.5 logical scale: 1280x720 logical.
    fn fractional(id: &str, x: i32) -> Monitor {
        Monitor {
            id: id.into(),
            name: id.into(),
            logical_x: x,
            logical_y: 0,
            logical_width: 1280,
            logical_height: 720,
            pixel_width: 1920,
            pixel_height: 1080,
            transform: Transform::Normal,
            profile: ColorProfile::Srgb,
        }
    }

    #[test]
    fn bounds_are_half_open_so_adjacent_monitors_never_overlap() {
        let left = unscaled("DP-2", 0);
        let right = unscaled("DP-1", 1920);

        assert!(left.contains(1919, 0));
        assert!(!left.contains(1920, 0));
        assert!(right.contains(1920, 0));
        assert!(!right.contains(1919, 0));
    }

    #[test]
    fn unscaled_displays_map_one_to_one() {
        let m = unscaled("DP-1", 1920);
        assert_eq!(m.to_pixel(1920, 0), Some((0, 0)));
        assert_eq!(m.to_pixel(2000, 50), Some((80, 50)));
        assert_eq!(m.to_pixel(100, 50), None);
        assert!((m.scale_x() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fractional_scaling_maps_logical_points_to_real_pixels() {
        // The case that a scale-1 desktop cannot catch. A 1.5-scaled display
        // is 1280 logical units wide but 1920 pixels wide.
        let m = fractional("DP-1", 0);
        assert!((m.scale_x() - 1.5).abs() < 1e-9);

        assert_eq!(m.to_pixel(0, 0), Some((0, 0)));
        assert_eq!(m.to_pixel(100, 100), Some((150, 150)));
        // The last logical unit must land inside the framebuffer, not past it.
        assert_eq!(m.to_pixel(1279, 719), Some((1918, 1078)));
        assert!(!m.contains(1280, 0), "logical width is 1280, not 1920");
    }

    #[test]
    fn a_fractional_monitor_to_the_right_uses_logical_offsets() {
        // Two 1.5-scaled displays side by side sit at logical 0 and 1280,
        // not at 0 and 1920. Using the physical width as the offset - the bug
        // this type now prevents - would put the seam in the wrong place.
        let left = fractional("DP-2", 0);
        let right = fractional("DP-1", 1280);

        assert!(left.contains(1279, 0));
        assert!(!left.contains(1280, 0));
        assert!(right.contains(1280, 0));
        assert_eq!(right.to_pixel(1280, 0), Some((0, 0)));
        assert_eq!(right.to_pixel(2559, 0), Some((1918, 0)));
    }

    #[test]
    fn mixed_scale_desktops_are_handled_per_monitor() {
        // A scaled laptop panel beside an unscaled external display.
        let laptop = fractional("eDP-1", 0);
        let external = unscaled("DP-1", 1280);

        assert_eq!(laptop.to_pixel(640, 360), Some((960, 540)));
        assert_eq!(external.to_pixel(1280 + 960, 540), Some((960, 540)));
    }

    #[test]
    fn negative_origins_work_for_monitors_left_of_primary() {
        let left = unscaled("DP-3", -1920);
        assert!(left.contains(-1920, 0));
        assert_eq!(left.to_pixel(-1820, 10), Some((100, 10)));
        assert!(!left.contains(0, 0));
    }

    #[test]
    fn a_zero_sized_logical_geometry_does_not_divide_by_zero() {
        let mut m = unscaled("BROKEN", 0);
        m.logical_width = 0;
        m.logical_height = 0;
        assert_eq!(m.scale_x(), 1.0);
        assert_eq!(m.scale_y(), 1.0);
    }

    #[test]
    fn srgb_stays_out_of_the_database() {
        assert_eq!(ColorProfile::Srgb.tag(), None);
        assert_eq!(ColorProfile::DisplayP3.tag().as_deref(), Some("display-p3"));
    }
}

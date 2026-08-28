//! Connected displays, described in physical pixels.

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
///
/// Every geometric field is in **physical pixels**. Logical coordinates are a
/// compositor concept and vary with scaling; the loupe needs to address exact
/// device pixels, so the abstraction refuses to deal in anything else.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// Stable identifier, e.g. `DP-1`.
    pub id: String,
    /// Human-readable description, when the platform offers one.
    pub name: String,
    /// Left edge in the global physical layout.
    pub x: i32,
    /// Top edge in the global physical layout.
    pub y: i32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Ratio of physical pixels to logical ones. May be fractional.
    pub scale: f64,
    /// Rotation applied to the output.
    pub transform: Transform,
    /// The colour space of this display's pixels.
    pub profile: ColorProfile,
}

impl Monitor {
    /// Whether a global physical point lands on this display.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x.saturating_add(self.width as i32)
            && y < self.y.saturating_add(self.height as i32)
    }

    /// Convert a global physical point to one local to this display.
    pub fn to_local(&self, x: i32, y: i32) -> Option<(u32, u32)> {
        self.contains(x, y)
            .then(|| ((x - self.x) as u32, (y - self.y) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: &str, x: i32, y: i32) -> Monitor {
        Monitor {
            id: id.into(),
            name: id.into(),
            x,
            y,
            width: 1920,
            height: 1080,
            scale: 1.0,
            transform: Transform::Normal,
            profile: ColorProfile::Srgb,
        }
    }

    #[test]
    fn bounds_are_half_open_so_adjacent_monitors_never_overlap() {
        let left = monitor("DP-2", 0, 0);
        let right = monitor("DP-1", 1920, 0);

        // The seam belongs to exactly one display.
        assert!(left.contains(1919, 0));
        assert!(!left.contains(1920, 0));
        assert!(right.contains(1920, 0));
        assert!(!right.contains(1919, 0));
    }

    #[test]
    fn local_coordinates_are_relative_to_the_monitor_origin() {
        let right = monitor("DP-1", 1920, 0);
        assert_eq!(right.to_local(1920, 0), Some((0, 0)));
        assert_eq!(right.to_local(2000, 50), Some((80, 50)));
        assert_eq!(right.to_local(100, 50), None);
    }

    #[test]
    fn negative_origins_work_for_monitors_left_of_primary() {
        let left = monitor("DP-3", -1920, 0);
        assert!(left.contains(-1920, 0));
        assert_eq!(left.to_local(-1820, 10), Some((100, 10)));
        assert!(!left.contains(0, 0));
    }

    #[test]
    fn srgb_stays_out_of_the_database() {
        assert_eq!(ColorProfile::Srgb.tag(), None);
        assert_eq!(ColorProfile::DisplayP3.tag().as_deref(), Some("display-p3"));
    }
}

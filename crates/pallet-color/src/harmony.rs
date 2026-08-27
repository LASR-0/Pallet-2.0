//! Harmony sets: related colours found by rotating hue.
//!
//! Rotation happens in Oklch rather than HSL. In HSL a fixed hue step covers
//! wildly different perceptual distances depending on where you start, which is
//! why naive complementary pairs so often look wrong. Oklch steps are even.

use crate::color::Color;
use crate::space::Space;

/// The harmony relationships offered on the Current screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Harmony {
    /// The colour and its opposite.
    Complementary,
    /// Neighbours to either side.
    Analogous,
    /// Three colours evenly spaced around the wheel.
    Triadic,
    /// The colour plus the two neighbours of its complement.
    Split,
}

impl Harmony {
    /// Every harmony, in the order the UI lists them.
    pub const ALL: [Harmony; 4] = [
        Harmony::Complementary,
        Harmony::Analogous,
        Harmony::Triadic,
        Harmony::Split,
    ];

    /// The short label used on the harmony selector.
    pub fn label(self) -> &'static str {
        match self {
            Harmony::Complementary => "Comp",
            Harmony::Analogous => "Analog",
            Harmony::Triadic => "Triad",
            Harmony::Split => "Split",
        }
    }

    /// The stable identifier used in config and IPC.
    pub fn id(self) -> &'static str {
        match self {
            Harmony::Complementary => "complementary",
            Harmony::Analogous => "analogous",
            Harmony::Triadic => "triadic",
            Harmony::Split => "split",
        }
    }

    /// Hue offsets in degrees, matching the prototype's sets exactly.
    pub fn offsets(self) -> &'static [f32] {
        match self {
            Harmony::Complementary => &[0.0, 180.0],
            Harmony::Analogous => &[-30.0, 0.0, 30.0, 60.0],
            Harmony::Triadic => &[0.0, 120.0, 240.0],
            Harmony::Split => &[0.0, 150.0, 210.0],
        }
    }

    /// The swatches for `base`, computed in `space`.
    ///
    /// The zero-degree member is returned as the base colour untouched, so the
    /// original is never altered by a round trip through the colour maths.
    pub fn swatches(self, base: Color, space: Space) -> Vec<Color> {
        self.offsets()
            .iter()
            .map(|&d| {
                if d == 0.0 {
                    base
                } else {
                    space.rotate_hue(base, d)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_match_the_prototype() {
        assert_eq!(Harmony::Complementary.offsets(), &[0.0, 180.0]);
        assert_eq!(Harmony::Analogous.offsets(), &[-30.0, 0.0, 30.0, 60.0]);
        assert_eq!(Harmony::Triadic.offsets(), &[0.0, 120.0, 240.0]);
        assert_eq!(Harmony::Split.offsets(), &[0.0, 150.0, 210.0]);
    }

    #[test]
    fn the_base_colour_is_returned_untouched() {
        let base = Color::parse_hex("#A5236E").unwrap();
        for space in [Space::Oklch, Space::Hsl] {
            for h in Harmony::ALL {
                let swatches = h.swatches(base, space);
                let zero_at = h.offsets().iter().position(|&d| d == 0.0).unwrap();
                assert_eq!(swatches[zero_at], base, "{h:?} in {space:?}");
            }
        }
    }

    #[test]
    fn swatch_count_matches_offset_count() {
        let base = Color::parse_hex("#289788").unwrap();
        for h in Harmony::ALL {
            assert_eq!(h.swatches(base, Space::Oklch).len(), h.offsets().len());
        }
    }

    #[test]
    fn rotating_a_full_turn_returns_the_original() {
        let base = Color::parse_hex("#E0BC67").unwrap();
        let back = Space::Oklch.rotate_hue(base, 360.0);
        let d = |x: u8, y: u8| (i16::from(x) - i16::from(y)).abs();
        assert!(d(base.r, back.r) <= 1 && d(base.g, back.g) <= 1 && d(base.b, back.b) <= 1);
    }
}

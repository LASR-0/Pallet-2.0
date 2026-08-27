//! Which colour space the derived-colour maths runs in.
//!
//! Pallet defaults to Oklch because it is perceptually uniform: equal steps in
//! lightness or hue look like equal steps. HSL is kept because the prototype
//! used it, and because some users want output that matches other tools that
//! also use HSL.

use palette::{FromColor, Hsl, IntoColor, Oklch, Srgb};

use crate::color::Color;

/// The working space for ramps and harmony.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Space {
    /// Perceptually uniform. The default.
    #[default]
    Oklch,
    /// What the prototype used. Faster to reason about, less even to look at.
    Hsl,
}

impl Space {
    /// The stable identifier used in config.
    pub fn id(self) -> &'static str {
        match self {
            Space::Oklch => "oklch",
            Space::Hsl => "hsl",
        }
    }

    /// Rotate `base` around the hue wheel by `degrees`.
    pub fn rotate_hue(self, base: Color, degrees: f32) -> Color {
        match self {
            Space::Oklch => {
                let c = base.to_oklch();
                Color::from_oklch_gamut_mapped(Oklch::new(
                    c.l,
                    c.chroma,
                    c.hue.into_positive_degrees() + degrees,
                ))
            }
            Space::Hsl => {
                let hsl: Hsl = base.srgb_f32().into_color();
                let rotated = Hsl::new(
                    hsl.hue.into_positive_degrees() + degrees,
                    hsl.saturation,
                    hsl.lightness,
                );
                srgb_to_color(Srgb::from_color(rotated))
            }
        }
    }

    /// Replace the lightness of `base`, keeping hue and as much chroma as the
    /// sRGB gamut allows at that lightness.
    ///
    /// `lightness` is 0..=1 in whichever space this is.
    pub fn with_lightness(self, base: Color, lightness: f32) -> Color {
        let lightness = lightness.clamp(0.0, 1.0);
        match self {
            Space::Oklch => {
                let c = base.to_oklch();
                Color::from_oklch_gamut_mapped(Oklch::new(lightness, c.chroma, c.hue))
            }
            Space::Hsl => {
                let hsl: Hsl = base.srgb_f32().into_color();
                let out = Hsl::new(hsl.hue, hsl.saturation, lightness);
                srgb_to_color(Srgb::from_color(out))
            }
        }
    }

    /// The lightness of `base` in this space, as 0..=1.
    pub fn lightness_of(self, base: Color) -> f32 {
        match self {
            Space::Oklch => base.to_oklch().l,
            Space::Hsl => base.to_hsl().2,
        }
    }
}

fn srgb_to_color(c: Srgb<f32>) -> Color {
    let c: Srgb<u8> = Srgb::new(
        c.red.clamp(0.0, 1.0),
        c.green.clamp(0.0, 1.0),
        c.blue.clamp(0.0, 1.0),
    )
    .into_format();
    Color::new(c.red, c.green, c.blue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightness_is_monotonic_in_both_spaces() {
        let base = Color::parse_hex("#A5236E").unwrap();
        for space in [Space::Oklch, Space::Hsl] {
            let mut previous = -1.0_f32;
            for step in 0..=10 {
                let l = step as f32 / 10.0;
                let got = space.lightness_of(space.with_lightness(base, l));
                assert!(
                    got >= previous - 0.02,
                    "{space:?} lightness went backwards at {l}: {got} after {previous}"
                );
                previous = got;
            }
        }
    }

    #[test]
    fn extremes_are_black_and_white() {
        let base = Color::parse_hex("#289788").unwrap();
        for space in [Space::Oklch, Space::Hsl] {
            assert_eq!(space.with_lightness(base, 0.0), Color::new(0, 0, 0));
            assert_eq!(space.with_lightness(base, 1.0), Color::new(255, 255, 255));
        }
    }

    #[test]
    fn oklch_is_the_default() {
        assert_eq!(Space::default(), Space::Oklch);
    }
}

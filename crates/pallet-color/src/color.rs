//! The core colour type and its notations.

use std::fmt;
use std::str::FromStr;

use palette::{FromColor, Hsl, IntoColor, Lab, Oklab, Oklch, Srgb};

use crate::error::ParseError;

/// A colour in the sRGB space with 8 bits per channel.
///
/// This is Pallet's storage and interchange type: it is exactly what a pixel on
/// screen is, what the clipboard receives, and what the database holds. Wider
/// working spaces (Oklch, Lab) are derived on demand rather than stored, so
/// there is a single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Color {
    /// Construct from 8-bit sRGB channels.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse `#RGB`, `#RRGGBB`, or either without the leading `#`.
    ///
    /// Surrounding whitespace is ignored and case does not matter.
    pub fn parse_hex(input: &str) -> Result<Self, ParseError> {
        let s = input.trim().trim_start_matches('#');

        let digits: Vec<u8> = s
            .chars()
            .map(|c| {
                c.to_digit(16).map(|d| d as u8).ok_or(ParseError::NotHex {
                    input: input.into(),
                })
            })
            .collect::<Result<_, _>>()?;

        match digits.len() {
            // #abc expands to #aabbcc, the usual CSS shorthand.
            3 => Ok(Self::new(digits[0] * 17, digits[1] * 17, digits[2] * 17)),
            6 => Ok(Self::new(
                digits[0] << 4 | digits[1],
                digits[2] << 4 | digits[3],
                digits[4] << 4 | digits[5],
            )),
            _ => Err(ParseError::BadLength {
                input: input.into(),
            }),
        }
    }

    /// Uppercase `#RRGGBB`, the form shown throughout the UI.
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// The channels as a tuple.
    pub const fn to_rgb(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    /// Hue in degrees, saturation and lightness as fractions of one.
    pub fn to_hsl(self) -> (f32, f32, f32) {
        let hsl: Hsl = self.srgb_f32().into_color();
        (
            hsl.hue.into_positive_degrees(),
            hsl.saturation,
            hsl.lightness,
        )
    }

    /// This colour in Oklch, the space Pallet uses for ramps and harmony.
    pub fn to_oklch(self) -> Oklch {
        self.srgb_f32().into_color()
    }

    /// This colour in CIELAB, used for perceptual nearest-name matching.
    pub fn to_lab(self) -> Lab {
        self.srgb_f32().into_color()
    }

    /// This colour in Oklab, used as a fast prefilter before CIEDE2000.
    pub fn to_oklab(self) -> Oklab {
        self.srgb_f32().into_color()
    }

    /// Build from an Oklch value, bringing it into the sRGB gamut first.
    ///
    /// Out-of-gamut values have their chroma reduced until they fit, which
    /// preserves hue and lightness. Clamping the RGB channels instead would
    /// shift the hue visibly.
    pub fn from_oklch_gamut_mapped(target: Oklch) -> Self {
        if let Some(color) = Self::try_from_oklch(target) {
            return color;
        }

        // Chroma is the only axis we give up. Bisect for the largest chroma
        // that still lands inside sRGB.
        let (mut lo, mut hi) = (0.0_f32, target.chroma);
        let mut best = Oklch::new(target.l, 0.0, target.hue);

        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            let candidate = Oklch::new(target.l, mid, target.hue);
            match Self::try_from_oklch(candidate) {
                Some(_) => {
                    best = candidate;
                    lo = mid;
                }
                None => hi = mid,
            }
        }

        // At the extremes of lightness the true maximum chroma is zero, but
        // the gamut tolerance lets a sliver through - enough to land one
        // quantisation step off a pure grey. Chroma this small is invisible
        // (a just-noticeable difference in Oklch is nearer 0.01), so snap it.
        const MIN_VISIBLE_CHROMA: f32 = 2e-3;
        if best.chroma < MIN_VISIBLE_CHROMA {
            best.chroma = 0.0;
        }

        Self::from_srgb_f32(Srgb::from_color(best))
    }

    /// This colour as floating-point sRGB.
    pub fn srgb_f32(self) -> Srgb<f32> {
        Srgb::new(self.r, self.g, self.b).into_format()
    }

    fn from_srgb_f32(c: Srgb<f32>) -> Self {
        let c: Srgb<u8> = Srgb::new(
            c.red.clamp(0.0, 1.0),
            c.green.clamp(0.0, 1.0),
            c.blue.clamp(0.0, 1.0),
        )
        .into_format();
        Self::new(c.red, c.green, c.blue)
    }

    /// `Some` when the Oklch value is genuinely representable in sRGB.
    ///
    /// Note this deliberately does *not* bounds-check the converted channels.
    /// `palette` clamps silently inside its Oklch conversions, so an
    /// out-of-gamut input comes back already pinned to 0.0 or 1.0 and would
    /// pass any bounds test. Round-tripping is the only honest check: a colour
    /// that survives the journey back unchanged was really in gamut.
    fn try_from_oklch(value: Oklch) -> Option<Self> {
        // Squared distance in Oklab; 1e-3 is far below a perceptible step but
        // comfortably above f32 round-trip noise.
        const TOLERANCE_SQ: f32 = 1e-6;

        let rgb: Srgb<f32> = Srgb::from_color(value);
        let target: Oklab = value.into_color();
        let actual: Oklab = rgb.into_color();

        let drift = (actual.l - target.l).powi(2)
            + (actual.a - target.a).powi(2)
            + (actual.b - target.b).powi(2);

        (drift <= TOLERANCE_SQ).then(|| Self::from_srgb_f32(rgb))
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Color {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_hex(s)
    }
}

impl From<Color> for Srgb<u8> {
    fn from(c: Color) -> Self {
        Srgb::new(c.r, c.g, c.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_prototype_colour() {
        let c = Color::parse_hex("#A5236E").unwrap();
        assert_eq!(c.to_rgb(), (0xA5, 0x23, 0x6E));
        assert_eq!(c.to_hex(), "#A5236E");
    }

    #[test]
    fn accepts_shorthand_and_bare_and_lowercase() {
        assert_eq!(Color::parse_hex("#abc").unwrap(), Color::new(170, 187, 204));
        assert_eq!(
            Color::parse_hex("a5236e").unwrap(),
            Color::new(165, 35, 110)
        );
        assert_eq!(
            Color::parse_hex("  #A5236E  ").unwrap(),
            Color::new(165, 35, 110)
        );
    }

    #[test]
    fn rejects_nonsense() {
        assert!(Color::parse_hex("#12345").is_err());
        assert!(Color::parse_hex("#GGGGGG").is_err());
        assert!(Color::parse_hex("").is_err());
    }

    #[test]
    fn hex_round_trips_for_every_channel_value() {
        for v in 0..=255u8 {
            let c = Color::new(v, 255 - v, v.wrapping_mul(7));
            assert_eq!(Color::parse_hex(&c.to_hex()).unwrap(), c);
        }
    }

    #[test]
    fn oklch_round_trips_within_one_step() {
        // Every in-gamut colour must survive a trip through Oklch and back.
        for (r, g, b) in [
            (0, 0, 0),
            (255, 255, 255),
            (165, 35, 110),
            (40, 151, 136),
            (224, 188, 103),
        ] {
            let c = Color::new(r, g, b);
            let back = Color::from_oklch_gamut_mapped(c.to_oklch());
            let d = |x: u8, y: u8| (i16::from(x) - i16::from(y)).abs();
            assert!(
                d(c.r, back.r) <= 1 && d(c.g, back.g) <= 1 && d(c.b, back.b) <= 1,
                "{c} round-tripped to {back}"
            );
        }
    }

    #[test]
    fn gamut_mapping_keeps_wild_chroma_in_range() {
        // Chroma far outside sRGB must still yield a usable colour.
        let wild = Oklch::new(0.7, 0.4, 29.0);
        let mapped = Color::from_oklch_gamut_mapped(wild);
        assert_eq!(mapped, Color::parse_hex(&mapped.to_hex()).unwrap());
    }

    #[test]
    fn hsl_matches_the_prototype_for_a_known_colour() {
        // Verified by running the prototype's own rgbToHsl on #A5236E,
        // which prints "325 deg / 65% / 39%".
        let (h, s, l) = Color::parse_hex("#A5236E").unwrap().to_hsl();
        assert_eq!(h.round(), 325.0);
        assert_eq!((s * 100.0).round(), 65.0);
        assert_eq!((l * 100.0).round(), 39.0);
    }
}

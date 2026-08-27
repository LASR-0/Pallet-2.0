//! Tints and shades: a nine-step lightness ramp through a colour.
//!
//! The prototype pinned the top four steps to fixed HSL lightnesses and scaled
//! the lower ones by multiplication, which bunches the dark end together and
//! makes mid-tones drift. Pallet instead spaces the steps evenly in Oklch, so
//! each swatch is a visually equal distance from its neighbours, and reduces
//! chroma only where the sRGB gamut forces it.

use crate::color::Color;
use crate::space::Space;

/// The step labels, matching the design-token convention the prototype used.
pub const STEPS: [u16; 9] = [50, 100, 200, 300, 400, 500, 600, 700, 800];

/// Target lightnesses for each step, from lightest to darkest.
const LIGHTNESS: [f32; 9] = [0.97, 0.92, 0.84, 0.75, 0.66, 0.56, 0.46, 0.36, 0.25];

/// One swatch on the ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swatch {
    /// The design-token step, e.g. `400`.
    pub step: u16,
    /// The colour at this step.
    pub color: Color,
    /// True when this step is the closest one to the original colour.
    pub is_base: bool,
}

/// Build the nine-step ramp through `base`.
///
/// The step nearest the base colour's own lightness is replaced with the base
/// colour itself and marked [`Swatch::is_base`], so the picked colour always
/// appears on its own ramp exactly as picked.
pub fn ramp(base: Color, space: Space) -> Vec<Swatch> {
    let base_l = space.lightness_of(base);

    // Whichever target lightness sits closest to the colour we were given.
    let base_index = LIGHTNESS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - base_l)
                .abs()
                .partial_cmp(&(*b - base_l).abs())
                .expect("lightness values are never NaN")
        })
        .map(|(i, _)| i)
        .unwrap_or(4);

    STEPS
        .iter()
        .zip(LIGHTNESS)
        .enumerate()
        .map(|(i, (&step, l))| Swatch {
            step,
            color: if i == base_index {
                base
            } else {
                space.with_lightness(base, l)
            },
            is_base: i == base_index,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_nine_labelled_steps() {
        let r = ramp(Color::parse_hex("#A5236E").unwrap(), Space::Oklch);
        assert_eq!(r.len(), 9);
        assert_eq!(r.iter().map(|s| s.step).collect::<Vec<_>>(), STEPS.to_vec());
    }

    #[test]
    fn exactly_one_step_is_the_base_and_it_is_unaltered() {
        let base = Color::parse_hex("#A5236E").unwrap();
        for space in [Space::Oklch, Space::Hsl] {
            let r = ramp(base, space);
            let bases: Vec<_> = r.iter().filter(|s| s.is_base).collect();
            assert_eq!(bases.len(), 1, "{space:?}");
            assert_eq!(bases[0].color, base, "{space:?}");
        }
    }

    #[test]
    fn gets_darker_from_start_to_finish() {
        // The property the prototype's multiplicative ramp could violate.
        for hex in ["#A5236E", "#289788", "#E0BC67", "#273148", "#FFECD3"] {
            let base = Color::parse_hex(hex).unwrap();
            let r = ramp(base, Space::Oklch);
            for pair in r.windows(2) {
                let a = Space::Oklch.lightness_of(pair[0].color);
                let b = Space::Oklch.lightness_of(pair[1].color);
                assert!(
                    a > b,
                    "{hex}: step {} was not lighter than {}",
                    pair[0].step,
                    pair[1].step
                );
            }
        }
    }

    #[test]
    fn every_swatch_survives_a_hex_round_trip() {
        let r = ramp(Color::parse_hex("#289788").unwrap(), Space::Oklch);
        for s in r {
            assert_eq!(Color::parse_hex(&s.color.to_hex()).unwrap(), s.color);
        }
    }
}

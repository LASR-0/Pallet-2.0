//! Ways of grouping colours that a person actually thinks in.
//!
//! Every band here is computed in Oklch, not HSL. HSL's lightness says pure
//! yellow and pure blue are equally light, which is plainly false and would put
//! them in the same band; Oklch's does not.
//!
//! The thresholds are judgement calls tuned by eye rather than derived
//! constants. They are named and gathered here so they can be argued with in
//! one place instead of being scattered through the UI.

use crate::color::Color;

/// Below this chroma a colour reads as a grey, and its hue stops carrying
/// meaning — a very slightly warm grey is still just a grey. Set low enough
/// that a cream (chroma 0.039) still reads warm and a navy (0.044) still reads
/// cool; only true greys fall through.
const NEUTRAL_CHROMA: f32 = 0.03;

/// Where muted ends and vivid begins.
const VIVID_CHROMA: f32 = 0.09;

/// Lightness band edges, placed in the gaps of a real sample rather than on
/// round numbers. A mid grey sits at 0.600, pure blue at 0.452 and a cream at
/// 0.952; edges at 0.45 or 0.72 would cut straight through one of them.
const DARK_MAX: f32 = 0.50;
const LIGHT_MIN: f32 = 0.80;

/// Hue arc, in Oklch degrees, that reads as warm: reds through yellows, wrapping
/// past magenta. Everything else — greens, cyans, blues, violets — reads cool.
const WARM_END: f32 = 110.0;
const WARM_START: f32 = 330.0;

/// How a colour reads in temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Temperature {
    /// Reds, oranges, yellows.
    Warm,
    /// Greens, cyans, blues, violets.
    Cool,
    /// Too little chroma for hue to mean anything.
    Neutral,
}

/// How light a colour is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lightness {
    /// Darker than roughly a mid grey.
    Dark,
    /// Neither.
    Mid,
    /// Lighter than roughly a mid grey.
    Light,
}

/// How saturated a colour is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chroma {
    /// Dusty, greyed, desaturated.
    Muted,
    /// Saturated.
    Vivid,
}

/// Which temperature band a colour falls in.
pub fn temperature(color: Color) -> Temperature {
    let oklch = color.to_oklch();
    if oklch.chroma < NEUTRAL_CHROMA {
        return Temperature::Neutral;
    }
    // The warm arc wraps past 0, so it reads as "outside the cool arc".
    let hue = oklch.hue.into_positive_degrees();
    if (WARM_END..WARM_START).contains(&hue) {
        Temperature::Cool
    } else {
        Temperature::Warm
    }
}

/// Which lightness band a colour falls in.
pub fn lightness(color: Color) -> Lightness {
    let l = color.to_oklch().l;
    if l < DARK_MAX {
        Lightness::Dark
    } else if l >= LIGHT_MIN {
        Lightness::Light
    } else {
        Lightness::Mid
    }
}

/// Which chroma band a colour falls in.
pub fn chroma(color: Color) -> Chroma {
    if color.to_oklch().chroma < VIVID_CHROMA {
        Chroma::Muted
    } else {
        Chroma::Vivid
    }
}

/// A named facet, as the UI's chips address them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facet {
    /// Temperature.
    Warm,
    /// Temperature.
    Cool,
    /// Temperature.
    Neutral,
    /// Lightness.
    Light,
    /// Lightness.
    Mid,
    /// Lightness.
    Dark,
    /// Chroma.
    Vivid,
    /// Chroma.
    Muted,
}

impl Facet {
    /// Parse the identifier the UI sends.
    pub fn parse(id: &str) -> Option<Self> {
        Some(match id {
            "warm" => Self::Warm,
            "cool" => Self::Cool,
            "neutral" => Self::Neutral,
            "light" => Self::Light,
            "mid" => Self::Mid,
            "dark" => Self::Dark,
            "vivid" => Self::Vivid,
            "muted" => Self::Muted,
            _ => return None,
        })
    }

    /// Whether a colour belongs to this facet.
    pub fn matches(self, color: Color) -> bool {
        match self {
            Self::Warm => temperature(color) == Temperature::Warm,
            Self::Cool => temperature(color) == Temperature::Cool,
            Self::Neutral => temperature(color) == Temperature::Neutral,
            Self::Light => lightness(color) == Lightness::Light,
            Self::Mid => lightness(color) == Lightness::Mid,
            Self::Dark => lightness(color) == Lightness::Dark,
            Self::Vivid => chroma(color) == Chroma::Vivid,
            Self::Muted => chroma(color) == Chroma::Muted,
        }
    }

    /// Which group this facet belongs to.
    ///
    /// Facets within a group are alternatives — "warm or cool" — while facets
    /// across groups narrow — "warm *and* light". Selecting every chip in a
    /// group would otherwise mean the same as selecting none, which is not
    /// what a person expects a filter to do.
    pub fn group(self) -> u8 {
        match self {
            Self::Warm | Self::Cool | Self::Neutral => 0,
            Self::Light | Self::Mid | Self::Dark => 1,
            Self::Vivid | Self::Muted => 2,
        }
    }
}

/// Does a colour satisfy every selected group?
pub fn matches_all(color: Color, facets: &[Facet]) -> bool {
    if facets.is_empty() {
        return true;
    }
    // Within a group any match will do; every group with a selection must be
    // satisfied.
    (0..=2).all(|group| {
        let mut selected = facets.iter().filter(|f| f.group() == group).peekable();
        selected.peek().is_none() || selected.any(|f| f.matches(color))
    })
}

/// How a list of colours is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// The order they were added, oldest first.
    Added,
    /// Around the hue wheel, greys last since their hue means nothing.
    Hue,
    /// Darkest to lightest.
    Lightness,
    /// Most muted to most vivid.
    Chroma,
    /// Alphabetical by name.
    Name,
}

impl Sort {
    /// Parse the identifier the UI sends.
    pub fn parse(id: &str) -> Option<Self> {
        Some(match id {
            "added" => Self::Added,
            "hue" => Self::Hue,
            "lightness" => Self::Lightness,
            "chroma" => Self::Chroma,
            "name" => Self::Name,
            _ => return None,
        })
    }

    /// A key for ordering, alongside the colour's name.
    ///
    /// Greys sort after every hue: their hue angle is noise, so interleaving
    /// them through the spectrum would scatter them unpredictably.
    pub fn key(self, color: Color) -> (u8, f32) {
        let oklch = color.to_oklch();
        match self {
            Self::Added | Self::Name => (0, 0.0),
            Self::Hue => {
                if oklch.chroma < NEUTRAL_CHROMA {
                    (1, oklch.l)
                } else {
                    (0, oklch.hue.into_positive_degrees())
                }
            }
            Self::Lightness => (0, oklch.l),
            Self::Chroma => (0, oklch.chroma),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(hex: &str) -> Color {
        Color::parse_hex(hex).expect("test colours are valid")
    }

    #[test]
    fn temperature_splits_the_wheel_where_people_expect() {
        assert_eq!(temperature(c("#E8564C")), Temperature::Warm); // red
        assert_eq!(temperature(c("#E0BC67")), Temperature::Warm); // yellow
        assert_eq!(temperature(c("#FC7643")), Temperature::Warm); // orange
        assert_eq!(temperature(c("#289788")), Temperature::Cool); // teal
        assert_eq!(temperature(c("#3B5F86")), Temperature::Cool); // blue
        assert_eq!(temperature(c("#6E5A78")), Temperature::Cool); // violet
    }

    #[test]
    fn greys_are_neutral_whatever_their_hue_says() {
        // A hue angle on a near-grey is noise; it must not decide temperature.
        for hex in ["#808080", "#2A2426", "#EFEDE6", "#FFFFFF", "#000000"] {
            assert_eq!(temperature(c(hex)), Temperature::Neutral, "{hex}");
        }
    }

    #[test]
    fn lightness_bands_order_as_they_read() {
        assert_eq!(lightness(c("#FFFFFF")), Lightness::Light);
        assert_eq!(lightness(c("#000000")), Lightness::Dark);
        assert_eq!(lightness(c("#808080")), Lightness::Mid);
    }

    #[test]
    fn oklch_lightness_separates_yellow_from_blue() {
        // The reason these bands are not computed in HSL: HSL calls both of
        // these 50% lightness, which no one looking at them would agree with.
        // Oklch puts yellow at 0.968 and blue at 0.452.
        assert_eq!(lightness(c("#FFFF00")), Lightness::Light);
        assert_eq!(lightness(c("#0000FF")), Lightness::Dark);
    }

    #[test]
    fn band_edges_do_not_fall_on_common_colours() {
        // Thresholds sitting on top of a colour people actually use make the
        // filter feel arbitrary: the same swatch lands in different bands
        // after an imperceptible edit.
        for hex in [
            "#000000", "#273148", "#0000FF", "#808080", "#289788", "#FF0000", "#C87D5B", "#FFECD3",
            "#FFFF00", "#FFFFFF",
        ] {
            let l = c(hex).to_oklch().l;
            for edge in [DARK_MAX, LIGHT_MIN] {
                assert!(
                    (l - edge).abs() > 0.03,
                    "{hex} sits at {l:.3}, too close to the {edge} edge"
                );
            }
        }
    }

    #[test]
    fn chroma_bands_separate_dusty_from_saturated() {
        assert_eq!(chroma(c("#FF0000")), Chroma::Vivid);
        assert_eq!(chroma(c("#B4A79E")), Chroma::Muted);
        assert_eq!(chroma(c("#808080")), Chroma::Muted);
    }

    #[test]
    fn facets_within_a_group_widen_and_across_groups_narrow() {
        let warm_light = c("#FFD3B6");
        let warm_dark = c("#8C4B54");
        let cool_light = c("#A8E6CF");

        // One group: alternatives.
        let temps = [Facet::Warm, Facet::Cool];
        assert!(matches_all(warm_light, &temps));
        assert!(matches_all(cool_light, &temps));

        // Two groups: both must hold.
        let warm_and_light = [Facet::Warm, Facet::Light];
        assert!(matches_all(warm_light, &warm_and_light));
        assert!(!matches_all(warm_dark, &warm_and_light), "dark fails light");
        assert!(!matches_all(cool_light, &warm_and_light), "cool fails warm");
    }

    #[test]
    fn no_facets_matches_everything() {
        assert!(matches_all(c("#A5236E"), &[]));
    }

    #[test]
    fn hue_sort_puts_greys_after_the_spectrum() {
        let grey = Sort::Hue.key(c("#808080"));
        let red = Sort::Hue.key(c("#FF0000"));
        let blue = Sort::Hue.key(c("#0000FF"));
        assert_eq!(grey.0, 1, "greys go in the second bucket");
        assert_eq!(red.0, 0);
        assert_eq!(blue.0, 0);
        assert!(red.1 < blue.1, "red comes before blue around the wheel");
    }

    #[test]
    fn identifiers_round_trip_from_the_ui() {
        for id in [
            "warm", "cool", "neutral", "light", "mid", "dark", "vivid", "muted",
        ] {
            assert!(Facet::parse(id).is_some(), "{id}");
        }
        assert!(Facet::parse("chartreuse").is_none());
        for id in ["added", "hue", "lightness", "chroma", "name"] {
            assert!(Sort::parse(id).is_some(), "{id}");
        }
        assert!(Sort::parse("vibes").is_none());
    }
}

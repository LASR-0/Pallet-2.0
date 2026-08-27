//! Contrast metrics for judging whether text will be readable on a colour.
//!
//! Two metrics are offered because they disagree, and the disagreement matters.
//! WCAG 2.1 is what accessibility audits and most design tools still check
//! against, so Pallet must report it. APCA models perceived lightness far more
//! accurately, especially for dark themes and thin type, where WCAG 2.1 is
//! known to be badly wrong in both directions.

use palette::color_difference::Wcag21RelativeContrast;

use crate::color::Color;

/// WCAG 2.1 relative contrast ratio, from 1.0 to 21.0.
pub fn wcag21_ratio(a: Color, b: Color) -> f32 {
    a.srgb_f32().relative_contrast(b.srgb_f32())
}

/// The WCAG 2.1 conformance level a pair reaches for normal-sized body text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WcagLevel {
    /// Below 3.0 — fails every level.
    Fail,
    /// At least 3.0 — passes AA for large text only.
    AaLarge,
    /// At least 4.5 — passes AA for body text.
    Aa,
    /// At least 7.0 — passes AAA for body text.
    Aaa,
}

impl WcagLevel {
    /// Classify a contrast ratio.
    pub fn of(ratio: f32) -> Self {
        if ratio >= 7.0 {
            Self::Aaa
        } else if ratio >= 4.5 {
            Self::Aa
        } else if ratio >= 3.0 {
            Self::AaLarge
        } else {
            Self::Fail
        }
    }

    /// A short label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Fail => "Fail",
            Self::AaLarge => "AA Large",
            Self::Aa => "AA",
            Self::Aaa => "AAA",
        }
    }
}

// APCA-W3 0.1.9 constants. These are not tunable: they are the published
// values, and changing any of them makes the output no longer APCA.
const MAIN_TRC: f32 = 2.4;
const R_CO: f32 = 0.2126729;
const G_CO: f32 = 0.7151522;
const B_CO: f32 = 0.0721750;
const NORM_BG: f32 = 0.56;
const NORM_TXT: f32 = 0.57;
const REV_TXT: f32 = 0.62;
const REV_BG: f32 = 0.65;
const BLK_THRS: f32 = 0.022;
const BLK_CLMP: f32 = 1.414;
const SCALE_BOW: f32 = 1.14;
const SCALE_WOB: f32 = 1.14;
const LO_BOW_OFFSET: f32 = 0.027;
const LO_WOB_OFFSET: f32 = 0.027;
const DELTA_Y_MIN: f32 = 0.0005;
const LO_CLIP: f32 = 0.1;

/// Screen luminance with APCA's black-level soft clamp applied.
fn apca_luminance(c: Color) -> f32 {
    let ch = |v: u8| (f32::from(v) / 255.0).powf(MAIN_TRC);
    let y = R_CO * ch(c.r) + G_CO * ch(c.g) + B_CO * ch(c.b);

    // Near-black needs lifting, or contrast is wildly overstated down there.
    if y < BLK_THRS {
        y + (BLK_THRS - y).powf(BLK_CLMP)
    } else {
        y
    }
}

/// APCA lightness contrast (Lc) for `text` drawn on `background`.
///
/// The result runs roughly -108..=106. The sign carries meaning: positive is
/// dark text on a light background, negative is light text on dark. Use the
/// absolute value to judge strength — around 60 for body text, 75 for small or
/// thin type, 45 for large headings.
pub fn apca_lc(text: Color, background: Color) -> f32 {
    let y_txt = apca_luminance(text);
    let y_bg = apca_luminance(background);

    // Colours too close in luminance to carry any contrast at all.
    if (y_bg - y_txt).abs() < DELTA_Y_MIN {
        return 0.0;
    }

    if y_bg > y_txt {
        // Dark text on a light background.
        let sapc = (y_bg.powf(NORM_BG) - y_txt.powf(NORM_TXT)) * SCALE_BOW;
        if sapc < LO_CLIP {
            0.0
        } else {
            (sapc - LO_BOW_OFFSET) * 100.0
        }
    } else {
        // Light text on a dark background.
        let sapc = (y_bg.powf(REV_BG) - y_txt.powf(REV_TXT)) * SCALE_WOB;
        if sapc > -LO_CLIP {
            0.0
        } else {
            (sapc + LO_WOB_OFFSET) * 100.0
        }
    }
}

/// Pick black or white body text for `background`, whichever reads better.
///
/// Judged by APCA rather than a lightness threshold, because a mid-tone can
/// favour the opposite choice from what a naive cutoff would give.
pub fn best_text_on(background: Color) -> Color {
    const BLACK: Color = Color::new(0, 0, 0);
    const WHITE: Color = Color::new(255, 255, 255);

    if apca_lc(BLACK, background).abs() >= apca_lc(WHITE, background).abs() {
        BLACK
    } else {
        WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Color = Color::new(0, 0, 0);
    const WHITE: Color = Color::new(255, 255, 255);

    #[test]
    fn wcag_extremes_are_the_known_bounds() {
        assert!((wcag21_ratio(BLACK, WHITE) - 21.0).abs() < 0.01);
        assert!((wcag21_ratio(WHITE, WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn wcag_is_symmetric() {
        let a = Color::parse_hex("#A5236E").unwrap();
        let b = Color::parse_hex("#E0BC67").unwrap();
        assert!((wcag21_ratio(a, b) - wcag21_ratio(b, a)).abs() < 1e-4);
    }

    #[test]
    fn wcag_levels_sit_on_the_published_thresholds() {
        assert_eq!(WcagLevel::of(2.9), WcagLevel::Fail);
        assert_eq!(WcagLevel::of(3.0), WcagLevel::AaLarge);
        assert_eq!(WcagLevel::of(4.5), WcagLevel::Aa);
        assert_eq!(WcagLevel::of(7.0), WcagLevel::Aaa);
    }

    #[test]
    fn apca_matches_the_published_reference_values() {
        // The two canonical APCA-W3 extremes. If these drift, the constants or
        // the formula have been altered and the output is no longer APCA.
        assert!(
            (apca_lc(BLACK, WHITE) - 106.04).abs() < 0.05,
            "black on white was {}",
            apca_lc(BLACK, WHITE)
        );
        assert!(
            (apca_lc(WHITE, BLACK) + 107.88).abs() < 0.05,
            "white on black was {}",
            apca_lc(WHITE, BLACK)
        );
    }

    #[test]
    fn apca_sign_encodes_polarity() {
        // Dark on light is positive; light on dark is negative.
        assert!(apca_lc(BLACK, WHITE) > 0.0);
        assert!(apca_lc(WHITE, BLACK) < 0.0);
    }

    #[test]
    fn apca_reports_no_contrast_for_identical_colours() {
        assert_eq!(apca_lc(WHITE, WHITE), 0.0);
        let mid = Color::parse_hex("#808080").unwrap();
        assert_eq!(apca_lc(mid, mid), 0.0);
    }

    #[test]
    fn text_choice_flips_across_the_lightness_range() {
        assert_eq!(best_text_on(WHITE), BLACK);
        assert_eq!(best_text_on(BLACK), WHITE);
        // The prototype's own accents. White wins on the terracotta accent
        // (APCA -64.5 versus +45.0 for black), which is what the prototype
        // itself does: it draws #fff on var(--accent).
        assert_eq!(best_text_on(Color::parse_hex("#C87D5B").unwrap()), WHITE);
        assert_eq!(best_text_on(Color::parse_hex("#273148").unwrap()), WHITE);
        assert_eq!(best_text_on(Color::parse_hex("#FFECD3").unwrap()), BLACK);
    }
}

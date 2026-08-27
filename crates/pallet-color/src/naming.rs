//! Nearest-name lookup for picked colours.
//!
//! Matching is two-stage for speed. A cheap squared-Euclidean pass in Oklab
//! narrows 31,916 candidates to a shortlist, then CIEDE2000 — accurate but far
//! too slow to run across the whole set on every cursor move — ranks only the
//! survivors. That keeps a lookup well inside a frame budget, so the loupe can
//! name colours live while the pointer moves.

use std::sync::OnceLock;

use palette::color_difference::Ciede2000;
use palette::{Lab, Oklab};

use crate::color::Color;

/// Raw dataset, compiled into the binary. See `assets/colornames/README.md`.
const DATA: &str = include_str!("../../../assets/colornames/colornames.csv");

/// How many Oklab candidates get the expensive CIEDE2000 treatment.
const SHORTLIST: usize = 48;

/// A named colour from the dataset.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedColor {
    /// The name, e.g. `Atomic Tangerine`.
    pub name: &'static str,
    /// The colour the name refers to.
    pub color: Color,
    /// Whether upstream includes this name in its curated subset.
    ///
    /// Note this marks membership of a hand-picked set (~4,959 of 31,916), not
    /// familiarity: "Crisps" and "Fabric of Love" carry the flag while "Carrot"
    /// and "Mandarin" rank below them. Useful only as a tie-break.
    pub well_known: bool,
}

/// A lookup result.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// The name that was matched.
    pub named: NamedColor,
    /// CIEDE2000 distance. Below roughly 2.0 is imperceptible.
    pub distance: f32,
    /// True when the dataset colour is exactly the queried colour.
    pub exact: bool,
}

struct Entry {
    name: &'static str,
    color: Color,
    well_known: bool,
    oklab: Oklab,
    lab: Lab,
}

fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let mut lines = DATA.lines();

        // Fail loudly if the upstream format ever shifts under us.
        let header = lines.next().unwrap_or_default().trim();
        assert_eq!(
            header, "name,hex,good name",
            "colornames.csv header changed; check assets/colornames/README.md"
        );

        lines
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| {
                let mut f = line.split(',');
                let name = f.next()?.trim();
                let hex = f.next()?.trim();
                let well_known = f.next().is_some_and(|g| g.trim() == "x");
                let color = Color::parse_hex(hex).ok()?;
                Some(Entry {
                    name,
                    color,
                    well_known,
                    oklab: color.to_oklab(),
                    lab: color.to_lab(),
                })
            })
            .collect()
    })
}

/// How many names are loaded.
pub fn len() -> usize {
    entries().len()
}

/// Find the closest name to `query`.
///
/// Returns `None` only if the dataset is empty, which cannot happen with the
/// vendored file but is expressible rather than a panic.
pub fn nearest(query: Color) -> Option<Match> {
    let all = entries();
    if all.is_empty() {
        return None;
    }

    let q_oklab = query.to_oklab();
    let q_lab = query.to_lab();

    // Stage one: cheap squared distance in Oklab over everything.
    let mut shortlist: Vec<(f32, &Entry)> = Vec::with_capacity(all.len());
    for e in all {
        let d = (e.oklab.l - q_oklab.l).powi(2)
            + (e.oklab.a - q_oklab.a).powi(2)
            + (e.oklab.b - q_oklab.b).powi(2);
        shortlist.push((d, e));
    }
    let cut = SHORTLIST.min(shortlist.len());
    shortlist.select_nth_unstable_by(cut - 1, |a, b| {
        a.0.partial_cmp(&b.0).expect("distances are never NaN")
    });
    shortlist.truncate(cut);

    // Stage two: CIEDE2000 on the survivors, preferring well-known names when
    // two candidates are perceptually indistinguishable.
    let best = shortlist
        .iter()
        .map(|(_, e)| (e, q_lab.difference(e.lab)))
        .min_by(|(ea, da), (eb, db)| {
            const TIE: f32 = 0.5;
            if (da - db).abs() < TIE && ea.well_known != eb.well_known {
                // Indistinguishable to the eye, so fall back to the curated
                // subset. This is a mild preference, not a guarantee of a
                // recognisable name - see NamedColor::well_known.
                return eb.well_known.cmp(&ea.well_known);
            }
            da.partial_cmp(db).expect("distances are never NaN")
        })?;

    let (entry, distance) = best;
    Some(Match {
        named: NamedColor {
            name: entry.name,
            color: entry.color,
            well_known: entry.well_known,
        },
        distance,
        exact: entry.color == query,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whole_dataset_parses() {
        // 31,916 rows upstream; allow drift without letting a broken parse pass.
        assert!(len() > 31_000, "only parsed {} names", len());
    }

    #[test]
    fn exact_dataset_colours_match_themselves() {
        for hex in ["#ff9966", "#048243", "#35514f"] {
            let c = Color::parse_hex(hex).unwrap();
            let m = nearest(c).unwrap();
            assert!(m.exact, "{hex} matched {} at {}", m.named.name, m.distance);
            assert_eq!(m.distance, 0.0);
        }
    }

    #[test]
    fn known_names_come_back_for_known_colours() {
        let m = nearest(Color::parse_hex("#ff9966").unwrap()).unwrap();
        assert_eq!(m.named.name, "Atomic Tangerine");
    }

    #[test]
    fn pure_black_and_white_are_named_sensibly() {
        assert_eq!(nearest(Color::new(0, 0, 0)).unwrap().named.name, "Black");
        assert_eq!(
            nearest(Color::new(255, 255, 255)).unwrap().named.name,
            "White"
        );
    }

    #[test]
    fn every_colour_gets_a_close_name() {
        // A coarse sweep of the cube: no colour should be far from every name.
        for r in (0..=255).step_by(51) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    let c = Color::new(r, g, b);
                    let m = nearest(c).expect("dataset is never empty");
                    assert!(
                        m.distance < 12.0,
                        "{c} nearest was {} at {}",
                        m.named.name,
                        m.distance
                    );
                }
            }
        }
    }

    #[test]
    fn the_prototype_colours_all_resolve() {
        for hex in [
            "#A5236E", "#C05060", "#FFECD3", "#FEA465", "#FC7643", "#AF4F41", "#273148", "#254350",
            "#289788", "#E0BC67",
        ] {
            let c = Color::parse_hex(hex).unwrap();
            assert!(nearest(c).is_some(), "{hex} had no match");
        }
    }
}

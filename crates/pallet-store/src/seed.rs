//! The prototype's sample library, used to populate a fresh install.
//!
//! These are the exact palettes and colours from `Prototype/package`. Two of
//! the names there ("Freckled Bluewood" and "Dim Red") appear in no colour-name
//! dataset, so they are carried as authored names rather than looked up.

use pallet_color::Color;

use crate::error::Result;
use crate::model::NewColour;
use crate::store::Store;

/// A palette from the prototype.
#[derive(Debug)]
pub struct SeedPalette {
    /// Display name.
    pub name: &'static str,
    /// The year the prototype dates it to, shown on the card as "5 · 2019".
    pub year: i32,
    /// Members, in order.
    pub colours: &'static [&'static str],
}

/// The four palettes shown on the prototype's Palettes screen.
pub const PALETTES: &[SeedPalette] = &[
    SeedPalette {
        name: "Winter Sunset",
        year: 2019,
        colours: &["#F9A08A", "#F0657A", "#B76C7E", "#6E5A78", "#3B5F86"],
    },
    SeedPalette {
        name: "Bussel",
        year: 2019,
        colours: &["#8FB593", "#FCCB95", "#F96A5B", "#DC3B4E", "#22333B"],
    },
    SeedPalette {
        name: "Glasshouse",
        year: 2020,
        colours: &["#A8E6CF", "#DCEDC1", "#FFD3B6", "#FFAAA5", "#FF8B94"],
    },
    SeedPalette {
        name: "Rob Roy",
        year: 2021,
        colours: &["#E0BC67", "#C9736A", "#8C4B54", "#3F3244", "#1E2430"],
    },
];

/// The named colours from the prototype's Colours screen.
pub const COLOURS: &[(&str, &str)] = &[
    ("Dim Red", "#C05060"),
    ("Karry", "#FFECD3"),
    ("Atomic Tangerine", "#FEA465"),
    ("Crusta", "#FC7643"),
    ("Apple Blossom", "#AF4F41"),
    ("Freckled Bluewood", "#273148"),
    ("Blue Dianne", "#254350"),
    ("Jungle Green", "#289788"),
    ("Rob Roy", "#E0BC67"),
];

/// Populate an empty library with the prototype's sample data.
///
/// Does nothing if the library already holds colours, so it is safe to call on
/// every start-up.
pub fn seed_if_empty(store: &Store) -> Result<bool> {
    if !store.colours()?.is_empty() {
        return Ok(false);
    }

    for (name, hex) in COLOURS {
        let color = Color::parse_hex(hex).expect("seed colours are valid hex");
        store.add_colour(&NewColour::new(color).named(*name))?;
    }

    for (index, palette) in PALETTES.iter().enumerate() {
        let ids = palette
            .colours
            .iter()
            .map(|hex| {
                let color = Color::parse_hex(hex).expect("seed colours are valid hex");
                store.add_colour(&NewColour::new(color))
            })
            .collect::<Result<Vec<_>>>()?;
        // Dated to the prototype's own year so a fresh library renders exactly
        // as the design does. The index offsets the day because two samples
        // share a year, and identical timestamps would leave their order to a
        // random uuid tiebreak.
        let created = time::Date::from_ordinal_date(palette.year, 1 + index as u16)
            .expect("the first days of a year always exist")
            .midnight()
            .assume_utc();
        store.create_palette_at(palette.name, &ids, created)?;
    }

    Ok(true)
}

//! Rows as Rust values.

use pallet_color::Color;
use time::OffsetDateTime;

/// A colour saved in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredColour {
    /// Stable identifier.
    pub id: String,
    /// The colour itself.
    pub color: Color,
    /// User-visible name, if it has one.
    pub name: Option<String>,
    /// Display profile this was captured from. `None` means sRGB.
    pub source_space: Option<String>,
    /// When it was added.
    pub created_at: OffsetDateTime,
}

/// A colour about to be saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewColour {
    /// The colour itself.
    pub color: Color,
    /// Optional name.
    pub name: Option<String>,
    /// Optional source display profile.
    pub source_space: Option<String>,
}

impl NewColour {
    /// A colour with no name and no recorded profile.
    pub fn new(color: Color) -> Self {
        Self {
            color,
            name: None,
            source_space: None,
        }
    }

    /// Attach a name.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// A named, ordered group of colours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// Stable identifier.
    pub id: String,
    /// User-visible name.
    pub name: String,
    /// Members, in display order.
    pub colours: Vec<StoredColour>,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it last changed.
    pub updated_at: OffsetDateTime,
}

/// One entry in the pick history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    /// Stable identifier.
    pub id: String,
    /// The colour that was picked.
    pub color: Color,
    /// Display profile it came from. `None` means sRGB.
    pub source_space: Option<String>,
    /// The application under the cursor, when known.
    pub source_app: Option<String>,
    /// When it happened.
    pub picked_at: OffsetDateTime,
}

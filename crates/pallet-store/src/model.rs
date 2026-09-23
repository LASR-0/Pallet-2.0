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

/// What one member of a palette is called when it is exported.
///
/// Two levels: `brand/primary`. Both sides are optional — a member that has
/// never been named carries `Token::default()`, and the exporters fall back to
/// a nearest-match colour name for it, as they did before tokens existed.
///
/// This belongs to the membership, not to the colour: the same colour is
/// `brand/primary` in one palette and `accent` in another, and
/// [`StoredColour::name`] stays what the colour is called in the library.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Token {
    /// The group it sits in, if any.
    pub group: Option<String>,
    /// Its name within that group, if any.
    pub name: Option<String>,
}

impl Token {
    /// Whether this member has been given any token at all.
    pub fn is_empty(&self) -> bool {
        self.group.is_none() && self.name.is_none()
    }
}

/// A colour going into a palette, with what it is called there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The library colour this slot holds.
    pub colour_id: String,
    /// What it is called in this palette.
    pub token: Token,
}

impl Member {
    /// A member with no token, which is every member of a palette built
    /// before tokens existed and of one whose colours were never named.
    pub fn new(colour_id: impl Into<String>) -> Self {
        Self {
            colour_id: colour_id.into(),
            token: Token::default(),
        }
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
    /// What each member is called, index-parallel with [`Palette::colours`].
    ///
    /// Parallel rather than folded into the element so that the many readers
    /// that only want the colours are untouched. Both vectors are built in one
    /// place — `Store::palette` — from one query, so the two cannot come out
    /// different lengths.
    pub tokens: Vec<Token>,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it last changed.
    pub updated_at: OffsetDateTime,
}

impl Palette {
    /// The colour in a slot together with what it is called.
    pub fn member(&self, index: usize) -> Option<(&StoredColour, &Token)> {
        Some((self.colours.get(index)?, self.tokens.get(index)?))
    }
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

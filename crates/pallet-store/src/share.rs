//! Handing a library to someone else running Pallet.
//!
//! Not a copy of the database. Shipping `pallet.db` would be faithful and
//! trivial to implement, and it would also arrive as a replacement for
//! everything the recipient had: their colours, their palettes, and — since
//! the file holds it too — their pick history, which is nobody's business.
//! What gets written here is a document describing a library, which the other
//! end merges into the one it already has.
//!
//! Merging is possible because identity was settled long before this module
//! existed. The schema's opening comment says rows carry UUIDs "so that rows
//! keep a stable identity across exports, imports and any future sync", and
//! that is exactly the property being spent: the same colour exported twice
//! and imported twice is one row, not two, without anything having to compare
//! names or channel values and guess.
//!
//! Nothing here overwrites. A row whose id is already present is left as the
//! recipient has it — they may have renamed that colour, or reordered that
//! palette, and an import is not the moment to discard that. So the operation
//! is strictly additive, which also makes it safe to repeat: importing the
//! same bundle twice changes nothing the second time.
//!
//! Pick history is deliberately absent. It is a record of what someone was
//! doing on their own screen, it is the one table that grows without bound,
//! and it would mean nothing in another library.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use pallet_color::Color;

use crate::error::{Error, Result};
use crate::store::Store;

/// The bundle layout this build writes.
///
/// Bumped only for a change a previous Pallet could not read correctly.
/// Adding a field that older builds can ignore does not need it, because
/// unknown fields are dropped on the way in rather than refused.
pub const FORMAT: u32 = 1;

/// The conventional extension for a written bundle.
pub const EXTENSION: &str = "pallet";

/// A library, or part of one, in transit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    /// Layout version. See [`FORMAT`].
    pub format: u32,
    /// When this was written, RFC 3339.
    pub exported_at: String,
    /// The Pallet that wrote it, for a human reading the file.
    pub app: String,
    /// Every colour, with the tags it carries.
    pub colours: Vec<BundleColour>,
    /// Every palette, with what each slot holds and is called.
    pub palettes: Vec<BundlePalette>,
}

/// One colour in a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleColour {
    /// The identity that makes merging work.
    pub id: String,
    /// `#RRGGBB`.
    ///
    /// Hex rather than the three integers the table stores, because this file
    /// is meant to survive being opened in a text editor by someone trying to
    /// work out what they have been sent, and `"#C87D5B"` answers that where
    /// `{"r":200,"g":125,"b":91}` makes them do arithmetic.
    pub hex: String,
    /// What the colour is called in the library, if anything.
    #[serde(default)]
    pub name: Option<String>,
    /// The display profile it was captured from. Absent means sRGB.
    #[serde(default)]
    pub source_space: Option<String>,
    /// When it was added, RFC 3339.
    pub created_at: String,
    /// Tag names. Tags are matched by name on the way in, not by id — they
    /// are a shared vocabulary rather than rows someone owns, and a
    /// recipient who already has "muted" should not end up with two.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// One palette in a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlePalette {
    /// The identity that makes merging work.
    pub id: String,
    /// User-visible name.
    pub name: String,
    /// When it was created, RFC 3339.
    pub created_at: String,
    /// When it last changed, RFC 3339.
    pub updated_at: String,
    /// Slots, in display order.
    pub members: Vec<BundleMember>,
}

/// One slot of a palette in a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMember {
    /// The id of the colour this slot holds.
    pub colour: String,
    /// The group half of its export token, if it has one.
    #[serde(default)]
    pub group: Option<String>,
    /// The name half of its export token, if it has one.
    #[serde(default)]
    pub name: Option<String>,
}

/// What an import actually did.
///
/// Both halves of each pair are reported because "nothing happened" and "it
/// worked" look identical otherwise, and re-importing a bundle you already
/// have is a thing people will do by accident.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Merged {
    /// Colours written.
    pub colours_added: usize,
    /// Colours already present, left untouched.
    pub colours_known: usize,
    /// Palettes written.
    pub palettes_added: usize,
    /// Palettes already present, left untouched.
    pub palettes_known: usize,
    /// Slots dropped because the colour they named was in neither the bundle
    /// nor the library. A bundle this crate wrote cannot produce any; a
    /// hand-edited one can.
    pub members_dropped: usize,
}

impl Merged {
    /// Whether anything at all was written.
    pub fn is_empty(&self) -> bool {
        self.colours_added == 0 && self.palettes_added == 0
    }
}

impl Bundle {
    /// Read a bundle from JSON, refusing one this build cannot honour.
    pub fn from_json(text: &str) -> Result<Self> {
        let bundle: Bundle = serde_json::from_str(text)?;
        if bundle.format > FORMAT {
            return Err(Error::BundleTooNew {
                found: bundle.format,
                supported: FORMAT,
            });
        }
        Ok(bundle)
    }

    /// Write this bundle as JSON.
    ///
    /// Pretty-printed on purpose: it costs a little size and buys a file that
    /// diffs line by line and can be read by whoever receives it.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

impl Store {
    /// Describe this library as a bundle.
    pub fn export_bundle(&self) -> Result<Bundle> {
        let colours = self
            .colours()?
            .into_iter()
            .map(|colour| {
                Ok(BundleColour {
                    tags: self.tags_for(&colour.id)?,
                    hex: colour.color.to_hex(),
                    id: colour.id,
                    name: colour.name,
                    source_space: colour.source_space,
                    created_at: stamp(colour.created_at),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let palettes = self
            .palettes()?
            .into_iter()
            .map(|palette| BundlePalette {
                members: palette
                    .colours
                    .iter()
                    .zip(&palette.tokens)
                    .map(|(colour, token)| BundleMember {
                        colour: colour.id.clone(),
                        group: token.group.clone(),
                        name: token.name.clone(),
                    })
                    .collect(),
                id: palette.id,
                name: palette.name,
                created_at: stamp(palette.created_at),
                updated_at: stamp(palette.updated_at),
            })
            .collect();

        Ok(Bundle {
            format: FORMAT,
            exported_at: stamp(OffsetDateTime::now_utc()),
            app: format!("pallet {}", env!("CARGO_PKG_VERSION")),
            colours,
            palettes,
        })
    }

    /// Merge a bundle into this library, adding what is missing and touching
    /// nothing that is already here.
    ///
    /// One transaction: a bundle that fails half way through would otherwise
    /// leave palettes referring to colours that never arrived, and the user
    /// with no way to tell which half they got.
    pub fn import_bundle(&self, bundle: &Bundle) -> Result<Merged> {
        if bundle.format > FORMAT {
            return Err(Error::BundleTooNew {
                found: bundle.format,
                supported: FORMAT,
            });
        }

        let mut merged = Merged::default();
        let tx = self.conn.unchecked_transaction()?;

        for colour in &bundle.colours {
            // Parsed rather than trusted: this file may have been hand-edited,
            // and the channel CHECK constraints in the schema are the last
            // line of defence, not the first.
            let color = Color::parse_hex(&colour.hex).map_err(|_| Error::BadBundleColour {
                id: colour.id.clone(),
                value: colour.hex.clone(),
            })?;
            let (r, g, b) = color.to_rgb();

            let written = tx.execute(
                "INSERT INTO colours (id, r, g, b, name, source_space, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (id) DO NOTHING",
                rusqlite::params![
                    colour.id,
                    r,
                    g,
                    b,
                    colour.name,
                    colour.source_space,
                    colour.created_at,
                ],
            )?;
            if written == 1 {
                merged.colours_added += 1;
            } else {
                merged.colours_known += 1;
            }

            // Tags are applied whether or not the colour is new. A colour the
            // recipient already has may be arriving with a tag they have not
            // got, and that is worth keeping.
            for tag in &colour.tags {
                let tag = tag.trim();
                if tag.is_empty() {
                    continue;
                }
                let tag_id: String = match tx
                    .query_row(
                        "SELECT id FROM tags WHERE name = ?1",
                        rusqlite::params![tag],
                        |row| row.get(0),
                    )
                    .ok()
                {
                    Some(id) => id,
                    None => {
                        let id = uuid::Uuid::new_v4().to_string();
                        tx.execute(
                            "INSERT INTO tags (id, name) VALUES (?1, ?2)",
                            rusqlite::params![id, tag],
                        )?;
                        id
                    }
                };
                tx.execute(
                    "INSERT OR IGNORE INTO colour_tags (colour_id, tag_id) VALUES (?1, ?2)",
                    rusqlite::params![colour.id, tag_id],
                )?;
            }
        }

        for palette in &bundle.palettes {
            let written = tx.execute(
                "INSERT INTO palettes (id, name, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (id) DO NOTHING",
                rusqlite::params![
                    palette.id,
                    palette.name,
                    palette.created_at,
                    palette.updated_at,
                ],
            )?;
            if written == 0 {
                // Already here. Its membership is the recipient's business —
                // they may have reordered it since — so it is left alone.
                merged.palettes_known += 1;
                continue;
            }
            merged.palettes_added += 1;

            // `position` is half the primary key, so it has to stay dense.
            // Counting placed slots rather than using the loop index keeps it
            // that way when a member is dropped below.
            let mut position = 0i64;
            for member in &palette.members {
                let exists: bool = tx
                    .query_row(
                        "SELECT 1 FROM colours WHERE id = ?1",
                        rusqlite::params![member.colour],
                        |_| Ok(true),
                    )
                    .unwrap_or(false);
                if !exists {
                    merged.members_dropped += 1;
                    continue;
                }
                tx.execute(
                    "INSERT INTO palette_colours
                         (palette_id, colour_id, position, token_group, token_name)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        palette.id,
                        member.colour,
                        position,
                        blank_to_none(member.group.as_deref()),
                        blank_to_none(member.name.as_deref()),
                    ],
                )?;
                position += 1;
            }
        }

        tx.commit()?;
        Ok(merged)
    }
}

fn stamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339)
        .expect("RFC3339 formatting of a valid timestamp cannot fail")
}

/// As `store::blank_to_none`, which is private to that module and stays so:
/// this is the import path's copy of the same rule, that a token field which
/// is absent or only whitespace is stored as NULL.
fn blank_to_none(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

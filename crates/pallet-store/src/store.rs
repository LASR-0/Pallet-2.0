//! The SQLite library: colours, palettes, pick history and tags.

use std::path::Path;

use pallet_color::Color;
use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::{NewColour, Palette, Pick, StoredColour};

/// Ordered schema migrations. Append only: never edit a shipped migration,
/// because databases in the wild have already applied it.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("../migrations/0001_initial.sql"))])
}

/// A handle to Pallet's library.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the library at `path` and bring the schema up to date.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// Open a private in-memory library. Used by tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut conn: Connection) -> Result<Self> {
        // WAL keeps the resident picker's writes from blocking the window's
        // reads. Foreign keys are off by default in SQLite and must be asked
        // for on every connection, or the cascades in the schema do nothing.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        migrations().to_latest(&mut conn)?;
        Ok(Self { conn })
    }

    /// The schema version currently applied.
    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    // ---- colours ----

    /// Save a colour and return its identifier.
    pub fn add_colour(&self, colour: &NewColour) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let (r, g, b) = colour.color.to_rgb();
        self.conn.execute(
            "INSERT INTO colours (id, r, g, b, name, source_space, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, r, g, b, colour.name, colour.source_space, now()],
        )?;
        Ok(id)
    }

    /// Fetch one colour.
    pub fn colour(&self, id: &str) -> Result<Option<StoredColour>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, r, g, b, name, source_space, created_at
                 FROM colours WHERE id = ?1",
                params![id],
                colour_from_row,
            )
            .optional()?)
    }

    /// Every colour, newest first.
    pub fn colours(&self) -> Result<Vec<StoredColour>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, r, g, b, name, source_space, created_at
             FROM colours ORDER BY created_at DESC, id",
        )?;
        let rows = stmt.query_map([], colour_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Colours whose name contains `needle`, case-insensitively.
    pub fn search_colours(&self, needle: &str) -> Result<Vec<StoredColour>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, r, g, b, name, source_space, created_at
             FROM colours
             WHERE name LIKE '%' || ?1 || '%' COLLATE NOCASE
             ORDER BY created_at DESC, id",
        )?;
        let rows = stmt.query_map(params![needle], colour_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Rename a colour.
    pub fn rename_colour(&self, id: &str, name: Option<&str>) -> Result<()> {
        let touched = self.conn.execute(
            "UPDATE colours SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        expect_one(touched, "colour", id)
    }

    /// Delete a colour. Its palette slots and tag links go with it.
    pub fn delete_colour(&self, id: &str) -> Result<()> {
        let touched = self
            .conn
            .execute("DELETE FROM colours WHERE id = ?1", params![id])?;
        expect_one(touched, "colour", id)
    }

    // ---- palettes ----

    /// Create a palette from colours already in the library.
    pub fn create_palette(&self, name: &str, colour_ids: &[String]) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let stamp = now();
        self.conn.execute(
            "INSERT INTO palettes (id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![id, name, stamp],
        )?;
        self.set_palette_colours(&id, colour_ids)?;
        Ok(id)
    }

    /// Replace a palette's membership, in order.
    pub fn set_palette_colours(&self, palette_id: &str, colour_ids: &[String]) -> Result<()> {
        // One transaction so a failure part-way cannot leave a half-written
        // palette behind. `unchecked_transaction` is rusqlite's supported way
        // to do this from `&self`; the borrow checker cannot prove there is no
        // outer transaction, and here there never is.
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM palette_colours WHERE palette_id = ?1",
            params![palette_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO palette_colours (palette_id, colour_id, position)
                 VALUES (?1, ?2, ?3)",
            )?;
            for (position, colour_id) in colour_ids.iter().enumerate() {
                stmt.execute(params![palette_id, colour_id, position as i64])?;
            }
        }
        tx.execute(
            "UPDATE palettes SET updated_at = ?2 WHERE id = ?1",
            params![palette_id, now()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Fetch one palette with its colours in order.
    pub fn palette(&self, id: &str) -> Result<Option<Palette>> {
        let head = self
            .conn
            .query_row(
                "SELECT id, name, created_at, updated_at FROM palettes WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((id, name, created_at, updated_at)) = head else {
            return Ok(None);
        };

        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.r, c.g, c.b, c.name, c.source_space, c.created_at
             FROM palette_colours pc
             JOIN colours c ON c.id = pc.colour_id
             WHERE pc.palette_id = ?1
             ORDER BY pc.position",
        )?;
        let colours = stmt
            .query_map(params![&id], colour_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Some(Palette {
            id,
            name,
            colours,
            created_at: parse_time(&created_at),
            updated_at: parse_time(&updated_at),
        }))
    }

    /// Every palette, newest first.
    pub fn palettes(&self) -> Result<Vec<Palette>> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM palettes ORDER BY created_at DESC, id")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        ids.iter()
            .map(|id| {
                self.palette(id)?.ok_or_else(|| Error::NotFound {
                    kind: "palette",
                    id: id.clone(),
                })
            })
            .collect()
    }

    /// Rename a palette.
    pub fn rename_palette(&self, id: &str, name: &str) -> Result<()> {
        let touched = self.conn.execute(
            "UPDATE palettes SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, name, now()],
        )?;
        expect_one(touched, "palette", id)
    }

    /// Delete a palette. Member colours stay in the library.
    pub fn delete_palette(&self, id: &str) -> Result<()> {
        let touched = self
            .conn
            .execute("DELETE FROM palettes WHERE id = ?1", params![id])?;
        expect_one(touched, "palette", id)
    }

    // ---- pick history ----

    /// Record a pick. Never stores image data.
    pub fn record_pick(
        &self,
        color: Color,
        source_space: Option<&str>,
        source_app: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let (r, g, b) = color.to_rgb();
        self.conn.execute(
            "INSERT INTO picks (id, r, g, b, source_space, source_app, picked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, r, g, b, source_space, source_app, now()],
        )?;
        Ok(id)
    }

    /// The most recent picks, newest first.
    pub fn recent_picks(&self, limit: usize) -> Result<Vec<Pick>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, r, g, b, source_space, source_app, picked_at
             FROM picks ORDER BY picked_at DESC, id LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(Pick {
                id: row.get(0)?,
                color: Color::new(row.get(1)?, row.get(2)?, row.get(3)?),
                source_space: row.get(4)?,
                source_app: row.get(5)?,
                picked_at: parse_time(&row.get::<_, String>(6)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Drop all but the newest `keep` picks.
    pub fn trim_picks(&self, keep: usize) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM picks WHERE id NOT IN (
                 SELECT id FROM picks ORDER BY picked_at DESC, id LIMIT ?1
             )",
            params![keep as i64],
        )?)
    }

    // ---- tags ----

    /// Find or create a tag, returning its id.
    pub fn ensure_tag(&self, name: &str) -> Result<String> {
        if let Some(id) = self
            .conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1",
                params![name],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1, ?2)",
            params![id, name],
        )?;
        Ok(id)
    }

    /// Attach a tag to a colour. Doing it twice is harmless.
    pub fn tag_colour(&self, colour_id: &str, tag_name: &str) -> Result<()> {
        let tag_id = self.ensure_tag(tag_name)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO colour_tags (colour_id, tag_id) VALUES (?1, ?2)",
            params![colour_id, tag_id],
        )?;
        Ok(())
    }

    /// Remove a tag from a colour.
    pub fn untag_colour(&self, colour_id: &str, tag_name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM colour_tags
             WHERE colour_id = ?1
               AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            params![colour_id, tag_name],
        )?;
        Ok(())
    }

    /// Tags on a colour, alphabetically.
    pub fn tags_for(&self, colour_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM colour_tags ct
             JOIN tags t ON t.id = ct.tag_id
             WHERE ct.colour_id = ?1 ORDER BY t.name",
        )?;
        let rows = stmt.query_map(params![colour_id], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Colours carrying a tag, newest first.
    pub fn colours_tagged(&self, tag_name: &str) -> Result<Vec<StoredColour>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.r, c.g, c.b, c.name, c.source_space, c.created_at
             FROM colour_tags ct
             JOIN colours c ON c.id = ct.colour_id
             JOIN tags t    ON t.id = ct.tag_id
             WHERE t.name = ?1
             ORDER BY c.created_at DESC, c.id",
        )?;
        let rows = stmt.query_map(params![tag_name], colour_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn colour_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredColour> {
    Ok(StoredColour {
        id: row.get(0)?,
        color: Color::new(row.get(1)?, row.get(2)?, row.get(3)?),
        name: row.get(4)?,
        source_space: row.get(5)?,
        created_at: parse_time(&row.get::<_, String>(6)?),
    })
}

fn expect_one(touched: usize, kind: &'static str, id: &str) -> Result<()> {
    if touched == 0 {
        Err(Error::NotFound {
            kind,
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC3339 formatting of a valid timestamp cannot fail")
}

fn parse_time(text: &str) -> OffsetDateTime {
    OffsetDateTime::parse(text, &Rfc3339).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

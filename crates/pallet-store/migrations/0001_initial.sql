-- Pallet's initial schema.
--
-- Identifiers are UUID text rather than rowids so that rows keep a stable
-- identity across exports, imports and any future sync.
--
-- Colours store the sRGB triple as three integers rather than a hex string:
-- it is the form the picker produces and the form every conversion wants, and
-- it lets SQLite range-check each channel. `source_space` records the display
-- profile a colour was captured from; NULL means plain sRGB.
--
-- The channel CHECK constraints are defence in depth against a future writer
-- (an importer, a sync client, a hand-edited row) rather than against Rust:
-- pallet_color::Color holds u8, so out-of-range values are unrepresentable in
-- the typed API and cannot be produced through Store.

CREATE TABLE colours (
    id            TEXT    PRIMARY KEY,
    r             INTEGER NOT NULL CHECK (r BETWEEN 0 AND 255),
    g             INTEGER NOT NULL CHECK (g BETWEEN 0 AND 255),
    b             INTEGER NOT NULL CHECK (b BETWEEN 0 AND 255),
    name          TEXT,
    source_space  TEXT,
    created_at    TEXT    NOT NULL
) STRICT;

CREATE INDEX colours_created_at ON colours (created_at DESC);
CREATE INDEX colours_name       ON colours (name);

CREATE TABLE palettes (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
) STRICT;

CREATE INDEX palettes_created_at ON palettes (created_at DESC);

-- Ordered membership. The primary key pins one colour per slot, and the
-- cascade means deleting a palette never strands its rows.
CREATE TABLE palette_colours (
    palette_id  TEXT    NOT NULL REFERENCES palettes (id) ON DELETE CASCADE,
    colour_id   TEXT    NOT NULL REFERENCES colours  (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    PRIMARY KEY (palette_id, position)
) STRICT;

CREATE INDEX palette_colours_colour ON palette_colours (colour_id);

-- Pick history. Deliberately holds no image data: Pallet never writes a
-- captured frame to disk. `source_app` is optional and may be NULL.
CREATE TABLE picks (
    id            TEXT    PRIMARY KEY,
    r             INTEGER NOT NULL CHECK (r BETWEEN 0 AND 255),
    g             INTEGER NOT NULL CHECK (g BETWEEN 0 AND 255),
    b             INTEGER NOT NULL CHECK (b BETWEEN 0 AND 255),
    source_space  TEXT,
    source_app    TEXT,
    picked_at     TEXT    NOT NULL
) STRICT;

CREATE INDEX picks_picked_at ON picks (picked_at DESC);

CREATE TABLE tags (
    id    TEXT PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE colour_tags (
    colour_id  TEXT NOT NULL REFERENCES colours (id) ON DELETE CASCADE,
    tag_id     TEXT NOT NULL REFERENCES tags    (id) ON DELETE CASCADE,
    PRIMARY KEY (colour_id, tag_id)
) STRICT;

CREATE INDEX colour_tags_tag ON colour_tags (tag_id);

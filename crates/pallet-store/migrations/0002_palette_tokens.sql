-- Token names for a palette's members.
--
-- A token is two levels — a group and a name within it, `brand/primary` — and
-- both live on the membership row rather than on the colour. The same colour
-- is `brand/primary` in one palette and `accent` in another, so a column on
-- `colours` would make the last palette to name it overwrite every other.
-- `colours.name` stays what it is: what the colour is called in the library.
--
-- Both are nullable, and NULL is not the same as ''. NULL means the user never
-- named this member, and the export falls back to a nearest-match colour name
-- as it always has; an empty string would be a name they chose to be blank,
-- which is not a thing the UI can produce and not a thing the exporters can
-- use. `pallet_export::Swatch::group` trims and discards blanks for the same
-- reason on the way out.
--
-- No index: tokens are read with the palette they belong to and never searched
-- across palettes.

ALTER TABLE palette_colours ADD COLUMN token_group TEXT;
ALTER TABLE palette_colours ADD COLUMN token_name  TEXT;

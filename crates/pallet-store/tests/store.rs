//! Library CRUD, ordering, cascades and migrations.

use pallet_color::Color;
use pallet_store::{NewColour, Store};

fn store() -> Store {
    Store::open_in_memory().expect("in-memory library")
}

fn colour(hex: &str) -> NewColour {
    NewColour::new(Color::parse_hex(hex).unwrap())
}

#[test]
fn migrations_apply_and_report_a_version() {
    let s = store();
    assert_eq!(s.schema_version().unwrap(), 1);
}

#[test]
fn migrations_are_idempotent_across_reopens() {
    let dir = std::env::temp_dir().join(format!("pallet-db-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pallet.db");

    let id = {
        let s = Store::open(&path).unwrap();
        s.add_colour(&colour("#A5236E").named("Disco")).unwrap()
    };

    // Reopening must migrate cleanly and preserve everything.
    let s = Store::open(&path).unwrap();
    assert_eq!(s.schema_version().unwrap(), 1);
    assert_eq!(
        s.colour(&id).unwrap().unwrap().name.as_deref(),
        Some("Disco")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn colours_round_trip() {
    let s = store();
    let id = s.add_colour(&colour("#A5236E").named("Disco")).unwrap();

    let got = s.colour(&id).unwrap().expect("just inserted");
    assert_eq!(got.color, Color::parse_hex("#A5236E").unwrap());
    assert_eq!(got.name.as_deref(), Some("Disco"));
    assert_eq!(got.source_space, None);
}

#[test]
fn a_capture_profile_survives_the_round_trip() {
    // The wide-gamut provenance that makes picks reproducible.
    let s = store();
    let mut c = colour("#FF0000");
    c.source_space = Some("display-p3".into());
    let id = s.add_colour(&c).unwrap();
    assert_eq!(
        s.colour(&id).unwrap().unwrap().source_space.as_deref(),
        Some("display-p3")
    );
}

#[test]
fn missing_rows_are_none_not_errors() {
    assert!(store().colour("no-such-id").unwrap().is_none());
}

#[test]
fn renaming_and_deleting_a_missing_colour_is_an_error() {
    let s = store();
    assert!(s.rename_colour("nope", Some("x")).is_err());
    assert!(s.delete_colour("nope").is_err());
}

#[test]
fn search_is_case_insensitive_and_partial() {
    let s = store();
    s.add_colour(&colour("#289788").named("Jungle Green"))
        .unwrap();
    s.add_colour(&colour("#E0BC67").named("Rob Roy")).unwrap();

    assert_eq!(s.search_colours("jungle").unwrap().len(), 1);
    assert_eq!(s.search_colours("ROY").unwrap().len(), 1);
    assert_eq!(s.search_colours("een").unwrap().len(), 1);
    assert_eq!(s.search_colours("mauve").unwrap().len(), 0);
}

#[test]
fn palettes_keep_their_colour_order() {
    let s = store();
    let hexes = ["#F9A08A", "#F0657A", "#B76C7E", "#6E5A78", "#3B5F86"];
    let ids: Vec<_> = hexes
        .iter()
        .map(|h| s.add_colour(&colour(h)).unwrap())
        .collect();

    let pid = s.create_palette("Winter Sunset", &ids).unwrap();
    let p = s.palette(&pid).unwrap().expect("just created");

    assert_eq!(p.name, "Winter Sunset");
    let got: Vec<_> = p.colours.iter().map(|c| c.color.to_hex()).collect();
    assert_eq!(got, hexes);
}

#[test]
fn reordering_a_palette_replaces_membership_wholesale() {
    let s = store();
    let ids: Vec<_> = ["#A5236E", "#289788", "#E0BC67"]
        .iter()
        .map(|h| s.add_colour(&colour(h)).unwrap())
        .collect();
    let pid = s.create_palette("Three", &ids).unwrap();

    let reversed: Vec<_> = ids.iter().rev().cloned().collect();
    s.set_palette_colours(&pid, &reversed).unwrap();

    let p = s.palette(&pid).unwrap().unwrap();
    assert_eq!(p.colours.len(), 3, "no duplicate slots left behind");
    assert_eq!(p.colours[0].color.to_hex(), "#E0BC67");
    assert_eq!(p.colours[2].color.to_hex(), "#A5236E");
}

#[test]
fn deleting_a_colour_removes_it_from_palettes() {
    // Relies on the foreign-key cascade, which needs PRAGMA foreign_keys=ON.
    let s = store();
    let ids: Vec<_> = ["#A5236E", "#289788"]
        .iter()
        .map(|h| s.add_colour(&colour(h)).unwrap())
        .collect();
    let pid = s.create_palette("Pair", &ids).unwrap();

    s.delete_colour(&ids[0]).unwrap();

    let p = s.palette(&pid).unwrap().unwrap();
    assert_eq!(p.colours.len(), 1);
    assert_eq!(p.colours[0].color.to_hex(), "#289788");
}

#[test]
fn deleting_a_palette_leaves_its_colours_in_the_library() {
    let s = store();
    let ids: Vec<_> = ["#A5236E", "#289788"]
        .iter()
        .map(|h| s.add_colour(&colour(h)).unwrap())
        .collect();
    let pid = s.create_palette("Pair", &ids).unwrap();

    s.delete_palette(&pid).unwrap();

    assert!(s.palette(&pid).unwrap().is_none());
    assert_eq!(s.colours().unwrap().len(), 2);
}

#[test]
fn picks_come_back_newest_first_and_trim_to_a_cap() {
    let s = store();
    for hex in ["#111111", "#222222", "#333333", "#444444"] {
        s.record_pick(Color::parse_hex(hex).unwrap(), None, Some("firefox"))
            .unwrap();
    }

    let recent = s.recent_picks(10).unwrap();
    assert_eq!(recent.len(), 4);
    assert_eq!(recent[0].color.to_hex(), "#444444");
    assert_eq!(recent[0].source_app.as_deref(), Some("firefox"));

    let removed = s.trim_picks(2).unwrap();
    assert_eq!(removed, 2);
    assert_eq!(s.recent_picks(10).unwrap().len(), 2);
}

#[test]
fn tags_attach_detach_and_are_idempotent() {
    let s = store();
    let id = s.add_colour(&colour("#A5236E")).unwrap();

    s.tag_colour(&id, "brand").unwrap();
    s.tag_colour(&id, "brand").unwrap(); // twice must not duplicate
    s.tag_colour(&id, "warm").unwrap();

    assert_eq!(s.tags_for(&id).unwrap(), vec!["brand", "warm"]);
    assert_eq!(s.colours_tagged("brand").unwrap().len(), 1);

    s.untag_colour(&id, "brand").unwrap();
    assert_eq!(s.tags_for(&id).unwrap(), vec!["warm"]);
    assert_eq!(s.colours_tagged("brand").unwrap().len(), 0);
}

#[test]
fn deleting_a_colour_removes_its_tag_links() {
    let s = store();
    let id = s.add_colour(&colour("#A5236E")).unwrap();
    s.tag_colour(&id, "brand").unwrap();

    s.delete_colour(&id).unwrap();
    assert_eq!(s.colours_tagged("brand").unwrap().len(), 0);
}

#[test]
fn seeding_populates_the_prototypes_library_exactly_once() {
    let s = store();

    assert!(
        pallet_store::seed::seed_if_empty(&s).unwrap(),
        "first run seeds"
    );
    let palettes = s.palettes().unwrap();
    assert_eq!(palettes.len(), 4);
    assert!(palettes.iter().any(|p| p.name == "Winter Sunset"));
    assert!(palettes.iter().all(|p| p.colours.len() == 5));

    let named = s.search_colours("Freckled Bluewood").unwrap();
    assert_eq!(named.len(), 1, "authored names survive seeding");

    assert!(
        !pallet_store::seed::seed_if_empty(&s).unwrap(),
        "second run is a no-op"
    );
    assert_eq!(s.palettes().unwrap().len(), 4);
}

#[test]
fn seeded_palettes_carry_the_prototypes_dates_and_order() {
    // The Palettes screen shows "5 · 2019" and numbers Winter Sunset 01. Both
    // come from created_at, so a fresh library only matches the design if the
    // seed carries the original years and a stable order within a year.
    let s = store();
    pallet_store::seed::seed_if_empty(&s).unwrap();

    let mut palettes = s.palettes().unwrap();
    palettes.reverse(); // the library returns newest first

    let shown: Vec<(String, i32, usize)> = palettes
        .iter()
        .map(|p| (p.name.clone(), p.created_at.year(), p.colours.len()))
        .collect();

    assert_eq!(
        shown,
        vec![
            ("Winter Sunset".to_string(), 2019, 5),
            ("Bussel".to_string(), 2019, 5),
            ("Glasshouse".to_string(), 2020, 5),
            ("Rob Roy".to_string(), 2021, 5),
        ]
    );
}

#[test]
fn seeded_colours_keep_the_prototypes_order() {
    let s = store();
    pallet_store::seed::seed_if_empty(&s).unwrap();

    let mut named: Vec<String> = s
        .colours()
        .unwrap()
        .into_iter()
        .filter_map(|c| c.name)
        .collect();
    named.reverse(); // insertion order, as the Colours screen shows it

    assert_eq!(named.first().map(String::as_str), Some("Dim Red"));
    assert_eq!(named.last().map(String::as_str), Some("Rob Roy"));
    assert_eq!(named.len(), 9);
}

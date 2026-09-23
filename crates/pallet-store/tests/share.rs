//! Round-tripping a library between installations, and the merge rules.

use pallet_color::Color;
use pallet_store::{Bundle, Member, NewColour, Store, Token};

fn store() -> Store {
    Store::open_in_memory().expect("in-memory library")
}

fn colour(hex: &str) -> NewColour {
    NewColour::new(Color::parse_hex(hex).unwrap())
}

/// A library with a tagged colour, an untagged one, and a palette naming both.
fn populated() -> Store {
    let s = store();
    let warm = s.add_colour(&colour("#C87D5B").named("Clay")).unwrap();
    let cool = s.add_colour(&colour("#358FDE").named("Signal")).unwrap();
    s.tag_colour(&warm, "muted").unwrap();
    s.tag_colour(&warm, "brand").unwrap();
    s.create_palette_with(
        "Studio",
        &[
            Member {
                colour_id: warm,
                token: Token {
                    group: Some("brand".into()),
                    name: Some("primary".into()),
                },
            },
            Member::new(cool),
        ],
    )
    .unwrap();
    s
}

#[test]
fn a_bundle_round_trips_through_json_into_an_empty_library() {
    let source = populated();
    let json = source.export_bundle().unwrap().to_json().unwrap();

    let target = store();
    let merged = target
        .import_bundle(&Bundle::from_json(&json).unwrap())
        .unwrap();

    assert_eq!(merged.colours_added, 2);
    assert_eq!(merged.palettes_added, 1);
    assert_eq!(merged.members_dropped, 0);

    let palettes = target.palettes().unwrap();
    assert_eq!(palettes.len(), 1);
    assert_eq!(palettes[0].name, "Studio");

    // Order, tokens and names all survive the trip.
    let (colour, token) = palettes[0].member(0).unwrap();
    assert_eq!(colour.color.to_hex(), "#C87D5B");
    assert_eq!(colour.name.as_deref(), Some("Clay"));
    assert_eq!(token.group.as_deref(), Some("brand"));
    assert_eq!(token.name.as_deref(), Some("primary"));
    assert!(palettes[0].member(1).unwrap().1.is_empty());

    assert_eq!(
        target.tags_for(&palettes[0].colours[0].id).unwrap(),
        vec!["brand".to_string(), "muted".to_string()]
    );
}

#[test]
fn ids_survive_the_trip_so_the_two_libraries_agree_on_identity() {
    let source = populated();
    let bundle = source.export_bundle().unwrap();

    let target = store();
    target.import_bundle(&bundle).unwrap();

    let mut here: Vec<String> = source
        .colours()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    let mut there: Vec<String> = target
        .colours()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    here.sort();
    there.sort();
    assert_eq!(here, there, "identity is what makes the merge a merge");
}

#[test]
fn importing_the_same_bundle_twice_changes_nothing_the_second_time() {
    let source = populated();
    let bundle = source.export_bundle().unwrap();

    let target = store();
    let first = target.import_bundle(&bundle).unwrap();
    let second = target.import_bundle(&bundle).unwrap();

    assert_eq!(first.colours_added, 2);
    assert_eq!(second.colours_added, 0);
    assert_eq!(second.colours_known, 2);
    assert_eq!(second.palettes_added, 0);
    assert_eq!(second.palettes_known, 1);
    assert!(second.is_empty());

    assert_eq!(target.colours().unwrap().len(), 2);
    assert_eq!(target.palettes().unwrap().len(), 1);
}

#[test]
fn a_merge_never_overwrites_what_the_recipient_already_had() {
    let source = populated();
    let bundle = source.export_bundle().unwrap();

    // The recipient has the same colour, renamed, and the same palette with a
    // name of their own.
    let target = store();
    target.import_bundle(&bundle).unwrap();
    let id = target.colours().unwrap()[0].id.clone();
    target.rename_colour(&id, Some("Mine")).unwrap();
    let palette_id = target.palettes().unwrap()[0].id.clone();
    target.rename_palette(&palette_id, "Mine too").unwrap();

    target.import_bundle(&bundle).unwrap();

    assert_eq!(
        target.colour(&id).unwrap().unwrap().name.as_deref(),
        Some("Mine"),
        "a re-import must not revert a local rename"
    );
    assert_eq!(target.palettes().unwrap()[0].name, "Mine too");
}

#[test]
fn a_merge_adds_to_a_library_that_already_has_its_own_work() {
    let target = store();
    let own = target.add_colour(&colour("#111111").named("Own")).unwrap();
    target.create_palette("Own palette", &[own]).unwrap();

    let merged = target
        .import_bundle(&populated().export_bundle().unwrap())
        .unwrap();

    assert_eq!(merged.colours_added, 2);
    assert_eq!(merged.palettes_added, 1);
    assert_eq!(target.colours().unwrap().len(), 3);
    assert_eq!(target.palettes().unwrap().len(), 2);
}

#[test]
fn pick_history_is_not_shared() {
    let source = populated();
    source
        .record_pick(Color::parse_hex("#A5236E").unwrap(), None, Some("Firefox"))
        .unwrap();

    let json = source.export_bundle().unwrap().to_json().unwrap();
    assert!(
        !json.contains("Firefox") && !json.contains("A5236E"),
        "a bundle must carry no trace of what was on someone's screen"
    );

    let target = store();
    target
        .import_bundle(&Bundle::from_json(&json).unwrap())
        .unwrap();
    assert!(target.recent_picks(10).unwrap().is_empty());
}

#[test]
fn a_bundle_from_a_newer_pallet_is_refused_by_name() {
    let mut bundle = populated().export_bundle().unwrap();
    bundle.format = 99;

    let err = store().import_bundle(&bundle).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("newer Pallet"),
        "the message has to say to update, not that the file is broken: {text}"
    );

    // And the same refusal arrives when it comes in as text.
    let json = serde_json::to_string(&bundle).unwrap();
    assert!(Bundle::from_json(&json).is_err());
}

#[test]
fn a_slot_naming_a_colour_nobody_has_is_dropped_rather_than_failing() {
    let source = populated();
    let mut bundle = source.export_bundle().unwrap();
    // Hand-edit the document the way a person with a text editor might.
    bundle.palettes[0].members[0].colour = "not-a-colour-anyone-has".into();

    let target = store();
    let merged = target.import_bundle(&bundle).unwrap();

    assert_eq!(merged.members_dropped, 1);
    let palettes = target.palettes().unwrap();
    assert_eq!(palettes[0].colours.len(), 1);
    // Positions stay dense, so the surviving slot is at 0 rather than 1.
    assert_eq!(palettes[0].colours[0].color.to_hex(), "#358FDE");
}

#[test]
fn an_unreadable_colour_aborts_the_whole_import() {
    let mut bundle = populated().export_bundle().unwrap();
    bundle.colours[1].hex = "lilac".into();

    let target = store();
    assert!(target.import_bundle(&bundle).is_err());
    assert!(
        target.colours().unwrap().is_empty(),
        "a failed import must leave nothing behind, not half a library"
    );
    assert!(target.palettes().unwrap().is_empty());
}

#[test]
fn nonsense_is_rejected_as_not_being_a_library() {
    assert!(Bundle::from_json("{\"nope\":1}").is_err());
    assert!(Bundle::from_json("not json at all").is_err());
}

#[test]
fn colours_are_written_as_hex_so_the_file_can_be_read_by_a_person() {
    let json = populated().export_bundle().unwrap().to_json().unwrap();
    assert!(json.contains("#C87D5B"));
    assert!(json.contains("\"format\": 1"));
}

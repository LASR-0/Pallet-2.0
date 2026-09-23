//! Every format, with particular attention to the ones that read back.

use pallet_color::Color;
use pallet_export::{Format, Palette, Swatch, read, write};

fn sample() -> Palette {
    Palette::new(
        "Winter Sunset",
        vec![
            Swatch::named(Color::parse_hex("#F9A08A").unwrap(), "Vivid Tangerine"),
            Swatch::named(Color::parse_hex("#F0657A").unwrap(), "Froly"),
            Swatch::named(Color::parse_hex("#B76C7E").unwrap(), "Turkish Rose"),
            Swatch::named(Color::parse_hex("#6E5A78").unwrap(), "Rum"),
            Swatch::named(Color::parse_hex("#3B5F86").unwrap(), "Chambray"),
        ],
    )
}

#[test]
fn ase_round_trips_every_colour_exactly() {
    // The milestone's acceptance test. ASE stores components as floats, so the
    // question is whether an 8-bit channel survives the trip through f32.
    let original = sample();
    let bytes = write(&original, Format::Ase).unwrap();
    let back = read(&bytes, Format::Ase).unwrap();

    assert_eq!(
        back.name, original.name,
        "the group carries the palette name"
    );
    assert_eq!(back.swatches.len(), original.swatches.len());
    for (a, b) in original.swatches.iter().zip(&back.swatches) {
        assert_eq!(a.color, b.color, "{} changed", a.label());
        assert_eq!(a.name, b.name);
    }
}

#[test]
fn ase_round_trips_every_possible_channel_value() {
    // Not just the sample: every one of the 256 values a channel can hold.
    let swatches = (0..=255u8)
        .map(|v| Swatch::new(Color::new(v, 255 - v, v.wrapping_mul(7))))
        .collect();
    let original = Palette::new("Exhaustive", swatches);

    let back = read(&write(&original, Format::Ase).unwrap(), Format::Ase).unwrap();
    for (a, b) in original.swatches.iter().zip(&back.swatches) {
        assert_eq!(a.color, b.color);
    }
}

#[test]
fn ase_starts_with_the_signature_and_a_sane_block_count() {
    let bytes = write(&sample(), Format::Ase).unwrap();
    assert_eq!(&bytes[0..4], b"ASEF");
    // version 1.0
    assert_eq!(&bytes[4..8], &[0, 1, 0, 0]);
    // five colours, plus the group open and close
    assert_eq!(u32::from_be_bytes(bytes[8..12].try_into().unwrap()), 7);
}

#[test]
fn ase_names_survive_beyond_ascii() {
    // Names are UTF-16 in the format; anything outside the BMP is the case a
    // naive implementation truncates.
    let original = Palette::new(
        "Paletă",
        vec![
            Swatch::named(Color::new(1, 2, 3), "Rosé"),
            Swatch::named(Color::new(4, 5, 6), "日本語"),
            Swatch::named(Color::new(7, 8, 9), "emoji 🎨"),
        ],
    );
    let back = read(&write(&original, Format::Ase).unwrap(), Format::Ase).unwrap();
    assert_eq!(back.name, "Paletă");
    let names: Vec<_> = back.swatches.iter().map(|s| s.name.clone()).collect();
    assert_eq!(
        names,
        vec![
            Some("Rosé".into()),
            Some("日本語".into()),
            Some("emoji 🎨".into())
        ]
    );
}

#[test]
fn a_truncated_ase_is_an_error_not_a_panic() {
    let bytes = write(&sample(), Format::Ase).unwrap();
    for cut in [4, 10, 20, bytes.len() - 1] {
        assert!(read(&bytes[..cut], Format::Ase).is_err(), "cut at {cut}");
    }
}

#[test]
fn something_that_is_not_ase_is_refused() {
    assert!(read(b"not a palette at all", Format::Ase).is_err());
}

#[test]
fn json_round_trips_exactly() {
    let original = sample();
    let back = read(&write(&original, Format::Json).unwrap(), Format::Json).unwrap();
    assert_eq!(back, original);
}

#[test]
fn json_is_readable_hex_not_channel_objects() {
    let text = String::from_utf8(write(&sample(), Format::Json).unwrap()).unwrap();
    assert!(text.contains("\"#F9A08A\""), "{text}");
    assert!(
        !text.contains("\"r\""),
        "channels should not be spelled out"
    );
}

#[test]
fn gpl_round_trips_colours_and_names() {
    let original = sample();
    let back = read(&write(&original, Format::Gpl).unwrap(), Format::Gpl).unwrap();
    assert_eq!(back.name, original.name);
    assert_eq!(back.swatches, original.swatches);
}

#[test]
fn gpl_reads_a_file_written_by_gimp() {
    // Tab-separated, extra blank lines and comments, names containing spaces.
    let text = "GIMP Palette\nName: Example\nColumns: 4\n#\n# a comment\n\n255   0   0\tRed\n  0 128   0\tDark Green\n 10  20  30\n";
    let palette = read(text.as_bytes(), Format::Gpl).unwrap();
    assert_eq!(palette.name, "Example");
    assert_eq!(palette.swatches.len(), 3);
    assert_eq!(palette.swatches[0].color, Color::new(255, 0, 0));
    assert_eq!(palette.swatches[1].name.as_deref(), Some("Dark Green"));
    assert_eq!(
        palette.swatches[2].name, None,
        "an unnamed row stays unnamed"
    );
}

#[test]
fn a_file_that_is_not_gpl_is_refused() {
    assert!(read(b"#ff0000\n#00ff00", Format::Gpl).is_err());
}

#[test]
fn css_and_scss_emit_usable_identifiers() {
    let css = String::from_utf8(write(&sample(), Format::CssVars).unwrap()).unwrap();
    assert!(css.contains(":root {"));
    assert!(css.contains("--vivid-tangerine: #F9A08A;"), "{css}");

    let scss = String::from_utf8(write(&sample(), Format::Scss).unwrap()).unwrap();
    assert!(scss.contains("$turkish-rose: #B76C7E;"), "{scss}");
}

#[test]
fn tailwind_nests_under_the_palette_name() {
    let js = String::from_utf8(write(&sample(), Format::Tailwind).unwrap()).unwrap();
    assert!(js.contains("\"winter-sunset\""), "{js}");
    assert!(js.contains("\"froly\": \"#F0657A\""), "{js}");
}

#[test]
fn unnamed_swatches_are_identified_by_position_not_by_their_hex() {
    // "--ff0000: #FF0000" restates the value and says nothing; worse, the SCSS
    // equivalent "$6e5a78" is not a legal identifier at all.
    let palette = Palette::new(
        "Anon",
        vec![
            Swatch::new(Color::new(255, 0, 0)),
            Swatch::new(Color::parse_hex("#6E5A78").unwrap()),
        ],
    );

    let css = String::from_utf8(write(&palette, Format::CssVars).unwrap()).unwrap();
    assert!(css.contains("--colour-1: #FF0000;"), "{css}");
    assert!(css.contains("--colour-2: #6E5A78;"), "{css}");

    let scss = String::from_utf8(write(&palette, Format::Scss).unwrap()).unwrap();
    assert!(scss.contains("$colour-2: #6E5A78;"), "{scss}");
}

#[test]
fn no_emitted_identifier_starts_with_a_digit() {
    // SCSS refuses those outright, and CSS custom properties that do are
    // legal but confusing. Hex-derived names hit this constantly.
    let palette = Palette::new(
        "Digits",
        vec![
            Swatch::named(Color::new(1, 2, 3), "100 Mph"),
            Swatch::named(Color::new(4, 5, 6), "1975 Earth Red"),
            Swatch::new(Color::parse_hex("#3B5F86").unwrap()),
        ],
    );

    let scss = String::from_utf8(write(&palette, Format::Scss).unwrap()).unwrap();
    for line in scss.lines().filter(|l| l.starts_with('$')) {
        let name = &line[1..line.find(':').unwrap()];
        assert!(
            !name.starts_with(|c: char| c.is_ascii_digit()),
            "`${name}` is not a legal SCSS identifier"
        );
    }
}

#[test]
fn gpl_omits_a_name_it_does_not_have() {
    // Writing the hex as the name restates the numbers on the same line.
    let palette = Palette::new("Anon", vec![Swatch::new(Color::new(255, 0, 0))]);
    let gpl = String::from_utf8(write(&palette, Format::Gpl).unwrap()).unwrap();
    assert!(gpl.contains("255   0   0\n"), "{gpl}");
    assert!(!gpl.contains("#FF0000"), "{gpl}");

    // And it still round-trips as unnamed.
    let back = read(gpl.as_bytes(), Format::Gpl).unwrap();
    assert_eq!(back.swatches[0].name, None);
}

#[test]
fn the_png_sheet_is_a_real_png_of_the_expected_size() {
    let bytes = write(&sample(), Format::Png).unwrap();
    assert_eq!(&bytes[1..4], b"PNG");
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!(image.width(), 160 * 5);
    assert_eq!(image.height(), 200);
}

#[test]
fn an_empty_palette_still_produces_a_valid_file_in_every_format() {
    // Exporting nothing is odd but must not panic or emit a broken file.
    let empty = Palette::new("Empty", Vec::new());
    for format in Format::ALL {
        let bytes = write(&empty, format).unwrap_or_else(|e| panic!("{format:?}: {e}"));
        assert!(!bytes.is_empty(), "{format:?} wrote nothing");
        if format.readable() {
            let back = read(&bytes, format).unwrap_or_else(|e| panic!("{format:?}: {e}"));
            assert!(back.swatches.is_empty());
        }
    }
}

#[test]
fn format_identifiers_and_extensions_are_distinct() {
    let mut ids: Vec<_> = Format::ALL.iter().map(|f| f.id()).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "two formats share an id");

    for format in Format::ALL {
        assert!(Format::parse(format.id()) == Some(format));
    }
    assert!(Format::parse("bmp").is_none());
}

// --- token sets -------------------------------------------------------------

/// A palette whose colours carry two-level tokens, with the groups
/// deliberately out of order: `grouped` buckets rather than taking runs, and
/// a format that nests cannot write the same key twice.
fn tokened() -> Palette {
    Palette::new(
        "Winter Sunset",
        vec![
            Swatch::token(Color::parse_hex("#F9A08A").unwrap(), "brand", "primary"),
            Swatch::token(Color::parse_hex("#F0657A").unwrap(), "surface", "base"),
            Swatch::token(Color::parse_hex("#B76C7E").unwrap(), "brand", "secondary"),
            Swatch::named(Color::parse_hex("#6E5A78").unwrap(), "Loose End"),
        ],
    )
}

#[test]
fn groups_are_buckets_in_first_appearance_order_not_runs() {
    let palette = tokened();
    let grouped = palette.grouped();

    let keys: Vec<_> = grouped.iter().map(|(g, _)| *g).collect();
    assert_eq!(keys, vec![Some("brand"), Some("surface"), None]);

    // Both brand members land together even though a surface sits between
    // them in the palette. A nested format cannot write "brand" twice.
    let brand: Vec<_> = grouped[0].1.iter().map(|(i, _)| *i).collect();
    assert_eq!(brand, vec![0, 2]);
}

#[test]
fn a_blank_group_is_treated_as_no_group() {
    // An empty text field arrives as Some(""), which would otherwise produce
    // an SCSS map key of nothing and an unnamed ASE folder.
    let swatch = Swatch::token(Color::parse_hex("#F9A08A").unwrap(), "   ", "primary");
    assert_eq!(swatch.group(), None);
    assert_eq!(swatch.qualified_identifier(0), "primary");
}

#[test]
fn css_flattens_the_token_into_the_property_name() {
    let css = String::from_utf8(write(&tokened(), Format::CssVars).unwrap()).unwrap();
    assert!(css.contains("--brand-primary: #F9A08A;"), "{css}");
    assert!(css.contains("--brand-secondary: #B76C7E;"), "{css}");
    assert!(css.contains("--surface-base: #F0657A;"), "{css}");
    // An ungrouped colour keeps its bare name rather than gaining a prefix.
    assert!(css.contains("--loose-end: #6E5A78;"), "{css}");
}

#[test]
fn scss_writes_both_flat_variables_and_a_map_per_group() {
    let scss = String::from_utf8(write(&tokened(), Format::Scss).unwrap()).unwrap();
    assert!(scss.contains("$brand-primary: #F9A08A;"), "{scss}");
    // The map refers to the variables rather than restating the values, so
    // the two cannot drift apart.
    assert!(scss.contains("$brand: ("), "{scss}");
    assert!(scss.contains("\"primary\": $brand-primary,"), "{scss}");
    assert!(scss.contains("\"secondary\": $brand-secondary,"), "{scss}");
    // Ungrouped colours get no map, because there is no group to name it.
    assert!(!scss.contains("$ungrouped: ("), "{scss}");
}

#[test]
fn tailwind_nests_groups_so_the_class_names_come_out_right() {
    let js = String::from_utf8(write(&tokened(), Format::Tailwind).unwrap()).unwrap();
    // bg-brand-primary, not bg-winter-sunset-brand-primary.
    assert!(js.contains("\"brand\": {"), "{js}");
    assert!(js.contains("\"primary\": \"#F9A08A\","), "{js}");
    assert!(js.contains("\"surface\": {"), "{js}");
    // The leftover colour falls under the palette's own name.
    assert!(js.contains("\"winter-sunset\": {"), "{js}");
}

#[test]
fn tailwind_without_groups_is_unchanged() {
    // The pre-token shape: one object named after the palette.
    let js = String::from_utf8(write(&sample(), Format::Tailwind).unwrap()).unwrap();
    assert!(js.contains("\"winter-sunset\": {"), "{js}");
    assert!(js.contains("\"vivid-tangerine\": \"#F9A08A\","), "{js}");
}

#[test]
fn design_tokens_are_dtcg_shaped() {
    let text = String::from_utf8(write(&tokened(), Format::Tokens).unwrap()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();

    assert_eq!(value["brand"]["primary"]["$value"], "#F9A08A");
    assert_eq!(value["brand"]["primary"]["$type"], "color");
    assert_eq!(value["surface"]["base"]["$value"], "#F0657A");
    // Ungrouped tokens sit at the root rather than under a wrapper.
    assert_eq!(value["loose-end"]["$value"], "#6E5A78");
}

#[test]
fn design_tokens_are_write_only_and_do_not_collide_with_json() {
    assert!(!Format::Tokens.readable());
    // Both are JSON; a shared extension would have each overwrite the other,
    // since the filename is the palette slug plus the extension.
    assert_ne!(Format::Tokens.extension(), Format::Json.extension());
}

#[test]
fn json_round_trips_tokens() {
    let bytes = write(&tokened(), Format::Json).unwrap();
    let back = read(&bytes, Format::Json).unwrap();
    assert_eq!(back, tokened());
}

#[test]
fn json_written_before_groups_existed_still_reads() {
    // The field is `#[serde(default)]`, so a file with no `group` key at all
    // loads as ungrouped rather than failing.
    // Doubled hashes: a hex value puts `"#` inside the literal, which would
    // close an `r#"..."#` at the first colour.
    let text = r##"{"name":"Old","swatches":[{"color":"#F9A08A","name":"Vivid Tangerine"}]}"##;
    let back = read(text.as_bytes(), Format::Json).unwrap();
    assert_eq!(back.swatches[0].group(), None);
    assert_eq!(back.swatches[0].name.as_deref(), Some("Vivid Tangerine"));
}

#[test]
fn a_palette_without_groups_writes_json_exactly_as_it_used_to() {
    // `skip_serializing_if` keeps the key out entirely, so files written by
    // this version are byte-identical to ones written before tokens existed.
    let text = String::from_utf8(write(&sample(), Format::Json).unwrap()).unwrap();
    assert!(!text.contains("group"), "{text}");
}

#[test]
fn gpl_round_trips_tokens_through_its_one_name_field() {
    let bytes = write(&tokened(), Format::Gpl).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("brand / primary"), "{text}");

    let back = read(&bytes, Format::Gpl).unwrap();
    assert_eq!(back.swatches[0].group(), Some("brand"));
    assert_eq!(back.swatches[0].name.as_deref(), Some("primary"));
    // The ungrouped one stays ungrouped rather than splitting on nothing.
    assert_eq!(back.swatches[3].group(), None);
    assert_eq!(back.swatches[3].name.as_deref(), Some("Loose End"));
}

#[test]
fn gpl_keeps_palette_order_rather_than_grouped_order() {
    // GIMP draws swatches in file order, so grouping must not reshuffle the
    // picture the user built.
    let text = String::from_utf8(write(&tokened(), Format::Gpl).unwrap()).unwrap();
    let primary = text.find("brand / primary").unwrap();
    let base = text.find("surface / base").unwrap();
    let secondary = text.find("brand / secondary").unwrap();
    assert!(primary < base && base < secondary, "{text}");
}

#[test]
fn ase_writes_one_folder_per_group_and_reads_them_back() {
    let bytes = write(&tokened(), Format::Ase).unwrap();
    let back = read(&bytes, Format::Ase).unwrap();

    // Grouped swatches keep their group; the loose one stays loose.
    let brand: Vec<_> = back
        .swatches
        .iter()
        .filter(|s| s.group() == Some("brand"))
        .map(|s| s.name.clone().unwrap())
        .collect();
    assert_eq!(brand, vec!["primary", "secondary"]);
    assert_eq!(
        back.swatches.iter().filter(|s| s.group().is_none()).count(),
        1
    );
}

#[test]
fn ase_without_groups_still_carries_the_palette_name() {
    // One folder holding everything means a palette name, not a token group —
    // the shape this crate wrote before tokens existed.
    let bytes = write(&sample(), Format::Ase).unwrap();
    let back = read(&bytes, Format::Ase).unwrap();
    assert_eq!(back.name, "Winter Sunset");
    assert!(back.swatches.iter().all(|s| s.group().is_none()));
    assert_eq!(back.swatches.len(), 5);
}

#[test]
fn design_tokens_keep_palette_order_rather_than_sorting_keys() {
    // serde_json sorts map keys unless `preserve_order` is on, which would
    // silently rearrange the palette the user built. Groups come out in first
    // appearance order and members in palette order, as in every other format.
    let text = String::from_utf8(write(&tokened(), Format::Tokens).unwrap()).unwrap();
    let brand = text.find("\"brand\"").unwrap();
    let surface = text.find("\"surface\"").unwrap();
    let loose = text.find("\"loose-end\"").unwrap();
    assert!(brand < surface && surface < loose, "{text}");

    let primary = text.find("\"primary\"").unwrap();
    let secondary = text.find("\"secondary\"").unwrap();
    assert!(primary < secondary, "{text}");
}

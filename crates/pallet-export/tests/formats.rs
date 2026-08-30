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

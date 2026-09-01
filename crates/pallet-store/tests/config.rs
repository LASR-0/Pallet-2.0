//! Settings must survive whatever a hand-edited file throws at them.

use pallet_store::Config;
use pallet_store::config::{ReportSpace, Space, Theme};

#[test]
fn defaults_match_the_prototypes_settings_screen() {
    let c = Config::default();
    assert_eq!(c.picker.shortcut, "CTRL+SHIFT+P");
    assert_eq!(c.picker.loupe_zoom, 16);
    assert_eq!(c.picker.average_size, 5);
    assert!(c.picker.copy_on_pick);
    assert_eq!(c.picker.multi_pick_length, 5);
    assert!(c.color.name_new_colors);
    assert!(!c.general.stay_on_top);
    // Decisions taken during planning.
    assert_eq!(c.color.space, Space::Oklch);
    assert_eq!(c.picker.report_space, ReportSpace::Srgb);
    assert_eq!(c.general.theme, Theme::Sketchbook);
}

#[test]
fn a_missing_file_is_not_an_error() {
    let loaded = Config::load(std::path::Path::new("/nonexistent/pallet/config.toml"));
    assert_eq!(loaded.config, Config::default());
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn round_trips_through_toml() {
    let mut original = Config::default();
    original.general.theme = Theme::Studio;
    original.picker.loupe_zoom = 32;
    original.color.space = Space::Hsl;

    let text = toml::to_string_pretty(&original).unwrap();
    let loaded = Config::from_toml(&text);

    assert_eq!(loaded.config, original);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn garbage_falls_back_to_defaults_rather_than_panicking() {
    let loaded = Config::from_toml("this is not TOML at all {{{");
    assert_eq!(loaded.config, Config::default());
    assert!(!loaded.warnings.is_empty());
}

#[test]
fn one_bad_section_does_not_discard_the_others() {
    // picker.loupe_zoom is a string, which cannot deserialise. The general and
    // color sections around it must still be honoured.
    let loaded = Config::from_toml(
        r#"
        [general]
        theme = "studio"

        [picker]
        loupe_zoom = "enormous"

        [color]
        space = "hsl"
        "#,
    );

    assert_eq!(loaded.config.general.theme, Theme::Studio);
    assert_eq!(loaded.config.color.space, Space::Hsl);
    // The unreadable section reverts to defaults...
    assert_eq!(loaded.config.picker.loupe_zoom, 16);
    // ...and says so.
    assert!(
        loaded.warnings.iter().any(|w| w.contains("picker")),
        "{:?}",
        loaded.warnings
    );
}

#[test]
fn an_unknown_theme_does_not_take_the_file_down_with_it() {
    let loaded = Config::from_toml(
        r#"
        [general]
        theme = "neon"

        [picker]
        loupe_zoom = 24
        "#,
    );
    assert_eq!(loaded.config.general.theme, Theme::Sketchbook);
    assert_eq!(loaded.config.picker.loupe_zoom, 24);
    assert!(!loaded.warnings.is_empty());
}

#[test]
fn out_of_range_numbers_are_clamped_with_a_warning() {
    let loaded = Config::from_toml("[picker]\nloupe_zoom = 200\nmulti_pick_length = 0\n");
    assert_eq!(loaded.config.picker.loupe_zoom, 64);
    assert_eq!(loaded.config.picker.multi_pick_length, 1);
    assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
}

#[test]
fn an_even_average_size_is_pushed_odd() {
    // A 4x4 sample has no centre pixel to anchor the loupe on.
    let loaded = Config::from_toml("[picker]\naverage_size = 4\n");
    assert_eq!(loaded.config.picker.average_size, 5);
    assert!(loaded.warnings.iter().any(|w| w.contains("average_size")));
}

#[test]
fn an_empty_shortcut_reverts_to_the_default() {
    let loaded = Config::from_toml("[picker]\nshortcut = \"\"\n");
    assert_eq!(loaded.config.picker.shortcut, "CTRL+SHIFT+P");
    assert!(!loaded.warnings.is_empty());
}

#[test]
fn partial_files_keep_defaults_for_everything_absent() {
    let loaded = Config::from_toml("[general]\nstay_on_top = true\n");
    assert!(loaded.config.general.stay_on_top);
    assert_eq!(loaded.config.picker, Config::default().picker);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn saves_and_reloads_from_disk() {
    let dir = std::env::temp_dir().join(format!("pallet-cfg-{}", std::process::id()));
    let path = Config::path_in(&dir);

    let mut c = Config::default();
    c.picker.multi_pick_length = 8;
    c.save(&path).unwrap();

    let loaded = Config::load(&path);
    assert_eq!(loaded.config.picker.multi_pick_length, 8);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn key_bindings_default_to_the_documented_set() {
    let k = Config::default().keys;
    assert_eq!(k.pick, "CTRL+SHIFT+P");
    assert_eq!(k.theme, "T");
    assert_eq!(k.search, "/");
    assert_eq!(k.save_palette, "CTRL+S");
    assert_eq!(k.loupe_commit, "Return");
    assert_eq!(k.loupe_save, "S");
    assert_eq!(k.loupe_cancel, "Escape");
}

#[test]
fn key_bindings_round_trip_and_survive_a_partial_file() {
    let loaded = Config::from_toml("[keys]\npick = \"CTRL+ALT+K\"\n");
    assert_eq!(loaded.config.keys.pick, "CTRL+ALT+K");
    // Everything unmentioned keeps its default rather than emptying.
    assert_eq!(loaded.config.keys.theme, "T");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

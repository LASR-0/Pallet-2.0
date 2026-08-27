//! The hand-editable settings file.
//!
//! Settings live in TOML rather than the database so they can be kept in
//! dotfiles and edited by hand. That makes malformed input a normal condition,
//! not an exceptional one, so [`Config::load`] cannot fail: it degrades to
//! defaults key by key and reports what it rejected. A typo in one setting must
//! never cost the user the rest of their configuration, let alone start-up.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Which of the prototype's two themes the window uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Warm light theme.
    #[default]
    Sketchbook,
    /// Dark theme.
    Studio,
}

/// How a picked colour's value is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReportSpace {
    /// Convert to sRGB. What a designer pasting into CSS expects.
    #[default]
    Srgb,
    /// Report the display's own values, untranslated.
    Display,
}

/// Which space harmony and ramps are computed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Space {
    /// Perceptually uniform. The default.
    #[default]
    Oklch,
    /// What the prototype used.
    Hsl,
}

impl From<Space> for pallet_color::Space {
    fn from(s: Space) -> Self {
        match s {
            Space::Oklch => pallet_color::Space::Oklch,
            Space::Hsl => pallet_color::Space::Hsl,
        }
    }
}

/// Window and application behaviour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct General {
    /// Active theme.
    pub theme: Theme,
    /// Whether the window floats above other applications. The prototype
    /// ships this off, and `bool::default()` agrees.
    pub stay_on_top: bool,
}

/// Behaviour of the screen picker and its loupe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Picker {
    /// Global shortcut that freezes the screen.
    ///
    /// On Wayland this is advisory: compositors own their key bindings, so
    /// Pallet prints the matching bind line rather than registering it.
    pub shortcut: String,
    /// Loupe magnification at rest.
    pub loupe_zoom: u8,
    /// Width of the square averaged while Shift is held.
    pub average_size: u8,
    /// Put the hex on the clipboard the moment a colour is picked.
    pub copy_on_pick: bool,
    /// How many colours one multi-pick pass gathers.
    pub multi_pick_length: u8,
    /// How picked values are reported.
    pub report_space: ReportSpace,
}

impl Default for Picker {
    fn default() -> Self {
        // Values taken from the prototype's Settings screen.
        Self {
            shortcut: "CTRL+SHIFT+P".into(),
            loupe_zoom: 16,
            average_size: 5,
            copy_on_pick: true,
            multi_pick_length: 5,
            report_space: ReportSpace::default(),
        }
    }
}

/// Colour maths preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorSettings {
    /// Space used for harmony and ramps.
    pub space: Space,
    /// Look up a name for every new colour.
    pub name_new_colors: bool,
}

impl Default for ColorSettings {
    fn default() -> Self {
        Self {
            space: Space::default(),
            name_new_colors: true,
        }
    }
}

/// The whole settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Window and application behaviour.
    pub general: General,
    /// Picker and loupe behaviour.
    pub picker: Picker,
    /// Colour maths preferences.
    pub color: ColorSettings,
}

/// The result of loading settings: always a usable config, plus any complaints.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The settings to use.
    pub config: Config,
    /// Human-readable notes about anything ignored or clamped.
    pub warnings: Vec<String>,
}

// Accepted ranges. Outside these a value is clamped, never rejected.
const ZOOM_RANGE: std::ops::RangeInclusive<u8> = 2..=64;
const AVERAGE_RANGE: std::ops::RangeInclusive<u8> = 1..=15;
const MULTI_PICK_RANGE: std::ops::RangeInclusive<u8> = 1..=32;

impl Config {
    /// Read settings from `path`, falling back to defaults for anything
    /// missing, malformed or out of range.
    ///
    /// A missing file is not a problem and produces no warnings; it simply
    /// means the user has never changed anything.
    pub fn load(path: &Path) -> Loaded {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Loaded {
                    config: Config::default(),
                    warnings: Vec::new(),
                };
            }
            Err(e) => {
                return Loaded {
                    config: Config::default(),
                    warnings: vec![format!("could not read {}: {e}", path.display())],
                };
            }
        };

        Self::from_toml(&text)
    }

    /// Parse settings from TOML text, degrading gracefully.
    pub fn from_toml(text: &str) -> Loaded {
        let mut warnings = Vec::new();

        // Whole-file parse first: it keeps every valid key when the file is
        // well-formed, which is the common case.
        match toml::from_str::<Config>(text) {
            Ok(config) => {
                let mut config = config;
                warnings.extend(config.clamp());
                Loaded { config, warnings }
            }
            Err(whole_file_error) => {
                // One bad value must not discard the rest, so fall back to
                // reading section by section.
                warnings.push(format!("settings rejected: {}", terse(&whole_file_error)));

                let mut config = Config::default();
                if let Ok(table) = toml::from_str::<toml::Table>(text) {
                    salvage(&mut config.general, &table, "general", &mut warnings);
                    salvage(&mut config.picker, &table, "picker", &mut warnings);
                    salvage(&mut config.color, &table, "color", &mut warnings);
                } else {
                    warnings.push("the file is not valid TOML; using defaults".into());
                }

                warnings.extend(config.clamp());
                Loaded { config, warnings }
            }
        }
    }

    /// Write settings to `path`, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::WriteConfig {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(path, text).map_err(|source| Error::WriteConfig {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Force every numeric setting into its supported range.
    fn clamp(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut fix = |name: &str, value: &mut u8, range: &std::ops::RangeInclusive<u8>| {
            if !range.contains(value) {
                let clamped = (*value).clamp(*range.start(), *range.end());
                warnings.push(format!(
                    "picker.{name} was {value}, outside {}..={}; using {clamped}",
                    range.start(),
                    range.end()
                ));
                *value = clamped;
            }
        };
        fix("loupe_zoom", &mut self.picker.loupe_zoom, &ZOOM_RANGE);
        fix(
            "average_size",
            &mut self.picker.average_size,
            &AVERAGE_RANGE,
        );
        fix(
            "multi_pick_length",
            &mut self.picker.multi_pick_length,
            &MULTI_PICK_RANGE,
        );

        // An even averaging window has no centre pixel to anchor on.
        if self.picker.average_size.is_multiple_of(2) {
            let odd = self.picker.average_size.saturating_add(1);
            warnings.push(format!(
                "picker.average_size must be odd so the sample has a centre pixel; using {odd}"
            ));
            self.picker.average_size = odd;
        }

        if self.picker.shortcut.trim().is_empty() {
            warnings.push("picker.shortcut was empty; using the default".into());
            self.picker.shortcut = Picker::default().shortcut;
        }

        warnings
    }

    /// Where settings live, given a config directory.
    pub fn path_in(config_dir: &Path) -> PathBuf {
        config_dir.join("config.toml")
    }
}

/// Try to read one section on its own, leaving the default in place if it fails.
fn salvage<T>(slot: &mut T, table: &toml::Table, key: &str, warnings: &mut Vec<String>)
where
    T: for<'de> Deserialize<'de>,
{
    let Some(value) = table.get(key) else {
        return;
    };
    match T::deserialize(value.clone()) {
        Ok(parsed) => *slot = parsed,
        Err(e) => warnings.push(format!("[{key}] ignored: {}", terse(&e))),
    }
}

/// TOML errors carry multi-line spans; keep warnings to one line.
fn terse(e: &impl std::fmt::Display) -> String {
    e.to_string()
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("invalid")
        .trim()
        .to_string()
}

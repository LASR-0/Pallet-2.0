//! What every format reads and writes.

use pallet_color::Color;
use serde::{Deserialize, Serialize};

/// One colour in a palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swatch {
    /// The colour itself.
    pub color: Color,
    /// Its name, when it has one. Several formats carry names; those that do
    /// not simply drop them.
    pub name: Option<String>,
}

impl Swatch {
    /// A swatch with no name.
    pub fn new(color: Color) -> Self {
        Self { color, name: None }
    }

    /// A named swatch.
    pub fn named(color: Color, name: impl Into<String>) -> Self {
        Self {
            color,
            name: Some(name.into()),
        }
    }

    /// The name, or a hex string to fall back on.
    ///
    /// Suitable where a format wants *something* human-facing, such as an ASE
    /// entry. Not suitable for variable names — see [`Swatch::identifier`].
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.color.to_hex())
    }

    /// A name for a CSS or SCSS variable.
    ///
    /// An unnamed swatch falls back to its position, not its hex: `--colour-4`
    /// says something, whereas `--f9a08a: #F9A08A` restates the value and tells
    /// the reader nothing.
    pub fn identifier(&self, index: usize) -> String {
        match &self.name {
            Some(name) => slug(name, index),
            None => format!("colour-{}", index + 1),
        }
    }
}

/// A palette, as exported or imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// Display name.
    pub name: String,
    /// Members, in order.
    pub swatches: Vec<Swatch>,
}

impl Palette {
    /// Build a palette.
    pub fn new(name: impl Into<String>, swatches: Vec<Swatch>) -> Self {
        Self {
            name: name.into(),
            swatches,
        }
    }

    /// Fill in a nearest-match name for every swatch that has none.
    ///
    /// Exported variable names are the reason: `--vivid-tangerine` is worth
    /// having where `--colour-1` is not. Names are suggestions in Pallet
    /// anyway, and a file being handed to a stylesheet is exactly where a
    /// suggestion beats a number.
    ///
    /// Applies to a copy destined for a file, never to the stored library.
    pub fn with_suggested_names(mut self) -> Self {
        for swatch in &mut self.swatches {
            if swatch.name.is_none() {
                swatch.name =
                    pallet_color::naming::nearest(swatch.color).map(|m| m.named.name.to_string());
            }
        }
        self
    }
}

/// Turn a name into something safe for a CSS custom property or SCSS variable.
///
/// Lowercase, non-alphanumerics collapsed to single hyphens, trimmed. A name
/// that reduces to nothing falls back to the index, because an empty variable
/// name would produce a file that does not parse.
pub fn slug(name: &str, index: usize) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if out.is_empty() {
        return format!("colour-{}", index + 1);
    }
    // SCSS refuses an identifier that starts with a digit, and a CSS custom
    // property that does is legal but confusing. A hex-derived name like
    // "6e5a78" hits this constantly.
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        format!("c-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_safe_for_css_and_scss() {
        assert_eq!(slug("Winter Sunset", 0), "winter-sunset");
        assert_eq!(slug("Rob Roy", 0), "rob-roy");
        assert_eq!(slug("  spaced  out  ", 0), "spaced-out");
        assert_eq!(slug("Atomic/Tangerine!", 0), "atomic-tangerine");
        // SCSS rejects a leading digit, so it is prefixed.
        assert_eq!(slug("100 Mph", 0), "c-100-mph");
    }

    #[test]
    fn an_unnamed_swatch_is_identified_by_position_not_by_its_hex() {
        use pallet_color::Color;
        let anon = Swatch::new(Color::parse_hex("#6E5A78").unwrap());
        // "--6e5a78: #6E5A78" restates the value and is invalid SCSS besides.
        assert_eq!(anon.identifier(3), "colour-4");

        let named = Swatch::named(Color::parse_hex("#6E5A78").unwrap(), "Rum");
        assert_eq!(named.identifier(3), "rum");
    }

    #[test]
    fn identifiers_never_start_with_a_digit() {
        for (name, index) in [("100 Mph", 0), ("2 Cool", 1), ("999", 2)] {
            let id = slug(name, index);
            assert!(
                !id.starts_with(|c: char| c.is_ascii_digit()),
                "{name} produced {id}"
            );
        }
    }

    #[test]
    fn a_name_with_nothing_usable_falls_back_to_its_position() {
        // An empty variable name would produce a file that does not parse.
        assert_eq!(slug("", 4), "colour-5");
        assert_eq!(slug("///", 0), "colour-1");
    }
}

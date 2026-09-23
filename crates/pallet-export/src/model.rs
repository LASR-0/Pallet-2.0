//! What every format reads and writes.

use pallet_color::Color;
use serde::{Deserialize, Serialize};

/// One colour in a palette.
///
/// `group` and `name` together are the colour's *token*: `brand/primary`,
/// `surface/base`. Two levels, deliberately — see [`Palette::grouped`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swatch {
    /// The colour itself.
    pub color: Color,
    /// Its name, when it has one. Several formats carry names; those that do
    /// not simply drop them.
    pub name: Option<String>,
    /// The token group it belongs to, when it has one.
    ///
    /// Skipped when absent so a palette that uses no groups serialises exactly
    /// as it did before groups existed, and defaulted on the way in so every
    /// JSON file written before them still reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

impl Swatch {
    /// A swatch with no name.
    pub fn new(color: Color) -> Self {
        Self {
            color,
            name: None,
            group: None,
        }
    }

    /// A named swatch, with no group.
    pub fn named(color: Color, name: impl Into<String>) -> Self {
        Self {
            color,
            name: Some(name.into()),
            group: None,
        }
    }

    /// A swatch carrying a full token: a group and a name within it.
    pub fn token(color: Color, group: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            color,
            name: Some(name.into()),
            group: Some(group.into()),
        }
    }

    /// The group, ignoring one that is present but blank.
    ///
    /// A text field left empty arrives as `Some("")`, which would otherwise
    /// produce a group whose name is nothing — an empty SCSS map key, a
    /// Tailwind object under `""`, an unnamed ASE group.
    pub fn group(&self) -> Option<&str> {
        self.group
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
    }

    /// The name, or a hex string to fall back on.
    ///
    /// Suitable where a format wants *something* human-facing, such as an ASE
    /// entry. Not suitable for variable names — see [`Swatch::identifier`].
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.color.to_hex())
    }

    /// The label with its group in front: `brand / primary`.
    ///
    /// For formats that show a flat list of human-facing names and have no
    /// nesting to put the group in — GPL, and ASE entries within a group.
    pub fn qualified_label(&self) -> String {
        match self.group() {
            Some(group) => format!("{group} / {}", self.label()),
            None => self.label(),
        }
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

    /// The identifier with its group slugged in front: `brand-primary`.
    ///
    /// For the formats that have to flatten the token into one symbol because
    /// the file has nowhere to nest it — CSS custom properties above all.
    pub fn qualified_identifier(&self, index: usize) -> String {
        match self.group() {
            Some(group) => format!("{}-{}", slug(group, index), self.identifier(index)),
            None => self.identifier(index),
        }
    }
}

/// One swatch within a group, with its index in the whole palette.
///
/// The index travels with it because [`Swatch::identifier`] falls back to it
/// when a swatch has no name, and that number has to mean the position in the
/// palette rather than the position in the bucket.
pub type Member<'a> = (usize, &'a Swatch);

/// A token group and its members. See [`Palette::grouped`].
pub type Group<'a> = (Option<&'a str>, Vec<Member<'a>>);

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

    /// The swatches bucketed by token group, for the formats that can nest.
    ///
    /// Groups come out in the order they first appear in the palette, and the
    /// swatches within each keep their palette order; ungrouped swatches
    /// gather under `None`, in its own position in that same sequence. So the
    /// output of every format follows the order on screen, which is the only
    /// order the user has any control over.
    ///
    /// Buckets rather than runs: a palette may well go brand, surface, brand
    /// again, and a format with real nesting cannot write the same key twice.
    /// An SCSS map with two `brand` entries silently keeps the last one.
    ///
    /// Each swatch keeps its index in the whole palette, because that is what
    /// [`Swatch::identifier`] falls back to when a swatch has no name — the
    /// number has to mean its position in the palette, not in its bucket.
    pub fn grouped(&self) -> Vec<Group<'_>> {
        let mut keys: Vec<Option<&str>> = Vec::new();
        let mut buckets: Vec<Vec<Member<'_>>> = Vec::new();

        for (index, swatch) in self.swatches.iter().enumerate() {
            let key = swatch.group();
            match keys.iter().position(|seen| *seen == key) {
                Some(at) => buckets[at].push((index, swatch)),
                None => {
                    keys.push(key);
                    buckets.push(vec![(index, swatch)]);
                }
            }
        }

        keys.into_iter().zip(buckets).collect()
    }

    /// Whether any swatch carries a group, and so whether the grouped shape of
    /// a format is worth reaching for at all.
    pub fn has_groups(&self) -> bool {
        self.swatches.iter().any(|s| s.group().is_some())
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

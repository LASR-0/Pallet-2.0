//! The text formats: CSS custom properties, SCSS, Tailwind, JSON and GPL.

use pallet_color::Color;

use crate::error::{Error, Result};
use crate::model::{Palette, Swatch, slug};

/// CSS custom properties, scoped to `:root`.
pub fn css_vars(palette: &Palette) -> String {
    let mut out = format!("/* {} */\n:root {{\n", palette.name);
    for (i, swatch) in palette.swatches.iter().enumerate() {
        out.push_str(&format!(
            "  --{}: {};\n",
            swatch.identifier(i),
            swatch.color.to_hex()
        ));
    }
    out.push_str("}\n");
    out
}

/// SCSS variables.
pub fn scss(palette: &Palette) -> String {
    let mut out = format!("// {}\n", palette.name);
    for (i, swatch) in palette.swatches.iter().enumerate() {
        out.push_str(&format!(
            "${}: {};\n",
            swatch.identifier(i),
            swatch.color.to_hex()
        ));
    }
    out
}

/// A Tailwind colour object, ready to spread into `theme.extend.colors`.
pub fn tailwind(palette: &Palette) -> String {
    let group = slug(&palette.name, 0);
    let mut out = String::from("module.exports = {\n  theme: {\n    extend: {\n      colors: {\n");
    out.push_str(&format!("        \"{group}\": {{\n"));
    for (i, swatch) in palette.swatches.iter().enumerate() {
        out.push_str(&format!(
            "          \"{}\": \"{}\",\n",
            swatch.identifier(i),
            swatch.color.to_hex()
        ));
    }
    out.push_str("        },\n      },\n    },\n  },\n};\n");
    out
}

/// Pallet's own JSON, which round-trips everything including names.
pub fn json(palette: &Palette) -> Result<String> {
    Ok(serde_json::to_string_pretty(palette)?)
}

/// Read Pallet's JSON back.
pub fn read_json(text: &str) -> Result<Palette> {
    Ok(serde_json::from_str(text)?)
}

/// A GIMP palette.
pub fn gpl(palette: &Palette) -> String {
    let mut out = String::from("GIMP Palette\n");
    out.push_str(&format!("Name: {}\n", palette.name));
    out.push_str("Columns: 0\n#\n");
    for swatch in &palette.swatches {
        let (r, g, b) = swatch.color.to_rgb();
        match &swatch.name {
            Some(name) => out.push_str(&format!("{r:>3} {g:>3} {b:>3}\t{name}\n")),
            None => out.push_str(&format!("{r:>3} {g:>3} {b:>3}\n")),
        }
    }
    out
}

/// Read a GIMP palette.
pub fn read_gpl(text: &str) -> Result<Palette> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("GIMP Palette") {
        return Err(Error::NotGpl);
    }

    let mut name = String::from("Imported");
    let mut swatches = Vec::new();

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Name:") {
            name = rest.trim().to_string();
            continue;
        }
        if line.starts_with("Columns:") {
            continue;
        }

        // "255   0   0\tRed" — whitespace-separated channels, then an
        // optional name that may itself contain spaces.
        let mut parts = line.split_whitespace();
        let (Some(r), Some(g), Some(b)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(r), Ok(g), Ok(b)) = (r.parse::<u8>(), g.parse::<u8>(), b.parse::<u8>()) else {
            continue;
        };
        let rest: Vec<&str> = parts.collect();
        let label = rest.join(" ");
        swatches.push(Swatch {
            color: Color::new(r, g, b),
            name: (!label.is_empty()).then_some(label),
        });
    }

    Ok(Palette { name, swatches })
}

//! The text formats: CSS custom properties, SCSS, Tailwind, JSON and GPL.

use pallet_color::Color;

use crate::error::{Error, Result};
use crate::model::{Palette, Swatch, slug};

/// CSS custom properties, scoped to `:root`.
///
/// CSS has no nesting to put a token group in, so the group is flattened into
/// the property name — `--brand-primary` — and marked with a comment above its
/// run. That is what every design-token pipeline emits for CSS, and it is the
/// only shape that stays usable: a group cannot become a nested selector
/// without changing which elements the variables apply to.
pub fn css_vars(palette: &Palette) -> String {
    let mut out = format!("/* {} */\n:root {{\n", palette.name);
    let grouped = palette.grouped();
    let sectioned = palette.has_groups();

    for (at, (group, members)) in grouped.iter().enumerate() {
        if sectioned {
            if at > 0 {
                out.push('\n');
            }
            out.push_str(&format!("  /* {} */\n", group.unwrap_or("ungrouped")));
        }
        for (i, swatch) in members {
            out.push_str(&format!(
                "  --{}: {};\n",
                swatch.qualified_identifier(*i),
                swatch.color.to_hex()
            ));
        }
    }

    out.push_str("}\n");
    out
}

/// SCSS variables, and a map per token group.
///
/// SCSS does have somewhere to put a group, and a map is where a stylesheet
/// author would look for one: `map.get($brand, primary)`. The flat variables
/// are still written, because most stylesheets reference `$brand-primary`
/// directly and dropping them would make this format a rewrite rather than an
/// export. The map is built from those variables, so the two cannot disagree.
pub fn scss(palette: &Palette) -> String {
    let mut out = format!("// {}\n", palette.name);
    let grouped = palette.grouped();
    let sectioned = palette.has_groups();

    for (group, members) in &grouped {
        if sectioned {
            out.push_str(&format!("\n// {}\n", group.unwrap_or("ungrouped")));
        }
        for (i, swatch) in members {
            out.push_str(&format!(
                "${}: {};\n",
                swatch.qualified_identifier(*i),
                swatch.color.to_hex()
            ));
        }
    }

    for (group, members) in &grouped {
        let Some(group) = group else { continue };
        out.push_str(&format!("\n${}: (\n", slug(group, 0)));
        for (i, swatch) in members {
            out.push_str(&format!(
                "  \"{}\": ${},\n",
                swatch.identifier(*i),
                swatch.qualified_identifier(*i)
            ));
        }
        out.push_str(");\n");
    }

    out
}

/// A Tailwind colour object, ready to spread into `theme.extend.colors`.
///
/// Tailwind's colour object is nested by design — `bg-brand-primary` comes
/// from `{ brand: { primary } }` — so a token group becomes a nesting level
/// and the class names come out right without anyone flattening anything.
///
/// Without groups the palette itself is the one level, as it always was.
/// With them the palette name would be a third level, which would push every
/// class to `bg-sunset-brand-primary`; the groups are the better level to keep.
pub fn tailwind(palette: &Palette) -> String {
    let mut out = String::from("module.exports = {\n  theme: {\n    extend: {\n      colors: {\n");

    if palette.has_groups() {
        for (group, members) in palette.grouped() {
            let key = group.map_or_else(|| slug(&palette.name, 0), |g| slug(g, 0));
            out.push_str(&format!("        \"{key}\": {{\n"));
            for (i, swatch) in members {
                out.push_str(&format!(
                    "          \"{}\": \"{}\",\n",
                    swatch.identifier(i),
                    swatch.color.to_hex()
                ));
            }
            out.push_str("        },\n");
        }
    } else {
        out.push_str(&format!("        \"{}\": {{\n", slug(&palette.name, 0)));
        for (i, swatch) in palette.swatches.iter().enumerate() {
            out.push_str(&format!(
                "          \"{}\": \"{}\",\n",
                swatch.identifier(i),
                swatch.color.to_hex()
            ));
        }
        out.push_str("        },\n");
    }

    out.push_str("      },\n    },\n  },\n};\n");
    out
}

/// W3C Design Tokens (DTCG), the interchange format for token sets.
///
/// A separate format from [`json`] rather than a reshaping of it. Pallet's own
/// JSON round-trips through [`read_json`], and giving it a `$value` shape would
/// mean every file this app has ever written stops importing. This one is
/// write-only by design — `Format::readable` says so — and exists because a
/// tool that lets you build token sets should be able to hand them to the
/// format the rest of that world reads.
pub fn dtcg(palette: &Palette) -> Result<String> {
    use serde_json::{Map, Value, json};

    let mut root = Map::new();
    root.insert("$description".into(), Value::String(palette.name.clone()));

    let token = |swatch: &Swatch| json!({ "$type": "color", "$value": swatch.color.to_hex() });

    for (group, members) in palette.grouped() {
        match group {
            // A group becomes an object; DTCG nests exactly as Tailwind does.
            Some(group) => {
                let mut nested = Map::new();
                for (i, swatch) in members {
                    nested.insert(swatch.identifier(i), token(swatch));
                }
                root.insert(slug(group, 0), Value::Object(nested));
            }
            // Ungrouped tokens sit at the root, which is legal DTCG and keeps
            // a palette with no groups from gaining a meaningless wrapper.
            None => {
                for (i, swatch) in members {
                    root.insert(swatch.identifier(i), token(swatch));
                }
            }
        }
    }

    Ok(serde_json::to_string_pretty(&Value::Object(root))?)
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
///
/// The format is a flat list and GIMP shows one name per entry, so a token
/// group goes into that name as `brand / primary`. Written in palette order
/// rather than grouped order: GIMP draws the swatches in file order, and
/// reordering them to suit a grouping it cannot show would change the picture
/// the user built for the sake of a label.
pub fn gpl(palette: &Palette) -> String {
    let mut out = String::from("GIMP Palette\n");
    out.push_str(&format!("Name: {}\n", palette.name));
    out.push_str("Columns: 0\n#\n");
    for swatch in &palette.swatches {
        let (r, g, b) = swatch.color.to_rgb();
        match swatch.name {
            Some(_) => out.push_str(&format!(
                "{r:>3} {g:>3} {b:>3}\t{}\n",
                swatch.qualified_label()
            )),
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
        // `gpl` writes a grouped token as "brand / primary", so the same
        // separator takes it apart again and a palette exported with groups
        // comes back with them. Split once and from the left: a name may well
        // contain a slash of its own, and the group cannot.
        let (group, name) = match label.split_once(" / ") {
            Some((group, name)) if !group.is_empty() && !name.is_empty() => {
                (Some(group.to_string()), name.to_string())
            }
            _ => (None, label),
        };
        swatches.push(Swatch {
            color: Color::new(r, g, b),
            name: (!name.is_empty()).then_some(name),
            group,
        });
    }

    Ok(Palette { name, swatches })
}

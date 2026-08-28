//! Binding a global shortcut to Pallet.
//!
//! On Wayland an application cannot simply grab a key: the compositor owns
//! every binding, by design. `global-hotkey` and friends are X11-only on Linux,
//! and the `org.freedesktop.portal.GlobalShortcuts` portal is unevenly
//! implemented — notably partial on `xdg-desktop-portal-hyprland`.
//!
//! So Pallet does not try to grab keys at all. It tells the user the exact line
//! to put in their compositor config, binding `pallet pick`. That is the
//! idiomatic arrangement on Wayland, works everywhere without a portal, and
//! makes the same entry point available to scripts.

#![warn(missing_docs)]

/// A desktop environment Pallet knows how to write a binding for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compositor {
    /// Hyprland.
    Hyprland,
    /// Sway, or another i3-compatible config.
    Sway,
    /// River.
    River,
    /// GNOME.
    Gnome,
    /// KDE Plasma.
    Kde,
    /// Something else, named as the desktop reported itself.
    Other(String),
    /// Nothing identified the session.
    Unknown,
}

impl Compositor {
    /// Identify the current session from the environment.
    pub fn detect() -> Self {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
            .unwrap_or_default();

        match desktop.to_ascii_lowercase().as_str() {
            "" => Self::Unknown,
            d if d.contains("hyprland") => Self::Hyprland,
            d if d.contains("sway") => Self::Sway,
            d if d.contains("river") => Self::River,
            d if d.contains("gnome") => Self::Gnome,
            d if d.contains("kde") || d.contains("plasma") => Self::Kde,
            _ => Self::Other(desktop),
        }
    }

    /// The config file this compositor keeps its bindings in.
    pub fn config_hint(&self) -> Option<&'static str> {
        match self {
            Self::Hyprland => Some("~/.config/hypr/hyprland.conf"),
            Self::Sway => Some("~/.config/sway/config"),
            Self::River => Some("~/.config/river/init"),
            _ => None,
        }
    }

    /// The line to add, binding `shortcut` to `command`.
    ///
    /// `shortcut` is Pallet's own notation, e.g. `CTRL+SHIFT+P`. Returns `None`
    /// for environments configured through a settings UI rather than a file.
    pub fn bind_line(&self, shortcut: &str, command: &str) -> Option<String> {
        let keys = Shortcut::parse(shortcut);
        match self {
            Self::Hyprland => Some(format!(
                "bind = {}, {}, exec, {command}",
                keys.modifiers.join(" "),
                keys.key
            )),
            Self::Sway => Some(format!(
                "bindsym {}+{} exec {command}",
                keys.modifiers.join("+"),
                keys.key
            )),
            Self::River => Some(format!(
                "riverctl map normal {} {} spawn '{command}'",
                keys.modifiers.join("+"),
                keys.key
            )),
            _ => None,
        }
    }

    /// Guidance for environments that have no config file to edit.
    pub fn manual_hint(&self) -> Option<&'static str> {
        match self {
            Self::Gnome => Some(
                "Settings > Keyboard > View and Customize Shortcuts > Custom Shortcuts, \
                 then add a shortcut running the command below.",
            ),
            Self::Kde => Some(
                "System Settings > Shortcuts > Add Command, then add a shortcut running \
                 the command below.",
            ),
            _ => None,
        }
    }
}

/// A shortcut split into modifiers and a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    /// Modifier names, normalised and uppercased.
    pub modifiers: Vec<String>,
    /// The non-modifier key.
    pub key: String,
}

impl Shortcut {
    /// Parse notation like `CTRL+SHIFT+P`.
    ///
    /// Accepts `+`, `-` and spaces as separators, and the common spellings of
    /// each modifier, because this is read from a hand-edited config file.
    pub fn parse(input: &str) -> Self {
        let parts: Vec<&str> = input
            .split(['+', '-', ' '])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();

        let mut modifiers = Vec::new();
        let mut key = String::new();

        for part in parts {
            match part.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => modifiers.push("CTRL".to_string()),
                "SHIFT" => modifiers.push("SHIFT".to_string()),
                "ALT" | "MOD1" => modifiers.push("ALT".to_string()),
                "SUPER" | "META" | "MOD4" | "WIN" | "LOGO" => modifiers.push("SUPER".to_string()),
                other => key = other.to_string(),
            }
        }

        Self { modifiers, key }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_default_shortcut() {
        let s = Shortcut::parse("CTRL+SHIFT+P");
        assert_eq!(s.modifiers, vec!["CTRL", "SHIFT"]);
        assert_eq!(s.key, "P");
    }

    #[test]
    fn accepts_the_spellings_a_hand_edited_config_will_contain() {
        for input in [
            "ctrl+shift+p",
            "Control-Shift-P",
            "CTRL SHIFT P",
            "  ctrl + shift + p  ",
        ] {
            let s = Shortcut::parse(input);
            assert_eq!(s.modifiers, vec!["CTRL", "SHIFT"], "{input}");
            assert_eq!(s.key, "P", "{input}");
        }
    }

    #[test]
    fn super_has_several_names_that_all_mean_the_same_key() {
        for name in ["SUPER", "META", "MOD4", "WIN", "LOGO"] {
            assert_eq!(
                Shortcut::parse(&format!("{name}+P")).modifiers,
                vec!["SUPER"]
            );
        }
    }

    #[test]
    fn a_bare_key_has_no_modifiers() {
        let s = Shortcut::parse("F9");
        assert!(s.modifiers.is_empty());
        assert_eq!(s.key, "F9");
    }

    #[test]
    fn hyprland_gets_its_own_comma_separated_syntax() {
        assert_eq!(
            Compositor::Hyprland
                .bind_line("CTRL+SHIFT+P", "pallet pick")
                .unwrap(),
            "bind = CTRL SHIFT, P, exec, pallet pick"
        );
    }

    #[test]
    fn sway_and_river_get_their_own_syntax() {
        assert_eq!(
            Compositor::Sway
                .bind_line("CTRL+SHIFT+P", "pallet pick")
                .unwrap(),
            "bindsym CTRL+SHIFT+P exec pallet pick"
        );
        assert_eq!(
            Compositor::River
                .bind_line("CTRL+SHIFT+P", "pallet pick")
                .unwrap(),
            "riverctl map normal CTRL+SHIFT P spawn 'pallet pick'"
        );
    }

    #[test]
    fn environments_configured_through_a_ui_get_guidance_not_a_line() {
        for c in [Compositor::Gnome, Compositor::Kde] {
            assert!(c.bind_line("CTRL+SHIFT+P", "pallet pick").is_none());
            assert!(c.manual_hint().is_some(), "{c:?} should explain itself");
        }
    }

    #[test]
    fn an_unknown_desktop_does_not_invent_a_binding() {
        assert!(
            Compositor::Unknown
                .bind_line("CTRL+SHIFT+P", "pallet pick")
                .is_none()
        );
        assert!(Compositor::Unknown.config_hint().is_none());
    }
}

//! The loupe's key bindings, and the string logic around them.
//!
//! Platform-agnostic on purpose: a binding is compared by name, not by any
//! platform's keysym or scancode type, so this one module serves both the
//! Wayland backend's `xkbcommon` events and Windows' `winit` ones.

/// The keys the loupe answers to, as configured.
#[derive(Debug, Clone)]
pub struct LoupeKeys {
    /// Take the colour.
    pub commit: String,
    /// Take it and keep it.
    pub save: String,
    /// Abandon the pick.
    pub cancel: String,
}

impl Default for LoupeKeys {
    fn default() -> Self {
        Self {
            commit: "Return".into(),
            save: "S".into(),
            cancel: "Escape".into(),
        }
    }
}

/// Whether a key name matches a configured binding.
///
/// Loupe bindings are a single key: the overlay holds an exclusive keyboard
/// grab, and a modifier combination there would fight Shift, which already
/// means "average this area". Any modifiers written in the binding are
/// therefore ignored rather than silently making it unmatchable.
pub fn name_binds(name: &str, binding: &str) -> bool {
    !name.is_empty()
        && binding
            .split(['+', '-'])
            .next_back()
            .is_some_and(|k| k.trim().eq_ignore_ascii_case(name))
}

/// The instruction pill's text, built from the keys actually in force.
///
/// The design's copy reads "Click to sample · Scroll to zoom · Space locks the
/// loupe · Esc cancels". Two of those are wrong for this picker — there is no
/// loupe lock, and the cancel key is remappable — and an instruction pill that
/// lies is worse than one that deviates by a word, so the middle items are
/// generated from the live bindings and the shape of the line is kept.
pub fn instructions(keys: &LoupeKeys, average: u32, palette: bool) -> String {
    let last = if palette {
        // Backing out of a palette pass keeps what it gathered, so calling it
        // "cancel" would read as a threat to discard the work.
        format!("{} finishes", pretty_key(&keys.cancel))
    } else {
        format!("{} cancels", pretty_key(&keys.cancel))
    };
    format!("Click to sample · Scroll to zoom · Shift averages {average}×{average} · {last}")
}

/// A key name as a reader would write it.
pub fn pretty_key(name: &str) -> String {
    match name.to_ascii_uppercase().as_str() {
        "ESCAPE" | "ESC" => "Esc".into(),
        "RETURN" | "ENTER" => "Enter".into(),
        "SPACE" => "Space".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()
                }
                None => String::new(),
            }
        }
    }
}

/// How much continuous scroll makes one zoom step.
///
/// A wheel notch and a trackpad's high-resolution scroll are both measured in
/// the same units on every platform this crate supports, at 15 units per
/// notch — `wl_pointer`'s convention, and also what Windows' `WHEEL_DELTA` is.
const SCROLL_NOTCH: f32 = 15.0;

/// How many zoom steps one scroll event is worth, positive to zoom in.
///
/// `discrete` counts whole wheel notches; when a source reports only
/// continuous motion it is accumulated in `carry` and spent a notch at a
/// time, so a trackpad swipe zooms proportionally and a wheel click still
/// moves exactly one step. Scroll is measured downward while zooming in is
/// upward, so the sign is flipped.
pub fn scroll_steps(discrete: i32, absolute: f64, carry: &mut f32) -> i32 {
    let steps = if discrete != 0 {
        // A source that reports notches is authoritative; anything part way
        // through the accumulator would double-count.
        *carry = 0.0;
        -discrete
    } else if absolute != 0.0 {
        *carry -= absolute as f32;
        let whole = (*carry / SCROLL_NOTCH).trunc();
        *carry -= whole * SCROLL_NOTCH;
        whole as i32
    } else {
        0
    };

    // A flick of a free-spinning wheel can deliver a lot at once, and zoom is
    // exponential, so clamp it to something a user can still follow.
    steps.clamp(-4, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binding_matches_its_key_whatever_the_case() {
        assert!(name_binds("RETURN", "Return"));
        assert!(name_binds("K", "k"));
        assert!(name_binds("ESCAPE", "Escape"));
    }

    #[test]
    fn a_remapped_key_stops_matching_the_old_one() {
        // The whole point of remapping: once commit is K, Return must not
        // still commit.
        assert!(!name_binds("RETURN", "K"));
        assert!(name_binds("K", "K"));
    }

    #[test]
    fn modifiers_in_a_binding_are_ignored_rather_than_breaking_it() {
        // Shift already means "average" inside the loupe, so a binding written
        // with modifiers still matches on its final key instead of never
        // matching at all.
        assert!(name_binds("K", "CTRL+K"));
        assert!(name_binds("RETURN", "SHIFT+Return"));
    }

    #[test]
    fn an_empty_press_matches_nothing() {
        assert!(!name_binds("", "K"));
        assert!(!name_binds("", ""));
    }

    #[test]
    fn a_wheel_notch_reported_as_discrete_is_one_zoom_step() {
        let mut carry = 0.0;
        assert_eq!(scroll_steps(-1, 0.0, &mut carry), 1, "up zooms in");
        assert_eq!(scroll_steps(1, 0.0, &mut carry), -1, "down zooms out");
    }

    #[test]
    fn high_resolution_scroll_still_zooms() {
        // The bug: a source speaking a high-resolution protocol leaves
        // `discrete` at zero and reports the movement in `absolute`, so the
        // wheel did nothing.
        let mut carry = 0.0;
        assert_eq!(scroll_steps(0, -15.0, &mut carry), 1);
        assert_eq!(scroll_steps(0, 15.0, &mut carry), -1);
    }

    #[test]
    fn a_partial_scroll_is_carried_rather_than_lost() {
        // A trackpad delivers a notch in fragments; dropping them would make
        // slow scrolling do nothing at all.
        let mut carry = 0.0;
        assert_eq!(scroll_steps(0, -5.0, &mut carry), 0);
        assert_eq!(scroll_steps(0, -5.0, &mut carry), 0);
        assert_eq!(
            scroll_steps(0, -5.0, &mut carry),
            1,
            "three fifths make one"
        );
        assert_eq!(
            scroll_steps(0, -5.0, &mut carry),
            0,
            "and the count restarts"
        );
    }

    #[test]
    fn a_discrete_notch_discards_any_part_spent_accumulation() {
        // Counting both would zoom twice for one movement.
        let mut carry = 0.0;
        scroll_steps(0, -10.0, &mut carry);
        assert!(carry != 0.0);
        assert_eq!(scroll_steps(-1, -15.0, &mut carry), 1);
        assert_eq!(carry, 0.0);
    }

    #[test]
    fn a_flick_of_the_wheel_is_capped() {
        // Zoom is exponential: 2x to 64x is five steps, so an uncapped flick
        // would jump the whole range at once.
        let mut carry = 0.0;
        assert_eq!(scroll_steps(0, -1000.0, &mut carry), 4);
        assert_eq!(scroll_steps(-40, 0.0, &mut carry), 4);
    }

    #[test]
    fn a_scroll_of_nothing_does_nothing() {
        let mut carry = 0.0;
        assert_eq!(scroll_steps(0, 0.0, &mut carry), 0);
        assert_eq!(carry, 0.0);
    }
}

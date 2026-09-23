/**
 * Matching keyboard events against configured bindings.
 *
 * Bindings are stored as text like `CTRL+SHIFT+P` so they stay legible in
 * `config.toml`, which people are expected to edit by hand. Both directions of
 * that string live here so the parser and the formatter cannot drift.
 */

/** Normalise the modifier spellings a hand-edited file might contain. */
function canonicalModifier(part: string): string | null {
  switch (part.toUpperCase()) {
    case "CTRL":
    case "CONTROL":
      return "CTRL";
    case "SHIFT":
      return "SHIFT";
    case "ALT":
    case "OPTION":
      return "ALT";
    case "SUPER":
    case "META":
    case "CMD":
    case "WIN":
    case "LOGO":
      return "SUPER";
    default:
      return null;
  }
}

interface Combo {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
  key: string;
}

/** Parse `CTRL+SHIFT+P` into something comparable with a KeyboardEvent. */
export function parseCombo(text: string): Combo | null {
  const parts = text
    .split(/[+\-\s]+/)
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length === 0) return null;

  const combo: Combo = {
    ctrl: false,
    shift: false,
    alt: false,
    meta: false,
    key: "",
  };
  for (const part of parts) {
    const modifier = canonicalModifier(part);
    if (modifier === "CTRL") combo.ctrl = true;
    else if (modifier === "SHIFT") combo.shift = true;
    else if (modifier === "ALT") combo.alt = true;
    else if (modifier === "SUPER") combo.meta = true;
    else combo.key = part.toUpperCase();
  }
  return combo.key ? combo : null;
}

/** The name this event's non-modifier key is stored under. */
function eventKeyName(event: KeyboardEvent): string {
  // `event.key` is layout-aware, which is what a user expects: pressing the
  // key labelled P should match "P" whatever the layout does underneath.
  const key = event.key;
  if (key === " ") return "SPACE";
  if (key.length === 1) return key.toUpperCase();
  return key.toUpperCase();
}

/** Whether a keyboard event matches a binding string. */
export function matches(event: KeyboardEvent, binding: string): boolean {
  const combo = parseCombo(binding);
  if (!combo) return false;
  return (
    event.ctrlKey === combo.ctrl &&
    event.shiftKey === combo.shift &&
    event.altKey === combo.alt &&
    event.metaKey === combo.meta &&
    eventKeyName(event) === combo.key
  );
}

/**
 * Spell a stored binding out for the screen: `CTRL+SHIFT+P` → `CTRL + SHIFT + P`.
 *
 * Goes through `parseCombo` rather than splitting the string, so a binding
 * hand-written in `config.toml` as `control-shift-p` is shown in the same form
 * as one captured in Settings. Modifiers come out in a fixed order for the
 * same reason: the order they were typed in is not worth showing.
 */
export function displayCombo(binding: string): string {
  const combo = parseCombo(binding);
  if (!combo) return "";

  const parts: string[] = [];
  if (combo.ctrl) parts.push("CTRL");
  if (combo.shift) parts.push("SHIFT");
  if (combo.alt) parts.push("ALT");
  if (combo.meta) parts.push("SUPER");
  parts.push(combo.key);
  return parts.join(" + ");
}

/** Modifier keys, which cannot be a binding on their own. */
const MODIFIER_KEYS = new Set(["CONTROL", "SHIFT", "ALT", "META", "OS"]);

/**
 * Turn a keypress into a binding string, or `null` if it is only modifiers.
 *
 * Used while capturing a new binding: holding Ctrl alone should keep waiting
 * rather than binding the command to Ctrl.
 */
export function comboFromEvent(event: KeyboardEvent): string | null {
  const key = eventKeyName(event);
  if (MODIFIER_KEYS.has(key)) return null;

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("CTRL");
  if (event.shiftKey) parts.push("SHIFT");
  if (event.altKey) parts.push("ALT");
  if (event.metaKey) parts.push("SUPER");
  parts.push(key);
  return parts.join("+");
}

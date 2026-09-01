/**
 * The library search field.
 *
 * Ghost-text completion is drawn as a span *behind* a transparent input rather
 * than inside it, because an `<input>` cannot style part of its own value. The
 * two layers share the same font declaration and padding so the characters land
 * on top of each other exactly; changing one without the other will visibly
 * desynchronise them.
 */

import Fuse from "fuse.js";

import { el, svg } from "../dom";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";
const FIELD_FONT = `400 11.5px/1 ${SANS}`;

/** A magnifier, sized to sit beside 11.5px text. */
function searchIcon(): SVGElement {
  const icon = svg("svg", {
    viewBox: "0 0 16 16",
    width: "13",
    height: "13",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": "1.5",
    "stroke-linecap": "round",
    style: "display:block;flex:none",
  });
  icon.append(
    svg("circle", { cx: "7", cy: "7", r: "4.6" }),
    svg("path", { d: "M10.4 10.4 L14 14" }),
  );
  return icon;
}

/**
 * The best *prefix* completion for `query` among `names`.
 *
 * Only a prefix can be completed inline: ghost text continues what the user
 * typed, so a match in the middle of a name has nothing to show. Fuzzy matches
 * still narrow the results below, they just cannot be auto-completed.
 */
export function completionFor(query: string, names: string[]): string | null {
  const q = query.trim().toLowerCase();
  if (!q) return null;

  const prefixed = names.filter((n) => n.toLowerCase().startsWith(q));
  if (prefixed.length === 0) return null;

  // Shortest match, so completion converges as the user types rather than
  // jumping to a long name they have to delete.
  prefixed.sort((a, b) => a.length - b.length || a.localeCompare(b));
  const best = prefixed[0]!;
  return best.toLowerCase() === q ? null : best;
}

/** Rank items by fuzzy match, falling back to the original order when empty. */
export function filterByQuery<T>(
  items: T[],
  query: string,
  keys: string[],
): T[] {
  const q = query.trim();
  if (!q) return items;

  const fuse = new Fuse(items, {
    keys,
    threshold: 0.4,
    ignoreLocation: true,
    minMatchCharLength: 1,
  });
  return fuse.search(q).map((r) => r.item);
}

export interface SearchActions {
  /** Called after the field's own DOM has already been updated. */
  onQuery: (value: string) => void;
  /** Best prefix completion for a query, or null. */
  complete: (value: string) => string | null;
}

/**
 * The library's heading and its search field, as one bar.
 *
 * They were two stacked bars saying much the same thing. Merged, the heading
 * names the collection, the field searches it, and the count on the right
 * belongs to both — one row instead of two, in a window where vertical space
 * is the scarce thing.
 *
 * The input element is created once and never replaced. Re-rendering the whole
 * screen on each keystroke — the pattern used everywhere else in this app —
 * destroys and recreates the field mid-word, which loses focus, the caret and
 * occasionally a character. A text field is the one place that has to hold
 * still, so it owns its own DOM and only tells the caller the query changed.
 */
export function renderLibraryBar(
  heading: string,
  count: number,
  query: string,
  placeholder: string,
  actions: SearchActions,
): HTMLElement {
  const shell = el("div", {
    style:
      "display:flex;align-items:center;gap:8px;height:34px;" +
      // Fully rounded, so the bar reads as a field rather than as a panel
      // with something typed into it.
      "padding:0 14px;border-radius:999px;" +
      "border:1px solid var(--line);background:var(--panel)",
  });

  // The ghost sits under the input, inside a wrapper that holds only those
  // two. Positioning it against the bar instead would mean re-deriving its
  // offset from the heading's width every time the heading changed.
  const field = el("div", {
    style:
      "position:relative;flex:1;min-width:0;display:flex;align-items:center",
  });
  const ghost = el("div", {
    style:
      "position:absolute;left:0;top:0;width:100%;height:100%;display:flex;" +
      `align-items:center;font:${FIELD_FONT};pointer-events:none;` +
      "white-space:pre;overflow:hidden",
  });

  const input = document.createElement("input");
  input.type = "text";
  input.value = query;
  input.placeholder = placeholder;
  input.spellcheck = false;
  input.setAttribute(
    "style",
    `flex:1;min-width:0;border:0;outline:none;background:transparent;` +
      `font:${FIELD_FONT};color:var(--ink);padding:0;` +
      // A desktop window suppresses selection everywhere else; a text field
      // is the one place it must work.
      "user-select:text;-webkit-user-select:text",
  );

  const paintGhost = (value: string) => {
    const suggestion = actions.complete(value);
    ghost.replaceChildren();
    if (suggestion) {
      ghost.append(
        el("span", { style: "color:transparent", text: value }),
        el("span", {
          style: "color:var(--mute)",
          text: suggestion.slice(value.length),
        }),
      );
    }
  };
  paintGhost(query);

  input.addEventListener("input", () => {
    paintGhost(input.value);
    actions.onQuery(input.value);
  });
  input.addEventListener("keydown", (event) => {
    const atEnd = input.selectionStart === input.value.length;
    const suggestion = actions.complete(input.value);
    const accepting =
      event.key === "Tab" || (event.key === "ArrowRight" && atEnd);
    if (suggestion && accepting) {
      event.preventDefault();
      input.value = suggestion;
      paintGhost(suggestion);
      actions.onQuery(suggestion);
    }
    if (event.key === "Escape") {
      event.preventDefault();
      if (input.value) {
        input.value = "";
        paintGhost("");
        actions.onQuery("");
      } else {
        // An empty field gives the keyboard back, so the digit shortcuts work
        // again without reaching for the mouse.
        input.blur();
      }
    }
    // Digits switch tabs globally; inside a text field they are just digits.
    event.stopPropagation();
  });

  field.append(ghost, input);

  shell.append(
    el("span", {
      // Nudged down a pixel. All-caps text has no descenders, so centring its
      // box puts the letters above the centre line that the icon and the
      // mixed-case field share.
      style:
        `font:400 10px/1 ${MONO};letter-spacing:.14em;color:var(--mute);` +
        "flex:none;position:relative;top:1px",
      text: heading,
    }),
    el("div", { style: "color:var(--mute);display:flex;flex:none" }, [
      searchIcon(),
    ]),
    el("span", {
      style: `font:400 11.5px/1 ${SANS};color:var(--mute);flex:none`,
      text: ":",
    }),
    field,
    el("span", {
      style: `font:400 10px/1 ${MONO};color:var(--mute);flex:none`,
      text: String(count),
    }),
  );
  return shell;
}

/** Focus the search field, for the "/" shortcut. */
export function focusSearch(container: HTMLElement): void {
  const input = container.querySelector("input");
  if (!(input instanceof HTMLInputElement)) return;
  input.focus();
  input.setSelectionRange(input.value.length, input.value.length);
}

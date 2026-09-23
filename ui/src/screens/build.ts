/**
 * The Build screen: gather colours by picking, then save them as a palette.
 *
 * Laid out as the palette it is making. A saved palette on the Palettes tab is
 * one flush strip of colour under a single ring; the tints ramp on Current is
 * the same thing with its values written underneath. This screen used to show
 * something else entirely — separate 180px columns, each with its own 16px
 * radius and a hex turned on its side — so what you were building looked
 * nothing like what you were going to get. It is now built from the two
 * patterns the rest of the window already uses.
 *
 * Taking a colour back out is the only destructive single click in the app, so
 * the strip says so rather than leaving it to a tooltip: hovering a colour
 * rings it in `--danger` and brings up a remove chip. The chip is drawn in
 * `--panel`, not in a colour worked out from the swatch underneath it — light
 * or dark text on an arbitrary background is a contrast decision, and that
 * decision lives in `pallet_color::contrast` in the backend. A second
 * implementation of it here is exactly the drift this codebase keeps out.
 */

import { el, svg } from "../dom";
import {
  filledRoles,
  mediumToggle,
  renderPreview,
  rolesFor,
  type PreviewActions,
} from "./preview";
import { onContextMenu } from "../menu";
import type { BuildState, ImportKind } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** Content width of the window, less the scroll area's padding. */
const CONTENT_WIDTH = 412;

/** The strip's height. Between the ramp's 44px and the swatch header's 138px:
 *  this is the subject of the screen, but it is not the only thing on it. */
const STRIP_HEIGHT = 104;

/** The width of the "pick more" panel at the end of the strip. */
const TAIL_WIDTH = 88;

/**
 * Narrowest a colour can be and still have its hex written under it.
 *
 * Seven characters of 9px DM Mono come to about 38px, so 44 leaves the gap a
 * column of labels needs to stay legible as separate values. Past that the
 * labels become slot numbers, as the ramp on Current does — the position is
 * still worth having when the value no longer fits.
 */
const HEX_MIN_WIDTH = 44;

/** Narrowest a colour can be and still carry a number. */
const INDEX_MIN_WIDTH = 11;

/**
 * Narrowest a colour can be and still hold the remove chip.
 *
 * Below this the hover ring and the tooltip carry it alone — a chip crushed
 * into a sliver would be less clear than no chip at all.
 */
const CHIP_MIN_WIDTH = 26;

export interface BuildActions {
  onPickNext: () => void;
  onSave: () => void;
  onRemove: (index: number) => void;
  onRename: (name: string) => void;
  /** Enter in the name field: save with whatever is there. */
  onSubmitName: () => void;
  onExport: (formatId: string) => void;
  /** A colour's export token changed. Raw text, parsed on the way out. */
  onToken: (index: number, text: string) => void;
  /** Go to the library to bring back palettes, or single colours. */
  onImport: (kind: ImportKind) => void;
}

export type BuildScreenActions = BuildActions & PreviewActions;

/**
 * One row of the token list: a swatch, a path field, and the hex.
 *
 * The field owns its own DOM and never re-renders, for the reason the search
 * bar gives at length: the screen re-renders on every keystroke elsewhere in
 * this app, and doing that to a text field destroys and rebuilds it mid-word,
 * losing the caret and occasionally a character.
 */
function tokenRow(
  hex: string,
  text: string,
  index: number,
  onToken: (index: number, text: string) => void,
): HTMLElement {
  const input = document.createElement("input");
  input.type = "text";
  input.value = text;
  input.placeholder = "name this colour";
  input.spellcheck = false;
  input.setAttribute(
    "style",
    "flex:1;min-width:0;border:0;outline:none;background:transparent;" +
      `font:400 11px/1 ${MONO};color:var(--ink);padding:0;` +
      "user-select:text;-webkit-user-select:text",
  );
  input.addEventListener("input", () => onToken(index, input.value));
  input.addEventListener("keydown", (event) => {
    // Digits switch tabs globally; inside a text field they are just digits.
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      input.blur();
    }
  });

  return el(
    "div",
    {
      class: "hv-row",
      style:
        "display:flex;align-items:center;gap:10px;padding:9px 11px;" +
        "background:var(--panel)",
    },
    [
      el("div", {
        style:
          `width:18px;height:18px;flex:none;border-radius:5px;background:${hex};` +
          "box-shadow:inset 0 0 0 1px rgba(var(--swatchInk),.12)",
      }),
      input,
      el("span", {
        style: `font:400 9.5px/1 ${MONO};color:var(--mute);flex:none`,
        text: hex,
      }),
    ],
  );
}

/**
 * The section heading from the Current screen: a label and a hairline rule.
 *
 * `trailing` is a note set in the same muted type as the label; `control` is
 * an element that takes its place, for a heading that does something as well
 * as naming something.
 */
function sectionHeading(
  label: string,
  trailing?: string,
  control?: HTMLElement,
): HTMLElement {
  return el("div", { style: "display:flex;align-items:center;gap:8px" }, [
    el("span", {
      style: `font:400 9.5px/1 ${MONO};letter-spacing:.14em;color:var(--mute)`,
      text: label,
    }),
    el("div", { style: "flex:1;height:1px;background:var(--line)" }),
    control ??
      (trailing
        ? el("span", {
            style: `font:400 9px/1 ${MONO};color:var(--mute);flex:none`,
            text: trailing,
          })
        : null),
  ]);
}

/** The plus on the strip's tail panel, drawn for the same reason as the gear. */
function plusIcon(): SVGElement {
  const icon = svg("svg", {
    viewBox: "0 0 16 16",
    width: "13",
    height: "13",
    fill: "none",
    stroke: "currentColor",
    "stroke-linecap": "round",
    style: "display:block",
  });
  icon.append(
    svg("path", { d: "M8 3.4 L8 12.6 M3.4 8 L12.6 8", "stroke-width": "1.6" }),
  );
  return icon;
}

/** The cross on a colour's remove chip. */
function crossIcon(): SVGElement {
  const icon = svg("svg", {
    viewBox: "0 0 16 16",
    width: "9",
    height: "9",
    fill: "none",
    stroke: "currentColor",
    "stroke-linecap": "round",
    style: "display:block",
  });
  icon.append(
    svg("path", {
      d: "M4.3 4.3 L11.7 11.7 M11.7 4.3 L4.3 11.7",
      "stroke-width": "2",
    }),
  );
  return icon;
}

export function renderBuild(
  build: BuildState,
  actions: BuildScreenActions,
): HTMLElement {
  const {
    colours,
    tokens,
    roles,
    medium,
    verdicts,
    choosingRole,
    name,
    suggested,
    needsName,
    capacity,
    min,
    picking,
    error,
    formats,
    exported,
  } = build;

  const full = colours.length >= capacity;
  // The tail is the whole strip while the palette is empty, and goes away
  // entirely once it is full.
  const tail = full ? 0 : colours.length === 0 ? CONTENT_WIDTH : TAIL_WIDTH;
  const each = colours.length > 0 ? (CONTENT_WIDTH - tail) / colours.length : 0;

  // --- the strip ---
  const swatches = colours.map((hex, i) =>
    el(
      "div",
      {
        // No radius and no gap: the segments are flush, and the container's own
        // rounding and ring are what the eye reads as the edge of the palette —
        // exactly how a saved palette is drawn on the Palettes tab.
        class: "pl-slot clickable",
        style:
          "flex:1;min-width:0;display:flex;align-items:center;" +
          `justify-content:center;background:${hex}`,
        title: `${hex}\nClick to remove`,
        onClick: () => actions.onRemove(i),
      },
      each >= CHIP_MIN_WIDTH
        ? [
            el(
              "div",
              {
                // Faded in by `.pl-slot:hover`. Present but invisible at rest
                // rather than added on hover, so it cannot arrive a frame late
                // under a pointer that is already moving along the strip.
                style:
                  "display:flex;align-items:center;justify-content:center;" +
                  "width:20px;height:20px;border-radius:6px;" +
                  "background:var(--panel);color:var(--danger);" +
                  "opacity:0;transition:opacity .14s ease",
              },
              [crossIcon()],
            ),
          ]
        : [],
    ),
  );

  const tailPanel = full
    ? null
    : el(
        "div",
        {
          class: "pl-tail clickable",
          style:
            `flex:none;width:${tail}px;display:flex;flex-direction:column;` +
            "align-items:center;justify-content:center;gap:6px;" +
            "background:var(--accentFaint);color:var(--accent)",
          // Left-click freezes the screen and picks; right-click offers the
          // library instead. Both are ways of putting a colour in the next
          // slot, so both live on the slot rather than in chrome elsewhere.
          title: "Click to pick from the screen\nRight-click to take from the library",
          onClick: picking ? undefined : actions.onPickNext,
        },
        [
          plusIcon(),
          el("span", {
            style: `font:500 9px/1 ${MONO};letter-spacing:.1em`,
            text: picking ? "PICKING…" : "PICK",
          }),
        ],
      );

  if (tailPanel) {
    onContextMenu(tailPanel, () => [
      {
        label: "Choose an existing palette",
        onSelect: () => actions.onImport("palette"),
      },
      {
        label: "Choose an existing colour",
        onSelect: () => actions.onImport("colour"),
      },
    ]);
  }

  const segments = [...swatches, tailPanel].filter(
    (c): c is HTMLElement => c !== null,
  );

  // The segments at each end take the container's own radius, so a hover ring
  // drawn on one follows the corner it sits in rather than cutting a square
  // one inside it. `overflow:hidden` already clips the colour to the right
  // shape — it is the ring, which is painted inside the segment, that needed
  // telling. A single segment, which is the empty palette's tail, is both ends.
  const first = segments[0];
  const last = segments[segments.length - 1];
  if (first) first.style.borderRadius = "var(--rad2) 0 0 var(--rad2)";
  if (last) {
    last.style.borderRadius =
      last === first ? "var(--rad2)" : "0 var(--rad2) var(--rad2) 0";
  }

  const strip = el(
    "div",
    {
      style:
        `display:flex;height:${STRIP_HEIGHT}px;border-radius:var(--rad2);` +
        "overflow:hidden;box-shadow:var(--lift),inset 0 0 0 1px rgba(var(--swatchInk),.09)",
    },
    segments,
  );

  // --- what is written under it ---
  // Hexes while they fit, slot numbers once they do not, nothing at all when
  // even a number would be a smear. The row keeps the strip's proportions so
  // each label stays under the colour it belongs to.
  const labelKind =
    each >= HEX_MIN_WIDTH ? "hex" : each >= INDEX_MIN_WIDTH ? "index" : "none";

  const labels =
    labelKind === "none" || colours.length === 0
      ? null
      : el(
          "div",
          { style: "display:flex" },
          [
            ...colours.map((hex, i) =>
              el("span", {
                style:
                  "flex:1;min-width:0;text-align:center;overflow:hidden;" +
                  `font:400 ${labelKind === "hex" ? "9" : "8"}px/1 ${MONO};` +
                  "color:var(--mute)",
                text: labelKind === "hex" ? hex : String(i + 1),
              }),
            ),
            tail > 0 ? el("span", { style: `flex:none;width:${tail}px` }) : null,
          ].filter((c): c is HTMLElement => c !== null),
        );

  // --- name and count ---
  const nameBar = el(
    "div",
    {
      // The library's search bar, reused: a pill rather than the bordered
      // rectangle this screen used to invent for itself, so the one place you
      // type on Build looks like the one place you type on Palettes.
      style:
        "display:flex;align-items:center;gap:8px;height:34px;padding:0 14px;" +
        "border-radius:999px;border:1px solid var(--line);" +
        "background:var(--panel);box-shadow:var(--lift)",
    },
    [
      el("span", {
        // Nudged down a pixel, as in the library bar: all-caps has no
        // descenders, so centring its box lifts it off the field's centre line.
        style:
          `font:400 10px/1 ${MONO};letter-spacing:.14em;color:var(--mute);` +
          "flex:none;position:relative;top:1px",
        text: "PALETTE",
      }),
      el("span", {
        style: `font:400 11.5px/1 ${SANS};color:var(--mute);flex:none`,
        text: ":",
      }),
      nameField(name, suggested, needsName, actions),
      el("span", {
        style: `font:400 10px/1 ${MONO};color:var(--mute);flex:none`,
        text: `${colours.length} / ${capacity}`,
      }),
    ],
  );
  if (needsName) {
    // Saving an unnamed palette lands here rather than in the library under
    // "Untitled 6", so the field has to be impossible to miss.
    nameBar.style.borderColor = "var(--accent)";
  }

  // --- buttons ---
  const canSave = colours.length >= min;
  const actionsRow = el("div", { style: "display:flex;gap:8px" }, [
    el("div", {
      // The one filled button on the screen, so it matches the filled tab and
      // the Clear pill rather than being a fourth kind of fill.
      style:
        "flex:1;padding:11px;text-align:center;border-radius:8px;" +
        (full
          ? "background:var(--hover);color:var(--mute);cursor:default;"
          : "background:var(--accent);color:var(--accentInk);cursor:pointer;") +
        `font:500 11.5px/1 ${SANS};letter-spacing:.03em`,
      // One press now gathers the rest of the palette in a single pass, so
      // "next colour" would undersell what the button does.
      text: full
        ? `Full (${capacity})`
        : picking
          ? "Picking…"
          : colours.length === 0
            ? "Pick colours"
            : "Pick more colours",
      onClick: full || picking ? undefined : actions.onPickNext,
    }),
    el("div", {
      class: canSave ? "hv-copy clickable" : undefined,
      style:
        "padding:11px 14px;text-align:center;border-radius:8px;" +
        "border-width:1px;border-style:solid;" +
        `font:500 11.5px/1 ${SANS};` +
        // While it can be saved, `.hv-copy` owns the colours so its hover rule
        // can win; while it cannot, they are stated here and stay put.
        (canSave
          ? "cursor:pointer;"
          : "color:var(--mute);border-color:var(--line);opacity:.45;cursor:default;"),
      text: needsName ? `Save as “${suggested}”` : "Save",
      title: canSave
        ? needsName
          ? "Type a name, or press Enter to keep the suggestion"
          : undefined
        : `Needs at least ${min} colours`,
      onClick: canSave ? actions.onSave : undefined,
    }),
  ]);

  // The minimum used to be stated by drawing empty slots for it. The strip no
  // longer has them, so it is said in words — and only while it is the thing
  // standing between the user and a saved palette.
  const shortfall =
    !error && !canSave && colours.length > 0
      ? `${min - colours.length} more to save`
      : null;
  const message = error ?? shortfall;

  const note = message
    ? el("div", {
        style:
          `font:400 10.5px/1.4 ${SANS};text-align:center;` +
          `color:${error ? "var(--danger)" : "var(--mute)"}`,
        text: message,
      })
    : null;

  // --- export ---
  // Two columns of tiles rather than a wrap of pills. Seven pills in a window
  // this narrow wrapped into a ragged block that ended halfway across the
  // screen and left the bottom third of the tab empty, and a pill reading
  // "Tailwind" never said that what you get is a `.js`. A tile has room to
  // name the format and the file it writes, and a grid of them is the one
  // thing on the screen that can be given the space there is.
  const ready = colours.length > 0;
  const tiles = el(
    "div",
    {
      style:
        "display:grid;grid-template-columns:1fr 1fr;gap:6px;" +
        // The odd seventh sits on its own row rather than being stretched
        // across both columns, which would single it out for no reason.
        "align-items:stretch",
    },
    formats.map((format) =>
      el(
        "div",
        {
          class: ready ? "hv-copy clickable" : undefined,
          style:
            "display:flex;align-items:center;gap:8px;padding:9px 11px;" +
            "border-radius:8px;border-width:1px;border-style:solid;" +
            (ready
              ? "cursor:pointer;"
              : "color:var(--mute);border-color:var(--line);opacity:.5;cursor:default"),
          // Exporting nothing would write an empty file; the tiles only come
          // alive once there is a palette to write.
          title: ready
            ? `Write ${name.trim() || suggested}.${format.extension}`
            : "Pick a colour first",
          onClick: ready ? () => actions.onExport(format.id) : undefined,
        },
        [
          el("span", {
            style: `flex:1;min-width:0;font:500 11px/1 ${SANS}`,
            text: format.label,
          }),
          el("span", {
            // The extension stays muted whatever the tile is doing: it is what
            // you get, not what you are choosing.
            style: `font:400 9px/1 ${MONO};color:var(--mute);flex:none`,
            text: `.${format.extension}`,
          }),
        ],
      ),
    ),
  );

  const exportSection = el(
    "div",
    { style: "display:flex;flex-direction:column;gap:9px" },
    [
      sectionHeading("EXPORT"),
      tiles,
      exported
        ? el("div", {
            // The written file's path, in a panel rather than as loose text
            // under the buttons.
            style:
              "padding:8px 10px;border-radius:8px;background:var(--panel);" +
              `border:1px solid var(--line);font:400 9.5px/1.45 ${MONO};` +
              "color:var(--mute);word-break:break-all",
            text: exported,
          })
        : null,
    ].filter((c): c is HTMLElement => c !== null),
  );

  const paletteSection = el(
    "div",
    { style: "display:flex;flex-direction:column;gap:9px" },
    [strip, labels].filter((c): c is HTMLElement => c !== null),
  );

  // --- tokens ---
  // What each colour is called when it is written out. Only once there is
  // something to name: an empty list under a heading is an instruction to do
  // work that cannot be done yet.
  const named = tokens.filter((t) => t.trim()).length;
  const tokenSection =
    colours.length === 0
      ? null
      : el(
          "div",
          { style: "display:flex;flex-direction:column;gap:9px" },
          [
            sectionHeading(
              "TOKENS",
              named > 0 ? `${named} of ${colours.length}` : undefined,
            ),
            el(
              "div",
              {
                // The grouped-row block from Current and Settings: 1px of
                // `--line` showing between rows, one lift on the container.
                style:
                  "display:flex;flex-direction:column;gap:1px;" +
                  "border-radius:var(--rad2);overflow:hidden;" +
                  "background:var(--line);box-shadow:var(--lift)",
              },
              colours.map((hex, i) =>
                tokenRow(hex, tokens[i] ?? "", i, actions.onToken),
              ),
            ),
            el("span", {
              style: `font:400 9.5px/1.45 ${MONO};color:var(--mute)`,
              text: "brand/primary groups it · primary on its own does not",
            }),
          ],
        );

  // --- preview ---
  // Between the tokens and the export, because it is the thing that decides
  // whether either is worth doing: what the palette looks like being used,
  // and whether the pairings it produces are actually readable.
  const preview = renderPreview(
    medium,
    colours,
    roles,
    verdicts,
    choosingRole,
    actions,
  );

  const previewSection =
    colours.length === 0
      ? null
      : el(
          "div",
          { style: "display:flex;flex-direction:column;gap:9px" },
          [
            // The heading carries the medium switch once one has been picked,
            // so changing it does not mean hunting for the chooser again.
            medium
              ? sectionHeading("PREVIEW", undefined, mediumToggle(medium, actions))
              : sectionHeading("PREVIEW", "what is this palette for?"),
            preview.scene,
            medium && preview.roles
              ? sectionHeading(
                  "ROLES",
                  `${filledRoles(medium, roles, colours)} of ${rolesFor(medium).length}`,
                )
              : null,
            preview.roles,
            preview.verdicts ? sectionHeading("CONTRAST") : null,
            preview.verdicts,
          ].filter((c): c is HTMLElement => c !== null),
        );

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:14px" },
    [
      nameBar,
      paletteSection,
      note,
      actionsRow,
      tokenSection,
      previewSection,
      exportSection,
    ].filter((c): c is HTMLElement => c !== null),
  );
}

/**
 * The palette name, with the auto-generated one showing through as the
 * placeholder.
 *
 * Left blank, Enter saves under the suggestion; typing replaces it. This is
 * the same bargain the Colours screen offers a freshly kept colour, so naming
 * is optional everywhere rather than a form to dismiss in one place.
 */
function nameField(
  name: string,
  suggested: string,
  needsName: boolean,
  actions: Pick<BuildActions, "onRename" | "onSubmitName">,
): HTMLElement {
  const input = document.createElement("input");
  input.type = "text";
  input.value = name;
  input.placeholder = suggested;
  input.spellcheck = false;
  input.setAttribute(
    "style",
    `flex:1;min-width:0;border:0;outline:none;background:transparent;` +
      `font:500 12px/1 ${SANS};color:var(--ink);padding:0;` +
      "user-select:text;-webkit-user-select:text",
  );
  input.addEventListener("input", () => actions.onRename(input.value));
  input.addEventListener("keydown", (event) => {
    // Digits switch tabs globally; inside a text field they are just digits.
    event.stopPropagation();
    if (event.key === "Enter") {
      event.preventDefault();
      actions.onSubmitName();
    }
  });

  // Focus has to wait for the element to be in the document, which happens
  // after this function returns.
  if (needsName) {
    requestAnimationFrame(() => {
      input.focus();
      input.select();
    });
  }
  return input;
}

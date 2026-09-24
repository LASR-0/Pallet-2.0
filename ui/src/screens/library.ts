/**
 * The Palettes and Colours screens.
 *
 * Style strings are verbatim from the PALETTES and COLOURS blocks of
 * `Prototype/package/Pallet Window.dc.html`.
 */

import { el, spacer } from "../dom";
import { onContextMenu } from "../menu";
import { renderFilters } from "./filters";
import { completionFor, filterByQuery, renderLibraryBar } from "./search";
import type { ColourChip, ImportState, PaletteCard } from "../state";

/**
 * What a library screen needs while Build is importing from it.
 *
 * Present only during an import; `null` the rest of the time, which is when
 * the cards do their ordinary jobs.
 */
export interface ImportActions {
  state: ImportState;
  /** Ctrl-click: add to or remove from the selection, staying here. */
  onToggle: (id: string) => void;
  /** Plain click: the selection is over. The clicked row joins it if new. */
  onFinish: (id: string) => void;
  onCancel: () => void;
}

/**
 * The bar that says the screen is being browsed on Build's behalf.
 *
 * Import mode changes what a click does, which is not something a screen may
 * do quietly — every card here already had a meaning for a click, and it has
 * been taken away for the moment.
 */
function importBanner(importing: ImportActions): HTMLElement {
  const count = importing.state.selected.length;
  const noun = importing.state.kind === "palette" ? "palette" : "colour";

  return el(
    "div",
    {
      style:
        "display:flex;align-items:center;gap:9px;padding:9px 12px;" +
        "border-radius:var(--rad2);background:var(--accentFaint);" +
        "border:1px solid var(--accent);box-shadow:var(--lift)",
    },
    [
      el("span", {
        style: `font:500 10px/1 ${MONO};letter-spacing:.1em;color:var(--accent);flex:none`,
        text: "IMPORT",
      }),
      el("span", {
        style: `flex:1;min-width:0;font:400 10px/1.35 ${SANS};color:var(--mute)`,
        text:
          count > 0
            ? `${count} ${noun}${count === 1 ? "" : "s"} chosen · click one more to finish`
            : `Click a ${noun} to take it · ctrl-click to choose several`,
      }),
      el("span", {
        class: "hv-copy clickable",
        style:
          `padding:4px 9px;border-radius:6px;font:400 9.5px/1 ${MONO};` +
          "border-width:1px;border-style:solid;flex:none",
        text: "Cancel",
        onClick: () => importing.onCancel(),
      }),
    ],
  );
}

/** What a click does to a card while an import is running. */
function importClick(importing: ImportActions, id: string) {
  return (event: MouseEvent) => {
    // Ctrl on Windows and Linux, Cmd on a Mac keyboard: both mean "and also
    // this one" everywhere else those platforms are used.
    if (event.ctrlKey || event.metaKey) importing.onToggle(id);
    else importing.onFinish(id);
  };
}

/** The ring that marks a card as chosen. */
function importRing(selected: boolean): string {
  return selected
    ? "box-shadow:var(--lift),0 0 0 2px var(--accent);"
    : "box-shadow:var(--lift);";
}

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/**
 * How long a single click waits to see whether a second one follows.
 *
 * A double-click delivers two `click` events before `dblclick`, so the single
 * click's work has to be held back or copying would fire twice on the way to
 * opening the colour. Windows' own double-click threshold defaults to 500ms,
 * but that is the ceiling for deliberate double-clicks; 240ms covers the ones
 * people actually perform while keeping a plain click from feeling laggy.
 */
const DOUBLE_CLICK_MS = 240;

/** How long a copied swatch shows that it was copied. */
const FLASH_MS = 420;

/**
 * Wire a swatch so a click copies its hex and a double-click opens it.
 *
 * Copy is the common case by a wide margin — a palette is a source of colours
 * to paste elsewhere — so it gets the single click, and the rarer act of
 * loading one into Current gets the deliberate one.
 *
 * Nothing on screen changes when a hex is copied, which would leave the click
 * looking ignored, so the swatch is briefly ringed to acknowledge it.
 */
function copyOrOpen(
  node: HTMLElement,
  hex: string,
  ring: string,
  onCopy: (hex: string) => void,
  onOpen: (hex: string) => void,
): void {
  let pending = 0;
  let flash = 0;

  node.addEventListener("click", () => {
    window.clearTimeout(pending);
    pending = window.setTimeout(() => {
      onCopy(hex);
      window.clearTimeout(flash);
      node.style.boxShadow = `inset 0 0 0 2px var(--accent)`;
      flash = window.setTimeout(() => {
        node.style.boxShadow = ring;
      }, FLASH_MS);
    }, DOUBLE_CLICK_MS);
  });

  node.addEventListener("dblclick", () => {
    window.clearTimeout(pending);
    onOpen(hex);
  });
}

/**
 * A name that becomes editable in place.
 *
 * Renaming inline rather than in a dialog keeps the swatch visible while you
 * type, which is the whole point of naming a colour.
 */
function editableName(
  value: string,
  style: string,
  onCommit: (name: string) => void,
): HTMLElement {
  const label = el("span", { style, text: value });

  label.addEventListener("pallet:edit", () => {
    const input = document.createElement("input");
    input.type = "text";
    input.value = value;
    input.spellcheck = false;
    input.setAttribute(
      "style",
      `${style};border:0;outline:none;background:transparent;padding:0;` +
        "min-width:0;width:100%;user-select:text;-webkit-user-select:text",
    );

    const finish = (save: boolean) => {
      const next = input.value.trim();
      input.replaceWith(label);
      if (save && next && next !== value) onCommit(next);
    };
    input.addEventListener("keydown", (event) => {
      event.stopPropagation();
      if (event.key === "Enter") finish(true);
      if (event.key === "Escape") finish(false);
    });
    input.addEventListener("blur", () => finish(true));

    label.replaceWith(input);
    input.focus();
    input.select();
  });

  return label;
}

/**
 * The name field for a colour that was just added.
 *
 * Empty, with the auto-suggested name showing through as the placeholder:
 * typing replaces it, Enter on an empty field accepts it. Focus lands here so
 * the keyboard is already in the right place after a pick.
 */
function newlyKeptName(
  colour: ColourChip,
  style: string,
  actions: {
    onRenameColour: (id: string, name: string) => void;
    onNamed: () => void;
  },
): HTMLElement {
  const input = document.createElement("input");
  input.type = "text";
  input.value = "";
  input.placeholder = colour.name;
  input.spellcheck = false;
  input.setAttribute(
    "style",
    `${style};border:0;outline:none;background:transparent;padding:0;` +
      "min-width:0;width:100%;color:var(--ink);" +
      "user-select:text;-webkit-user-select:text",
  );

  const finish = (save: boolean) => {
    const typed = input.value.trim();
    if (save && typed && typed !== colour.name) {
      actions.onRenameColour(colour.id, typed);
    }
    actions.onNamed();
  };

  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") finish(true);
    if (event.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));

  queueMicrotask(() => {
    input.focus();
    // The library reads oldest-first, so a colour just added sits at the end
    // of a scrolling list. Without this the field has focus somewhere off
    // screen and the user types into nothing they can see.
    input.closest("div")?.scrollIntoView({ block: "center" });
  });
  return input;
}

/** Ask a name rendered by `editableName` to become editable. */
function beginEdit(node: HTMLElement | null): void {
  node?.dispatchEvent(new CustomEvent("pallet:edit"));
}

/** Shown when the library has nothing in it yet. */
function empty(message: string): HTMLElement {
  return el("div", {
    style: `padding:24px 4px;font:400 11.5px/1.5 ${SANS};color:var(--mute);text-align:center`,
    text: message,
  });
}

export function renderPalettes(
  palettes: PaletteCard[] | null,
  query: string,
  onPickHex: (hex: string) => void,
  search: { onQuery: (value: string) => void },
  actions: {
    onRenamePalette: (id: string, name: string) => void;
    onDeletePalette: (id: string) => void;
    onCopy: (value: string) => void;
    facets: string[];
    sort: string;
    onToggleFacet: (id: string) => void;
    onClearFacets: () => void;
    onSort: (id: string) => void;
  },
  /** Set while Build is importing whole palettes from here. */
  importing?: ImportActions | null,
): HTMLElement {
  if (palettes === null) return empty("Loading…");

  const names = palettes.map((p) => p.name);
  const results = el("div", {
    style: "display:flex;flex-direction:column;gap:12px",
  });

  const paint = (q: string) => {
    const shown = filterByQuery(palettes, q, ["name"]);
    results.replaceChildren(
      ...(shown.length
        ? shown.map(card)
        : [empty(q ? `Nothing matches "${q}".` : "No palettes yet.")]),
    );
  };

  const card = (p: PaletteCard) => {
    const name = editableName(
      p.name,
      `font:500 12px/1 ${SANS}`,
      (next) => actions.onRenamePalette(p.id, next),
    );

    const chosen = importing?.state.selected.includes(p.id) ?? false;

    const node = el(
      "div",
      {
        class: "hv-card",
        style:
          "display:flex;flex-direction:column;gap:8px;padding:11px;border-radius:var(--rad2);" +
          "background:var(--panel);border:1px solid var(--line);" +
          importRing(chosen) +
          (importing ? "cursor:pointer" : ""),
        // While importing, the whole card is the target: the swatches inside
        // it stop copying, so there is no part of a palette that does
        // something other than choose it.
        onClick: importing ? importClick(importing, p.id) : undefined,
      },
      [
        el(
          "div",
          {
            style:
              "display:flex;border-radius:7px;overflow:hidden;height:72px;" +
              "box-shadow:inset 0 0 0 1px rgba(var(--swatchInk),.07)",
          },
          p.colors.map((hex) => {
            const swatch = el("div", {
              style: `flex:1;background:${hex}${importing ? "" : ";cursor:pointer"}`,
              // The tip has to teach this: a swatch that copies on click and
              // opens on double-click looks exactly like one that opens on
              // click, and there is nowhere else to say so.
              title: importing
                ? hex
                : `${hex}\nClick to copy · double-click to open`,
            });
            // No resting ring of its own — the strip around all of them
            // carries that — so the flash returns to nothing.
            if (!importing) {
              copyOrOpen(swatch, hex, "", actions.onCopy, onPickHex);
            }
            return swatch;
          }),
        ),
        el("div", { style: "display:flex;align-items:baseline;gap:7px" }, [
          el("span", {
            style: `font:400 9px/1 ${MONO};color:var(--mute)`,
            text: p.num,
          }),
          name,
          spacer(),
          el("span", {
            style: `font:400 9px/1 ${MONO};letter-spacing:.08em;color:var(--mute)`,
            text: p.meta,
          }),
        ]),
      ],
    );

    // No menu while importing: renaming or deleting a palette the user is in
    // the middle of choosing is not something they came here to do.
    if (!importing) {
      onContextMenu(node, () => [
        { label: "Rename", onSelect: () => beginEdit(name) },
        {
          label: "Delete palette",
          destructive: true,
          onSelect: () => actions.onDeletePalette(p.id),
        },
      ]);
    }
    return node;
  };

  paint(query);

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:12px" },
    [
      importing ? importBanner(importing) : null,
      renderLibraryBar(
        palettes.length,
        query,
        "Search palettes",
        {
          complete: (q) => completionFor(q, names),
          onQuery: (q) => {
            paint(q);
            search.onQuery(q);
          },
        },
      ),
      renderFilters("palettes", actions.facets, actions.sort, {
        onToggle: actions.onToggleFacet,
        onSort: actions.onSort,
        onClearFacets: actions.onClearFacets,
      }),
      results,
    ].filter((c): c is HTMLElement => c !== null),
  );
}

export function renderColours(
  colours: ColourChip[] | null,
  query: string,
  onPickHex: (hex: string) => void,
  search: { onQuery: (value: string) => void },
  actions: {
    onRenameColour: (id: string, name: string) => void;
    onDeleteColour: (id: string) => void;
    onCopy: (hex: string) => void;
    facets: string[];
    sort: string;
    onToggleFacet: (id: string) => void;
    onClearFacets: () => void;
    onSort: (id: string) => void;
    /** A colour just added, whose name should be open for editing. */
    naming: string | null;
    onNamed: () => void;
  },
  /** Set while Build is importing single colours from here. */
  importing?: ImportActions | null,
): HTMLElement {
  if (colours === null) return empty("Loading…");

  const names = colours.map((c) => c.name);
  const results = el("div", {});

  const chip = (c: ColourChip) => {
    const nameStyle =
      `font:500 10.5px/1.2 ${SANS};text-align:center;max-width:100%;` +
      "overflow:hidden;text-overflow:ellipsis;white-space:nowrap";

    // A colour that has just been kept opens straight into its name field,
    // with the suggested name as the placeholder. Leaving it blank keeps that
    // suggestion, so naming is optional rather than a form to dismiss.
    const name =
      actions.naming === c.id
        ? newlyKeptName(c, nameStyle, actions)
        : editableName(c.name, nameStyle, (next) =>
            actions.onRenameColour(c.id, next),
          );

    const chosen = importing?.state.selected.includes(c.id) ?? false;

    const node = el(
      "div",
      {
        class: "hv-card",
        style:
          "display:flex;flex-direction:column;align-items:center;gap:6px;" +
          "padding:12px 6px 10px;border-radius:var(--rad2);background:var(--panel);" +
          "border:1px solid var(--line);cursor:pointer;" +
          importRing(chosen),
        onClick: importing
          ? importClick(importing, c.id)
          : () => onPickHex(c.hex),
      },
      [
        el("div", {
          style:
            `width:46px;height:46px;border-radius:50%;background:${c.hex};` +
            "box-shadow:inset 0 0 0 1px rgba(var(--swatchInk),.08)",
        }),
        name,
        el("span", {
          style: `font:400 9px/1 ${MONO};color:var(--mute)`,
          text: c.hex,
        }),
      ],
    );

    // No menu while importing, as on the palette cards above.
    if (!importing) {
      onContextMenu(node, () => [
        { label: "Rename", onSelect: () => beginEdit(name) },
        { label: "Copy hex", onSelect: () => actions.onCopy(c.hex) },
        {
          label: "Delete colour",
          destructive: true,
          onSelect: () => actions.onDeleteColour(c.id),
        },
      ]);
    }
    return node;
  };

  const paint = (q: string) => {
    // Hex is searchable too: "#289" should find a colour by its value.
    const shown = filterByQuery(colours, q, ["name", "hex"]);
    results.replaceChildren(
      shown.length
        ? el(
            "div",
            {
              style: "display:grid;grid-template-columns:repeat(3,1fr);gap:10px",
            },
            shown.map(chip),
          )
        : empty(q ? `Nothing matches "${q}".` : "No named colours yet."),
    );
  };
  paint(query);

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:12px" },
    [
      importing ? importBanner(importing) : null,
      renderLibraryBar(
        colours.length,
        query,
        "Search colours",
        {
          complete: (q) => completionFor(q, names),
          onQuery: (q) => {
            paint(q);
            search.onQuery(q);
          },
        },
      ),
      renderFilters("colours", actions.facets, actions.sort, {
        onToggle: actions.onToggleFacet,
        onSort: actions.onSort,
        onClearFacets: actions.onClearFacets,
      }),
      results,
    ].filter((c): c is HTMLElement => c !== null),
  );
}

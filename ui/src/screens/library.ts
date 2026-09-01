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
import type { ColourChip, PaletteCard } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

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
    facets: string[];
    sort: string;
    onToggleFacet: (id: string) => void;
    onClearFacets: () => void;
    onSort: (id: string) => void;
  },
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

    const node = el(
      "div",
      {
        class: "hv-card",
        style:
          "display:flex;flex-direction:column;gap:8px;padding:11px;border-radius:var(--rad2);" +
          "background:var(--panel);border:1px solid var(--line)",
      },
      [
        el(
          "div",
          {
            style:
              "display:flex;border-radius:7px;overflow:hidden;height:72px;" +
              "box-shadow:inset 0 0 0 1px rgba(0,0,0,.07)",
          },
          p.colors.map((hex) =>
            el("div", {
              style: `flex:1;cursor:pointer;background:${hex}`,
              title: hex,
              onClick: () => onPickHex(hex),
            }),
          ),
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

    onContextMenu(node, () => [
      { label: "Rename", onSelect: () => beginEdit(name) },
      {
        label: "Delete palette",
        destructive: true,
        onSelect: () => actions.onDeletePalette(p.id),
      },
    ]);
    return node;
  };

  paint(query);

  return el("div", { style: "display:flex;flex-direction:column;gap:12px" }, [
    renderLibraryBar(
      "PALETTES LIBRARY",
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
  ]);
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

    const node = el(
      "div",
      {
        class: "hv-card",
        style:
          "display:flex;flex-direction:column;align-items:center;gap:6px;" +
          "padding:12px 6px 10px;border-radius:var(--rad2);background:var(--panel);" +
          "border:1px solid var(--line);cursor:pointer",
        onClick: () => onPickHex(c.hex),
      },
      [
        el("div", {
          style:
            `width:46px;height:46px;border-radius:50%;background:${c.hex};` +
            "box-shadow:inset 0 0 0 1px rgba(0,0,0,.08)",
        }),
        name,
        el("span", {
          style: `font:400 9px/1 ${MONO};color:var(--mute)`,
          text: c.hex,
        }),
      ],
    );

    onContextMenu(node, () => [
      { label: "Rename", onSelect: () => beginEdit(name) },
      { label: "Copy hex", onSelect: () => actions.onCopy(c.hex) },
      {
        label: "Delete colour",
        destructive: true,
        onSelect: () => actions.onDeleteColour(c.id),
      },
    ]);
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

  return el("div", { style: "display:flex;flex-direction:column;gap:12px" }, [
    renderLibraryBar(
      "COLOURS LIBRARY",
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
  ]);
}

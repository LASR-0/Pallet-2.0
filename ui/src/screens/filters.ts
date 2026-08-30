/**
 * Filter chips and sort selector for the library screens.
 *
 * Both reuse patterns the prototype already established rather than inventing
 * new ones: the chips are the EXPORT pills from the Build screen, and the sort
 * row is the harmony selector from Current.
 */

import { el } from "../dom";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** Facet chips, grouped as they filter: within a group, alternatives. */
export const FACET_GROUPS: [string, string][][] = [
  [
    ["warm", "Warm"],
    ["cool", "Cool"],
    ["neutral", "Neutral"],
  ],
  [
    ["light", "Light"],
    ["mid", "Mid"],
    ["dark", "Dark"],
  ],
  [
    ["vivid", "Vivid"],
    ["muted", "Muted"],
  ],
];

export const SORTS: [string, string][] = [
  ["added", "Added"],
  ["hue", "Hue"],
  ["lightness", "Light"],
  ["chroma", "Chroma"],
  ["name", "Name"],
];

export function renderFilters(
  active: string[],
  sort: string,
  actions: { onToggle: (id: string) => void; onSort: (id: string) => void },
): HTMLElement {
  const chips = FACET_GROUPS.flat().map(([id, label]) => {
    const on = active.includes(id);
    return el("span", {
      // The EXPORT pill from the Build screen, lit when selected.
      style:
        "padding:6px 10px;border-radius:999px;" +
        `font:400 10px/1 ${MONO};cursor:pointer;` +
        (on
          ? "border:1px solid var(--accent);color:var(--accent);"
          : "border:1px solid var(--line);color:var(--mute);"),
      text: label,
      onClick: () => actions.onToggle(id),
    });
  });

  const sorts = SORTS.map(([id, label]) => {
    const on = sort === id;
    // The harmony selector from the Current screen.
    return el("div", {
      style:
        "flex:1;text-align:center;padding:6px 4px;border-radius:6px;cursor:pointer;" +
        `font:${on ? "600" : "400"} 10px/1 ${SANS};` +
        (on ? "background:var(--hover);color:var(--ink);" : "color:var(--mute);") +
        `border:1px solid ${on ? "var(--line)" : "transparent"}`,
      text: label,
      onClick: () => actions.onSort(id),
    });
  });

  return el("div", { style: "display:flex;flex-direction:column;gap:8px" }, [
    el("div", { style: "display:flex;gap:6px;flex-wrap:wrap" }, chips),
    el("div", { style: "display:flex;gap:4px" }, sorts),
  ]);
}

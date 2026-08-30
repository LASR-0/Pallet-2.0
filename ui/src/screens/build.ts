/**
 * The Build screen: gather colours by picking, then save them as a palette.
 *
 * Style strings are verbatim from the BUILD block of
 * `Prototype/package/Pallet Window.dc.html`, with one addition the prototype
 * did not have to solve: it shows a fixed five slots, while a palette here
 * holds three to twenty-five. At the wide end each slot is a few pixels across,
 * so the vertical label is dropped rather than squeezed.
 */

import { el } from "../dom";
import type { BuildState } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** Content width of the window, less the scroll area's padding. */
const CONTENT_WIDTH = 412;
const SLOT_GAP = 8;

/** Narrowest slot that can still carry a rotated hex label. */
const LABEL_MIN_WIDTH = 20;

/** The formats the prototype offers. Wired up in M9. */
const EXPORTS = ["CSS vars", "Tailwind", "ASE", "JSON", "PNG"];

function slotWidth(count: number): number {
  if (count <= 0) return CONTENT_WIDTH;
  return (CONTENT_WIDTH - SLOT_GAP * (count - 1)) / count;
}

export interface BuildActions {
  onPickNext: () => void;
  onSave: () => void;
  onRemove: (index: number) => void;
  onRename: (name: string) => void;
}

export function renderBuild(
  build: BuildState,
  actions: BuildActions,
): HTMLElement {
  const { colours, name, capacity, min, picking, error } = build;

  // One slot per colour, plus the next one to fill, up to capacity.
  const slotCount = Math.min(Math.max(colours.length + 1, min), capacity);
  const showLabels = slotWidth(slotCount) >= LABEL_MIN_WIDTH;

  const slots = Array.from({ length: slotCount }, (_, i) => {
    const hex = colours[i];
    const isNext = i === colours.length;

    const base =
      "flex:1;min-width:0;display:flex;align-items:flex-end;justify-content:center;" +
      "padding-bottom:10px;border-radius:var(--rad2);cursor:pointer;";
    const style = hex
      ? `${base}background:${hex};box-shadow:inset 0 0 0 1px rgba(0,0,0,.1);`
      : `${base}border:1px dashed ${isNext ? "var(--accent)" : "var(--line)"};`;

    const label = hex
      ? hex
      : isNext
        ? picking
          ? "PICKING…"
          : "NEXT"
        : "EMPTY";

    return el(
      "div",
      {
        style,
        title: hex ? `${hex} — click to remove` : undefined,
        onClick: hex ? () => actions.onRemove(i) : actions.onPickNext,
      },
      showLabels
        ? [
            el("span", {
              style:
                `font:500 9px/1 ${MONO};letter-spacing:.04em;writing-mode:vertical-rl;` +
                `color:${hex ? "rgba(255,255,255,.85)" : "var(--mute)"}`,
              text: label,
            }),
          ]
        : [],
    );
  });

  const header = el(
    "div",
    {
      style:
        "display:flex;align-items:center;gap:9px;padding:10px 12px;border-radius:8px;" +
        "border:1px solid var(--line);background:var(--panel)",
    },
    [
      el("span", {
        style: `font:400 10px/1 ${MONO};letter-spacing:.14em;color:var(--mute)`,
        text: "PALETTE :",
      }),
      nameField(name, actions.onRename),
    ],
  );

  const canSave = colours.length >= min;
  const actionsRow = el("div", { style: "display:flex;gap:8px" }, [
    el("div", {
      style:
        "flex:1;padding:11px;text-align:center;border-radius:8px;" +
        (colours.length >= capacity
          ? "background:var(--hover);color:var(--mute);cursor:default;"
          : "background:var(--accent);color:#fff;cursor:pointer;") +
        `font:500 11.5px/1 ${SANS};letter-spacing:.03em`,
      text:
        colours.length >= capacity
          ? `Full (${capacity})`
          : picking
            ? "Picking…"
            : "Pick next colour",
      onClick:
        colours.length >= capacity || picking ? undefined : actions.onPickNext,
    }),
    el("div", {
      class: canSave ? "hv-copy clickable" : undefined,
      style:
        "padding:11px 14px;text-align:center;border-radius:8px;" +
        "border:1px solid var(--line);" +
        `font:500 11.5px/1 ${SANS};` +
        (canSave
          ? "color:var(--mute);cursor:pointer;"
          : "color:var(--mute);opacity:.45;cursor:default;"),
      text: "Save",
      title: canSave ? undefined : `Needs at least ${min} colours`,
      onClick: canSave ? actions.onSave : undefined,
    }),
  ]);

  const exportRow = el(
    "div",
    { style: "display:flex;flex-direction:column;gap:7px;padding-top:2px" },
    [
      el("span", {
        style: `font:400 9.5px/1 ${MONO};letter-spacing:.14em;color:var(--mute)`,
        text: "EXPORT",
      }),
      el(
        "div",
        { style: "display:flex;gap:6px;flex-wrap:wrap" },
        EXPORTS.map((name) =>
          el("span", {
            style:
              "padding:6px 10px;border-radius:999px;border:1px solid var(--line);" +
              `font:400 10px/1 ${MONO};color:var(--mute);opacity:.5;cursor:default`,
            text: name,
            title: "Exports land in M9",
          }),
        ),
      ),
    ],
  );

  const children: (HTMLElement | null)[] = [
    header,
    el("div", { style: "display:flex;gap:8px;height:180px" }, slots),
    error
      ? el("div", {
          style: `font:400 10.5px/1.4 ${SANS};color:var(--accent);text-align:center`,
          text: error,
        })
      : null,
    actionsRow,
    exportRow,
  ];

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:14px" },
    children.filter((c): c is HTMLElement => c !== null),
  );
}

/** The palette name, editable in place. */
function nameField(name: string, onRename: (value: string) => void): HTMLElement {
  const input = document.createElement("input");
  input.type = "text";
  input.value = name;
  input.spellcheck = false;
  input.setAttribute(
    "style",
    `flex:1;min-width:0;border:0;outline:none;background:transparent;` +
      `font:500 12px/1 ${SANS};color:var(--ink);padding:0;` +
      "user-select:text;-webkit-user-select:text",
  );
  input.addEventListener("input", () => onRename(input.value));
  // Digits switch tabs globally; inside a text field they are just digits.
  input.addEventListener("keydown", (event) => event.stopPropagation());
  return input;
}

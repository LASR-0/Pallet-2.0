/**
 * The Palettes and Colours screens.
 *
 * Style strings are verbatim from the PALETTES and COLOURS blocks of
 * `Prototype/package/Pallet Window.dc.html`.
 */

import { el, spacer } from "../dom";
import type { ColourChip, PaletteCard } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** The "LIBRARY : X ... n" bar both screens open with. */
function libraryBar(label: string, count: number): HTMLElement {
  return el(
    "div",
    {
      style:
        "display:flex;align-items:center;gap:9px;padding:10px 12px;border-radius:8px;" +
        "border:1px solid var(--line);background:var(--panel)",
    },
    [
      el("span", {
        style: `font:400 10px/1 ${MONO};letter-spacing:.14em;color:var(--mute)`,
        text: label,
      }),
      spacer(),
      el("span", {
        style: `font:400 10px/1 ${MONO};color:var(--mute)`,
        text: String(count),
      }),
    ],
  );
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
  onPickHex: (hex: string) => void,
): HTMLElement {
  if (palettes === null) return empty("Loading…");

  const cards = palettes.map((p) =>
    el(
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
          el("span", { style: `font:500 12px/1 ${SANS}`, text: p.name }),
          spacer(),
          el("span", {
            style: `font:400 9px/1 ${MONO};letter-spacing:.08em;color:var(--mute)`,
            text: p.meta,
          }),
        ]),
      ],
    ),
  );

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:12px" },
    [
      libraryBar("LIBRARY : PALETTES", palettes.length),
      ...(palettes.length ? cards : [empty("No palettes yet.")]),
    ],
  );
}

export function renderColours(
  colours: ColourChip[] | null,
  onPickHex: (hex: string) => void,
): HTMLElement {
  if (colours === null) return empty("Loading…");

  const grid = el(
    "div",
    { style: "display:grid;grid-template-columns:repeat(3,1fr);gap:10px" },
    colours.map((c) =>
      el(
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
          el("span", {
            style:
              `font:500 10.5px/1.2 ${SANS};text-align:center;max-width:100%;` +
              "overflow:hidden;text-overflow:ellipsis;white-space:nowrap",
            text: c.name,
          }),
          el("span", {
            style: `font:400 9px/1 ${MONO};color:var(--mute)`,
            text: c.hex,
          }),
        ],
      ),
    ),
  );

  return el("div", { style: "display:flex;flex-direction:column;gap:12px" }, [
    libraryBar("LIBRARY : COLOURS", colours.length),
    colours.length ? grid : empty("No named colours yet."),
  ]);
}

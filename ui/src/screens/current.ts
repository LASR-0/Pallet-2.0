/**
 * The Current screen.
 *
 * Every style string below is taken verbatim from the CURRENT block of
 * `Prototype/package/Pallet Window.dc.html`, so the two can be diffed.
 */

import { el, spacer } from "../dom";
import { HARMONIES, type AppState, type Harmony } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** The prototype's section heading: a label and a hairline rule. */
function sectionHeading(label: string): HTMLElement {
  return el("div", { style: "display:flex;align-items:center;gap:8px" }, [
    el("span", {
      style: `font:400 9.5px/1 ${MONO};letter-spacing:.14em;color:var(--mute)`,
      text: label,
    }),
    el("div", { style: "flex:1;height:1px;background:var(--line)" }),
  ]);
}

export function renderCurrent(
  state: AppState,
  actions: {
    onCopy: (value: string) => void;
    onPickHex: (hex: string) => void;
    onHarmony: (harmony: Harmony) => void;
  },
): HTMLElement {
  const detail = state.detail;
  if (!detail) {
    return el("div", {
      style: `padding:24px 4px;font:400 12px/1.5 ${SANS};color:var(--mute);text-align:center`,
      text: "No colour yet. Pick one to begin.",
    });
  }

  // --- swatch header ---
  const header = el(
    "div",
    {
      style:
        "position:relative;height:138px;border-radius:var(--rad2);overflow:hidden;" +
        `background:${detail.hex};box-shadow:inset 0 0 0 1px rgba(0,0,0,.09)`,
    },
    [
      el(
        "div",
        {
          style:
            "position:absolute;left:14px;bottom:12px;display:flex;flex-direction:column;" +
            `gap:3px;color:${detail.onColor}`,
        },
        [
          el("span", {
            style: `font:400 9.5px/1 ${MONO};letter-spacing:.16em;opacity:.65`,
            text: "CURRENT",
          }),
          el("span", {
            style: `font:600 26px/1 ${MONO};letter-spacing:.01em`,
            text: detail.hex,
          }),
        ],
      ),
      el("div", {
        style:
          "position:absolute;right:12px;top:12px;padding:5px 9px;border-radius:7px;" +
          `font:500 10px/1 ${MONO};letter-spacing:.06em;background:rgba(255,255,255,.22);` +
          `backdrop-filter:blur(6px);color:${detail.onColor}`,
        text: detail.name,
      }),
    ],
  );

  // --- HEX / RGB / HSL rows ---
  const codeBlock = el(
    "div",
    {
      style:
        "display:flex;flex-direction:column;gap:1px;border-radius:var(--rad2);" +
        "overflow:hidden;background:var(--line)",
    },
    detail.codeRows.map((row) =>
      el(
        "div",
        {
          class: "hv-row",
          style:
            "display:flex;align-items:center;gap:10px;padding:10px 12px;background:var(--panel)",
        },
        [
          el("span", {
            style: `width:34px;font:400 9.5px/1 ${MONO};letter-spacing:.12em;color:var(--mute)`,
            text: row.label,
          }),
          el("span", {
            style: `flex:1;font:500 12.5px/1 ${MONO};letter-spacing:.03em`,
            text: row.value,
          }),
          el("span", {
            class: "hv-copy clickable",
            style:
              `padding:4px 8px;border-radius:6px;font:400 9.5px/1 ${MONO};` +
              "letter-spacing:.08em;color:var(--mute);border:1px solid var(--line)",
            text: "COPY",
            onClick: () => actions.onCopy(row.value),
          }),
        ],
      ),
    ),
  );

  // --- harmony ---
  const harmonyButtons = el(
    "div",
    { style: "display:flex;gap:4px" },
    HARMONIES.map(([id, label]) => {
      const on = state.harmony === id;
      return el("div", {
        style:
          "flex:1;text-align:center;padding:6px 4px;border-radius:6px;cursor:pointer;" +
          `font:${on ? "600" : "400"} 10px/1 ${SANS};` +
          (on
            ? "background:var(--hover);color:var(--ink);"
            : "color:var(--mute);") +
          `border:1px solid ${on ? "var(--line)" : "transparent"}`,
        text: label,
        onClick: () => actions.onHarmony(id),
      });
    }),
  );

  const harmonySwatches = el(
    "div",
    { style: "display:flex;gap:7px" },
    detail.harmony.map((hex) =>
      el(
        "div",
        {
          style:
            "flex:1;display:flex;flex-direction:column;gap:5px;cursor:pointer",
          onClick: () => actions.onPickHex(hex),
        },
        [
          el("div", {
            style: `height:56px;border-radius:8px;background:${hex};box-shadow:inset 0 0 0 1px rgba(0,0,0,.09)`,
          }),
          el("span", {
            style: `font:400 9px/1 ${MONO};letter-spacing:.02em;color:var(--mute);text-align:center`,
            text: hex,
          }),
        ],
      ),
    ),
  );

  const harmonySection = el(
    "div",
    { style: "display:flex;flex-direction:column;gap:9px" },
    [sectionHeading("HARMONY"), harmonyButtons, harmonySwatches],
  );

  // --- tints and shades ---
  const rampBar = el(
    "div",
    {
      style:
        "display:flex;border-radius:8px;overflow:hidden;box-shadow:inset 0 0 0 1px rgba(0,0,0,.09)",
    },
    detail.ramp.map((step) =>
      el("div", {
        style: `flex:1;height:44px;cursor:pointer;background:${step.hex}`,
        onClick: () => actions.onPickHex(step.hex),
      }),
    ),
  );

  const rampLabels = el(
    "div",
    { style: "display:flex" },
    detail.ramp.map((step) =>
      el("span", {
        style: `flex:1;text-align:center;font:400 8px/1 ${MONO};color:var(--mute)`,
        text: String(step.step),
      }),
    ),
  );

  const rampSection = el(
    "div",
    { style: "display:flex;flex-direction:column;gap:9px" },
    [sectionHeading("TINTS & SHADES"), rampBar, rampLabels],
  );

  return el("div", { style: "display:flex;flex-direction:column;gap:14px" }, [
    header,
    codeBlock,
    harmonySection,
    rampSection,
  ]);
}

export { spacer };

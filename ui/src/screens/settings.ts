/**
 * The Settings screen.
 *
 * Style strings are verbatim from the SETTINGS block of
 * `Prototype/package/Pallet Window.dc.html`. The prototype draws six rows; the
 * four after them are settings that exist in config.toml and would otherwise
 * only be reachable by editing the file by hand.
 */

import { el } from "../dom";
import type { SettingRow } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

export function renderSettings(
  rows: SettingRow[] | null,
  onCycle: (key: string) => void,
): HTMLElement {
  if (rows === null) {
    return el("div", {
      style: `padding:24px 4px;font:400 11.5px/1.5 ${SANS};color:var(--mute);text-align:center`,
      text: "Loading…",
    });
  }

  return el(
    "div",
    {
      style:
        "display:flex;flex-direction:column;gap:1px;border-radius:var(--rad2);" +
        "overflow:hidden;background:var(--line)",
    },
    rows.map((row) =>
      el(
        "div",
        {
          style:
            "display:flex;align-items:center;gap:10px;padding:12px 12px;background:var(--panel)",
        },
        [
          el(
            "div",
            { style: "flex:1;display:flex;flex-direction:column;gap:3px" },
            [
              el("span", { style: `font:500 12px/1 ${SANS}`, text: row.label }),
              el("span", {
                style: `font:400 10px/1.3 ${SANS};color:var(--mute)`,
                text: row.hint,
              }),
            ],
          ),
          el("span", {
            class: row.editable ? "clickable" : undefined,
            style:
              `padding:5px 9px;border-radius:6px;font:500 9.5px/1 ${MONO};` +
              "letter-spacing:.07em;" +
              (row.on
                ? "background:var(--accent);color:#fff;"
                : "background:var(--hover);color:var(--mute);") +
              (row.editable ? "" : "opacity:.6;cursor:default"),
            text: row.value,
            title: row.editable ? "Click to change" : undefined,
            onClick: row.editable ? () => onCycle(row.key) : undefined,
          }),
        ],
      ),
    ),
  );
}

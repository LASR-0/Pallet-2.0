/**
 * The Settings screen.
 *
 * Style strings are verbatim from the SETTINGS block of
 * `Prototype/package/Pallet Window.dc.html`. The prototype draws six rows; the
 * four after them are settings that exist in config.toml and would otherwise
 * only be reachable by editing the file by hand.
 */

import { el } from "../dom";
import type { Binding, SettingRow } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** The section heading used between groups of rows. */
function heading(label: string): HTMLElement {
  return el(
    "div",
    { style: "display:flex;align-items:center;gap:8px;padding:2px 0" },
    [
      el("span", {
        style: `font:400 9.5px/1 ${MONO};letter-spacing:.14em;color:var(--mute)`,
        text: label,
      }),
      el("div", { style: "flex:1;height:1px;background:var(--line)" }),
    ],
  );
}

/** The rounded list the rows sit in. */
function group(children: HTMLElement[]): HTMLElement {
  return el(
    "div",
    {
      style:
        "display:flex;flex-direction:column;gap:1px;border-radius:var(--rad2);" +
        "overflow:hidden;background:var(--line)",
    },
    children,
  );
}

/** One row: label, hint, and a control on the right. */
function settingRow(
  label: string,
  hint: string,
  control: HTMLElement,
): HTMLElement {
  return el(
    "div",
    {
      style:
        "display:flex;align-items:center;gap:10px;padding:12px 12px;background:var(--panel)",
    },
    [
      el("div", { style: "flex:1;display:flex;flex-direction:column;gap:3px" }, [
        el("span", { style: `font:500 12px/1 ${SANS}`, text: label }),
        el("span", {
          style: `font:400 10px/1.3 ${SANS};color:var(--mute)`,
          text: hint,
        }),
      ]),
      control,
    ],
  );
}

export function renderSettings(
  rows: SettingRow[] | null,
  bindings: Binding[] | null,
  capturing: string | null,
  actions: {
    onCycle: (key: string) => void;
    onCapture: (key: string) => void;
  },
): HTMLElement {
  if (rows === null) {
    return el("div", {
      style: `padding:24px 4px;font:400 11.5px/1.5 ${SANS};color:var(--mute);text-align:center`,
      text: "Loading…",
    });
  }

  const pill = (text: string, on: boolean, editable: boolean, onClick?: () => void) =>
    el("span", {
      class: editable ? "clickable" : undefined,
      style:
        `padding:5px 9px;border-radius:6px;font:500 9.5px/1 ${MONO};` +
        "letter-spacing:.07em;white-space:nowrap;" +
        (on
          ? "background:var(--accent);color:#fff;"
          : "background:var(--hover);color:var(--mute);") +
        (editable ? "" : "opacity:.6;cursor:default"),
      text,
      title: editable ? "Click to change" : undefined,
      onClick,
    });

  const general = group(
    rows.map((row) =>
      settingRow(
        row.label,
        row.hint,
        pill(row.value, row.on, row.editable, () =>
          row.editable ? actions.onCycle(row.key) : undefined,
        ),
      ),
    ),
  );

  const children: HTMLElement[] = [general];

  if (bindings && bindings.length) {
    children.push(heading("KEY BINDINGS"));
    children.push(
      group(
        bindings.map((binding) => {
          const active = capturing === binding.key;
          return settingRow(
            binding.label,
            binding.hint,
            pill(active ? "PRESS KEYS…" : binding.combo, active, true, () =>
              actions.onCapture(binding.key),
            ),
          );
        }),
      ),
    );
    children.push(
      el("span", {
        style: `font:400 10px/1.4 ${SANS};color:var(--mute);padding:0 2px`,
        text:
          "The global shortcut above is not one of these: no Wayland application can " +
          "grab a key while it is unfocused, so your compositor has to own it. " +
          "Run `pallet hotkey` for the exact line.",
      }),
    );
  }

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:12px" },
    children,
  );
}

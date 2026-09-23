/**
 * The Settings screen.
 *
 * Style strings are verbatim from the SETTINGS block of
 * `Prototype/package/Pallet Window.dc.html`. The prototype draws six rows; the
 * four after them are settings that exist in config.toml and would otherwise
 * only be reachable by editing the file by hand.
 */

import { el } from "../dom";
import type { Binding, SettingRow, ShareState } from "../state";

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
      // The lift goes on the group rather than on each row, for the same
      // reason as the code block on Current: a shadow per row would fill the
      // 1px seams between them.
      style:
        "display:flex;flex-direction:column;gap:1px;border-radius:var(--rad2);" +
        "overflow:hidden;background:var(--line);box-shadow:var(--lift)",
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

/** A small filled button, for the two things the sharing panel does. */
function button(
  label: string,
  enabled: boolean,
  onClick: () => void,
): HTMLElement {
  return el("span", {
    class: enabled ? "clickable" : undefined,
    style:
      `padding:6px 11px;border-radius:7px;font:500 9.5px/1 ${MONO};` +
      "letter-spacing:.07em;white-space:nowrap;" +
      (enabled
        ? "background:var(--accent);color:var(--accentInk);"
        : "background:var(--hover);color:var(--mute);cursor:default;opacity:.6"),
    text: label,
    onClick: enabled ? onClick : undefined,
  });
}

/**
 * Sharing a library, and taking one in.
 *
 * Both directions go through one folder, which is named here in full and
 * copyable. There is no file dialog in this window — `export_palette` explains
 * why — so the folder *is* the interface, and a user who cannot find it cannot
 * use the feature at all. Dropping a file on the window does the same thing
 * without the detour, and is mentioned right where it is relevant rather than
 * left to be discovered.
 */
function sharing(
  share: ShareState,
  actions: {
    onShare: () => void;
    onImport: (path: string) => void;
    onCopyPath: (path: string) => void;
  },
): HTMLElement[] {
  const children: HTMLElement[] = [heading("SHARING")];

  children.push(
    group([
      settingRow(
        "Share this library",
        "Writes a file holding your colours and palettes, to send to anyone " +
          "else running Pallet.",
        button("EXPORT", !share.busy, actions.onShare),
      ),
      settingRow(
        "Shared folder",
        share.dir ?? "…",
        button("COPY", share.dir !== null, () =>
          actions.onCopyPath(share.dir ?? ""),
        ),
      ),
    ]),
  );

  // The outcome of the last thing tried. In place rather than as a toast: it
  // is a count the user may want to read twice, and it belongs next to the
  // button that produced it.
  if (share.notice) {
    children.push(
      el("span", {
        style:
          `font:400 10.5px/1.45 ${SANS};color:var(--ink);padding:0 2px;` +
          "background:var(--accentFaint);border-radius:8px;padding:8px 10px",
        text: share.notice,
      }),
    );
  }

  if (share.files === null) {
    children.push(
      el("span", {
        style: `font:400 10px/1.4 ${SANS};color:var(--mute);padding:0 2px`,
        text: "Looking…",
      }),
    );
  } else if (share.files.length === 0) {
    children.push(
      el("span", {
        style: `font:400 10px/1.4 ${SANS};color:var(--mute);padding:0 2px`,
        text:
          "No shared libraries here yet. Drop one on this window, or put it " +
          "in the folder above and come back.",
      }),
    );
  } else {
    children.push(
      group(
        share.files.map((file) =>
          settingRow(
            file.name,
            `${Math.max(1, Math.round(file.size / 1024))} KB · adds what you ` +
              "do not already have, changes nothing you do",
            button("IMPORT", !share.busy, () => actions.onImport(file.path)),
          ),
        ),
      ),
    );
  }

  return children;
}

export function renderSettings(
  rows: SettingRow[] | null,
  bindings: Binding[] | null,
  capturing: string | null,
  share: ShareState,
  actions: {
    onCycle: (key: string) => void;
    onCapture: (key: string) => void;
    onShare: () => void;
    onImport: (path: string) => void;
    onCopyPath: (path: string) => void;
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
          ? "background:var(--accent);color:var(--accentInk);"
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

  children.push(...sharing(share, actions));

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

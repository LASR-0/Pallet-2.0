/**
 * The Pick screen: how to start a pick, and what has been picked recently.
 *
 * Style strings are verbatim from the PICK block of
 * `Prototype/package/Pallet Window.dc.html`.
 */

import { el } from "../dom";
import { onContextMenu } from "../menu";
import type { RecentPick } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** The colour-wheel mark with its crosshair, as the prototype draws it. */
function wheel(): HTMLElement {
  return el(
    "div",
    {
      style:
        "position:relative;width:64px;height:64px;border-radius:50%;" +
        "background:conic-gradient(from 0deg,#E8564C,#E0BC67,#289788,#3B5F86,#6E5A78,#E8564C);" +
        "box-shadow:inset 0 0 0 6px var(--panel)",
    },
    [
      el("div", {
        style:
          "position:absolute;left:50%;top:50%;width:15px;height:15px;" +
          "transform:translate(-50%,-50%);box-shadow:0 0 0 1.5px var(--ink)",
      }),
    ],
  );
}

export interface PickActions {
  onPick: () => void;
  onUseHex: (hex: string) => void;
  onSaveHex: (hex: string) => void;
  onCopy: (hex: string) => void;
}

export function renderPick(
  recents: RecentPick[] | null,
  shortcut: string,
  picking: boolean,
  actions: PickActions,
): HTMLElement {
  const invite = el(
    "div",
    {
      class: "hv-card clickable",
      style:
        "display:flex;flex-direction:column;align-items:center;gap:12px;" +
        "padding:30px 16px;border-radius:var(--rad2);background:var(--panel);" +
        "border:1px dashed var(--line)",
      onClick: actions.onPick,
    },
    [
      wheel(),
      el(
        "div",
        {
          style:
            "display:flex;flex-direction:column;align-items:center;gap:4px",
        },
        [
          el("span", {
            style: `font:600 14px/1 ${SANS}`,
            text: picking ? "Picking…" : "Freeze the screen and pick",
          }),
          el("span", {
            style: `font:400 11px/1.4 ${SANS};color:var(--mute);text-align:center`,
            // The prototype uses a <br>; the same break without raw markup.
            text:
              "The loupe magnifies 16× with a pixel grid.\nHold Shift to average a 5×5 area. S keeps it.",
          }),
        ],
      ),
      el("span", {
        style:
          "padding:6px 11px;border-radius:7px;background:var(--hover);" +
          `font:500 10px/1 ${MONO};letter-spacing:.08em;color:var(--mute)`,
        text: shortcut,
      }),
    ],
  );
  // The description is the one place the design wants a hard line break.
  invite.querySelectorAll("span").forEach((span) => {
    if (span.textContent?.includes("\n")) span.style.whiteSpace = "pre-line";
  });

  const heading = el(
    "div",
    { style: "display:flex;align-items:center;gap:8px" },
    [
      el("span", {
        style: `font:400 9.5px/1 ${MONO};letter-spacing:.14em;color:var(--mute)`,
        text: "RECENT PICKS",
      }),
      el("div", { style: "flex:1;height:1px;background:var(--line)" }),
    ],
  );

  const swatches =
    recents === null
      ? [
          el("span", {
            style: `font:400 11px/1 ${SANS};color:var(--mute)`,
            text: "Loading…",
          }),
        ]
      : recents.length === 0
        ? [
            el("span", {
              style: `font:400 11px/1.4 ${SANS};color:var(--mute)`,
              text: "Nothing picked yet.",
            }),
          ]
        : recents.map((r) => {
            const node = el("div", {
              style:
                `aspect-ratio:1;border-radius:7px;cursor:pointer;background:${r.hex};` +
                "box-shadow:inset 0 0 0 1px rgba(0,0,0,.08)",
              title: r.hex,
              onClick: () => actions.onUseHex(r.hex),
            });
            onContextMenu(node, () => [
              { label: "Add to library", onSelect: () => actions.onSaveHex(r.hex) },
              { label: "Copy hex", onSelect: () => actions.onCopy(r.hex) },
              { label: "Open in Current", onSelect: () => actions.onUseHex(r.hex) },
            ]);
            return node;
          });

  const grid = el(
    "div",
    {
      style:
        recents && recents.length
          ? "display:grid;grid-template-columns:repeat(6,1fr);gap:6px"
          : "",
    },
    swatches,
  );

  return el("div", { style: "display:flex;flex-direction:column;gap:16px" }, [
    invite,
    el("div", { style: "display:flex;flex-direction:column;gap:9px" }, [
      heading,
      grid,
    ]),
  ]);
}

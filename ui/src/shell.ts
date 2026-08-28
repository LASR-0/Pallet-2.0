/**
 * The window chrome: title bar, tabs and scroll area.
 *
 * Style strings are verbatim from `Prototype/package/Pallet Window.dc.html`.
 */

import { el, spacer, svg } from "./dom";
import { TABS, type AppState, type Screen } from "./state";

const SANS = "var(--font),sans-serif";
const MONO = "var(--mono),monospace";

/** The four-dot mark at the top left. */
function mark(): HTMLElement {
  const dot = (opacity: string) =>
    el("div", {
      style: `background:var(--accent);border-radius:1px${opacity ? `;opacity:${opacity}` : ""}`,
    });
  return el(
    "div",
    {
      style:
        "display:grid;grid-template-columns:repeat(2,4px);grid-template-rows:repeat(2,4px);gap:2px",
    },
    [dot(""), dot(".45"), dot(".6"), dot(".22")],
  );
}

function titleBar(actions: { onMinimise: () => void; onClose: () => void }) {
  return el(
    "div",
    {
      // `data-tauri-drag-region` makes the bar drag the window, which the
      // prototype could not express but a real window needs.
      style:
        "display:flex;align-items:center;gap:10px;height:40px;flex:none;" +
        "padding:0 6px 0 13px;background:var(--chrome);border-bottom:1px solid var(--line)",
    },
    [
      mark(),
      el("span", {
        style: `font:700 12px/1 ${SANS};letter-spacing:.16em`,
        text: "PALLET",
      }),
      el("span", {
        style: `font:400 10.5px/1 ${MONO};color:var(--mute);letter-spacing:.04em`,
        text: "2.0",
      }),
      spacer(),
      el("div", { style: "display:flex;gap:2px" }, [
        el(
          "div",
          {
            class: "hv-chrome clickable",
            style:
              "width:30px;height:26px;display:grid;place-items:center;border-radius:6px;color:var(--mute)",
            title: "Minimise",
            onClick: actions.onMinimise,
          },
          [el("div", { style: "width:9px;height:1px;background:currentColor" })],
        ),
        el("div", {
          class: "hv-close clickable",
          style:
            "width:30px;height:26px;display:grid;place-items:center;border-radius:6px;" +
            `color:var(--mute);font:400 13px/1 ${SANS}`,
          text: "✕",
          title: "Close",
          onClick: actions.onClose,
        }),
      ]),
    ],
  );
}

/**
 * The settings tab's gear.
 *
 * The prototype writes a literal "⚙". Neither Karla nor Sora carries U+2699,
 * so a real window falls back to whatever symbol font the system happens to
 * have — a dot here — and the result differs per machine. Drawing it keeps the
 * tab identical everywhere and needs no extra font.
 */
function gearIcon(): SVGElement {
  const icon = svg("svg", {
    viewBox: "0 0 16 16",
    width: "12",
    height: "12",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": "1.4",
    "stroke-linecap": "round",
    "stroke-linejoin": "round",
    style: "display:block",
  });
  icon.append(
    svg("circle", { cx: "8", cy: "8", r: "2.4" }),
    svg("path", {
      d:
        "M8 1.4v1.7M8 12.9v1.7M14.6 8h-1.7M3.1 8H1.4" +
        "M12.67 3.33l-1.2 1.2M4.53 11.47l-1.2 1.2" +
        "M12.67 12.67l-1.2-1.2M4.53 4.53l-1.2-1.2",
    }),
  );
  return icon;
}

function tabs(state: AppState, onTab: (screen: Screen) => void) {
  return el(
    "div",
    {
      style:
        "display:flex;gap:3px;flex:none;padding:9px 10px;background:var(--chrome);" +
        "border-bottom:1px solid var(--line)",
    },
    TABS.map(([id, label]) => {
      const on = state.screen === id;
      const style =
        "padding:6px 9px;border-radius:7px;cursor:pointer;white-space:nowrap;" +
        `font:${on ? "600" : "400"} 11px/1 ${SANS};letter-spacing:.01em;` +
        (on ? "background:var(--accent);color:#fff;" : "color:var(--mute);");

      if (id === "settings") {
        return el(
          "div",
          {
            style: `${style}display:flex;align-items:center`,
            title: "Settings",
            onClick: () => onTab(id),
          },
          [gearIcon()],
        );
      }

      return el("div", { style, text: label, onClick: () => onTab(id) });
    }),
  );
}

export function renderShell(
  state: AppState,
  body: HTMLElement,
  actions: {
    onTab: (screen: Screen) => void;
    onMinimise: () => void;
    onClose: () => void;
  },
): HTMLElement {
  const scroll = el(
    "div",
    {
      class: "pl-scroll",
      style: "flex:1;min-height:0;overflow-y:auto;padding:14px 14px 18px",
    },
    [body],
  );

  return el(
    "div",
    {
      style:
        "display:flex;flex-direction:column;width:100%;height:100%;overflow:hidden;" +
        "border-radius:var(--rad);background:var(--bg);color:var(--ink);" +
        `font-family:${SANS};box-shadow:var(--shadow);border:1px solid var(--edge)`,
    },
    [
      titleBar({ onMinimise: actions.onMinimise, onClose: actions.onClose }),
      tabs(state, actions.onTab),
      scroll,
    ],
  );
}

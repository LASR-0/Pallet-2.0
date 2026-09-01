/**
 * The window chrome: title bar, tabs and scroll area.
 *
 * Style strings are verbatim from `Prototype/package/Pallet Window.dc.html`.
 */

import { el, spacer, svg } from "./dom";
import { TABS, type AppState, type Screen } from "./state";
import { onWheelStep, stepIndex } from "./wheel";

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
        // No right padding: the window buttons run to the edge, so their hover
        // fills the corner instead of leaving a sliver of bar around it. The
        // shell's own rounded corner clips what overflows.
        "padding:0 0 0 13px;background:var(--chrome);border-bottom:1px solid var(--line)",
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
      // Hard against the top and right of the bar, so each hover fills its
      // corner of the window rather than floating inside it. The shell's own
      // rounded corner clips whatever overflows.
      el("div", { style: "display:flex;align-self:stretch;height:100%" }, [
        el(
          "div",
          {
            class: "hv-chrome clickable",
            style: "width:38px;height:100%;display:grid;place-items:center",
            title: "Minimise",
            onClick: actions.onMinimise,
          },
          [el("div", { style: "width:9px;height:1px;background:currentColor" })],
        ),
        el(
          "div",
          {
            class: "hv-close clickable",
            // Narrower than its neighbour, which brings the cross in to about
            // 16px from the corner. Centred in a wider button it read as
            // sitting well short of the window's edge, even though the button
            // itself already reached it.
            //
            // No inline `color`: it would outrank the hover rule in the
            // stylesheet, which is why the cross stayed grey however the
            // danger tokens were set. `.hv-close` owns both states.
            style: "width:32px;height:100%;display:grid;place-items:center",
            title: "Close",
            onClick: actions.onClose,
          },
          [closeIcon()],
        ),
      ]),
    ],
  );
}

/**
 * The settings tab's cog.
 *
 * The prototype writes a literal "⚙", which no font in this app carries, so a
 * real window falls back to whatever symbol font the system happens to have and
 * the tab differs per machine. Drawn instead, and drawn as a solid six-tooth
 * cog with the hub punched out rather than a ring with spokes: at 12px a
 * stroked ring with eight radiating lines reads as a sun, not a setting.
 *
 * The outline is a plain polygon — teeth at radius 6.5, valleys at 4.7 — with
 * round joins softening every corner, so it stays a cog at any size.
 */
function gearIcon(): SVGElement {
  const icon = svg("svg", {
    viewBox: "0 0 16 16",
    width: "13",
    height: "13",
    fill: "currentColor",
    "stroke-linejoin": "round",
    style: "display:block",
  });
  const teeth =
    "M5.99 1.82 L10.01 1.82 L8.98 3.40 L11.49 4.86 L12.35 3.17 L14.36 6.65 " +
    "L12.47 6.55 L12.47 9.45 L14.36 9.35 L12.35 12.83 L11.49 11.14 " +
    "L8.98 12.60 L10.01 14.18 L5.99 14.18 L7.02 12.60 L4.51 11.14 " +
    "L3.65 12.83 L1.64 9.35 L3.53 9.45 L3.53 6.55 L1.64 6.65 L3.65 3.17 " +
    "L4.51 4.86 L7.02 3.40 Z";
  // The hub, as a second subpath: `evenodd` turns it into a hole.
  const hub = "M8 5.85 A2.15 2.15 0 1 0 8 10.15 A2.15 2.15 0 1 0 8 5.85 Z";
  icon.append(
    svg("path", {
      d: `${teeth} ${hub}`,
      "fill-rule": "evenodd",
      // Round the polygon's corners without changing its silhouette.
      stroke: "currentColor",
      "stroke-width": "0.9",
      "stroke-linejoin": "round",
    }),
  );
  return icon;
}

/**
 * The close button's cross.
 *
 * Drawn rather than set as "✕", for the same reason as the gear: no font here
 * carries the glyph, so the system substitutes one and the button differs per
 * machine. Drawing it also gives the stroke a width to animate — a typeface
 * only offers whole weights, and none of them thickens a cross on hover.
 *
 * Weights come from `.hv-close` in the stylesheet so the hover can change them.
 */
function closeIcon(): SVGElement {
  const icon = svg("svg", {
    viewBox: "0 0 16 16",
    width: "13",
    height: "13",
    fill: "none",
    stroke: "currentColor",
    "stroke-linecap": "round",
    // A 3px stroke on a 13px icon reaches past the viewBox at the corners.
    style: "display:block;overflow:visible",
  });
  icon.append(svg("path", { d: "M4.3 4.3 L11.7 11.7 M11.7 4.3 L4.3 11.7" }));
  return icon;
}

function tabs(state: AppState, onTab: (screen: Screen) => void) {
  const row = el(
    "div",
    {
      // `align-items:center` rather than the default stretch: the gear tab is
      // two pixels taller than a text tab, and stretching left the icon
      // sitting below everything else's centre line.
      style:
        "display:flex;align-items:center;gap:3px;flex:none;padding:9px 10px;" +
        "background:var(--chrome);border-bottom:1px solid var(--line)",
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
            // Dimmed while unselected: a filled shape carries far more weight
            // than a word at the same colour, so an unselected cog at full
            // `--mute` shouts louder than the labels beside it.
            style:
              `${style}display:flex;align-items:center;` +
              (on ? "" : "opacity:.62"),
            title: "Settings",
            onClick: () => onTab(id),
          },
          [gearIcon()],
        );
      }

      return el("div", { style, text: label, onClick: () => onTab(id) });
    }),
  );

  // Scrolling anywhere over the tab strip moves between screens, the same
  // gesture the harmony, sort and filter rows answer to.
  onWheelStep(row, (direction) => {
    const at = TABS.findIndex(([id]) => id === state.screen);
    const next = TABS[stepIndex(at < 0 ? 0 : at, direction, TABS.length)];
    if (next) onTab(next[0]);
  });
  return row;
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
      // The window's outline is an inset ring, not a border. A border occupies
      // layout space, so everything inside started a pixel in from the true
      // edge — which is why the close button's hover could never quite reach
      // the top-right corner however wide the button was made. Drawn as a
      // shadow instead, the outline sits over the content and the chrome runs
      // right up to the glass.
      style:
        "display:flex;flex-direction:column;width:100%;height:100%;overflow:hidden;" +
        "border-radius:var(--shell-rad);background:var(--bg);color:var(--ink);" +
        `font-family:${SANS};` +
        "box-shadow:var(--shadow),inset 0 0 0 1px var(--edge)",
    },
    [
      titleBar({ onMinimise: actions.onMinimise, onClose: actions.onClose }),
      tabs(state, actions.onTab),
      scroll,
    ],
  );
}

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

/**
 * The minimise and close buttons.
 *
 * 26px square at the tabs' 7px radius: the same shape as a tab pill, which is
 * what the eye has directly below them. Stated once so the two cannot drift.
 */
/**
 * How far the window buttons sit from the edges of the window.
 *
 * The same on all three sides they meet. A 26px button in a 40px bar is left
 * with 7px above and below by the centring, so the bar's right padding is that
 * same 7px — it was 10px, chosen to line the close button up with the right
 * edge of the tab strip below, which bought that alignment at the cost of the
 * button sitting in a corner with more room to one side of it than the other.
 * The corner is the thing being looked at, so the corner wins.
 */
const CHROME_INSET = 7;

const CHROME_BUTTON =
  "width:26px;height:26px;border-radius:7px;display:grid;place-items:center";

/**
 * The close button, whose top-right corner opens out to follow the window's.
 *
 * It is the one control that sits inside a corner of the shell, and a 7px
 * curve inside a 20px one reads as two unrelated arcs rather than as a button
 * seated in a corner. Concentric rounding wants the inner radius to be the
 * outer one less the gap between them, which is exactly `CHROME_INSET` now
 * that the gap is the same on both edges of the corner — so the curve tracks
 * `--shell-rad` and comes out at 13px on Sketchbook and 15px on Studio, rather
 * than being a number that silently stops matching if the inset or either
 * radius is retuned.
 *
 * The other three corners keep the tab pills' 7px: they sit in open bar, with
 * no larger curve nearby to answer to.
 *
 * `max()` guards the tiling-compositor case: there `--shell-rad` is 0, the
 * window has no curve to echo, and the bare subtraction would be negative —
 * which is not merely ignored but invalid, taking all four corners with it.
 */
const CLOSE_BUTTON =
  "width:26px;height:26px;display:grid;place-items:center;" +
  `border-radius:7px max(7px, calc(var(--shell-rad) - ${CHROME_INSET}px)) 7px 7px`;

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
      // Makes the bar drag the window, which the prototype could not express
      // but a real window needs.
      dragRegion: true,
      // The right padding is `CHROME_INSET`, so the window buttons have the
      // same gap to the right of them as the bar's height leaves above and
      // below. They used to run hard into the corner with no padding at all,
      // so that their hover filled it — but the window now draws a 2px frame
      // over that corner, and a red wash arriving underneath it read as a
      // mistake.
      style:
        "display:flex;align-items:center;gap:10px;height:40px;flex:none;" +
        `padding:0 ${CHROME_INSET}px 0 13px;background:var(--chrome);` +
        "border-bottom:1px solid var(--line)",
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
      // Two 26px rounded squares rather than two full-height slabs.
      //
      // The old pair were 38 and 32 pixels wide, floor to ceiling, and their
      // hover states were the only rectangles in an app built out of rounded
      // ones — the close button additionally animating its stroke from 1.5 to
      // 3px, a doubling that read as the cross swelling rather than as the
      // button answering. Sized and rounded like the tabs directly beneath
      // them, they belong to the same window; and since the tabs already say
      // "this one is active" by filling with the accent and going white, close
      // can say "this one is destructive" the same way in `--danger` instead
      // of inventing a translucent wash for it.
      //
      // Both buttons are the same width now. The old 38/32 split existed to
      // drag the cross in from an edge it no longer sits against.
      //
      // Neither carries an inline `color`: it would outrank the hover rules in
      // the stylesheet, which is why the cross once stayed grey however the
      // danger tokens were set. `.hv-chrome` and `.hv-close` own both states.
      el("div", { style: "display:flex;align-items:center;gap:2px" }, [
        el(
          "div",
          {
            class: "hv-chrome clickable",
            style: CHROME_BUTTON,
            // No tooltip: a dash at the top right of a window and a cross
            // beside it need no caption, and a tip that pops up over the
            // content every time the pointer crosses the bar on its way to
            // somewhere else costs more than it explains.
            onClick: actions.onMinimise,
          },
          // 1.5px, not the prototype's 1: it sits beside a 1.6px cross, and at
          // 1px it looked like the lighter of two weights rather than its pair.
          [
            el("div", {
              style:
                "width:10px;height:1.5px;border-radius:1px;background:currentColor",
            }),
          ],
        ),
        el(
          "div",
          {
            class: "hv-close clickable",
            style: CLOSE_BUTTON,
            // No tooltip, as above.
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
 * machine.
 *
 * 12px in a 26px button, at one constant weight. It used to be 13px and to
 * thicken from 1.5px to 3px on hover, which is a doubling — the cross appeared
 * to swell under the pointer, and it was the only thing in the app that
 * answered a hover by changing shape rather than colour. The button now fills
 * with `--danger` instead, which is how every other active control here
 * behaves, so the glyph has no work left to do but stay still.
 *
 * The width comes from `.hv-close` in the stylesheet, which pairs it with
 * `non-scaling-stroke` so it lands in rendered pixels rather than viewBox units.
 */
function closeIcon(): SVGElement {
  const icon = svg("svg", {
    viewBox: "0 0 16 16",
    width: "12",
    height: "12",
    fill: "none",
    stroke: "currentColor",
    "stroke-linecap": "round",
    style: "display:block",
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
      // `--chromeMid`, not `--chrome`: the title bar is the darker tone and
      // this strip sits halfway between it and the page, so the chrome steps
      // down to the content instead of the two rows reading as one slab.
      style:
        "display:flex;align-items:center;gap:3px;flex:none;padding:9px 10px;" +
        "background:var(--chromeMid);border-bottom:1px solid var(--line)",
    },
    TABS.flatMap(([id, label]) => {
      const on = state.screen === id;
      const style =
        "padding:6px 9px;border-radius:7px;cursor:pointer;white-space:nowrap;" +
        `font:${on ? "600" : "400"} 11px/1 ${SANS};letter-spacing:.01em;` +
        (on ? "background:var(--accent);color:var(--accentInk);" : "color:var(--mute);");

      if (id === "settings") {
        // Pushed to the far right by a spacer, away from the five screens:
        // settings configures the app rather than being another thing to look
        // at, and a cog in the run of labels invited stepping into it while
        // moving between them. `flatMap` so the spacer and the cog can be
        // returned together — settings is last in `TABS`, so the keyboard
        // digits and the wheel's order are untouched by the move.
        return [
          spacer(),
          el(
            "div",
            {
              // Dimmed while unselected: a filled shape carries far more
              // weight than a word at the same colour, so an unselected cog at
              // full `--mute` shouts louder than the labels beside it.
              style:
                `${style}display:flex;align-items:center;` +
                (on ? "" : "opacity:.62"),
              title: "Settings",
              onClick: () => onTab(id),
            },
            [gearIcon()],
          ),
        ];
      }

      return [el("div", { style, text: label, onClick: () => onTab(id) })];
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
      // The window's outline is an overlay, not a border. A border occupies
      // layout space, so everything inside would start two pixels in from the
      // true edge — which is why the close button's hover could never quite
      // reach the top-right corner however wide the button was made.
      //
      // It was an `inset` box-shadow, which has the same virtue but one fatal
      // flaw: an inset shadow paints beneath the element's children, and the
      // title bar and tab strip are opaque. They covered the top of the ring,
      // leaving a frame that only appeared where the scroll area's
      // transparent background let it through — the bottom of the window.
      // `.pl-shell::after` in the stylesheet draws it above everything
      // instead. See there for the rest.
      //
      // `--shell-shadow` rather than `--shadow`: it resolves to `none` where
      // the compositor draws the window's own.
      class: "pl-shell",
      style:
        "position:relative;display:flex;flex-direction:column;width:100%;height:100%;" +
        "overflow:hidden;border-radius:var(--shell-rad);background:var(--bg);" +
        `color:var(--ink);font-family:${SANS};box-shadow:var(--shell-shadow)`,
    },
    [
      titleBar({ onMinimise: actions.onMinimise, onClose: actions.onClose }),
      tabs(state, actions.onTab),
      scroll,
    ],
  );
}

/**
 * Filter chips and sort selector for the library screens.
 *
 * The sort row is the harmony selector from Current. The chips are a carousel:
 * there are more tags than fit across a 412px window, and a wrapping grid of
 * them pushed the results off the screen. Rotating them away and fading them
 * out at the edges keeps the row one line tall while making it obvious that
 * there is more in both directions.
 */

import { el } from "../dom";
import { onWheelStep, stepIndex } from "../wheel";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** Facet chips, grouped as they filter: within a group, alternatives. */
export const FACET_GROUPS: [string, string][][] = [
  [
    ["warm", "Warm"],
    ["cool", "Cool"],
    ["neutral", "Neutral"],
  ],
  [
    ["light", "Light"],
    ["mid", "Mid"],
    ["dark", "Dark"],
  ],
  [
    ["vivid", "Vivid"],
    ["muted", "Muted"],
  ],
];

export const SORTS: [string, string][] = [
  ["added", "Added"],
  ["hue", "Hue"],
  ["lightness", "Light"],
  ["chroma", "Chroma"],
  ["name", "Name"],
];

/**
 * How far from the centre a pill stays fully lit, as a fraction of the
 * half-width. Past this it rotates away and fades.
 */
const CLEAR_ZONE = 0.34;

/** How far a pill at the very edge has turned, in degrees. */
const MAX_TURN = 62;

/**
 * Scroll positions, kept across re-renders.
 *
 * The screen re-renders whenever a filter is toggled, which destroys the rail
 * and builds a new one. Without this, clicking a chip would snap the row back
 * to the beginning — losing the place the user had scrolled to, on the very
 * interaction most likely to be repeated.
 */
const railScroll = new Map<string, number>();

/** How long the rail must be still before its position is normalised. */
const SETTLE_MS = 140;

/**
 * A horizontally scrolling row of pills that snaps each one to the centre and
 * never runs out in either direction.
 *
 * The pills are laid out three times over. Whichever copy the user has
 * scrolled into, the offset is quietly moved back to the middle one once the
 * rail comes to rest — every copy is identical, so the jump cannot be seen,
 * and the row behaves as though the tags went round for ever.
 *
 * Rendering one copy and moving pills between the ends would be lighter, but
 * it fights scroll-snap: moving the element the browser has snapped to yanks
 * the rail out from under the finger.
 */
function pillRail(
  key: string,
  build: () => HTMLElement[],
  restingOn: number,
): HTMLElement {
  const copies = [build(), build(), build()];
  const pills = copies.flat();
  const perCopy = copies[0]!.length;

  const track = el("div", { class: "pl-rail" });
  track.setAttribute(
    "style",
    "display:flex;gap:6px;overflow-x:auto;overflow-y:hidden;" +
      "padding:3px 0;scroll-snap-type:x mandatory;perspective:520px;" +
      "scrollbar-width:none;-ms-overflow-style:none",
  );

  for (const pill of pills) {
    pill.style.flex = "none";
    pill.style.scrollSnapAlign = "center";
    pill.style.transformStyle = "preserve-3d";
    track.append(pill);
  }

  // Position, opacity and turn are recomputed from the scroll offset rather
  // than animated by CSS: they are a continuous function of where the pill
  // sits, and a transition would lag a frame behind the finger.
  let queued = false;
  const paint = () => {
    queued = false;
    const middle = track.scrollLeft + track.clientWidth / 2;
    const reach = track.clientWidth / 2;
    for (const pill of pills) {
      const centre = pill.offsetLeft + pill.offsetWidth / 2;
      const offset = reach > 0 ? (centre - middle) / reach : 0;
      const away = Math.min(1, Math.abs(offset));
      const faded = Math.max(0, (away - CLEAR_ZONE) / (1 - CLEAR_ZONE));

      pill.style.opacity = String(1 - faded);
      pill.style.transform =
        `rotateY(${-offset * MAX_TURN}deg) scale(${1 - faded * 0.22})`;
      // A pill that has faded out must not still be clickable.
      pill.style.pointerEvents = faded > 0.85 ? "none" : "auto";
    }
  };

  /** The width of one copy of the list, including the gap that follows it. */
  const copyWidth = () => {
    const first = pills[0];
    const second = pills[perCopy];
    return first && second ? second.offsetLeft - first.offsetLeft : 0;
  };

  /**
   * Move the offset back into the middle copy.
   *
   * Only ever called once scrolling has stopped: changing `scrollLeft` during
   * a smooth scroll or a snap would abort it visibly.
   */
  const normalise = () => {
    const width = copyWidth();
    if (width <= 0) return;
    const low = width * 0.5;
    if (track.scrollLeft < low) {
      track.scrollLeft += width;
    } else if (track.scrollLeft > low + width) {
      track.scrollLeft -= width;
    }
  };

  let settled = false;
  let settleTimer = 0;
  track.addEventListener("scroll", () => {
    if (settled) railScroll.set(key, track.scrollLeft);
    if (!queued) {
      queued = true;
      requestAnimationFrame(paint);
    }
    if (settled) {
      clearTimeout(settleTimer);
      settleTimer = window.setTimeout(normalise, SETTLE_MS);
    }
  });

  const centreOn = (index: number, smooth: boolean) => {
    const pill = pills[index];
    if (!pill) return;
    track.scrollTo({
      left: pill.offsetLeft + pill.offsetWidth / 2 - track.clientWidth / 2,
      behavior: smooth ? "smooth" : "auto",
    });
  };

  /** The pill nearest the centre, which is the one the wheel steps from. */
  const centred = () => {
    const middle = track.scrollLeft + track.clientWidth / 2;
    let best = 0;
    let bestDistance = Infinity;
    pills.forEach((pill, i) => {
      const distance = Math.abs(pill.offsetLeft + pill.offsetWidth / 2 - middle);
      if (distance < bestDistance) {
        bestDistance = distance;
        best = i;
      }
    });
    return best;
  };

  // Down and right, up and left: the row runs horizontally, so a downward
  // flick should carry it the way the eye reads. No wrapping here — there is
  // always another copy to step into.
  onWheelStep(track, (direction) => {
    centreOn(Math.min(pills.length - 1, Math.max(0, centred() + direction)), true);
  });

  // Positioning needs the rail's width, which is zero until it has been laid
  // out — and a tab switch can take a frame or two to get there. Placing it
  // against a width of zero silently lands on the first pill, which is what
  // made the row look like it always started at the beginning.
  let attempts = 0;
  const place = () => {
    if (track.clientWidth === 0 && attempts < 10) {
      attempts += 1;
      requestAnimationFrame(place);
      return;
    }
    const saved = railScroll.get(key);
    if (saved !== undefined) {
      track.scrollLeft = saved;
    } else {
      // Start in the middle copy, so there is already a full list of tags in
      // both directions before the user has scrolled at all.
      centreOn(perCopy + restingOn, false);
    }
    paint();
    settled = true;
  };
  requestAnimationFrame(place);

  return track;
}

export function renderFilters(
  key: string,
  active: string[],
  sort: string,
  actions: {
    onToggle: (id: string) => void;
    onSort: (id: string) => void;
    onClearFacets: () => void;
  },
): HTMLElement {
  const facets = FACET_GROUPS.flat();
  // Built on demand: the rail lays the list out three times over, and the same
  // element cannot be in three places at once.
  const chips = () =>
    facets.map(([id, label]) => {
      const on = active.includes(id);
      return el("span", {
        // The EXPORT pill from the Build screen, lit when selected. Its own
        // class rather than `hv-copy`, because that transitions `transform`
        // and the rail rewrites each pill's rotation every frame — the two
        // together would leave the row lagging behind the scroll.
        class: on ? "pl-pill pl-on clickable" : "pl-pill clickable",
        style:
          // Colours come from `.pl-pill` and `.pl-on`, so hover can win.
          "padding:6px 10px;border-radius:999px;white-space:nowrap;" +
          `font:400 10px/1 ${MONO};border-width:1px;border-style:solid`,
        text: label,
        onClick: () => actions.onToggle(id),
      });
    });

  const sorts = SORTS.map(([id, label]) => {
    const on = sort === id;
    // The harmony selector from the Current screen.
    return el("div", {
      style:
        "flex:1;text-align:center;padding:6px 4px;border-radius:6px;cursor:pointer;" +
        `font:${on ? "600" : "400"} 10px/1 ${SANS};` +
        (on
          ? "background:var(--hover);color:var(--ink);"
          : "color:var(--mute);") +
        `border:1px solid ${on ? "var(--line)" : "transparent"}`,
      text: label,
      onClick: () => actions.onSort(id),
    });
  });

  // Where the rail rests before it has been scrolled: on the first active
  // filter if there is one, otherwise in the middle. Resting on the first pill
  // would leave half the row blank, which reads as broken rather than as a
  // carousel with more to either side.
  const firstActive = facets.findIndex(([id]) => active.includes(id));
  const resting =
    firstActive >= 0 ? firstActive : Math.floor((facets.length - 1) / 2);

  const sortRow = el("div", { style: "display:flex;gap:4px" }, sorts);
  onWheelStep(sortRow, (direction) => {
    const at = SORTS.findIndex(([id]) => id === sort);
    const next = SORTS[stepIndex(at < 0 ? 0 : at, direction, SORTS.length)];
    if (next) actions.onSort(next[0]);
  });

  // Clear sits over the rail's left edge, in the band where the pills have
  // already faded to nothing. That space is dead otherwise, and putting the
  // control on its own row cost a line of a window that has none to spare.
  const rail = el("div", { style: "position:relative" }, [
    pillRail(key, chips, resting),
  ]);
  if (active.length > 0) {
    rail.append(
      el("span", {
        class: "pl-pop pl-clear clickable",
        style:
          "position:absolute;left:0;top:50%;transform:translateY(-50%);z-index:2;" +
          "padding:5px 11px;border-radius:999px;background:var(--bg);" +
          `font:400 10px/1 ${MONO};letter-spacing:.06em;` +
          "border:1px solid var(--accent)",
        text: "Clear",
        title:
          active.length === 1
            ? "Remove the filter"
            : `Remove all ${active.length} filters`,
        onClick: actions.onClearFacets,
      }),
    );
  }

  return el("div", { style: "display:flex;flex-direction:column;gap:8px" }, [
    rail,
    sortRow,
  ]);
}

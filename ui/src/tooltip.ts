/**
 * Tooltips drawn by the app.
 *
 * A native `title` attribute is rendered by the platform — on Linux that means
 * the GTK theme, so the tooltip arrives in whatever colours the user's file
 * manager uses and looks nothing like the window it belongs to. These are
 * built from the same tokens as everything else.
 *
 * Elements opt in through `data-tip` rather than `title`; anything still using
 * `title` would get the platform's version as well as this one.
 */

import { el } from "./dom";

const MONO = "var(--mono),monospace";

/** How long the pointer must rest before a tip appears. */
const DELAY_MS = 450;
/** Gap between the pointer and the tip. */
const OFFSET = 14;

let tip: HTMLElement | null = null;
let timer: number | null = null;

function hide(): void {
  if (timer !== null) {
    window.clearTimeout(timer);
    timer = null;
  }
  tip?.remove();
  tip = null;
}

function show(text: string, x: number, y: number): void {
  hide();

  tip = el("div", {
    style:
      "position:fixed;z-index:200;pointer-events:none;max-width:240px;" +
      "padding:5px 8px;border-radius:6px;background:var(--chrome);" +
      "color:var(--ink);border:1px solid var(--line);box-shadow:var(--shadow);" +
      `font:400 10px/1.35 ${MONO};letter-spacing:.02em;white-space:pre-wrap`,
    text,
  });
  document.body.append(tip);

  // Keep it inside a 440px window, which has very little room to spare.
  const box = tip.getBoundingClientRect();
  const left = Math.min(Math.max(6, x + OFFSET), window.innerWidth - box.width - 6);
  const below = y + OFFSET;
  const top =
    below + box.height + 6 > window.innerHeight ? y - box.height - OFFSET : below;

  tip.style.left = `${left}px`;
  tip.style.top = `${Math.max(6, top)}px`;
}

/** Start watching for elements carrying `data-tip`. */
export function installTooltips(): void {
  document.addEventListener("mouseover", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const host = target.closest<HTMLElement>("[data-tip]");
    if (!host) return;

    const text = host.dataset.tip;
    if (!text) return;

    hide();
    timer = window.setTimeout(
      () => show(text, event.clientX, event.clientY),
      DELAY_MS,
    );
  });

  // Anything that moves the element out from under the pointer must dismiss
  // it, or the tip is left describing something that is no longer there.
  document.addEventListener("mouseout", (event) => {
    const related = event.relatedTarget;
    if (related instanceof Element && related.closest("[data-tip]")) return;
    hide();
  });
  document.addEventListener("mousedown", hide, true);
  window.addEventListener("scroll", hide, true);
  window.addEventListener("keydown", hide, true);
}

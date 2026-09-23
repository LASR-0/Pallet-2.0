/**
 * A minimal element helper.
 *
 * No framework: the prototype expresses its design as inline styles on plain
 * elements, and keeping that shape means each block here maps one-to-one onto
 * a block of `Pallet Window.dc.html`. That traceability is worth more than the
 * ergonomics of a component library while the design is being matched exactly.
 */

type Attrs = {
  style?: string;
  class?: string;
  /**
   * Tooltip text.
   *
   * Set as `data-tip`, not `title`: a native title is drawn by the platform,
   * which on Linux means the GTK theme rather than this app's.
   */
  title?: string;
  /**
   * Click handler.
   *
   * Receives the event, for the handful of places that need a modifier —
   * ctrl-click to add to a selection rather than replace it. Handlers that do
   * not care simply ignore the argument.
   */
  onClick?: (event: MouseEvent) => void;
  text?: string;
  /**
   * Marks the element as a window-drag handle.
   *
   * Only the exact element carrying this survives to Tauri's drag listener —
   * a mousedown on a child button inside it (the title bar's minimise/close)
   * never reaches the attribute, so those stay clickable rather than also
   * dragging the window.
   */
  dragRegion?: boolean;
};

export function el(
  tag: keyof HTMLElementTagNameMap,
  attrs: Attrs = {},
  children: (Node | string | null)[] = [],
): HTMLElement {
  const node = document.createElement(tag);
  if (attrs.style) node.setAttribute("style", attrs.style);
  if (attrs.class) node.className = attrs.class;
  if (attrs.title) node.dataset.tip = attrs.title;
  if (attrs.dragRegion) node.setAttribute("data-tauri-drag-region", "");
  if (attrs.text !== undefined) node.textContent = attrs.text;
  if (attrs.onClick) {
    const handler = attrs.onClick;
    node.addEventListener("click", (event) => handler(event as MouseEvent));
    node.classList.add("clickable");
  }
  for (const child of children) {
    if (child === null) continue;
    node.append(child);
  }
  return node;
}

/**
 * The transparent gutter between the viewport and the shell, in pixels.
 *
 * `--shell-pad` in tokens.css keeps a margin around the window for its shadow
 * to fall into, so the viewport is larger than the visible window on every
 * side. Anything positioned against the viewport — the tooltip, the context
 * menu — has to subtract this or it lands out in the glass.
 */
export function gutter(): number {
  const value = getComputedStyle(document.documentElement).getPropertyValue(
    "--shell-pad",
  );
  return Number.parseFloat(value) || 0;
}

/** A flex spacer, the prototype's `<div style="flex:1">`. */
export function spacer(): HTMLElement {
  return el("div", { style: "flex:1" });
}

/** An SVG element. Attributes must be set in the SVG namespace. */
export function svg(
  tag: string,
  attrs: Record<string, string> = {},
): SVGElement {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [key, value] of Object.entries(attrs)) {
    node.setAttribute(key, value);
  }
  return node;
}

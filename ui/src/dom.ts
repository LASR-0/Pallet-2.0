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
  title?: string;
  onClick?: () => void;
  text?: string;
};

export function el(
  tag: keyof HTMLElementTagNameMap,
  attrs: Attrs = {},
  children: (Node | string | null)[] = [],
): HTMLElement {
  const node = document.createElement(tag);
  if (attrs.style) node.setAttribute("style", attrs.style);
  if (attrs.class) node.className = attrs.class;
  if (attrs.title) node.title = attrs.title;
  if (attrs.text !== undefined) node.textContent = attrs.text;
  if (attrs.onClick) {
    const handler = attrs.onClick;
    node.addEventListener("click", () => handler());
    node.classList.add("clickable");
  }
  for (const child of children) {
    if (child === null) continue;
    node.append(child);
  }
  return node;
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

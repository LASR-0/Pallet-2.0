/**
 * A right-click menu.
 *
 * The prototype has no design for this, so it is derived from the tokens the
 * rest of the window already uses: the panel background, the hairline border,
 * the secondary radius, and the accent reserved for the one destructive item.
 * Nothing here invents a new colour or metric.
 */

import { el } from "./dom";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  /** Requires a second click, and shows `confirmLabel` in between. */
  destructive?: boolean;
}

const SANS = "var(--font),sans-serif";

/** The menu currently open, so a second right-click replaces rather than stacks. */
let open: HTMLElement | null = null;

export function closeMenu(): void {
  open?.remove();
  open = null;
}

export function showMenu(x: number, y: number, items: MenuItem[]): void {
  closeMenu();

  const menu = el("div", {
    style:
      "position:fixed;z-index:100;min-width:150px;padding:4px;" +
      "border-radius:var(--rad2);background:var(--panel);" +
      "border:1px solid var(--line);box-shadow:var(--shadow)",
  });

  for (const item of items) {
    let armed = false;
    const row = el("div", {
      class: "hv-chrome clickable",
      style:
        `padding:8px 10px;border-radius:7px;font:400 11.5px/1 ${SANS};` +
        "color:var(--ink);white-space:nowrap",
      text: item.label,
      onClick: () => {
        // A destructive item asks once rather than opening a dialog: the
        // second click is the confirmation, and moving the mouse away or
        // pressing Escape is the cancel.
        if (item.destructive && !armed) {
          armed = true;
          row.textContent = "Click again to confirm";
          row.setAttribute(
            "style",
            `padding:8px 10px;border-radius:7px;font:500 11.5px/1 ${SANS};` +
              "color:var(--accent);white-space:nowrap",
          );
          return;
        }
        closeMenu();
        item.onSelect();
      },
    });
    menu.append(row);
  }

  document.body.append(menu);
  open = menu;

  // Keep the menu inside the window; a 440px window has little room to spare.
  const box = menu.getBoundingClientRect();
  const left = Math.min(x, window.innerWidth - box.width - 6);
  const top = Math.min(y, window.innerHeight - box.height - 6);
  menu.style.left = `${Math.max(6, left)}px`;
  menu.style.top = `${Math.max(6, top)}px`;
}

/** Dismiss on the next click, right-click, scroll or Escape anywhere. */
export function installMenuDismiss(): void {
  const dismiss = (event: Event) => {
    if (!open) return;
    if (event.target instanceof Node && open.contains(event.target)) return;
    closeMenu();
  };
  window.addEventListener("mousedown", dismiss, true);
  window.addEventListener("contextmenu", dismiss, true);
  window.addEventListener("scroll", () => closeMenu(), true);
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") closeMenu();
  });
  // The window is a desktop app, not a page: the browser's own menu has
  // nothing useful to offer, and it would fight ours.
  window.addEventListener("contextmenu", (event) => event.preventDefault());
}

/** Attach a right-click menu to an element. */
export function onContextMenu(
  node: HTMLElement,
  items: () => MenuItem[],
): void {
  node.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    showMenu(event.clientX, event.clientY, items());
  });
}

import "./styles/base.css";

import { getCurrentWindow } from "@tauri-apps/api/window";

import * as api from "./api";
import { el } from "./dom";
import { renderCurrent } from "./screens/current";
import { renderShell } from "./shell";
import type { AppState, Harmony, Screen, Theme } from "./state";

const state: AppState = {
  screen: "current",
  theme: "sketchbook",
  harmony: "complementary",
  detail: null,
};

const root = document.getElementById("app");
if (!root) throw new Error("#app is missing from index.html");

function placeholder(label: string): HTMLElement {
  return el("div", {
    style:
      "padding:28px 4px;font:400 12px/1.5 var(--font),sans-serif;color:var(--mute);text-align:center",
    text: `${label} lands in a later milestone.`,
  });
}

function body(): HTMLElement {
  switch (state.screen) {
    case "current":
      return renderCurrent(state, {
        onCopy: copy,
        onPickHex: (hex) => void setColor(hex),
        onHarmony: (harmony) => void setHarmony(harmony),
      });
    case "pick":
      return placeholder("Pick");
    case "palettes":
      return placeholder("Palettes");
    case "colours":
      return placeholder("Colours");
    case "build":
      return placeholder("Build");
    case "settings":
      return placeholder("Settings");
  }
}

function render(): void {
  document.documentElement.setAttribute("data-theme", state.theme);
  root!.replaceChildren(
    renderShell(state, body(), {
      onTab: (screen: Screen) => {
        state.screen = screen;
        render();
      },
      onMinimise: () => void getCurrentWindow().minimize(),
      onClose: () => void getCurrentWindow().close(),
    }),
  );
}

function copy(value: string): void {
  void navigator.clipboard.writeText(value);
}

async function setColor(hex: string): Promise<void> {
  state.detail = await api.colorDetail(hex, state.harmony);
  render();
}

async function setHarmony(harmony: Harmony): Promise<void> {
  state.harmony = harmony;
  if (state.detail) {
    state.detail = await api.colorDetail(state.detail.hex, harmony);
  }
  render();
}

/** Toggle themes with a keypress while the design is being matched. */
function installThemeToggle(): void {
  window.addEventListener("keydown", (event) => {
    if (event.key.toLowerCase() !== "t" || event.metaKey || event.ctrlKey) return;
    const next: Theme = state.theme === "sketchbook" ? "studio" : "sketchbook";
    state.theme = next;
    render();
  });
}

async function start(): Promise<void> {
  render();
  installThemeToggle();

  // Open on the most recent pick, falling back to the prototype's own colour
  // so the window is never empty on a fresh install.
  const latest = (await api.latestPick().catch(() => null)) ?? "#A5236E";
  await setColor(latest);
}

void start();

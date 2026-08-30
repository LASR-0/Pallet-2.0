import "./styles/base.css";

import { getCurrentWindow } from "@tauri-apps/api/window";

import * as api from "./api";
import { el } from "./dom";
import { renderCurrent } from "./screens/current";
import { renderColours, renderPalettes } from "./screens/library";
import { renderShell } from "./shell";
import { TABS, type AppState, type Harmony, type Screen, type Theme } from "./state";

const state: AppState = {
  screen: "current",
  theme: "sketchbook",
  harmony: "complementary",
  detail: null,
  palettes: null,
  colours: null,
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
      return renderPalettes(state.palettes, (hex) => void goToColor(hex));
    case "colours":
      return renderColours(state.colours, (hex) => void goToColor(hex));
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
        // The library is read on demand, then kept: it only changes when the
        // user edits it, and re-reading on every tab switch would flicker.
        if (screen === "palettes" && state.palettes === null) void loadPalettes();
        if (screen === "colours" && state.colours === null) void loadColours();
      },
      onMinimise: () => void getCurrentWindow().minimize(),
      onClose: () => void getCurrentWindow().close(),
    }),
  );
}

async function loadPalettes(): Promise<void> {
  state.palettes = await api.palettes().catch(() => []);
  render();
}

async function loadColours(): Promise<void> {
  state.colours = await api.colours().catch(() => []);
  render();
}

function copy(value: string): void {
  void navigator.clipboard.writeText(value);
}

/** Choosing a swatch from the library opens it on Current. */
async function goToColor(hex: string): Promise<void> {
  state.screen = "current";
  await setColor(hex);
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

/**
 * Keyboard shortcuts for reviewing the design: `t` toggles theme, `1`-`6`
 * jump between tabs.
 */
function installShortcuts(): void {
  window.addEventListener("keydown", (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;

    if (event.key.toLowerCase() === "t") {
      const next: Theme = state.theme === "sketchbook" ? "studio" : "sketchbook";
      state.theme = next;
      render();
      return;
    }

    const index = Number(event.key) - 1;
    const tab = TABS[index];
    if (!tab) return;
    state.screen = tab[0];
    render();
    if (state.screen === "palettes" && state.palettes === null) void loadPalettes();
    if (state.screen === "colours" && state.colours === null) void loadColours();
  });
}

async function start(): Promise<void> {
  render();
  installShortcuts();

  // Open on the most recent pick, falling back to the prototype's own colour
  // so the window is never empty on a fresh install.
  const latest = (await api.latestPick().catch(() => null)) ?? "#A5236E";
  await setColor(latest);
}

void start();

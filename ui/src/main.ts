import "./styles/base.css";

import { getCurrentWindow } from "@tauri-apps/api/window";

import * as api from "./api";
import { el } from "./dom";
import { renderCurrent } from "./screens/current";
import { renderBuild } from "./screens/build";
import { renderColours, renderPalettes } from "./screens/library";
import { focusSearch } from "./screens/search";
import { renderShell } from "./shell";
import { TABS, type AppState, type Harmony, type Screen, type Theme } from "./state";

const state: AppState = {
  screen: "current",
  theme: "sketchbook",
  harmony: "complementary",
  detail: null,
  palettes: null,
  colours: null,
  queries: { palettes: "", colours: "" },
  build: {
    colours: [],
    name: "Untitled",
    min: 3,
    capacity: 25,
    picking: false,
    error: null,
  },
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
      return renderPalettes(
        state.palettes,
        state.queries.palettes,
        (hex) => void goToColor(hex),
        searchActions("palettes"),
      );
    case "colours":
      return renderColours(
        state.colours,
        state.queries.colours,
        (hex) => void goToColor(hex),
        searchActions("colours"),
      );
    case "build":
      return renderBuild(state.build, {
        onPickNext: () => void pickNext(),
        onSave: () => void savePalette(),
        onRemove: (index) => {
          state.build.colours.splice(index, 1);
          state.build.error = null;
          render();
        },
        onRename: (name) => {
          state.build.name = name;
        },
      });
    case "settings":
      return placeholder("Settings");
  }
}

/**
 * Remember the query so switching tabs and back does not lose it.
 *
 * Deliberately does *not* re-render: the field updates its own results, and a
 * full re-render would replace the input mid-keystroke.
 */
function searchActions(screen: "palettes" | "colours") {
  return {
    onQuery: (value: string) => {
      state.queries[screen] = value;
    },
  };
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

/** Ask the picker for a colour and add it to the palette being built. */
async function pickNext(): Promise<void> {
  if (state.build.picking) return;
  state.build.picking = true;
  state.build.error = null;
  render();

  try {
    const hex = await api.pickColour();
    if (hex) state.build.colours.push(hex);
  } catch (e) {
    state.build.error = String(e);
  } finally {
    state.build.picking = false;
    render();
  }
}

async function savePalette(): Promise<void> {
  try {
    await api.savePalette(state.build.name, state.build.colours);
    state.build.colours = [];
    state.build.error = null;
    // The library changed, so drop the cache and pick up a fresh default name.
    state.palettes = null;
    state.build.name = await api.nextPaletteName().catch(() => "Untitled");
    state.screen = "palettes";
    render();
    void loadPalettes();
  } catch (e) {
    state.build.error = String(e);
    render();
  }
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
    // Ctrl+S saves the palette being built, the one shortcut that earns a
    // modifier because it commits something.
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
      if (state.screen === "build") {
        event.preventDefault();
        void savePalette();
      }
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) return;

    if (event.key.toLowerCase() === "t") {
      const next: Theme = state.theme === "sketchbook" ? "studio" : "sketchbook";
      state.theme = next;
      render();
      return;
    }

    // "/" focuses search, the convention in tools that are keyboard-first.
    // Autofocusing on tab entry would be more convenient but would swallow the
    // digit shortcuts below the moment either library screen opened.
    if (
      event.key === "/" &&
      (state.screen === "palettes" || state.screen === "colours")
    ) {
      event.preventDefault();
      focusSearch(root!);
      return;
    }

    // Enter is the Build screen's primary action, so the whole gather-a-palette
    // flow works without the mouse.
    if (event.key === "Enter" && state.screen === "build") {
      event.preventDefault();
      void pickNext();
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

  // Limits and the default name come from the backend so they are stated once.
  const [min, capacity] = await api.paletteLimits().catch(() => [3, 25] as const);
  state.build.min = min;
  state.build.capacity = capacity;
  state.build.name = await api.nextPaletteName().catch(() => "Untitled");
  render();
}

void start();

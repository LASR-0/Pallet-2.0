import "./styles/base.css";

import { getCurrentWindow } from "@tauri-apps/api/window";

import * as api from "./api";
import { renderCurrent } from "./screens/current";
import { installMenuDismiss } from "./menu";
import { renderBuild } from "./screens/build";
import { renderPick } from "./screens/pick";
import { renderSettings } from "./screens/settings";
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
  facets: { palettes: [], colours: [] },
  sorts: { palettes: "added", colours: "added" },
  build: {
    colours: [],
    name: "Untitled",
    min: 3,
    capacity: 25,
    picking: false,
    error: null,
    formats: [],
    exported: null,
  },
  recents: null,
  picking: false,
  settings: null,
};

const root = document.getElementById("app");
if (!root) throw new Error("#app is missing from index.html");

function body(): HTMLElement {
  switch (state.screen) {
    case "current":
      return renderCurrent(state, {
        onCopy: copy,
        onPickHex: (hex) => void setColor(hex),
        onHarmony: (harmony) => void setHarmony(harmony),
      });
    case "pick":
      return renderPick(state.recents, "CTRL + SHIFT + P", state.picking, {
        onPick: () => void pickFromScreen(),
        onUseHex: (hex) => void goToColor(hex),
        onSaveHex: (hex) => void mutate(() => api.saveColour(hex)),
        onCopy: copy,
      });
    case "palettes":
      return renderPalettes(
        state.palettes,
        state.queries.palettes,
        (hex) => void goToColor(hex),
        searchActions("palettes"),
        {
          onRenamePalette: (id, name) =>
            void mutate(() => api.renamePalette(id, name)),
          onDeletePalette: (id) => void mutate(() => api.deletePalette(id)),
          facets: state.facets.palettes,
          sort: state.sorts.palettes,
          onToggleFacet: (id) => toggleFacet("palettes", id),
          onSort: (id) => setSort("palettes", id),
        },
      );
    case "colours":
      return renderColours(
        state.colours,
        state.queries.colours,
        (hex) => void goToColor(hex),
        searchActions("colours"),
        {
          onRenameColour: (id, name) =>
            void mutate(() => api.renameColour(id, name)),
          onDeleteColour: (id) => void mutate(() => api.deleteColour(id)),
          onCopy: copy,
          facets: state.facets.colours,
          sort: state.sorts.colours,
          onToggleFacet: (id) => toggleFacet("colours", id),
          onSort: (id) => setSort("colours", id),
        },
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
        onExport: (format) => void exportPalette(format),
      });
    case "settings":
      return renderSettings(state.settings, (key) => void cycleSetting(key));
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
        if (screen === "pick" && state.recents === null) void loadRecents();
        if (screen === "settings" && state.settings === null) void loadSettings();
      },
      onMinimise: () => void getCurrentWindow().minimize(),
      onClose: () => void getCurrentWindow().close(),
    }),
  );
}

/** Pick from the Pick screen: the colour opens on Current. */
async function pickFromScreen(): Promise<void> {
  if (state.picking) return;
  state.picking = true;
  render();
  try {
    const hex = await api.pickColour();
    state.recents = null;
    if (hex) {
      state.picking = false;
      await goToColor(hex);
      return;
    }
  } catch (e) {
    console.error(String(e));
  } finally {
    state.picking = false;
  }
  await loadRecents();
}

async function loadSettings(): Promise<void> {
  state.settings = await api.settings().catch(() => []);
  render();
}

/**
 * Change a setting and re-read everything it can affect.
 *
 * The theme and the colour space both change what is already on screen, so the
 * cached Current detail is recomputed rather than left stale.
 */
async function cycleSetting(key: string): Promise<void> {
  try {
    await api.cycleSetting(key);
  } catch (e) {
    console.error(String(e));
    return;
  }
  await loadSettings();

  const rows = state.settings ?? [];
  const theme = rows.find((r) => r.key === "theme")?.value.toLowerCase();
  if (theme === "sketchbook" || theme === "studio") state.theme = theme;

  if (state.detail) state.detail = await api.colorDetail(state.detail.hex, state.harmony);
  render();
}

async function loadRecents(): Promise<void> {
  state.recents = await api.recentPicks().catch(() => []);
  render();
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

/** Write the palette being built and report where it landed. */
async function exportPalette(format: string): Promise<void> {
  try {
    state.build.exported = await api.exportPalette(
      state.build.name,
      state.build.colours,
      format,
    );
    state.build.error = null;
  } catch (e) {
    state.build.exported = null;
    state.build.error = String(e);
  }
  render();
}

async function savePalette(): Promise<void> {
  try {
    await api.savePalette(state.build.name, state.build.colours);
    state.build.colours = [];
    state.build.error = null;
    state.build.exported = null;
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

/**
 * Run a library edit, then reload whichever screen is showing.
 *
 * The caches are dropped rather than patched: an edit is rare and a reload is
 * a single query, whereas keeping a mirror of the library in the frontend in
 * step with the database is a bug factory.
 */
async function mutate(action: () => Promise<unknown>): Promise<void> {
  try {
    await action();
  } catch (e) {
    tracing(String(e));
    return;
  }
  state.palettes = null;
  state.colours = null;
  if (state.screen === "palettes") await loadPalettes();
  if (state.screen === "colours") await loadColours();
  if (state.screen === "pick") await loadRecents();
}

/** Surface a backend failure without a dialog. */
function tracing(message: string): void {
  console.error(message);
}

async function loadPalettes(): Promise<void> {
  state.palettes = await api
    .palettes(state.facets.palettes, state.sorts.palettes)
    .catch(() => []);
  render();
}

async function loadColours(): Promise<void> {
  state.colours = await api
    .colours(state.facets.colours, state.sorts.colours)
    .catch(() => []);
  render();
}

/** Toggle a facet chip and reload the screen it belongs to. */
function toggleFacet(screen: "palettes" | "colours", id: string): void {
  const current = state.facets[screen];
  state.facets[screen] = current.includes(id)
    ? current.filter((f) => f !== id)
    : [...current, id];
  void reload(screen);
}

function setSort(screen: "palettes" | "colours", id: string): void {
  state.sorts[screen] = id;
  void reload(screen);
}

/** Filtering and sorting happen in Rust, so a change means asking again. */
async function reload(screen: "palettes" | "colours"): Promise<void> {
  if (screen === "palettes") await loadPalettes();
  else await loadColours();
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
    if (state.screen === "pick" && state.recents === null) void loadRecents();
    if (state.screen === "settings" && state.settings === null) void loadSettings();
  });
}

async function start(): Promise<void> {
  render();
  installShortcuts();
  installMenuDismiss();

  // Open on the most recent pick, falling back to the prototype's own colour
  // so the window is never empty on a fresh install.
  const latest = (await api.latestPick().catch(() => null)) ?? "#A5236E";
  await setColor(latest);

  // Limits and the default name come from the backend so they are stated once.
  await loadSettings();
  const theme = state.settings
    ?.find((r) => r.key === "theme")
    ?.value.toLowerCase();
  if (theme === "sketchbook" || theme === "studio") state.theme = theme;

  const [min, capacity] = await api.paletteLimits().catch(() => [3, 25] as const);
  state.build.min = min;
  state.build.capacity = capacity;
  state.build.name = await api.nextPaletteName().catch(() => "Untitled");
  state.build.formats = await api.exportFormats().catch(() => []);
  render();
}

void start();

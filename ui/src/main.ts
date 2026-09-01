import "./styles/base.css";

import { getCurrentWindow } from "@tauri-apps/api/window";

import * as api from "./api";
import { renderCurrent } from "./screens/current";
import { installMenuDismiss } from "./menu";
import { installTooltips } from "./tooltip";
import { renderBuild } from "./screens/build";
import { renderPick } from "./screens/pick";
import { renderSettings } from "./screens/settings";
import { renderColours, renderPalettes } from "./screens/library";
import { focusSearch } from "./screens/search";
import { comboFromEvent, matches } from "./keys";
import { renderShell } from "./shell";
import { TABS, type AppState, type Harmony, type Screen } from "./state";

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
    name: "",
    suggested: "Untitled",
    needsName: false,
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
  bindings: null,
  capturing: null,
  naming: null,
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
        onKeep: (hex) => void keepColour(hex),
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
          onClearFacets: () => clearFacets("palettes"),
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
          onClearFacets: () => clearFacets("colours"),
          onSort: (id) => setSort("colours", id),
          naming: state.naming,
          onNamed: () => {
            state.naming = null;
            render();
          },
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
          // Typing answers the prompt, so the field stops asking.
          if (name.trim()) state.build.needsName = false;
        },
        onSubmitName: () => void savePalette(),
        onExport: (format) => void exportPalette(format),
      });
    case "settings":
      return renderSettings(state.settings, state.bindings, state.capturing, {
        onCycle: (key) => void cycleSetting(key),
        onCapture: (key) => {
          // Arm capture; the next keypress becomes the binding.
          state.capturing = state.capturing === key ? null : key;
          render();
        },
      });
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
    const [hex] = await api.pickColour();
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

/**
 * Keep the Current colour, then hand straight over to naming it.
 *
 * No dialog: the colour is already saved under its suggested name by the time
 * the Colours screen appears, and the open field is an invitation to change it
 * rather than a form standing between the user and their library.
 */
async function keepColour(hex: string): Promise<void> {
  let id: string;
  try {
    id = await api.saveColour(hex);
  } catch (e) {
    console.error(String(e));
    return;
  }

  state.colours = null;
  state.palettes = null;
  state.screen = "colours";
  // Clear any filter or search that would hide the colour just added.
  state.queries.colours = "";
  state.facets.colours = [];
  state.naming = id;
  await loadColours();
}

async function loadSettings(): Promise<void> {
  state.settings = await api.settings().catch(() => []);
  state.bindings = await api.bindings().catch(() => []);
  render();
}

/** Read the bindings without re-rendering, for the shortcut handler. */
function binding(key: string): string {
  return state.bindings?.find((b) => b.key === key)?.combo ?? "";
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

/**
 * Gather the rest of the palette in one pass.
 *
 * One press freezes the screen once and collects colours until the HUD's tray
 * fills, or until the user finishes early — whatever they took by then is
 * added. Re-freezing the screen for each colour made it impossible to choose
 * colours that sit well together, which is the whole point of the screen.
 */
async function pickNext(): Promise<void> {
  if (state.build.picking) return;
  state.build.picking = true;
  state.build.error = null;
  render();

  try {
    const taken = await api.pickColour([...state.build.colours]);
    state.build.colours.push(
      ...taken.slice(0, state.build.capacity - state.build.colours.length),
    );
    // Those colours are now in the pick history too, so the cached list is
    // stale even though the Build screen is what the user is looking at.
    if (taken.length > 0) state.recents = null;
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
      paletteName(),
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

/** What the palette would be saved as right now. */
function paletteName(): string {
  return state.build.name.trim() || state.build.suggested;
}

/**
 * Save the palette, asking for a name first if it has never been given one.
 *
 * The name field is easy to miss, and a palette saved as "Untitled 6" has to be
 * renamed in the library afterwards. Saving an unnamed palette therefore puts
 * the cursor in the field instead, with the suggestion showing through: type a
 * name, or press Enter to accept the one already there.
 */
async function savePalette(): Promise<void> {
  if (!state.build.name.trim() && !state.build.needsName) {
    state.build.needsName = true;
    state.build.error = null;
    render();
    return;
  }

  try {
    await api.savePalette(paletteName(), state.build.colours);
    state.build.colours = [];
    state.build.error = null;
    state.build.exported = null;
    state.build.name = "";
    state.build.needsName = false;
    // The library changed, so drop the cache and pick up a fresh default name.
    state.palettes = null;
    state.build.suggested = await api.nextPaletteName().catch(() => "Untitled");
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

/** Drop every tag filter at once, for the Clear pill. */
function clearFacets(screen: "palettes" | "colours"): void {
  if (state.facets[screen].length === 0) return;
  state.facets[screen] = [];
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
    // Capturing a new binding swallows everything until a real key arrives.
    if (state.capturing) {
      event.preventDefault();
      if (event.key === "Escape") {
        state.capturing = null;
        render();
        return;
      }
      const combo = comboFromEvent(event);
      if (combo) void applyBinding(state.capturing, combo);
      return;
    }

    // Bindings come from config, so remapping takes effect immediately.
    if (matches(event, binding("pick"))) {
      event.preventDefault();
      void (state.screen === "build" ? pickNext() : pickFromScreen());
      return;
    }

    if (matches(event, binding("theme"))) {
      event.preventDefault();
      void cycleSetting("theme");
      return;
    }

    if (
      matches(event, binding("search")) &&
      (state.screen === "palettes" || state.screen === "colours")
    ) {
      event.preventDefault();
      focusSearch(root!);
      return;
    }

    if (matches(event, binding("save_palette")) && state.screen === "build") {
      event.preventDefault();
      void savePalette();
      return;
    }

    if (
      matches(event, binding("save_colour")) &&
      state.screen === "current" &&
      state.detail
    ) {
      event.preventDefault();
      void keepColour(state.detail.hex);
      return;
    }

    // Tab digits stay fixed: they are positional rather than a command, and
    // binding six of them would fill the settings list for little gain.
    if (event.ctrlKey || event.metaKey || event.altKey) return;
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

/** Store a captured binding and refresh what is on screen. */
async function applyBinding(key: string, combo: string): Promise<void> {
  const target = key;
  state.capturing = null;
  try {
    await api.setBinding(target, combo);
  } catch (e) {
    console.error(String(e));
  }
  await loadSettings();
}

async function start(): Promise<void> {
  render();
  installShortcuts();
  installMenuDismiss();
  installTooltips();

  // A tiling compositor rounds its own windows; drawing ours on top of that
  // gives a double corner at two curvatures.
  const systemCorners = await api.compositorRoundsWindows().catch(() => false);
  document.documentElement.setAttribute(
    "data-corners",
    systemCorners ? "system" : "app",
  );

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
  state.build.suggested = await api.nextPaletteName().catch(() => "Untitled");
  state.build.formats = await api.exportFormats().catch(() => []);
  render();
}

void start();

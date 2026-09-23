import "./styles/base.css";

import { getCurrentWindow } from "@tauri-apps/api/window";

import * as api from "./api";
import { renderCurrent } from "./screens/current";
import { installMenuDismiss } from "./menu";
import { installTooltips } from "./tooltip";
import { renderBuild } from "./screens/build";
import { previewPairs } from "./screens/preview";
import { renderPick } from "./screens/pick";
import { renderSettings } from "./screens/settings";
import { renderColours, renderPalettes } from "./screens/library";
import { focusSearch } from "./screens/search";
import { comboFromEvent, displayCombo, matches } from "./keys";
import { renderShell } from "./shell";
import {
  TABS,
  formatToken,
  parseToken,
  type AppState,
  type Harmony,
  type ImportKind,
  type Screen,
  type ShareReport,
} from "./state";
import type { ImportActions } from "./screens/library";

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
    tokens: [],
    roles: {},
    medium: null,
    verdicts: null,
    choosingRole: null,
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
  importing: null,
  share: { dir: null, files: null, notice: null, busy: false },
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
      // Read from the bindings, not written in: the shortcut is remappable in
      // Settings, and a hard-coded caption went on naming the old keys.
      return renderPick(state.recents, displayCombo(binding("pick")), state.picking, {
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
          onCopy: copy,
          facets: state.facets.palettes,
          sort: state.sorts.palettes,
          onToggleFacet: (id) => toggleFacet("palettes", id),
          onClearFacets: () => clearFacets("palettes"),
          onSort: (id) => setSort("palettes", id),
        },
        state.importing?.kind === "palette" ? importActions() : null,
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
        state.importing?.kind === "colour" ? importActions() : null,
      );
    case "build":
      return renderBuild(state.build, {
        onPickNext: () => void pickNext(),
        onSave: () => void savePalette(),
        onRemove: (index) => {
          state.build.colours.splice(index, 1);
          // The token goes with the colour it named. Leaving it behind would
          // slide every name below it up onto the wrong swatch.
          state.build.tokens.splice(index, 1);
          state.build.error = null;
          render();
          // A removed colour may have been filling a role, which changes which
          // pairings exist. Roles themselves need no tidying — they are held
          // by hex and resolved against the palette when drawn.
          void judgeRoles();
        },
        onToken: (index, text) => {
          state.build.tokens[index] = text;
          // Deliberately no re-render: the field updates itself, and replacing
          // it mid-word would lose the caret — the same bargain the search
          // field and the palette name field make.
        },
        onImport: (kind) => beginImport(kind),
        onMedium: (medium) => {
          if (medium === state.build.medium) return;
          state.build.medium = medium;
          // The two media have different roles, so an assignment made under
          // one means nothing under the other — `background` is the only name
          // they share, and it is not the same slot.
          state.build.roles = {};
          state.build.verdicts = null;
          state.build.choosingRole = null;
          render();
        },
        onChooseRole: (role) => {
          state.build.choosingRole = role;
          render();
        },
        onAssignRole: (role, hex) => {
          if (hex === null) delete state.build.roles[role];
          else state.build.roles[role] = hex;
          // Assigning closes the chooser: the next thing the user wants is to
          // see the card, not to keep staring at the palette they just used.
          state.build.choosingRole = null;
          render();
          void judgeRoles();
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
      return renderSettings(
        state.settings,
        state.bindings,
        state.capturing,
        state.share,
        {
          onCycle: (key) => void cycleSetting(key),
          onCapture: (key) => {
            // Arm capture; the next keypress becomes the binding.
            state.capturing = state.capturing === key ? null : key;
            render();
          },
          onShare: () => void shareLibrary(),
          onImport: (path) => void importLibrary(path),
          onCopyPath: (path) => void api.copyText(path),
        },
      );
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

/**
 * The screen the last render drew, so the next one knows whether the user is
 * still looking at the same page or has moved to another.
 */
let rendered: Screen | null = null;

/**
 * Redraw the window.
 *
 * Every interaction in this app re-renders the whole screen, which replaces
 * the scroll container along with everything else — and a new element starts
 * at the top. Anything below the fold therefore threw the page back to the
 * beginning whenever it was touched: assigning a role, arming a key binding,
 * removing a colour. The offset is carried across so the page stays where the
 * user left it.
 *
 * Only within one screen. Switching tabs *should* start at the top, since it
 * is a different page and landing halfway down it would be the real surprise.
 */
function render(): void {
  document.documentElement.setAttribute("data-theme", state.theme);

  const before = root!.querySelector<HTMLElement>(".pl-scroll");
  const offset = state.screen === rendered ? (before?.scrollTop ?? 0) : 0;

  root!.replaceChildren(
    renderShell(state, body(), {
      onTab: (screen: Screen) => {
        leavingScreen(screen);
        state.screen = screen;
        render();
        // The library is read on demand, then kept: it only changes when the
        // user edits it, and re-reading on every tab switch would flicker.
        if (screen === "palettes" && state.palettes === null) void loadPalettes();
        if (screen === "colours" && state.colours === null) void loadColours();
        if (screen === "pick" && state.recents === null) void loadRecents();
        if (screen === "settings" && state.settings === null) void loadSettings();
        // Always, not only when unread: see `loadShare`.
        if (screen === "settings") void loadShare();
      },
      onMinimise: () => void getCurrentWindow().minimize(),
      onClose: () => void getCurrentWindow().close(),
    }),
  );

  // Assigning `scrollTop` forces the layout it needs, and a value past the new
  // content's height clamps itself — which is the right answer when a render
  // made the page shorter.
  if (offset > 0) {
    const after = root!.querySelector<HTMLElement>(".pl-scroll");
    if (after) after.scrollTop = offset;
  }
  rendered = state.screen;
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

/**
 * Re-read the shared folder.
 *
 * Unlike the library, this is read every time Settings is opened rather than
 * cached: the folder is the interface, and the whole point of it is that
 * something may have been dropped in there from outside Pallet since last
 * time. A stale list would be a list that cannot show the file the user just
 * put there, which is precisely the case it exists for.
 */
async function loadShare(): Promise<void> {
  state.share.dir = await api.sharedDir().catch(() => null);
  state.share.files = await api.sharedLibraries().catch(() => []);
  render();
}

/** Write this library out to the shared folder. */
async function shareLibrary(): Promise<void> {
  if (state.share.busy) return;
  state.share.busy = true;
  state.share.notice = null;
  render();

  try {
    const path = await api.shareLibrary();
    const name = path.split(/[\\/]/).pop() ?? path;
    state.share.notice = `Wrote ${name}. Send that file to anyone else running Pallet.`;
  } catch (e) {
    state.share.notice = String(e);
  } finally {
    state.share.busy = false;
  }
  // Reload rather than push the new file onto the list: the backend decides
  // the name, and it is the one that has just seen the folder.
  await loadShare();
}

/**
 * Take a shared library in.
 *
 * Everything the library screens hold is dropped afterwards, because a merge
 * can add to both of them and a cached list would show the user a library
 * that no longer matches the one they have.
 */
async function importLibrary(path: string): Promise<void> {
  if (state.share.busy) return;
  state.share.busy = true;
  state.share.notice = null;
  render();

  try {
    const report = await api.importLibrary(path);
    state.share.notice = shareSummary(report);
    state.palettes = null;
    state.colours = null;
  } catch (e) {
    state.share.notice = String(e);
  } finally {
    state.share.busy = false;
  }
  await loadShare();
}

/** What an import did, as a sentence. */
function shareSummary(report: ShareReport): string {
  const added: string[] = [];
  if (report.coloursAdded) added.push(plural(report.coloursAdded, "colour"));
  if (report.palettesAdded) added.push(plural(report.palettesAdded, "palette"));

  if (added.length === 0) {
    // The merge succeeded and wrote nothing, which is what re-importing a file
    // you already have looks like. Said plainly, or it reads as a failure.
    return "Nothing new — you already have everything in that file.";
  }

  let text = `Added ${added.join(" and ")}.`;
  const known = report.coloursKnown + report.palettesKnown;
  if (known) text += ` ${known} you already had ${known === 1 ? "was" : "were"} left alone.`;
  if (report.membersDropped) {
    text += ` ${plural(report.membersDropped, "palette slot")} named a colour that was not in the file and had to be skipped.`;
  }
  return text;
}

function plural(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
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
    const added = taken.slice(
      0,
      state.build.capacity - state.build.colours.length,
    );
    state.build.colours.push(...added);
    // New colours arrive unnamed. Padding here rather than at the point of
    // use keeps the two arrays the same length everywhere else.
    state.build.tokens.push(...added.map(() => ""));
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
      state.build.tokens.map(parseToken),
    );
    state.build.error = null;
  } catch (e) {
    state.build.exported = null;
    state.build.error = String(e);
  }
  render();
}

// --- importing from the library ---------------------------------------------
//
// Build sends the user to the library to fetch something, and the library
// hands it back. The selection lives here rather than on either screen because
// it belongs to neither: it is the errand itself.

/** Send the user to the library to choose what to bring back. */
function beginImport(kind: ImportKind): void {
  state.importing = { kind, selected: [] };
  state.screen = kind === "palette" ? "palettes" : "colours";
  render();
  if (kind === "palette" && state.palettes === null) void loadPalettes();
  if (kind === "colour" && state.colours === null) void loadColours();
}

/** Give up and go back with nothing. */
function cancelImport(): void {
  state.importing = null;
  state.screen = "build";
  render();
}

/**
 * Drop an import the user has walked away from.
 *
 * Switching tabs mid-errand abandons it: the banner would otherwise be left
 * behind on a screen nobody is looking at, and coming back to that screen
 * later would silently resume a selection made for a palette that may since
 * have been saved and cleared. Called from both ways of changing screen.
 */
function leavingScreen(next: Screen): void {
  if (!state.importing) return;
  const home = state.importing.kind === "palette" ? "palettes" : "colours";
  if (next !== home) state.importing = null;
}

/** Ctrl-click: add to the selection, or take it back out, and stay here. */
function toggleImportPick(id: string): void {
  const importing = state.importing;
  if (!importing) return;
  const at = importing.selected.indexOf(id);
  if (at >= 0) importing.selected.splice(at, 1);
  else importing.selected.push(id);
  render();
}

/**
 * Plain click: the selection is over.
 *
 * The clicked row joins it first, unless it is already in — so a single click
 * with nothing chosen takes one thing, and a click after several ctrl-clicks
 * takes those and this one too. Clicking something already chosen just ends
 * the selection, which is the natural way to say "that's all" once the thing
 * you wanted last is already ringed.
 */
function finishImport(id: string): void {
  const importing = state.importing;
  if (!importing) return;
  const ids = [...importing.selected];
  if (!ids.includes(id)) ids.push(id);
  applyImport(importing.kind, ids);
}

/** Put what was chosen into the palette being built, and go back to it. */
function applyImport(kind: ImportKind, ids: string[]): void {
  const hexes: string[] = [];
  const tokens: string[] = [];

  if (kind === "palette") {
    for (const id of ids) {
      const card = state.palettes?.find((p) => p.id === id);
      if (!card) continue;
      hexes.push(...card.colors);
      // A saved palette carries what its members were called, so the tokens
      // come across with the colours rather than arriving blank.
      tokens.push(
        ...card.colors.map((_, i) =>
          formatToken(card.tokens[i] ?? { group: null, name: null }),
        ),
      );
    }
  } else {
    for (const id of ids) {
      const chip = state.colours?.find((c) => c.id === id);
      if (!chip) continue;
      hexes.push(chip.hex);
      // Deliberately no token. A library name is what the colour is called in
      // the library, not what it should be called in this palette's exports —
      // and `with_suggested_names` already fills that in on the way out.
      tokens.push("");
    }
  }

  // Appending, not replacing: this was reached from the empty slot at the end
  // of the strip, which is the place that means "and then this one".
  const room = state.build.capacity - state.build.colours.length;
  const taken = Math.max(0, room);
  state.build.colours.push(...hexes.slice(0, taken));
  state.build.tokens.push(...tokens.slice(0, taken));

  const dropped = hexes.length - taken;
  state.build.error =
    dropped > 0
      ? `${dropped} colour${dropped === 1 ? "" : "s"} did not fit — a palette holds ${state.build.capacity}`
      : null;

  state.importing = null;
  state.screen = "build";
  render();
  // New colours can complete a pairing that was half-assigned before.
  void judgeRoles();
}

/** The handlers a library screen needs while an import is running. */
function importActions(): ImportActions | null {
  if (!state.importing) return null;
  return {
    state: state.importing,
    onToggle: toggleImportPick,
    onFinish: finishImport,
    onCancel: cancelImport,
  };
}

/**
 * Ask the backend what the preview's pairings come to.
 *
 * Called whenever the roles change and whenever a colour leaves the palette,
 * since either can make a pairing appear or vanish. Failure clears the
 * verdicts rather than leaving the last set on screen: stale contrast figures
 * describing colours that are no longer paired are worse than none.
 */
async function judgeRoles(): Promise<void> {
  const pairs = previewPairs(
    state.build.medium,
    state.build.roles,
    state.build.colours,
  );
  if (pairs.length === 0) {
    state.build.verdicts = null;
    render();
    return;
  }
  state.build.verdicts = await api.contrastPairs(pairs).catch((e) => {
    tracing(String(e));
    return null;
  });
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
    await api.savePalette(
      paletteName(),
      state.build.colours,
      state.build.tokens.map(parseToken),
    );
    state.build.colours = [];
    state.build.tokens = [];
    // Roles belong to the palette that has just left the screen. Carrying
    // them over would point every one of them at colours the next palette
    // does not contain.
    state.build.roles = {};
    state.build.medium = null;
    state.build.verdicts = null;
    state.build.choosingRole = null;
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

/**
 * Copy, through the backend.
 *
 * Every COPY chip, context-menu item and click-to-copy swatch comes here, so
 * there is one clipboard path rather than one per platform's quirks.
 */
function copy(value: string): void {
  void api.copyText(value).catch((e) => tracing(String(e)));
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
    // An import has the user somewhere they did not navigate to, so Escape
    // has to be the way back — and it takes precedence over the library
    // screen's own Escape, which only clears the search field.
    if (state.importing && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelImport();
      return;
    }

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
    leavingScreen(tab[0]);
    state.screen = tab[0];
    render();
    if (state.screen === "palettes" && state.palettes === null) void loadPalettes();
    if (state.screen === "colours" && state.colours === null) void loadColours();
    if (state.screen === "pick" && state.recents === null) void loadRecents();
    if (state.screen === "settings" && state.settings === null) void loadSettings();
    if (state.screen === "settings") void loadShare();
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

/**
 * Taking in a shared library dropped on the window.
 *
 * The shared folder is the documented route, but it asks the user to move a
 * file there before Pallet will look at it, and the file has almost always
 * just arrived somewhere else. Dropping it is the same operation without that
 * step, so it goes straight to the same merge.
 *
 * Only `.pallet` files. Anything else dropped is ignored rather than refused:
 * a window that argues with you about a file you did not mean to drop on it is
 * worse than one that does nothing.
 */
async function installFileDrop(): Promise<void> {
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  await getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type !== "drop") return;
    const path = event.payload.paths.find((p) =>
      p.toLowerCase().endsWith(".pallet"),
    );
    if (!path) return;

    // Straight to Settings, where the result is reported. Dropping a file and
    // being told nothing — on Pick, say — would look like it failed.
    leavingScreen("settings");
    state.screen = "settings";
    if (state.settings === null) void loadSettings();
    void importLibrary(path);
  });
}

async function start(): Promise<void> {
  render();
  installShortcuts();
  installMenuDismiss();
  installTooltips();
  void installFileDrop();

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

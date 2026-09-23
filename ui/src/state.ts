/** What the window is showing. */

export type Screen =
  | "pick"
  | "current"
  | "palettes"
  | "colours"
  | "build"
  | "settings";

export type Theme = "sketchbook" | "studio";

export type Harmony = "complementary" | "analogous" | "triadic" | "split";

/** One row of the HEX / RGB / HSL block. */
export interface CodeRow {
  label: string;
  value: string;
}

/** One swatch on the tints and shades ramp. */
export interface RampStep {
  step: number;
  hex: string;
}

/** Everything the Current screen needs, computed by the Rust side. */
export interface ColorDetail {
  hex: string;
  name: string;
  onColor: string;
  codeRows: CodeRow[];
  harmony: string[];
  ramp: RampStep[];
}

/** A palette card on the Palettes screen. */
export interface PaletteCard {
  id: string;
  num: string;
  name: string;
  meta: string;
  colors: string[];
  /** What each member is called, index-parallel with {@link colors}. */
  tokens: Token[];
}

/** A swatch on the Colours screen. */
export interface ColourChip {
  id: string;
  name: string;
  hex: string;
}

/** One row of the Settings screen. */
export interface SettingRow {
  key: string;
  label: string;
  hint: string;
  value: string;
  on: boolean;
  editable: boolean;
}

/** A remappable command. */
export interface Binding {
  key: string;
  label: string;
  hint: string;
  combo: string;
  inLoupe: boolean;
}

/** One entry in the pick history. */
export interface RecentPick {
  id: string;
  hex: string;
}

/** The palette being assembled on the Build screen. */
/** An export format offered by the backend. */
export interface ExportFormat {
  id: string;
  label: string;
  /** The extension the written file gets, without a dot. */
  extension: string;
}

/** A token as the backend wants it: a group and a name within it. */
export interface Token {
  group: string | null;
  name: string | null;
}

/**
 * Split what the user typed into a token.
 *
 * `brand/primary` is a group and a name; `primary` on its own is a name with
 * no group, which is a legal token — plenty of palettes want a flat set.
 * Splits on the *first* slash only: a name may well contain one, a group may
 * not, and two levels is as deep as the formats project.
 */
export function parseToken(text: string): Token {
  const trimmed = text.trim();
  if (!trimmed) return { group: null, name: null };

  const at = trimmed.indexOf("/");
  if (at < 0) return { group: null, name: trimmed };

  const group = trimmed.slice(0, at).trim();
  const name = trimmed.slice(at + 1).trim();
  return {
    group: group || null,
    name: name || null,
  };
}

/**
 * What the palette is being made for.
 *
 * The formats Pallet writes fall into two kinds, and they want different
 * things previewed. A stylesheet's colours end up as a themed interface, so
 * the question is whether text is readable on its panels. A contact sheet's
 * colours end up as a picture, so the question is whether the shapes read
 * against each other. One preview cannot answer both, and guessing from the
 * export tile the user last hovered would be guessing.
 *
 * `null` means unchosen, which is what the preview asks about first: the roles
 * differ per medium, so there is nothing sensible to assign until it is known.
 */
export type Medium = "web" | "image";

/** Which colour does which job. Values are hexes from the palette. */
export type Roles = Record<string, string | undefined>;

/** Whether an import is collecting whole palettes or single colours. */
export type ImportKind = "palette" | "colour";

/**
 * An import in progress: the library is being browsed on Build's behalf.
 *
 * Started from the pick panel's right-click menu, which sends the user to
 * whichever library screen holds what they asked for. While this is set, that
 * screen's cards select rather than do their usual job, and the shortcut
 * handler treats Escape as "give up and go back".
 */
export interface ImportState {
  kind: ImportKind;
  /**
   * What has been ctrl-clicked so far, by id, in the order it was added.
   *
   * Ids rather than colours: the library is the authority on what a row
   * holds, and a palette can be renamed or edited underneath a selection.
   */
  selected: string[];
}

/** One pairing sent to the backend to be judged. */
export interface ContrastPair {
  id: string;
  fore: string;
  back: string;
  graphic?: boolean;
}

/** What the backend says about a pairing. */
export interface ContrastVerdict {
  id: string;
  ratio: number;
  level: string;
  apca: number;
  passes: boolean;
}

/**
 * Put a stored token back into the text form the field edits.
 *
 * The inverse of {@link parseToken}, for opening a saved palette on Build: the
 * store keeps the two halves apart, the field shows them joined.
 */
export function formatToken(token: Token): string {
  if (token.group && token.name) return `${token.group}/${token.name}`;
  return token.name ?? token.group ?? "";
}

export interface BuildState {
  colours: string[];
  /**
   * Which colour fills which role, keyed by role id.
   *
   * Held by hex rather than by index: removing a colour would otherwise
   * silently slide every role onto its neighbour. A hex that is no longer in
   * the palette is treated as unassigned when the preview is drawn, so a
   * removed colour releases the roles it held without any bookkeeping here.
   *
   * Not persisted. Roles live for as long as the palette is being built; the
   * vocabulary above should settle before a schema is committed to it.
   */
  roles: Roles;
  /**
   * What the palette is for, which decides what the preview draws.
   *
   * `null` until chosen. Roles are per-medium, so changing it clears them
   * rather than carrying assignments onto slots that do not exist.
   */
  medium: Medium | null;
  /** The last judgements returned for the current roles, or null while none. */
  verdicts: ContrastVerdict[] | null;
  /** Which role's colour is being chosen, if any. */
  choosingRole: string | null;
  /**
   * What each colour is called on export, index-parallel with `colours`.
   *
   * Held as the raw text the user typed rather than as a parsed
   * {@link Token}, so the field shows back exactly what went into it —
   * including a half-typed `brand/` — and is parsed only on the way out.
   * Kept the same length as `colours` by every path that changes either.
   */
  tokens: string[];
  /** What the user typed. Empty means take {@link BuildState.suggested}. */
  name: string;
  /**
   * The auto-generated name, shown as the field's placeholder.
   *
   * Kept apart from `name` so an untouched field is visibly empty. Pre-filling
   * it made the field look like a label rather than something to edit, and it
   * went unnoticed until after the palette had been saved under it.
   */
  suggested: string;
  /** Save was pressed while unnamed, so the field is asking to be filled. */
  needsName: boolean;
  min: number;
  capacity: number;
  picking: boolean;
  error: string | null;
  formats: ExportFormat[];
  /** Where the last export went, shown under the chips. */
  exported: string | null;
}

/** One shared library sitting in the shared folder. */
export interface SharedFile {
  /** Full path, handed back to the backend to take it in. */
  path: string;
  /** The file name, which is all the user is shown. */
  name: string;
  /** Bytes on disk. */
  size: number;
}

/**
 * What taking a shared library in actually changed.
 *
 * The untouched counts matter as much as the added ones: a merge is additive,
 * so re-importing a file you already have succeeds and does nothing, and
 * "nothing happened" needs to be distinguishable from "it failed".
 */
export interface ShareReport {
  coloursAdded: number;
  coloursKnown: number;
  palettesAdded: number;
  palettesKnown: number;
  membersDropped: number;
}

/**
 * The sharing panel on Settings.
 *
 * Named for sharing rather than importing because [`ImportState`] already
 * means something else here — Build pulling a palette out of the library —
 * and the two would be a nuisance to tell apart in a stack trace.
 */
export interface ShareState {
  /** Where bundles are written and looked for. `null` until asked. */
  dir: string | null;
  /** What is sitting in that folder. `null` until read. */
  files: SharedFile[] | null;
  /** The outcome of the last thing tried, reported in place. */
  notice: string | null;
  /** Set while a write or a merge is in flight, to disable the buttons. */
  busy: boolean;
}

export interface AppState {
  screen: Screen;
  theme: Theme;
  harmony: Harmony;
  detail: ColorDetail | null;
  /** `null` until the library has been read. */
  palettes: PaletteCard[] | null;
  colours: ColourChip[] | null;
  /** Search text, kept per screen so switching tabs does not lose it. */
  queries: Record<"palettes" | "colours", string>;
  /** Selected facet chips, per screen. */
  facets: Record<"palettes" | "colours", string[]>;
  /** Sort order, per screen. */
  sorts: Record<"palettes" | "colours", string>;
  build: BuildState;
  recents: RecentPick[] | null;
  picking: boolean;
  settings: SettingRow[] | null;
  bindings: Binding[] | null;
  /** A colour just kept, whose name is open for editing on Colours. */
  naming: string | null;
  /** The command currently waiting for a keypress, if any. */
  capturing: string | null;
  /** An import Build started, while the user is choosing what to bring back. */
  importing: ImportState | null;
  /** The sharing panel on Settings. */
  share: ShareState;
}

export const TABS: [Screen, string][] = [
  ["pick", "Pick"],
  ["current", "Current"],
  ["palettes", "Palettes"],
  ["colours", "Colours"],
  ["build", "Build"],
  ["settings", "⚙"],
];

export const HARMONIES: [Harmony, string][] = [
  ["complementary", "Comp"],
  ["analogous", "Analog"],
  ["triadic", "Triad"],
  ["split", "Split"],
];

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
}

export interface BuildState {
  colours: string[];
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

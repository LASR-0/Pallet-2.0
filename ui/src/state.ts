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

/** One entry in the pick history. */
export interface RecentPick {
  id: string;
  hex: string;
}

/** The palette being assembled on the Build screen. */
export interface BuildState {
  colours: string[];
  name: string;
  min: number;
  capacity: number;
  picking: boolean;
  error: string | null;
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

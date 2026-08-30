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

export interface AppState {
  screen: Screen;
  theme: Theme;
  harmony: Harmony;
  detail: ColorDetail | null;
  /** `null` until the library has been read. */
  palettes: PaletteCard[] | null;
  colours: ColourChip[] | null;
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

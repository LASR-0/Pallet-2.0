/**
 * Calls into the Rust side.
 *
 * All colour maths lives there. Reimplementing conversions in TypeScript — as
 * the prototype did — would give two implementations that drift, and the hex
 * shown in the window has to be the hex the picker recorded.
 */

import { invoke } from "@tauri-apps/api/core";
import type { ColorDetail, ColourChip, Harmony, PaletteCard } from "./state";

export async function colorDetail(
  hex: string,
  harmony: Harmony,
): Promise<ColorDetail> {
  return invoke<ColorDetail>("color_detail", { hex, harmony });
}

export async function latestPick(): Promise<string | null> {
  return invoke<string | null>("latest_pick");
}

export async function palettes(): Promise<PaletteCard[]> {
  return invoke<PaletteCard[]>("palettes");
}

export async function colours(): Promise<ColourChip[]> {
  return invoke<ColourChip[]>("colours");
}

/** Blocks until the user commits or abandons the pick. */
export async function pickColour(): Promise<string | null> {
  return invoke<string | null>("pick_colour");
}

export async function paletteLimits(): Promise<[number, number]> {
  return invoke<[number, number]>("palette_limits");
}

export async function nextPaletteName(): Promise<string> {
  return invoke<string>("next_palette_name");
}

export async function savePalette(
  name: string,
  hexes: string[],
): Promise<string> {
  return invoke<string>("save_palette", { name, hexes });
}

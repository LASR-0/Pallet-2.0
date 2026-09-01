/**
 * Calls into the Rust side.
 *
 * All colour maths lives there. Reimplementing conversions in TypeScript — as
 * the prototype did — would give two implementations that drift, and the hex
 * shown in the window has to be the hex the picker recorded.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  ColorDetail,
  ColourChip,
  Harmony,
  PaletteCard,
  RecentPick,
  ExportFormat,
  Binding,
  SettingRow,
} from "./state";

export async function colorDetail(
  hex: string,
  harmony: Harmony,
): Promise<ColorDetail> {
  return invoke<ColorDetail>("color_detail", { hex, harmony });
}

export async function latestPick(): Promise<string | null> {
  return invoke<string | null>("latest_pick");
}

export async function palettes(
  facets: string[],
  sort: string,
): Promise<PaletteCard[]> {
  return invoke<PaletteCard[]>("palettes", { facets, sort });
}

export async function colours(
  facets: string[],
  sort: string,
): Promise<ColourChip[]> {
  return invoke<ColourChip[]>("colours", { facets, sort });
}

/**
 * Blocks until the user finishes or abandons the pick.
 *
 * Passing `held` — the colours the palette already has — turns this into a
 * palette pass: the overlay stays up until its tray fills and returns every
 * colour taken, so a palette is chosen against a single frozen screen. Omit it
 * for a single colour; the tray then stays hidden rather than implying a
 * palette is being built.
 *
 * Returns the colours taken, newest last, or an empty array if the user backed
 * out without taking any. How long a palette pass runs for comes from the
 * multi-pick setting, so it is decided in the backend rather than here.
 */
export async function pickColour(held?: string[]): Promise<string[]> {
  return invoke<string[]>("pick_colour", { held: held ?? null });
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

export async function renamePalette(id: string, name: string): Promise<void> {
  return invoke("rename_palette", { id, name });
}

export async function deletePalette(id: string): Promise<void> {
  return invoke("delete_palette", { id });
}

export async function renameColour(id: string, name: string): Promise<void> {
  return invoke("rename_colour", { id, name });
}

export async function deleteColour(id: string): Promise<void> {
  return invoke("delete_colour", { id });
}

export async function recentPicks(limit = 32): Promise<RecentPick[]> {
  return invoke<RecentPick[]>("recent_picks", { limit });
}

export async function saveColour(hex: string): Promise<string> {
  return invoke<string>("save_colour", { hex });
}

export async function settings(): Promise<SettingRow[]> {
  return invoke<SettingRow[]>("settings");
}

export async function cycleSetting(key: string): Promise<void> {
  return invoke("cycle_setting", { key });
}

export async function exportFormats(): Promise<ExportFormat[]> {
  return invoke<ExportFormat[]>("export_formats");
}

export async function exportPalette(
  name: string,
  hexes: string[],
  format: string,
): Promise<string> {
  return invoke<string>("export_palette", { name, hexes, format });
}

export async function compositorRoundsWindows(): Promise<boolean> {
  return invoke<boolean>("compositor_rounds_windows");
}

export async function bindings(): Promise<Binding[]> {
  return invoke<Binding[]>("bindings");
}

export async function setBinding(key: string, combo: string): Promise<void> {
  return invoke("set_binding", { key, combo });
}

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

export async function recentPicks(limit = 24): Promise<RecentPick[]> {
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

/**
 * Calls into the Rust side.
 *
 * All colour maths lives there. Reimplementing conversions in TypeScript — as
 * the prototype did — would give two implementations that drift, and the hex
 * shown in the window has to be the hex the picker recorded.
 */

import { invoke } from "@tauri-apps/api/core";
import type { ColorDetail, Harmony } from "./state";

export async function colorDetail(
  hex: string,
  harmony: Harmony,
): Promise<ColorDetail> {
  return invoke<ColorDetail>("color_detail", { hex, harmony });
}

export async function latestPick(): Promise<string | null> {
  return invoke<string | null>("latest_pick");
}

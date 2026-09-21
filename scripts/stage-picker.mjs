// Copy the built picker to where Tauri's bundler expects a sidecar.
//
// The app finds the picker beside its own binary (`connect_to_picker`), which
// is true in a build tree by accident and in an installed bundle only if the
// bundler is told to put it there. Tauri does that for `externalBin` entries,
// but insists each one is named `<name>-<target triple><ext>` on disk and
// copies it in as plain `<name><ext>`. That suffix is the whole reason this
// script exists: `cargo` writes `pallet-picker.exe` and the bundler will not
// look at it under that name.
//
// Run from `beforeBuildCommand`, after the picker is built. Node rather than a
// shell one-liner because that command runs under `cmd` on Windows and `sh`
// everywhere else, and there is no copy invocation that means the same thing
// in both.
//
// Tauri runs that command from `apps/`, not from the directory holding
// `tauri.conf.json` — hence the `../scripts/` in it, matching the `../ui` the
// frontend build alongside it already relies on. Paths below are resolved from
// this file's own location instead, so only the invocation depends on that.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Asked of the toolchain rather than assumed, so a cross-compile or an
// aarch64 machine stages a file the bundler will actually find.
const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host:"))
  ?.slice("host:".length)
  .trim();

if (!triple) {
  console.error("stage-picker: could not read the host triple from `rustc -vV`");
  process.exit(1);
}

const ext = process.platform === "win32" ? ".exe" : "";
// Honours `CARGO_TARGET_DIR` because the MSI bundler forces the issue: WiX
// writes absolute paths into an XML document without escaping them, so a
// checkout under a directory containing an `&` — "KSB SE & Co KGaA", say —
// produces a `main.wxs` that will not parse. Building with the target
// directory somewhere plainer is the way out, and this has to follow it there.
const targetDir = process.env.CARGO_TARGET_DIR || join(root, "target");
const from = join(targetDir, "release", `pallet-picker${ext}`);
const outDir = join(root, "apps", "pallet-app", "binaries");
const to = join(outDir, `pallet-picker-${triple}${ext}`);

mkdirSync(outDir, { recursive: true });
copyFileSync(from, to);
console.log(`stage-picker: ${from} -> ${to}`);

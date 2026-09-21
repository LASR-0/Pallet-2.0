// Gather the installers and a runnable portable folder into `dist/`.
//
// `just bundle` builds with `CARGO_TARGET_DIR` pointed outside the repository
// (see the recipe for why WiX forces that), which leaves the release the only
// build output not under the working tree. Meanwhile `target/release/` keeps
// whatever `just build` or `cargo build --release` last produced — the
// obvious place to look, and quite possibly months out of date. Copying the
// real artifacts to one predictable place is cheaper than remembering which
// of those two directories is the current one.

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const bundleDir = join(process.env.CARGO_TARGET_DIR || "C:/pallet-build", "release", "bundle");
const releaseDir = join(process.env.CARGO_TARGET_DIR || "C:/pallet-build", "release");
const dist = join(root, "dist");

if (!existsSync(bundleDir)) {
  console.error(`collect-bundle: nothing at ${bundleDir} — run the bundler first`);
  process.exit(1);
}

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

// The installers, wherever the bundler happened to file them.
const copied = [];
for (const kind of ["msi", "nsis"]) {
  const dir = join(bundleDir, kind);
  if (!existsSync(dir)) continue;
  for (const name of readdirSync(dir)) {
    if (name.endsWith(".msi") || name.endsWith(".exe")) {
      copyFileSync(join(dir, name), join(dist, name));
      copied.push(name);
    }
  }
}

// The portable folder: the app and the picker beside it, which is the layout
// `connect_to_picker` requires and the reason these two must travel together.
const ext = process.platform === "win32" ? ".exe" : "";
const portable = join(dist, "portable");
mkdirSync(portable, { recursive: true });
for (const bin of ["pallet-app", "pallet-picker"]) {
  copyFileSync(join(releaseDir, `${bin}${ext}`), join(portable, `${bin}${ext}`));
}
copied.push("portable/");

// Zipped for handing to someone else. PowerShell rather than a zip library:
// it is already on every Windows machine this runs on.
if (process.platform === "win32") {
  const zip = join(dist, "Pallet-portable-x64.zip");
  execFileSync(
    "powershell",
    ["-NoProfile", "-Command", `Compress-Archive -Path '${portable}' -DestinationPath '${zip}' -Force`],
    { stdio: "inherit" },
  );
  copied.push("Pallet-portable-x64.zip");
}

console.log(`collect-bundle: dist/ now holds:\n  ${copied.join("\n  ")}`);

# Pallet task runner. `cargo install just` if you don't have it.

default: check

# Format, lint and test everything.
check: fmt-check lint test

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

build:
    cargo build --workspace --release

# Build the installers and a portable folder: .msi, .exe setup, and a zip.
#
# `CARGO_TARGET_DIR` is not a preference. Tauri's WiX template writes absolute
# paths into an XML document without escaping them, so building from a checkout
# under a directory containing an `&` — "OneDrive - KSB SE & Co KGaA", say —
# emits a `main.wxs` that will not parse, and the bundler fails with nothing
# more helpful than "failed to run candle.exe". Moving only the *target*
# directory somewhere plainer keeps every path WiX interpolates clean, and
# `scripts/stage-picker.mjs` follows it there.
#
# Note that this builds into a different target directory from every other
# recipe here, so it compiles the world the first time it runs.
#
# Finished artifacts are copied back into `dist/` at the end. Without that
# they are the only build output that does not appear in the working tree,
# and the stale `target/release/pallet-app.exe` left over from `just build`
# sits exactly where someone would look for them — close enough to run, old
# enough to reproduce bugs that were already fixed.
bundle:
    cd apps/pallet-app && CARGO_TARGET_DIR=C:/pallet-build node ../../ui/node_modules/@tauri-apps/cli/tauri.js build
    node scripts/collect-bundle.mjs

# Print where Pallet stores things on this machine.
paths:
    cargo run -q -p pallet-cli -- paths

# Run against a throwaway data directory instead of your real library.
sandbox *ARGS:
    PALLET_HOME=.pallet-home cargo run -q -p pallet-cli -- {{ARGS}}

# Run the app against a live, hot-reloading frontend.
#
# `pnpm tauri dev` alone won't do this: the Tauri CLI has to be invoked from
# apps/pallet-app itself (that's where tauri.conf.json and Cargo.toml live),
# not from ui/ where the CLI package is installed, or it can't find the app
# at all — hence reaching across into ui/node_modules instead.
dev:
    cd apps/pallet-app && node ../../ui/node_modules/@tauri-apps/cli/tauri.js dev

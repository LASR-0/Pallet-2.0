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

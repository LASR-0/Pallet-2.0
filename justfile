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

#!/usr/bin/env bash
# Fresh-clone bootstrap. Brings a new macOS/Linux machine to a state
# where `just check` passes.
#
# Usage:
#   git clone https://github.com/mbeaman/mapgen
#   cd mapgen
#   ./scripts/bootstrap.sh
#
# Idempotent. Safe to rerun. Output explains each step so failures
# can be debugged without reading this script.

set -euo pipefail

note() {
    printf '\n\033[1;34m==>\033[0m %s\n' "$*"
}

# 1. rustup — installs the Rust toolchain manager if missing. The
#    project's rust-toolchain.toml then pins the channel; first
#    `cargo` invocation will auto-install the right version.
if ! command -v rustup >/dev/null 2>&1; then
    note "rustup not found — installing via https://sh.rustup.rs"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
        sh -s -- -y --default-toolchain stable --profile minimal
    # Make cargo available in this shell for the rest of the script.
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
else
    note "rustup already installed: $(rustup --version | head -1)"
fi

# 2. just — recipe runner used by the project's dev verbs. Installed
#    via cargo so it stays in $HOME/.cargo/bin alongside the rest of
#    the Rust toolchain.
if ! command -v just >/dev/null 2>&1; then
    note "just not found — installing via cargo"
    cargo install just
else
    note "just already installed: $(just --version)"
fi

# 3. wasm32 target — required by the validation gate's wasm release
#    build. `just setup` is idempotent and handles this; we run it so
#    the bootstrap script is a single source of truth.
note "Running 'just setup' (adds wasm32-unknown-unknown target if missing)"
just setup

# 4. Verify — full validation gate runs fmt + clippy + tests + wasm
#    build. If this passes, the checkout is ready for development.
note "Running 'just check' to verify the gate is green"
just check

note "Bootstrap complete. You're ready to develop."
echo
echo "Common next commands:"
echo "  just              # list all recipes"
echo "  just render-42    # render the canonical seed-42 ornate SVG"
echo "  just render-42-png # same render as PNG via the sweep CLI"
echo "  just perf         # check perf budget against docs/perf_baseline.md"

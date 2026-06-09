# mapgen recipes — one-command verbs for multi-machine development.
#
# Install just itself: `cargo install just`. Then `just setup` once
# per machine, and `just check` as the standard pre-push gate.
# `just --list` shows everything available.

# List available recipes.
default:
    @just --list

# Bootstrap a new checkout — adds the wasm32 rustup target (idempotent).
setup:
    rustup target add wasm32-unknown-unknown
    @echo ""
    @echo "Setup complete. Try 'just check' to run the full validation gate."

# Full pre-push gate — fmt + clippy + workspace tests + wasm release build.
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo build -p mapgen-wasm --target wasm32-unknown-unknown --release

# Workspace tests only (no fmt / clippy / wasm). Fast iteration.
test:
    cargo test --workspace

# Cross-platform determinism golden: run the full pipeline compiled to wasm32
# in Node and assert byte-identity with the native golden. Needs wasm-pack +
# node, so it's kept out of `just check` (the fmath-purity test in `check`
# catches source-level escapes fast; this proves the runtime end-to-end and
# runs in CI). Run it after touching anything in the generate pipeline.
test-wasm:
    wasm-pack test --node crates/mapgen-wasm

# Rewrite formatting in place (cargo fmt --all).
fmt:
    cargo fmt --all

# Generate + render canonical seed-42 ornate map (4k cells) to /tmp/mapgen/.
render-42:
    mkdir -p /tmp/mapgen
    cargo run --release -p mapgen-cli -- generate --seed 42 --cells 4000 --out /tmp/mapgen/w42.json.gz
    cargo run --release -p mapgen-cli -- render --in /tmp/mapgen/w42.json.gz --style ornate_antique --out /tmp/mapgen/w42.svg
    @echo "wrote /tmp/mapgen/w42.svg"

# Same render via the sweep CLI's PNG path — writes SVG + PNG + index.html.
render-42-png:
    rm -rf /tmp/mapgen-sweep
    cargo run --release -p mapgen-cli -- sweep --seed 42 --knob erosion_rate --range 0.04..0.04 --steps 2 --cells 4000 --style ornate_antique --out /tmp/mapgen-sweep
    @echo "open /tmp/mapgen-sweep/index.html in a browser"

# Perf regression gate against docs/perf_baseline.md (fails if >1.5x budget).
perf:
    cargo run --release -p mapgen-world --example perf_baseline -- --check

# Node 18+ and npm must already be installed — they are NOT cargo-installable;
# see web/README.md for how to get Node (apt / brew / nvm).
# One-time web-frontend setup: wasm-pack (via cargo) + npm deps + first wasm build.
web-setup:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v node >/dev/null 2>&1; then
        echo "error: node not found. Install Node 18+ and npm first (apt / brew / nvm); see web/README.md." >&2
        exit 1
    fi
    if command -v wasm-pack >/dev/null 2>&1; then
        echo "==> wasm-pack already installed: $(wasm-pack --version)"
    else
        echo "==> installing wasm-pack via cargo"
        cargo install wasm-pack
    fi
    cd web
    echo "==> installing web dependencies (npm install)"
    npm install
    echo "==> building wasm package (web/pkg)"
    npm run build:wasm
    echo "==> web setup complete. Run 'just web-dev' to start the dev server."

# Frontend unit tests (Vitest) — pure nav/geometry logic in web/src.
web-test:
    cd web && npm test

# Frontend e2e smoke (Playwright) — load → generate → toggle a lens → narrate.
# Needs the wasm pkg built (just web-build) and a Chromium (npx playwright
# install chromium). The Playwright webServer now builds the JS/TS bundle itself
# (it serves dist/), so no separate build here — a bare `npx playwright test` is
# equally safe from stale JS/TS-bundle bugs. (It does NOT rebuild wasm — run
# `just web-build` after a Rust change.) On a Linux distro Playwright doesn't yet
# ship a browser build for (e.g. Ubuntu 26.04), install + run with:
# PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64 (the 24.04 build is binary-compatible).
web-e2e:
    cd web && npm run e2e

# Rebuild the wasm package the frontend consumes (run after Rust changes).
web-build:
    cd web && npm run build:wasm

# Start the Vite dev server at http://localhost:5173 (run web-setup first).
web-dev:
    cd web && npm run dev

# cargo clean (drop target/, reset build cache).
clean:
    cargo clean

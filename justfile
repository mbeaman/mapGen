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

# cargo clean (drop target/, reset build cache).
clean:
    cargo clean

# mapgen

A lore-rich fantasy map generator. Rust + WASM core, geography pipeline grounded
in real Earth science, hand-drawn SVG output (planned), hybrid simulation + LLM
lore engine (planned).

## Status

**Phase 2 + 2.5 + 3a/b/c/d + 3e (MVP) shipped.** The full geography +
society + naming pipeline runs end-to-end on any seed and produces
deterministic worlds with cultures, religions, polities, settlements,
roads, and phonotactic in-world names. The ornate-antique render style
ships an MVP-scope screenshot (parchment, perturbed coastlines, Tolkien
mountains, forest scatter, settlement icons, sacred sites). Phase 3e
polish items (`roughr` pen-jitter, embedded fonts, compass / cartouche,
Imhof label placement, culture×architecture glyph derivation) and Phase
4 (history sim) come next. See
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the locked plan,
[`docs/TASKS.md`](docs/TASKS.md) for active work, and
[`docs/BACKLOG.md`](docs/BACKLOG.md) for deferred items with revival triggers.

## What works today

- Voronoi mesh + Lloyd-relaxed sites + Poisson-disk sampling.
- Plate tectonics → uplift heightmap.
- OpenSimplex2 + IQ domain-warp noise overlay.
- Stream-power + lateral-bank erosion.
- Priority-Flood depression fill with real lake extraction.
- Steepest-descent flow, flow accumulation, river extraction (adaptive
  threshold, highland-headwater rule, per-segment width ∝ √flow).
- Ocean currents (warm western-boundary on continent east coasts).
- 3-cell atmospheric circulation, seasonal climate (NH-summer / NH-winter
  passes with axial-tilt ITCZ shift).
- Köppen-Geiger biome classification (Whittaker LUT fallback).
- RIPARIAN biome override for rivers in arid surroundings (Nile-through-Sahara).
- `LorePatch` overlay foundation (every science stage queries patches for
  per-cell deltas / multipliers / overrides).
- **Cultures stage (Phase 3a)** — per-cell habitat-fitness scoring across a
  5-archetype MVP roster (River-valley Human, Wood Elf, Mountain Dwarf,
  Marginal-lands Orc, Pastoral Halfling) loaded from
  `crates/mapgen-world/data/race_archetypes.csv`. Weighted Voronoi BFS
  assignment writes `culture_id` per land cell; iterative culling drops
  cultures below the 0.3 mean-fitness floor (ARCHITECTURE.md §4 Phase 3a).
- **Four render styles:** `greyscale` (heightmap), `biomes` (Phase-2 data view),
  `cultures` (Phase-3a data view), and `ornate_antique` (Phase-3e marquee —
  parchment background, multi-offset coastline ripples, Tolkien triangular
  mountain glyphs, biome-keyed forest scatter, polity-colored settlement
  icons, dashed roads, sacred-site diamonds; MVP scope, `roughr` pen jitter +
  embedded fonts + compass / cartouche + Imhof label placement deferred).
- Parameter sweep CLI for manual tuning of `erosion_rate`, `base_precip`,
  `lapse_rate`, `axial_tilt` — see [`docs/tuning_log.md`](docs/tuning_log.md).

**Not yet:** history simulation, Claude-narrated chronicles, web frontend,
the remaining Phase-3e ornate-render polish items (roughr-perturbed
primitives, embedded WOFF2 fonts, compass / cartouche / edge-burn,
culture-driven glyph shapes). All tracked in `docs/ARCHITECTURE.md` +
`docs/TASKS.md`.

## Prerequisites

- Rust toolchain via [rustup](https://rustup.rs). Channel is pinned to
  `stable` in `rust-toolchain.toml`; `rustup` will install it
  automatically on first `cargo` invocation. (Developed against the
  current stable; CI tracks the same channel.)
- `wasm32-unknown-unknown` target is only required to build
  `mapgen-wasm`: `rustup target add wasm32-unknown-unknown`.

## Quick start

Generate a world and render it as a biome-colored SVG:

```sh
cargo run --release -p mapgen-cli -- generate --seed 42 --out worlds/w42.json.gz
cargo run --release -p mapgen-cli -- render --in worlds/w42.json.gz \
    --style biomes --out maps/w42.svg
```

Open `maps/w42.svg` in a browser. Default world is 15 000 cells on a
2048×1280 canvas; first run takes ~10 s to compile, ~5 s to generate, and
~1 s to render.

Smaller / faster:
```sh
cargo run --release -p mapgen-cli -- generate --seed 42 --cells 4000 \
    --out worlds/w42.json.gz
```

## Parameter sweep (manual tuning)

Render N maps with one knob varied across a range, plus an `index.html` grid
for eyeball comparison:

```sh
cargo run --release -p mapgen-cli -- sweep \
    --seed 42 --knob erosion_rate --range 0.01..0.10 --steps 8 \
    --out /tmp/mapgen-sweep
```

Open `/tmp/mapgen-sweep/index.html`. Supported knobs: `erosion_rate`,
`base_precip`, `lapse_rate`, `axial_tilt`. Run `mapgen sweep --help` for
each knob's default, typical range, and units.

## Run tests

The full local gate (matches CI):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p mapgen-wasm --target wasm32-unknown-unknown --release
```

All four steps should be silent / green on a clean checkout; the full
test suite runs in seconds.

## Crate layout

| Crate            | Role                                                           |
| ---------------- | -------------------------------------------------------------- |
| `mapgen-core`    | Entity schema, event log types, RNG harness, libm shim, LorePatch |
| `mapgen-geom`    | Voronoi mesh, Poisson-disk sampling, Lloyd relaxation          |
| `mapgen-world`   | Full pipeline: plates → erosion → hydrology → climate → biomes → cultures → religions → polities → naming |
| `mapgen-history` | Agent-based history simulation + append-only event log (stub)  |
| `mapgen-render`  | SVG renderer with pluggable style modules                      |
| `mapgen-lore`    | Claude integration, native only (stub)                         |
| `mapgen-cli`     | Native dev binary (`mapgen generate / render / sweep`)         |
| `mapgen-wasm`    | Browser-facing wasm-bindgen façade                             |

Dependency edges form a DAG: `core ← geom ← world ← history ← render`;
`lore` depends on `core + history`; `cli` depends on everything; `wasm`
depends on everything **except `lore`**.

## Pipeline

```
seed
 ├─► mesh (Voronoi + Lloyd)                        [implemented]
 ├─► plate tectonics → uplift heightmap            [implemented]
 ├─► noise (OpenSimplex2 + IQ warp)                [implemented]
 ├─► stream-power + lateral-bank erosion           [implemented]
 ├─► Priority-Flood depression fill + lakes        [implemented]
 ├─► flow direction + accumulation + rivers        [implemented]
 ├─► ocean currents (gyre limbs, upwelling)        [implemented]
 ├─► 3-cell seasonal climate                       [implemented]
 ├─► Köppen-Geiger → biome (+ RIPARIAN override)   [implemented]
 ├─► cultures (race archetype × habitat fitness)   [implemented]
 ├─► religions (founder culture × alignment spread) [implemented]
 ├─► polities (capital + towns + Dijkstra roads)   [implemented]
 ├─► naming (phonotactic per-language generator)   [implemented]
 ├─► religions (founder culture × spread)          [planned: Phase 3b]
 ├─► polities (Christaller k=4 hierarchy + roads)  [planned: Phase 3c]
 ├─► naming (phonotactic + Markov)                 [planned: Phase 3d]
 ├─► history (Turchin + Mearsheimer loops, 500y)   [planned: Phase 4]
 ├─► ornate SVG render                             [planned: Phase 3e]
 └─► Claude-narrated chronicles (NER-validated)    [planned: Phase 5]
```

Every stage queries `LorePatch` overlays at the end of its computation — magic,
cataclysms, divine intervention, and cultural land impact deviate from physics
*explicitly*, never accidentally.

## License

Dual-licensed under MIT OR Apache-2.0.

# mapgen

A lore-rich fantasy map generator. Rust + WASM core, geography pipeline grounded
in real Earth science, hand-drawn ornate SVG output, browser frontend, and a
hybrid simulation + LLM lore engine (planned).

## Status

**Phase 2 + 2.5 + 3a/b/c/d + 3e + 4 (per architecture spec) shipped.** The
full geography + society + naming + **history** pipeline runs end-to-end on
any seed and produces deterministic worlds with cultures, religions, polities,
settlements, roads, phonotactic in-world names, and a causally-linked 500-year
chronicle (dynasties, wars, schisms, heroes, mythic ages, narrative arcs). The
ornate-antique render style covers every Phase 3e architecture-spec item:
parchment, roughr-perturbed coastlines (4 ripples), Tolkien mountains,
biome-keyed forest scatter, culture × architecture × tier settlement glyphs,
bundled Cinzel / IM Fell English / EB Garamond typography, compass rose, corner
cartouche, edge-burn vignette. A browser frontend (`web/`) drives the wasm build
with live generation, pan/zoom, export, and permalinks. Phase 5 (Claude-narrated
chronicles over the event log) is the next major arc.

Project docs:

- [`docs/SESSION.md`](docs/SESSION.md) — current state, phase, gotchas
  (read first when resuming).
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — locked strategic plan.
- [`docs/TASKS.md`](docs/TASKS.md) — active tactical work.
- [`docs/BACKLOG.md`](docs/BACKLOG.md) — deferred items with revival triggers.
- [`docs/design-influences.md`](docs/design-influences.md) — how the history
  pipeline compares to Dwarf Fortress / RimWorld / Crusader Kings / Caves of Qud,
  and the depth-vs-narratability scope choices behind it.
- [`docs/adr/`](docs/adr/) — architectural decision records (e.g. the multi-scale
  atlas navigation/seam model that anchors the planned refinement framework).
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — dev process, validation gate,
  TDD discipline, commit conventions.
- [`AGENTS.md`](AGENTS.md) — guidance for AI collaborators (persona,
  interaction style, file-read order).

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
- **History stage (Phase 4)** — a deterministic 500-year simulation: a
  system-dynamics backbone (per-polity population / carrying capacity / fiscal
  health / instability / asabiyyah) drives six causal loops (Turchin demographic
  + fiscal, Ibn Khaldun asabiyyah, Mearsheimer power-transition wars, succession,
  religious schism, hero/megabeast sagas) over an agent layer of named
  Characters, Houses, and Dynasties with lineage and blood feuds. Writes an
  append-only, causally-chained `EventLog` plus `HistoryData` (mythic ages +
  classified narrative arcs); wars mutate the political map the renderer reads.
  Inspect with `mapgen events --in world.json.gz`.
- **Four render styles:** `greyscale` (heightmap), `biomes` (Phase-2 data view),
  `cultures` (Phase-3a data view), and `ornate_antique` (Phase-3e marquee —
  parchment, `roughr` pen-jitter coastlines (4 ripples), faint ocean
  hatching, Tolkien mountains with depth shadows, biome-keyed forest
  scatter, culture × architecture × tier settlement glyphs (population-
  scaled towns), dashed roads weighted trunk-vs-branch, dashed polity
  borders, per-pantheon sacred-site glyphs, embedded Cinzel / IM Fell /
  EB Garamond typography, compass rose, cartouche, irregular edge-burn,
  and Imhof simulated-annealing labels — settlements plus river,
  lake, and mountain-range names (the last two via `textPath`)).
- Parameter sweep CLI for manual tuning of `erosion_rate`, `base_precip`,
  `lapse_rate`, `axial_tilt` — see [`docs/tuning_log.md`](docs/tuning_log.md).
- **Browser frontend** (`web/`) — vanilla TypeScript + Vite driving the
  `mapgen-wasm` build through a WebWorker: seed/detail/nations/style
  controls, live stage-by-stage generation, pan / zoom, SVG + 2× PNG
  export, and shareable permalinks. Setup in [`web/README.md`](web/README.md)
  (needs Node + npm + `wasm-pack`; not covered by `scripts/bootstrap.sh`,
  which bootstraps the Rust core only).

**Not yet:** Claude-narrated chronicles (Phase 5) that read the now-populated
event log. The Phase-3e render-polish pass is complete; the only deferred render
item is a hand-authored `docs/target_aesthetic.svg` (a taste reference, revisited
only when starting a new style variant). All tracked in `docs/ARCHITECTURE.md`,
`docs/TASKS.md`, and `docs/BACKLOG.md`.

## First-time setup

On macOS / Linux, one command from a fresh clone:

```sh
./scripts/bootstrap.sh
```

The script installs rustup (if missing) → installs `just` via cargo
→ adds the wasm32 target → runs the full validation gate. Idempotent;
safe to rerun.

If you'd rather do it by hand (or you're on Windows):

1. Install [rustup](https://rustup.rs):
   `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. Install [`just`](https://github.com/casey/just):
   `cargo install just`
3. Bootstrap the project:
   `just setup`
4. Verify with the full validation gate:
   `just check`

(`rust-toolchain.toml` pins the channel; the right Rust version
auto-installs on first `cargo` invocation.)

### Web frontend

The bootstrap above sets up the **Rust core only**. The browser
frontend (`web/`) has its own toolchain. First install **Node 18+ and
npm** (via apt / brew / nvm — they are not cargo-installable), then:

```sh
just web-setup    # installs wasm-pack, npm deps, and builds the wasm package
just web-dev      # Vite dev server → http://localhost:5173
```

Rebuild the wasm package after Rust changes with `just web-build`. Full
detail (layout, dev loop, production build) in
[`web/README.md`](web/README.md).

## Prerequisites (reference)

- Rust toolchain via [rustup](https://rustup.rs). Channel is pinned to
  `stable` in `rust-toolchain.toml`.
- `wasm32-unknown-unknown` target — handled by `just setup`, or
  manually via `rustup target add wasm32-unknown-unknown`.
- [`just`](https://github.com/casey/just) for the project's
  one-command dev verbs. Direct `cargo` invocations remain documented
  below for users who don't want a new tool.

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

Read the generated world's chronicle — mythic ages, the most salient events,
and the narrative arcs the simulation wove:
```sh
cargo run --release -p mapgen-cli -- events --in worlds/w42.json.gz
```

`--min-salience <0..1>` (default 0.8) sets the major-event threshold and
`--limit <n>` caps how many are printed.

## Narrate a chronicle (Phase 5, opt-in)

Turn a major event into an in-world chronicle. **Offline by default** — a
deterministic, grounded template narrator, no key or network:

```sh
cargo run --release -p mapgen-cli -- lore --in worlds/w42.json.gz \
    --event auto-major-war --voice saga
```

`--voice` ∈ `saga | monastic-chronicle | hymn | courtly-letter | peasant-rumor`;
`--event` takes a numeric id or `auto-major-war`; `--out world.json.gz` writes the
world back with the chronicle persisted as a `Work`.

**Real Claude narration is opt-in** at both build and run time, so a stock build
makes no paid call:

```sh
export ANTHROPIC_API_KEY=sk-ant-...
cargo run --release -p mapgen-cli --features lore -- lore --in worlds/w42.json.gz
```

With `--features lore` and a key set it calls Claude (default model
`claude-sonnet-4-6`, override with `MAPGEN_LORE_MODEL`), validates the output
against the world's closed proper-noun set (rejecting any hallucinated name, one
retry, then the template fallback), and never invents people, places, or dates.
Without the feature or the key it stays on the offline template narrator.

For the browser frontend, run the narration **sidecar** (so the API key never
enters page JS) and click "Narrate a major event":

```sh
cargo run -p mapgen-cli --features lore -- serve --port 7878   # POST /narrate
```

The frontend POSTs the world it generated to the sidecar and renders the returned
chronicle. The sidecar also narrates offline (template) when no key is set.

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

The full local gate (matches CI). One command via the justfile:

```sh
just check
```

Or run the four steps directly:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p mapgen-wasm --target wasm32-unknown-unknown --release
```

All four steps should be silent / green on a clean checkout; the full
test suite runs in seconds. `just test` runs only the workspace tests
when fmt / clippy / wasm aren't relevant.

## Crate layout

| Crate            | Role                                                           |
| ---------------- | -------------------------------------------------------------- |
| `mapgen-core`    | Entity schema, event log types, RNG harness, libm shim, LorePatch |
| `mapgen-geom`    | Voronoi mesh, Poisson-disk sampling, Lloyd relaxation          |
| `mapgen-world`   | Full pipeline: plates → erosion → hydrology → climate → biomes → cultures → religions → polities → naming → history |
| `mapgen-history` | 500-year deterministic history sim: agent layer (dynasties/lineage) + six causal loops (Turchin/Khaldun/Mearsheimer/succession/schism/hero) → causal-chained event log, narrative arcs + mythic ages |
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
 ├─► history (6 causal loops + agents, 500y)        [implemented]
 ├─► ornate SVG render                             [implemented]
 └─► Claude-narrated chronicles (NER-validated)    [planned: Phase 5]
```

Every stage queries `LorePatch` overlays at the end of its computation — magic,
cataclysms, divine intervention, and cultural land impact deviate from physics
*explicitly*, never accidentally.

## License

Dual-licensed under MIT OR Apache-2.0.

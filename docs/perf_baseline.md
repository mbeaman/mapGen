# Performance baseline — per component

Reference points for the three user-facing hot paths, so the cost of each is
visible instead of hidden in compounding latency:

- **`generate_full`** — the geography→society→history pipeline (mesh → plates →
  noise → erosion → hydrology → ocean → seasonal climate → biomes → cultures →
  religions → polities → naming → history);
- **`render`** — the ornate SVG build over a generated world, plus the
  zoomed-out planisphere (`Style::Planet`) over an 18k-cell planet;
- **`refine_sector`** — one on-demand zoom-in tile (Phase 7 multi-scale); the
  navigation ADR flags sector latency as directly user-facing, so it carries its
  own budget.

## Numbers

**Re-anchored 2026-05-24 (Phase 4j)** to the current dev box for the *full*
pipeline including the Phase-4 history sim (mesh → … → biomes → cultures →
religions → polities → naming → **history**). The original 2026-05-17 anchor
(17/66/133 ms on a Ryzen 9 5950X at commit `eacba95`) is retired: that machine
is gone, the current box is ~1.6× slower, and the pipeline has since grown the
500-year history stage.

| target cells | median | vs. old Ryzen anchor |
|--------------|-------:|----------------------|
|        4,000 |  26 ms | 17 ms                |
|       15,000 | 111 ms | 66 ms                |
|       30,000 | 259 ms | 133 ms               |

Release build, three seeds (1/2/3) per size, one untimed warmup. History adds a
modest, war-count-bounded cost (per-war O(cells) capacity recompute) on top of
the machine difference. Still roughly linear in cell count.

**`render` and `refine_sector`** were added 2026-05-25 (anchored to the same
box). Render is nearly as costly as generation — the ornate style emits forests,
coastline ripples, mountains, glyphs and labels per cell. A refined sector
recomputes the shared base field from the root seed, runs the physical pipeline
over a haloed sub-mesh, and projects the parent society/hydrology.

**`render` with `Style::Planet`** (the zoomed-out planisphere) was added
2026-06-01. It is the heaviest render path *by cell count* (an 18k-cell planet),
yet far cheaper than ornate in wall time: the planisphere drops the per-cell
ornate clutter (forests, ripples, glyphs), drawing only biome-tinted fills, a
graticule, the major rivers/ranges, and continent/ocean labels. The Mollweide
globe projection (added 2026-06-01) maps every coordinate onto the oval; it is
kept to a per-row table lookup (no per-vertex trig), so 18k planet cells render
in ~19 ms (was ~13 ms equirectangular) versus ~78 ms for 15k ornate cells.

| component       | case       | median |
|-----------------|------------|-------:|
| `render`        | 4,000      |  22 ms |
| `render`        | 15,000     |  78 ms |
| `render` planet | 18,000 (Mollweide planisphere) |  36 ms |
| `refine_sector` | 15k → 4k tile (L2) |  68 ms |

### Environment

- The current dev box (replaced the Ryzen 9 5950X; ~1.6× slower) — see the
  `perf-baseline-machine-gap` project note. `just perf` is the source of truth.
- OS: Linux, x86_64
- Rust: 1.95.0
- Profile: `release` (workspace default)

## Regression budget

**A measurement is a regression if its median exceeds 1.5× the baseline on the
same hardware class.** Budget tables:

| `generate_full` | baseline | budget (1.5×) |
|-----------------|---------:|--------------:|
|           4,000 |    26 ms |         39 ms |
|          15,000 |   111 ms |        166 ms |
|          30,000 |   259 ms |        388 ms |

| `render`        | baseline | budget (1.5×) |
|-----------------|---------:|--------------:|
|           4,000 |    22 ms |         33 ms |
|          15,000 |    78 ms |        117 ms |
| 18,000 planet   |    36 ms |         54 ms |

| `refine_sector`     | baseline | budget (1.5×) |
|---------------------|---------:|--------------:|
| 15k → 4k tile (L2)  |    68 ms |        102 ms |

1.5× was chosen to absorb normal CPU/run noise (~5% × normal hardware
spread × measurement count) while still flagging any single stage that
doubles or worse. Tighten to 1.3× once Phase 3 lands and the pipeline is
stable again.

The budget is **not** in CI; the harness is run manually after work that
touches the science pipeline. It is, however, self-checking — see
"Re-running" below — so a stray `cargo run … -- --check` in a local
gate or developer workflow is enough to catch a regression.

## Re-running

```sh
# Print measurements only:
cargo run --release -p mapgen-world --example perf_baseline

# Self-check: also assert median ≤ budget, exit non-zero on violation:
cargo run --release -p mapgen-world --example perf_baseline -- --check
```

Both modes print a markdown table to stdout with a `status` column
showing `ok` / `OVER` per size. `--check` adds a stderr summary and exits
1 on any violation. The baseline + budget constants live in
`GEN_BASELINES` / `RENDER_BASELINES` / `REFINE_BASELINE` / `PLANET_RENDER_BASELINE`
at the top of
[`crates/mapgen-world/examples/perf_baseline.rs`](../crates/mapgen-world/examples/perf_baseline.rs)
and must stay in sync with the tables above.

If any size exceeds budget, identify which stage grew (`hyperfine` /
`cargo flamegraph` on the example binary), and either:

1. Optimize the offending stage back under budget, or
2. Update both the `BASELINES` table in the harness *and* the numbers
   in this doc, with a commit message explaining why the new cost is
   intrinsic to the feature (e.g., Phase 3a cultures adds a Voronoi-
   assignment pass — fine, but document it).

The harness imports nothing CI-relevant and never runs during `cargo
test`.

## WASM bundle size

Browser distribution is the constraint here — `mapgen-wasm` ships as a
single `.wasm` blob plus a JS shim. ARCHITECTURE.md set a 3 MB budget;
this is the pre-Phase-3 baseline. Phase 3a adds a CSV plus new enums,
which is the first time growth is structurally expected; tracking it now
makes that growth visible instead of cumulative-and-silent.

| artifact                                    | size      | budget | status |
|---------------------------------------------|----------:|-------:|:------:|
| `target/.../release/mapgen_wasm.wasm` (raw) | 152 KB    | 3 MB   | ok     |

Captured 2026-05-17 on commit `43d4e4a`, target `wasm32-unknown-unknown`,
profile `release` (workspace default — `opt-level = 3`, no `lto`, no
`strip`). No `wasm-opt`, no `wasm-strip`, no `--no-default-features` pass
applied; this is the raw `cargo build` artifact a CI pipeline would
produce. We are at ~5 % of budget pre-Phase-3.

### Re-measuring

```sh
cargo build -p mapgen-wasm --target wasm32-unknown-unknown --release
ls -la target/wasm32-unknown-unknown/release/mapgen_wasm.wasm
```

If the artifact crosses 1 MB, set up `twiggy` to attribute bytes by
function / crate, and consider enabling `lto = "fat"` + `wasm-opt -Oz`
before re-anchoring the budget. The `twiggy` integration is tracked in
session state as "WASM bundle size profiling" — not on BACKLOG yet
because the trigger (size > 1 MB) hasn't fired.

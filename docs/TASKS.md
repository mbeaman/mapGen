# Tasks — active work

Living tactical list. Architectural plan: `docs/ARCHITECTURE.md`. Deferred
work: `docs/BACKLOG.md`. Session snapshot (not committed): `.local/session-state.md`.

**Status legend:** `[ ]` todo · `[/]` in-progress · `[x]` done · `[-]` blocked
· `[~]` cut (with link to commit / BACKLOG entry).

**Discipline:** check off in the same commit that lands the work. When cutting
a task, either move it to BACKLOG.md with a revival trigger or note here why
it was abandoned. Don't let stale items linger.

---

## Phase 2.5 — Realism property tests + sweep CLI (~1-2 days)

Goal: lock in regression coverage for the bug classes the user caught this
session, and ship a manual tuning tool to replace the Refinery.

### Test coverage closure

- [x] **(1h) proptest cases for pipeline invariants.**
  - File: `crates/mapgen-world/tests/proptest_invariants.rs`
  - Cases landed: `patch.strength_at` ∈ [0,1] for random disks and rects
    (256 cases each); `generate_full` doesn't panic for seeds 1..100
    (16 cases); `flow_directions` strictly descends across the same
    range. Workspace `proptest` dep is now exercised.

- [x] **(1h) renderer tests.**
  - File: `crates/mapgen-render/tests/svg_invariants.rs`. 4 tests:
    envelope well-formed, polygon count = non-empty cell count, no NaN
    coords, and the common Köppen-reachable biome palette emits on
    seed 42. **Finding:** the original "all 14+RIPARIAN colors emit"
    acceptance was overspecified — `mapgen-world::koppen::to_biome`
    has no class mapping into TEMPERATE_GRASSLAND (id 4) or
    TROPICAL_DRY_FOREST (id 9), so they're unreachable via the
    seasonal Köppen path. The test now asserts the always-reachable
    subset and documents the gap with a pointer at the BACKLOG
    "Wider biome palette" item.

- [x] **(30m) CLI round-trip integration test.**
  - File: `crates/mapgen-cli/tests/roundtrip.rs`. Drives the `mapgen`
    binary end-to-end (generate → render) under a unique temp dir and
    asserts SVG ≥100KB, starts with `<svg` or `<?xml`. `mapgen-cli`
    now has 1 test.

- [x] **(30m) smoke seed sweep.**
  - File: `crates/mapgen-world/tests/smoke_seeds.rs`. Seeds 1..=10
    through `generate_full`; per-seed asserts on land/sea split, ≥3
    land biomes, ≥1 river, ≥1 lake. Runs in 1.5s debug / 0.2s release.

### Sweep CLI

- [x] **(3-4h) `mapgen sweep` subcommand.**
  - Shipped. New `Knob` enum in `crates/mapgen-cli/src/sweep.rs` maps
    `{erosion_rate, base_precip, lapse_rate, axial_tilt}` → field
    overrides on `ErosionParams` / `ClimateParams`. Pipeline now exposes
    `mapgen_world::generate_full_with(params, erosion_params,
    climate_params)`; the original `generate_full` is a thin
    defaults-only wrapper.
  - PNG export wired via `resvg` 0.47 / `usvg` 0.47 / `tiny-skia` 0.12
    (workspace-pinned, native-only — added to `mapgen-cli` only).
    Confirmed acceptance: `mapgen sweep --seed 42 --knob erosion_rate
    --range 0.01..0.10 --steps 8 --out /tmp/mapgen-sweep` writes 8 PNGs
    + 8 SVGs + `index.html`; first run with compile ~14s, subsequent
    runs <1s.
  - `parse_range` / `Knob::parse` covered by 6 unit tests in
    `sweep::tests`.

- [x] **(15m) tuning log.**
  - `docs/tuning_log.md` shipped. Retroactive first pass: every climate,
    erosion, Köppen, hydrology, and biome knob currently in the codebase
    listed with current value, justification, method (sweep / audit /
    derived / reference), and source commit. Closes with three named
    sweep candidates for the next round.

---

## Phase 3 — Cultures + polities + naming + ornate render (~10-14 days)

The aesthetic-payoff phase. Substages each gated by a phase-spec file with
failing tests.

### Phase 3a — Cultures stage

- [ ] (1d) `cultures_spec.rs` failing-test contract
- [ ] (1d) `Race`, `Culture`, `Alignment`, `TechProfile`, `MagicStyle`,
  `SettlementIcon`, `Architecture`, `DiplomaticPattern` enums in
  `mapgen-core/src/entities.rs`
- [ ] (1d) `cultures::populate` — habitat scoring + weighted Voronoi
  assignment writes `culture_id` per cell
- [ ] (4h) `crates/mapgen-world/data/race_archetypes.csv` with 4-5 MVP
  archetypes (1 human variant + 1 elf + 1 dwarf + 1 orc + 1 halfling)

### Phase 3b — Religions

- [ ] (4h) `Religion` entity + `PantheonPattern` enum
  (Mono/Poly/Dual/Animism/Ancestor/CosmicOrder) in `mapgen-core`
- [ ] (4h) `religions::found` — 1-3 religions per world, tied to founder
  culture, spread by alignment compatibility

### Phase 3c — Polities

- [ ] (1d) `polities_spec.rs` failing tests (every settlement reachable
  from its capital; capital on suitable cell; Zipf rank-size)
- [ ] (4h) Suitability-weighted Poisson capitals filtered by
  `Culture.settlement` preference
- [ ] (1d) Christaller k=4 hierarchy + A* roads with reuse discount

### Phase 3d — Naming

- [ ] (1d) Phonotactic generator + Markov fallback, per `Language`
- [ ] (deferred — see BACKLOG.md "Sound-change rules across language families")

### Phase 3e — Ornate render (the screenshot)

- [ ] (4h) Probe `roughr` 0.12 API — `Generator::new` is private; find
  the correct builder entry point. If unworkable, vendor ~600 LOC of
  Rough.js bezier-perturbation algorithm.
- [ ] (2h) Hand-author `docs/target_aesthetic.svg` as the visual reference
  every render decision compares against (per the original Phase 3 day-1
  recommendation).
- [ ] (1d) Parchment background + perturbed coastline (4 offset ripples,
  roughr-jittered)
- [ ] (1d) Tolkien triangular mountain icons + biome-keyed scatter tree
  forests
- [ ] (4h) Typography: bundle Cinzel + IM Fell English + EB Garamond
  WOFF2 in SVG `<defs>`
- [ ] (4h) Compass rose + corner cartouche + vignette + edge-burn aging
- [ ] (1d) Settlement glyphs derived from
  `Culture.settlement × Culture.architecture`
- [ ] (4h) Imhof-style label placement (basic — full simulated-annealing
  variant is post-MVP)

---

## Cross-cutting / hygiene

- [ ] (15m) Bump `SCHEMA_VERSION` (currently v2) when the next breaking
  WorldData change lands (likely with Phase 3a — cultures field).
- [ ] (30m) Baseline performance — measure `generate_full` for 4k / 15k /
  30k cell counts, record in `docs/perf_baseline.md`. Set a regression
  budget (e.g., "must stay under 1.5x of baseline").
- [ ] (deferred — see BACKLOG.md "Cross-platform byte-identical golden
  hashes") `wasm-bindgen-test` for native↔wasm32 hash parity.

---

## Done (recent context)

Last ~30 commits, summarized. Full history in `git log --oneline`.

- [x] Phase 1: workspace + mesh + plate heightmap + greyscale render
- [x] Phase 2: erosion (stream-power + lateral-bank), hydrology
  (Priority-Flood, flow, rivers), climate, biomes
- [x] LorePatch foundation (`mapgen-core/src/patch.rs`)
- [x] 3-cell atmospheric circulation
- [x] Ocean currents (with sign-bug fix mid-session)
- [x] Real lake extraction in Priority-Flood
- [x] Köppen-Geiger seasonal climate + classification
- [x] Realism fix: 5 compounding bugs that produced desert-everywhere maps
- [x] River realism: density, physical headwaters, riparian biome,
  confluence width growth
- [x] Architecture review + lock (constructive + 2 devil's advocates →
  Refinery cut, scope tightened)
- [x] `docs/BACKLOG.md` — 30 deferred items with revival triggers
- [x] `.local/session-state.md` + `.local/resume-prompt.md` (gitignored)
- [x] This task list

---

When adding a new task: keep it small enough to land in one focused
work-session (≤4h). Larger items get a sub-list. When in doubt, prefer
"start one task" over "design five tasks." This file is a working tool,
not a plan.

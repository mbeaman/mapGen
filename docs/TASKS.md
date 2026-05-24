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

- [x] (1d) `Race`, `Culture`, `Alignment`, `TechProfile`, `MagicStyle`,
  `SettlementIcon`, `Architecture`, `DiplomaticPattern` enums in
  `mapgen-core/src/entities.rs`. **Shipped `98c7422`** as the skeleton
  half: types + module stub + `WorldData::cultures` field + schema
  v2→v3 + `Stage::Cultures = 13` (appended). `populate` is `todo!()`.
- [x] (1d) `cultures_spec.rs` failing-test contract. **Shipped** —
  8 tests covering structural invariants (`culture_id` length matches
  cell count, indices in range, sea cells `None`) and ARCHITECTURE.md
  §4 Phase 3a exit criteria (every land cell has a culture, ≥2
  distinct cultures, every roster entry has cells, determinism for
  fixed seed). All RED via `populate()`'s `todo!()` — the contract.
- [x] (4h) `crates/mapgen-world/data/race_archetypes.csv` with 4-5 MVP
  archetypes (1 human variant + 1 elf + 1 dwarf + 1 orc + 1 halfling).
  **Shipped `5ac66af`** (C1). 5-archetype CSV + `RaceArchetype` struct
  + panic-on-bad-data loader + 8 unit tests covering parse/coverage/
  range/biome-validity/error-handling.
- [x] (1d) `cultures::populate` — habitat scoring + weighted Voronoi
  assignment writes `culture_id` per cell. **Shipped `1451d5f`** (C2)
  + wired into `generate_full` in the C3 commit. Algorithm: seed
  selection → multi-source BFS Voronoi → iterative culling. Geometric
  mean across (biome, temperature, elevation, water) axes; greens all
  8 prior RED tests + the deferred `mean_habitat_fitness_per_culture
  _above_floor` test (architecture's 0.3 floor exit criterion). 88
  passing across 17 test suites.

**Phase 3a is complete.** Cultures runs in `generate_full`; tunables
recorded in `docs/tuning_log.md` (Cultures section). Next aesthetic-
payoff substage is Phase 3e (ornate render) or Phase 3b (religions),
depending on whether you want visual payoff or world-shape depth next.

### Phase 3b — Religions

- [x] (4h) `Religion` entity + `PantheonPattern` enum
  (Mono/Poly/Dual/Animism/Ancestor/CosmicOrder) in `mapgen-core`.
  **Shipped `bdfeeb7`** (skeleton).
- [x] (4h) `religions::found` — 1-3 religions per world, tied to founder
  culture, spread by alignment compatibility. **Shipped `c94e856`**
  (after RED spec `b0ff362`). Founders picked by population; pantheon
  derived from founder's `MagicStyle`; spread by 2D-alignment-distance
  Voronoi with 1.5-radius soft cap; sacred sites at top-3 highest-
  elevation adherent cells per religion. 10 spec tests green.

### Phase 3c — Polities

- [x] (1d) `polities_spec.rs` failing tests (every settlement reachable
  from its capital; capital on suitable cell; Zipf rank-size).
  **Shipped `59e84bf`** as RED spec; greens with `48957b8`.
- [x] (4h) Suitability-weighted capital placement filtered by
  `Culture.settlement` preference (reified as habitat-fitness against
  the founding culture's archetype + min-separation BFS).
  **Shipped `48957b8`**.
- [x] (1d) Christaller-style hierarchy (Capital + Towns; villages
  deferred to follow-up) + Dijkstra roads with reuse discount.
  **Shipped `48957b8`**. Min-separation enforces non-clumped towns;
  road cost is `1` for cells in any existing road and `2` for fresh
  cells — produces the trunk-and-branch shape the architecture wants.

### Phase 3d — Naming

- [x] (1d) Phonotactic generator + Markov fallback, per `Language`.
  **Shipped `c74089f`** (after skeleton `9aedddb` + RED spec `6ba0ff6`).
  Phonotactic-only — per-Race language profiles for the 5 MVP races
  (Lalrian / Eldarin / Khuzdic / Grimsh / Greenfolk) + 4 reserve
  languages. Generated names land on settlements, polities, and
  religions, replacing the `"{culture} Capital"` / `"{culture} Realm"` /
  `"{culture} Faith"` templates. Markov fallback **deferred** — see
  BACKLOG.md "Sound-change rules across language families."
- [ ] (deferred — see BACKLOG.md "Sound-change rules across language families")

### Phase 3e — Ornate render (the screenshot)

- [x] (4h-1d actual: ~4h) `roughr` 0.12 pen-jitter coastlines.
  **Shipped** — public API works via `Generator::default()` +
  per-call `Options` (the `Generator::new(opts)` constructor is
  private but `linear_path(..., &Some(opts))` accepts custom options
  per draw). Coastline rendering now traces continuous polylines from
  the cell-edge graph (vertex adjacency walk, closed-loop + open-
  chain detection) and hands each one to roughr's `linear_path` with
  a per-ripple seed + roughness. 4 ripples per arch spec, each with
  its own scratchy Bezier perturbation pass. Includes a workaround
  for a real roughr 0.12 bug where `OpType::Move` is serialized as
  `L` (lineto) instead of `M` (moveto) — `roughr_path_with_move_fix`
  fixes the leading character. wasm32 build needed
  `getrandom = { features = ["js"] }` in mapgen-wasm because rand →
  getrandom otherwise rejects wasm. Pinned by `svg_invariants::
  ornate_antique_coastlines_use_roughr_perturbed_paths`.
- [ ] (2h) Hand-author `docs/target_aesthetic.svg` as the visual
  reference every render decision compares against. **Deferred** — the
  MVP shipped without a target SVG; revisit before tuning glyph
  shapes / palette.
- [x] (1d) Parchment background + perturbed coastline (multi-offset
  ripples). **Shipped `342f606`** — 3 ripples (faded outer +
  middle + dark inner), per-edge wobble via deterministic hash.
  `roughr` upgrade is the open item above.
- [x] (1d) Tolkien triangular mountain icons + biome-keyed scatter tree
  forests. **Shipped `342f606`**. Mountains on ALPINE/SNOW cells,
  scaled by elevation, with snowcaps on SNOW + tall ALPINE. Trees on
  TEMPERATE_FOREST (3), TEMPERATE_RAINFOREST (4), TAIGA (2),
  TROPICAL_RAINFOREST (4) — deterministic per-cell positions.
- [x] (4h actual: ~3h) Typography: bundle Cinzel + IM Fell English +
  EB Garamond static TTFs in SVG `<defs>` via base64 `@font-face`.
  **Shipped** — `mapgen_render::FONTS_TTF` is the single source of
  truth; the renderer base64-embeds the bytes in a `<style>` block,
  and `mapgen-cli::sweep::svg_to_png` registers the same bytes in
  `usvg::Options::fontdb_mut()` so browsers and the PNG path see
  identical typography. WOFF2 was rejected (usvg can't decompress
  brotli for fontdb); subset-static TTFs from Google webfonts-helper
  keep total binary < 200 KB (was 851 KB for EB Garamond variable
  alone). Capital labels use Cinzel, town/village labels use EB
  Garamond, sacred-site labels use IM Fell English Italic — all with
  Georgia / serif fallback. Pinned by `svg_invariants::
  ornate_antique_embeds_vendored_typography_via_at_font_face`.
- [x] (4h actual: ~2h) Compass rose + corner cartouche + vignette +
  edge-burn aging. **Shipped** — 8-point compass rose in the NW
  corner (4 long cardinal spikes + 4 inter-cardinal + medallion + N
  marker in Cinzel); double-bordered cartouche in the SE corner
  with "A MAP OF THE KNOWN WORLD" title; second radial gradient
  (`url(#edge-burn)`) painted on top of map content darkens the
  periphery into aged-paper shadow. All three layers sit above
  labels (edge-burn intentionally fades edge labels into the
  vignette). Pinned by `svg_invariants::
  ornate_antique_renders_compass_cartouche_and_edge_burn`.
- [x] (1d) Settlement glyphs derived from
  `Culture.settlement × Culture.architecture × SettlementTier`.
  **Shipped** — 8 icon silhouettes (Castle/Tower/Hall/Spire/
  Longhouse/Treehouse/Gate/Yurt) × 4 architecture style modifiers
  (stroke / fill darkening / corner radius / Gothic vertical accent)
  × 3 tier behaviors (Capital adds pennant above; Town uses base
  silhouette at 0.75 scale; Village collapses to polity-tinted dot).
  Lookup is `Settlement.polity_id → Nation.capital_cell →
  culture_id[cell] → cultures.cultures[idx]`, cached per polity at
  the start of `render_settlements` so frontier towns inherit their
  founder culture's glyph regardless of which cell they sit on.
  Dispatch is class-marked (`class="settlement icon-X arch-Y
  tier-Z"`) and pinned by an exhaustive 96-combo test in
  `svg_invariants::ornate_antique_dispatches_glyph_for_every_
  icon_arch_tier_combination`. Seed 42 at 4k cells shows
  Riverfolk=Hall+Classical, Wildwood=Treehouse+Organic, Iron Hold
  =Gate+Megalithic, Burning Horde=Longhouse+Megalithic — Greendale
  drops per the known Halfling-fitness issue.
- [x] (4h) Imhof-style label placement (basic — full simulated-annealing
  variant is post-MVP). **Shipped** in the labels follow-up commit —
  settlements, polities, and religion sacred sites all carry SVG text
  labels. Polity labels at territorial centroid; settlement labels
  offset by tier; sacred-site labels small italic. No SA optimization,
  no curve-along-feature; basic positioning. White stroke + dark fill
  via `paint-order="stroke"` gives readability against the parchment +
  forest scatter. Test pinned by `svg_invariants::
  ornate_antique_emits_settlement_labels_with_phonotactic_names`.

**Phase 3e MVP shipped in `342f606`**: parchment + ripples + mountains
+ forests + roads (dashed russet) + settlement icons + sacred-site
diamonds. The screenshot exists; refinement items above are
incremental polish.

### Phase 3e polish (post-MVP)

Promoted from `docs/BACKLOG.md` Render section — trigger fired: user
asked to finish all Phase-3e polish before moving to Phase 4.

- [x] Town size variation by population — town glyphs scale 0.6×–0.95×
  with `Settlement.population` (`town_scale`). Unit-tested.
- [x] Mountain depth shadow — semi-transparent offset triangle under
  each peak (`class="mtn-shadow"`). Pinned in `svg_invariants`.
- [x] Irregular parchment edge burn — hash-positioned dark stains on
  the vignette (`class="edge-stain"`).
- [x] Ocean hatching / contour texture — faint horizontal dashes on
  ~40% of sea cells (`class="ocean-hatch"`).
- [x] Polity border lines — dashed frontier on shared edges across
  `control[]` (`class="polity-borders"`).
- [x] Roads differentiated by trunk vs branch — per-segment stroke
  scales with per-cell road-traversal count (`class="roads"`).
- [x] Sacred sites differentiated by pantheon — 6 per-pantheon glyphs
  (cross / sun / split-disc / leaf / tablet / ring), `class="sacred X"`.
- [ ] Major-river + lake names (needs naming-stage extension).
- [ ] Curve-along-feature labels (rivers / mountain ranges).
- [ ] Lake labels.
- [ ] Imhof simulated-annealing label placement.

---

## Cross-cutting / hygiene

- [ ] (15m) Bump `SCHEMA_VERSION` (currently v2) when the next breaking
  WorldData change lands (likely with Phase 3a — cultures field).
- [x] (30m) Baseline performance — measure `generate_full` for 4k / 15k /
  30k cell counts, record in `docs/perf_baseline.md`. Set a regression
  budget. **Shipped:** 17/66/133 ms median (4k/15k/30k) on Ryzen 9 5950X,
  budget 1.5× baseline. Harness at
  `crates/mapgen-world/examples/perf_baseline.rs`, manual re-run.
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

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
- [x] Major-river + lake names — naming stage labels major rivers
  (≥8 cells) + sizeable lakes (≥3 cells); schema v7.
- [x] Curve-along-feature labels — river names follow the channel and
  mountain-range names follow a west→east spine, both via `<textPath>`
  (`class="river-labels"` / `"range-labels"`). Ranges are clustered +
  named in the naming stage (schema v8).
- [x] Lake labels — centroid point labels (`class="lake-labels"`).
- [x] Imhof simulated-annealing label placement — settlement labels
  placed by SA over 8 candidate positions, minimizing label/glyph
  overlap (`anneal_labels`). Deterministic (fixed-seed xorshift, no exp).

---

## Phase 4 — History simulation + event log (full six-loop scope)

Goal: fill the empty `events` / `entities` log with a deterministic 500-year
sim that reads cultures/religions/polities and writes a causally-linked,
chronicler-ready history. **Scope expanded past the locked MVP** (was Turchin +
Mearsheimer live, four stubbed) to **all six loops + the uplevel layer** —
trigger fired: user directive 2026-05-24 ("time is not a factor" + "uplevel the
output"). Forks resolved with user: full six-loop scope (amends LOCKED
ARCHITECTURE §2/§4 in 4j); Option-B wiring (History is a `PipelineStage`, shows
in the web live build-up, costs a `mapgen-world → mapgen-history` dep and a
`seed42_full` re-anchor per output-changing substage). Plan of record:
`.claude/plans/cosmic-dreaming-comet.md`.

Approach: **hybrid system-dynamics backbone + agent-based character layer.** SD
state vectors (population, carrying capacity, elite count, fiscal health,
instability, asabiyyah, relative power) drive macro transitions over
O(polities)≈8 actors/year; an ABM layer mints named Characters with lifecycles,
lineages, and rivalries. Research basis: Turchin SDT, Ibn Khaldun asabiyyah,
Mearsheimer/power-transition, DF legends mode, Crusader Kings, Caves of Qud.

**Standing constraints for every substage:** determinism via hierarchical
sub-seeding `splitmix64(master, Stage::History) → year → LoopId → entity_id`
(re-rolling one loop can't perturb another — `LoopId` discriminants are
append-only like `Stage`); all transcendentals through `mapgen_core::fmath`;
`IndexMap`/`Vec`/`BTreeMap` only; `tests/history_spec.rs` follows the
synthetic-passes-now / populate-RED-until-impl pattern (model:
`cultures_spec.rs`); each output-changing commit re-anchors
`seed42_full.blake3.txt`; `just check` green per commit, `just perf` within 1.5×
after 4a and 4e.

### Phase 4a — `CausalLoop` trait + tick driver + pipeline wiring *(shipped)*

The foundation — over-invested in the determinism spec; everything inherits it.

**Refinement vs. plan:** schema v9 moved *out* of 4a to its consumers (4b events,
4c entity fields), following the cultures-skeleton precedent (each substage lands
*its* schema). With Option B + no-op loops, `generate_full` output is byte-identical,
so 4a needed **no schema bump and no golden re-anchor** — a clean, low-risk foundation.

- [x] **`CausalLoop` trait + tick driver** in `mapgen-history/src/lib.rs` +
  `loops/mod.rs`: `LoopId` enum (stable append-only discriminants, fixed `ORDER`),
  `TickCtx`, `SimState` scratch (never serialized), `HistoryParams{years:500}`,
  hierarchical sub-seeding (`loop_seed` = `splitmix64(splitmix64(sim,year),loop)`),
  annual driver + `run_with_loops` test seam. Six no-op `impl CausalLoop`. Runs 500
  empty years.
- [x] **Pipeline wiring (Option B).** `mapgen-world/Cargo.toml` gains the
  `mapgen-history` dep; `pipeline.rs` `PipelineStage::History` after `Naming`
  (`ORDER`→11, id/label/weight/run_stage arms); CLI `progressive_style` arm.
  `mapgen-core` exposes `splitmix64` so the sim mixes identically.
- [x] **Determinism spec.** `mapgen-history` unit tests: driver ticks each loop
  once/year in order; `loop_seed` pure + distinct per loop/year; a loop's stream is
  unaffected by adding other loops; `ORDER` matches `default_loops`. `mapgen-core`:
  `Stage::History` independent stream + `splitmix64` purity. `mapgen-world/tests/
  history_spec.rs`: History runs last, no-op emits nothing yet, deterministic.
  Golden hashes pass **unchanged** (no re-anchor). `just check` green; `just perf`
  30k +10% = known machine artifact (this box ≈1.6× the Ryzen anchor), not re-anchored.
- [ ] **fmath-purity guard** — deferred to 4b/4d (the first loop that uses a
  transcendental); 4a introduces no float math, so there's nothing to guard yet.

### Phase 4b — Turchin demographic backbone → first events *(visible-value milestone)*

- [ ] (1d) Per-region/polity logistic population vs carrying capacity (from
  biome/agriculture inputs, in `SimState`); Malthusian stress threshold →
  `Famine`/`Plague`/`Drought` events. **First events appear.** Provisional salience
  from kind-weight.
- [ ] (4h) Spec: event count grows with `years`; valid `year∈[0,years]`, real
  `location` cell, monotonic IDs, `salience∈[0,1]`; famines correlate with
  low-agriculture/drought cells (doc comment cites Turchin secular cycles);
  determinism pin (byte-identical logs). Re-anchor.

### Phase 4c — Agent layer: Characters, Houses, Dynasties, lineage, Titles

- [ ] (1d) Per-polity ruling Characters (`born/died_year`, names via the Phase-3d
  language engine in `naming.rs`), Houses→Dynasties, parent/child via
  `Relationship`, one `Title` per polity + `TitleHolding`. Emits `Birth`/`Death`/
  `Coronation`/`Marriage`.
- [ ] (4h) Spec: `born_year ≤ died_year`; ruler→House→Dynasty chains valid; lineage
  acyclic; coronation follows death; title-holdings have start/end events;
  determinism pin. Re-anchor.

### Phase 4d — Turchin fiscal half + Khaldun asabiyyah *(promotes Khaldun from Phase 6)*

- [ ] (1d) Elite overproduction, fiscal health, instability (Turchin); asabiyyah
  rise on frontier / decay in metropole over dynasty generations (Khaldun). Drives
  rise/collapse → `CityFounded`/`CityAbandoned`/`Migration`/`Exile`; sets
  `Polity.dissolved_year`.
- [ ] (4h) Spec: asabiyyah ∈[0,1] decays monotonically in a stable dynasty absent
  frontier pressure (cites Ibn Khaldun); fiscal collapse precedes dissolution;
  instability rises with elite overproduction (cites Turchin); determinism pin.
  Re-anchor.

### Phase 4e — Mearsheimer power-transition wars + Claims *(first wars)*

- [ ] (1-2d) In-sim relative power per polity (tech/military/population/territory);
  dyadic ratios over `mesh.neighbors` adjacency; Thucydides-trap ignition. Emits
  `WarDeclared`/`BattleFought`/`Siege`/`TreatySigned`/`AllianceFormed` with
  `casus_belli`. Claims system (`Claim` entities + `ClaimAsserted` →
  `CasusBelli::DynasticClaim`). Mutates `society.control[]`/`nations` →
  **post-history political map.**
- [ ] (4h) Spec: every `WarDeclared` has `casus_belli: Some`; participants adjacent
  or share a claim; battles reference valid entity IDs; territory transfers conserve
  total controlled cells; ≥1 event `salience ≥ 0.8`; post-sim `control[]` ≠ pre-sim;
  determinism pin. **Re-check `just perf` (heaviest loop).** Re-anchor.

### Phase 4f — Succession crises *(promotes succession from Phase 6)*

- [ ] (1d) Ruler death + contested heirs (lineage from 4c, claims from 4e) →
  `Succession`, succession wars, dormant claims activating.
- [ ] (2h) Spec: every `Succession` follows a `Death`; contested cases reference ≥2
  claimants; `Claim.dormant` flips correctly; no succession without a prior
  coronation; determinism pin. Re-anchor.

### Phase 4g — Religious schism *(promotes schism from Phase 6)*

- [ ] (1d) Reads `world.religions`; alignment drift between adherent cultures →
  `ReligionFounded`/`Schism`, `CasusBelli::ReligiousSchism`, splinter sect entities
  inheriting + drifting alignment.
- [ ] (2h) Spec: `Schism` references a parent religion; splinter inherits then
  drifts alignment; schism-driven wars carry the right casus belli; determinism
  pin. Re-anchor.

### Phase 4h — Hero/megabeast + Artifacts + Prophecy + mythic ages *(highest-risk; ship minimal)*

- [ ] (1-2d) `MegabeastRise`/`MegabeastSlain`, hero `Ascension`/`Return`,
  `ArtifactForged`/`Stolen`/`Destroyed` (ordered `provenance`),
  `ProphecyUttered`/`ProphecyFulfilled` (pending-prophecy queue; unfulfilled →
  Phase-5 `lacunae`). Mythic-age framing → `HistoryData.ages` — **fixed-window
  ages first**, not turbulence detection.
- [ ] (4h) Spec: every `ProphecyFulfilled` cites its `ProphecyUttered`;
  `MegabeastSlain` cites a prior `MegabeastRise`; artifact provenance ordered
  forge→steal→destroy; ages partition [0,500] with no gaps/overlaps; determinism
  pin. Re-anchor.

### Phase 4i — Uplevel capstone: causal chains + salience + rivalries + arcs

- [ ] (1d) **Causal chaining grammar** populated inline at emission, validated by a
  debug check: fixed small cause-set per effect kind (war→casus event;
  battle/siege→war; succession→death+claim; prophecy-fulfilled→uttered;
  city-abandoned→siege/plague/famine/megabeast). Roots cite nothing. Fan-in cap 1-2.
- [ ] (4h) **Persistent rivalries** — `BloodFeud`/`Rival` edges inherited across
  generations preserving `origin_event`; **minimal first**: binary inherited-or-not,
  dies on house extinction.
- [ ] (4h) **Salience post-sim pass** — `clamp01(0.30·kind_weight + 0.25·actor_
  prestige + 0.15·scale + 0.15·causal_depth + 0.10·rarity + 0.05·first_of_kind)`.
  Fixed linear combo — **no tunable optimizer** (Refinery anti-pattern). Guarantees
  ≥3 events ≥0.8.
- [ ] (1d) **Narrative-arc extraction** → `HistoryData.arcs`: weakly-connected
  components of the salience-floored cause-DAG sharing an actor/house/title/artifact
  → `NarrativeArc{title,kind,start/climax/end_event,member_events,key_characters,
  theme_tags,peak_salience}`. **3 arc kinds first** (HolyWar/HeroSaga/
  DynasticConflict), rest = generic `Chronicle`.
- [ ] (4h) **Phase-5 boundary API** in `mapgen-history` (unused until Phase 5):
  `entity_brief`, `arc_slice` (events + transitive `cause_ids` closure),
  `ner_lexicon` (closed proper-noun set; exact because all names are deterministic).
- [ ] (2h) Spec: cause graph acyclic; ≥X% of wars have non-empty `cause_ids`; ≥3
  events ≥0.8; a rivalry persists ≥2 generations on some seed; `extract_arcs(42)`
  returns ≥1 multi-event arc; arc/age extraction is a pure function of the log.
  Re-anchor.

### Phase 4j — CLI + docs + perf + architecture amendment *(phase "D")*

- [ ] (2h) `mapgen events --in world.json.gz --major` subcommand (MVP exit
  criterion) in `mapgen-cli/src/main.rs`.
- [ ] (2h) README pipeline diagram → 11 stages; check off this list;
  `docs/tuning_log.md` Phase-4 section (growth rate, asabiyyah decay, war-ignition
  power ratio, salience weights, arc/age thresholds); `docs/perf_baseline.md`
  history-stage entry.
- [ ] (1h) **Amend LOCKED `docs/ARCHITECTURE.md` §2/§4** — record six-loop scope +
  the 2026-05-24 trigger (promotion of the four Phase-6 loops). Update
  `docs/SESSION.md`. Update `docs/BACKLOG.md` (cataclysm-clock item's relation to
  the now-live four loops).

**Phase-4 acceptance (extended MVP exit):** `mapgen events --major` lists ≥3
`salience ≥ 0.8` events; ≥200 events with populated `cause_ids`; `extract_arcs`
returns named multi-event arcs; `render-42` shows the **post-history** political
map; determinism pins prove byte-identical logs across runs and native↔wasm.

**Risk register (from the plan's DA):** 4a is the foundation — review sub-seeding
before 4b. 4h is the most speculative (strongest cut candidate; ship minimal).
4i salience must stay a fixed linear combo. Schema-bloat guards: 5 RelationKinds /
6 traits / 1 title-per-polity / 3 arc-kinds / fixed-window ages first; a field
with no mechanical consumer doesn't ship. Perf: aggregate cells → ~8 per-polity
state vectors once, run loops over O(polities) not O(cells); cache adjacency once.
Lock-honoring fallback if direction changes: ship 4a–4e + 4i + 4j, route
4f/4g/4h back to Phase 6.

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

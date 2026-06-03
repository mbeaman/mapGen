# Claims registry

Every claim we make about the generator — in commit messages, in
`docs/inter_continental_design.md`, in `.local/sessionstate.md`, in the README —
should map to **one layer** and to **one test that goes red if the claim becomes
false**. This file is that map.

It exists because of a failure that has recurred four times: a test passes at a
*lower* layer (the data is correct) and the claim quietly inflates to a *higher*
one (you can see it). The mod-5 exclave-invisibility shipped as "done" for
exactly this reason — a data-layer test was green, so "the data is right" was
sold as "you can see the sundering." See `[[feedback_false_green_tests]]` and the
"verify the observable, not the data" lesson.

## The rule

> **A feature is not "done at layer L" until a test exists at layer L.**

If a row's test cell says **GAP**, the claim is *not yet validated at that layer*
— do not describe it as done there. A GAP is a unit of work, not a footnote.

## The layers

| Layer | Proves | Asserted where |
|---|---|---|
| **Determinism** | byte-reproducibility & cross-platform identity (the contract under everything) | blake3 goldens, `cross_platform.rs`, `fmath_purity.rs` |
| **Data** | the `WorldData` is semantically correct after the real `Pipeline` runs | Rust `tests/*.rs` stepping the pipeline on canonical seeds |
| **Replay** | the time channel (`border_changes`) actually *evolves* — the thing animates | year-over-year deltas in history / e2e slider |
| **Observable** | the rendered artifact (SVG / raster / DOM) *shows* the thing | `svg_invariants.rs`, `visual_regression.rs`, Playwright e2e |

**Canonical seeds** live in `crates/mapgen-testsupport` (`REFERENCE_SEED=42`,
`CROSSING_SEEDS=[11,19,7,4]`, `SUNDERED_SEEDS=[23,42]`, `SIZABLE_BODY_MIN=24`) and
in the param builders `reference_params` / `planet_params`. Reference them by
name, never as a magic literal.

The **Red-mutation** column is the one-line change that *should* flip the test
red — it is the proof the test is not false-green. Each new claim test must ship
with its red-mutation recorded here and verified once by hand.

---

## Determinism contract (cross-cutting)

| Claim | Layer | Seed | Test | Red-mutation |
|---|---|---|---|---|
| Same seed → byte-identical world | Determinism | 42 | full `WorldData`: golden `tests/golden/seed42_full.blake3.txt` (via `pipeline_spec.rs::full_pipeline_golden_hash`) — the load-bearing cite. Geography run-to-run only: `determinism.rs::same_seed_same_world` (2000 cells, `generate()`, no society/history) | perturb any `StageRng` draw or a `BTree` iteration order |
| Different seed → different world | Determinism | 42 vs other | `determinism.rs::different_seed_different_world` | ignore the seed in any stage |
| Native ≡ wasm32 byte-identical (full pipeline) | Determinism | 42 | `mapgen-wasm/tests/cross_platform.rs::full_pipeline_golden_hash_matches_native_under_wasm` | replace an `fmath` call with raw `f32::sin` in a stage |
| Native ≡ wasm32 byte-identical (refined sector) | Determinism | 42 sector | `cross_platform.rs::refined_sector_golden_matches_native_under_wasm` | change a refine-path transcendental |
| No raw transcendentals in any `src/` | Determinism | — | `mapgen-history/tests/fmath_purity.rs` | add `x.sin()` / `.powf()` in any crate `src` |
| Pipeline stepping ≡ `generate_full` (coarse) | Data | 42 | `pipeline_spec.rs::stepper_matches_generate_full` | skip a stage in the stepper |
| Pipeline stepping ≡ `generate_full` (fine) | Data | 7 | `pipeline_spec.rs::fine_stepper_matches_generate_full` | merge two fine erosion sub-steps |
| `WorldData` round-trips through JSON | Data | 42 | `determinism.rs::full_world_round_trips_through_json` | drop a `#[serde]` field |

## Foundation — landmass-distinct society (schema v19)

| Claim | Layer | Seed | Test | Red-mutation |
|---|---|---|---|---|
| Each culture is instanced per landmass (same archetype on 2 continents → 2 ids) | Data | planet | `cultures_spec.rs::cultures_are_instanced_per_landmass` | revert `populate` to the global roster build |
| Polities are confined to one landmass **at gen-time** (0 spanning) | Data | planet | `cultures_spec.rs::polities_are_confined_to_one_landmass_yet_the_planet_is_populated` (stepped to `PipelineStage::Polities`) | stamp control by global culture id again |
| The planet stays earned-sparse (land controlled, not gutted) | Data | planet | same test (≥80% sizable-body land) | confine cultures to a single cell each |
| No culture instance spans a sea-lanes body (predicate regression guard) | Data | planet | `cultures_spec.rs::no_culture_instance_spans_a_sea_lanes_body` — cultures & sea_lanes now share the one canonical land predicate `> 0.0`, so this holds by construction; the test guards against re-diverging | re-introduce `>= 0.0` in `cultures::populate` so a 0.0 cell could bridge two `>0.0` bodies |
| seed 42 (single landmass) is a byte-identical no-op under v19 | Determinism | 42 | the goldens above (re-anchored for the version byte only) | make instancing fire on a single-body world |

## Sea-lane substrate (Phase 1 Step 3a)

| Claim | Layer | Seed | Test | Red-mutation |
|---|---|---|---|---|
| Every crossing seed grows a both-tier lane graph (a crossable strait `≤40` **and** an open-ocean wall `>40`) | Data | 11, 19, 7, 4 | `sea_lanes_spec.rs::every_crossing_seed_grows_lanes_spanning_both_tiers` (tier split 11→6/9, 19→4/6, 7→3/5, 4→3/5) | clamp `min_naval` to a single tier |
| A one-cell strait forms a lane the sea-scan alone would miss | Data | synthetic | `sea_lanes_spec.rs::one_cell_pinch_strait_forms_a_lane_the_sea_scan_alone_would_miss` | drop the sea→land pinch scan |
| A strait maps to crossable, open ocean to a wall | Data | synthetic | `sea_lanes_spec.rs::synthetic_fixture_maps_strait_to_crossable_and_ocean_to_wall` | invert the cost gate |
| Lanes are deterministic for a fixed seed | Determinism | 19 | `sea_lanes_spec.rs::lanes_are_deterministic_for_a_fixed_seed` | key the lane heap on a non-stable tiebreak |
| Sundered seeds yield no earned crossing (lanes exist but are gated too-expensive) | Data | 23, 42 | `sundered_lanes_claims.rs::no_cross_water_conquest_on_any_sundered_seed` pins the *outcome*. The mutation proof showed seed 23 *does* chart inter-body lanes (they'd carry `[0,2,8]` with the gate off) — so "sundered" = gated, not laneless. The substrate-level "no lane crossable at achievable naval" is confirmed but not a standing assertion | (see the carrier gate row) |
| Sundered seeds carry only walls (no crossable lane) | Data | 23, 42 | covered by the carrier outcome test (`sundered_lanes_claims.rs::no_cross_water_conquest_on_any_sundered_seed`) — see the carrier section | (see carrier gate row) |
| Only the open ocean bridges landmasses — no inland lake forges a lane (latent-hazard guard) | Data | 11, 19, 7, 4, 23, 42 | `sea_lanes_spec.rs::only_the_open_ocean_bridges_landmasses_no_inland_pool_does` (mutation-verified). The algorithmic fix is deferred — strait ≡ bridging-lake topologically here; needs ocean-connectivity geometry | a non-dominant `<=0.0` sea pool comes to border ≥2 sizable bodies |

## Carrier — beachhead cross-water conquest (Phase 1)

| Claim | Layer | Seed | Test | Red-mutation |
|---|---|---|---|---|
| Earned cross-water conquest fires on **every** crossing seed | Data | 11, 19, 7, 4 | `sundered_lanes_claims.rs::cross_water_conquest_fires_on_every_crossing_seed` (mutation-verified: early-returning `cross_water_targets` flips it red on seed 11) | early-return from `cross_water_targets` |
| **No** earned crossing on any sundered seed — the naval gate is load-bearing | Data | 23, 42 | `sundered_lanes_claims.rs::no_cross_water_conquest_on_any_sundered_seed` (mutation-verified: removing the gate makes seed 23 span `[0,2,8]`) | remove the `lane.min_naval > naval_a` gate |
| Exact per-seed crossing counts (11→4, 19→3, 7→2, 4→1) | Data | 11, 19, 7, 4 | **GAP (intentional, won't fix)** — *presence* is pinned per seed; exact counts are deliberately not (any history tweak shifts them → brittle regression gate) | n/a |
| The earned crossing flips in at a specific year under the slider's reconstruction | Replay | 11, 19, 7, 4 | `sundered_lanes_claims.rs::the_earned_crossing_replays_faithfully_in_the_time_slider` (mutation-verified: off-by-one in `control_at_year` flips it red; uses the slider's own `control_at_year` — earned cell is *not* the conqueror's at year-1, *is* at year, and `control_at_year(last) == society.control`) | `>` → `>=` in `control_at_year` |
| Scrubbing the slider **visibly** reveals the overseas exclave (web/DOM) | Replay+Observable | crossing | **PARTIAL.** Legibility is now built (row above) so a human *sees* the distinct exclave appear; but an automated e2e still can't point at *the* exclave region — the planisphere is per-cell polygons with no per-region polity id in the DOM. Remaining work: expose polity per region (e.g. group wash polygons by realm) so an e2e can assert the specific exclave flips in | (deferred — needs DOM polity exposure) |
| Bordering realms (incl. the overseas exclave) render in distinct colors | Observable | 11, 19, 7, 4 | `political_legibility.rs::{bordering_realms_render_in_distinct_colours, the_overseas_exclave_is_colour_distinct_from_the_realms_it_borders, the_political_wash_renders_exactly_the_realms_legible_colours}` (mutation-verified: disabling `recolor_political` reverts the adjacency test to red). FIXED by post-history greedy graph-coloring (`polities::recolor_political`) replacing mod-5 `polity_color` | disable `recolor_political` |
| Every territorial change visibly flips a cell's color in the slider | Replay+Observable | 4@2k, 11, 19, 7 | `political_legibility.rs::every_territorial_change_flips_the_rendered_colour` (mutation-verified: dropping the from↔to edges reverts it to red). A conqueror that retreats is no longer present-adjacent to its victim, so present-adjacency alone froze the slider — caught by the seed-4 e2e; fixed by also joining `from`↔`to` of every `border_change` in the coloring graph | drop the `border_change` from↔to edges in `recolor_political` |

## Planet & globe presentation

| Claim | Layer | Seed | Test | Red-mutation |
|---|---|---|---|---|
| Planisphere is a Mollweide oval; drill unproject stays correct | Observable | — | `web/src/sector.test.ts` (mollweide project/unproject duality) + shared vector pin (`crates/mapgen-render` + web) | perturb the Mollweide forward constant in one place only |
| A planet generates and a continent drill refines (≥ L2) | Observable | 4 | `smoke.spec.ts::generates a planet, then drills into a continent` | make the drill no-op |
| `?scale=planet` permalink reloads as a planet | Observable | — | `smoke.spec.ts::?scale=planet permalink reloads as a planet` | drop the permalink read |
| The globe renders a frame and applies the texture | Observable | — | `smoke.spec.ts::globe scale mounts a 3D sphere and renders a frame` (`data-rendered`/`data-textured`) | never call `renderer.render` |
| The globe locks the style control & survives scale toggling | Observable | — | `smoke.spec.ts::globe locks the style control and survives scale toggling` | leak the GL context on swap |
| The political wash animates over years | Replay | 4 | `smoke.spec.ts::planet time-slider animates political control` + `svg_invariants.rs::planet_style_washes_in_political_control_and_animates_with_history` | freeze the wash to year 0 |

## Render invariants

| Claim | Layer | Seed | Test | Red-mutation |
|---|---|---|---|---|
| Biomes SVG well-formed; polygon count = cell count; no NaN coords | Observable | 42 | `mapgen-render/tests/svg_invariants.rs::{svg_envelope_is_well_formed, polygon_count_matches_cell_count, no_nan_coordinates_in_output}` | emit one stray `<rect>` / a NaN coord |
| Ornate / planet / every preset rasterize to a sane image | Observable | 42 | `mapgen-cli/tests/visual_regression.rs::{ornate_render…, planet_render…, every_preset…}` | render a blank/uniform canvas |
| Ornate draws borders between adjacent polities | Observable | 42 | `svg_invariants.rs::ornate_antique_draws_borders_between_adjacent_polities` | skip the border layer |

---

## Open gaps (the work this registry exposes)

1. ~~**Observable — exclave distinctness**~~ *(CLOSED, substage 4)*: `recolor_political`
   greedy-graph-colors the polity adjacency post-history; `political_legibility.rs`
   pins it (mutation-verified). Goldens re-anchored (delta proven to be `nation.color`
   only). **Residual:** an *automated* e2e that scrubs to the crossing year and asserts
   *the* exclave region appears still needs per-region polity exposure in the DOM
   (the colors are now distinct, so a human sees it; the machine can't yet point at it).
2. ~~**Replay — earned crossing → year-specific flip**~~ *(Rust side CLOSED, substage 3)*:
   `the_earned_crossing_replays_faithfully_in_the_time_slider` pins it via the
   slider's own `control_at_year`, mutation-verified. The **web/DOM** half ("scrubbing
   *visibly* reveals the exclave") is blocked on legibility → folded into #1.
3. ~~**Data — sundered seeds grow no crossable lane**~~ *(CLOSED, substage 2)*: the
   *absent* half is now pinned in both directions by `sundered_lanes_claims.rs`,
   mutation-verified.
4. ~~**Data — both-tier substrate across all crossing seeds**~~ *(CLOSED)*: all four
   crossing seeds now assert a crossable strait + an open-ocean wall. (Exact
   per-seed crossing *counts* are intentionally never pinned — brittle.)

## How to extend this file

When you add a feature, add its rows here *in the same change* as its tests:
name the layer, the canonical seed, the test `path::fn`, and the red-mutation you
verified flips it red. When you claim something is done, grep this file first —
if the row says GAP at that layer, the claim is wrong.

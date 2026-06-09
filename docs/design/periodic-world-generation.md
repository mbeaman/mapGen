<!-- Design doc: longitude-periodic world generation (seamless globe).
Provenance: mapped by a 7-reader understand-workflow over the generation pipeline +
synthesized (2026-06-08). Decision driver: the globe became the primary view and the
flat world's antimeridian coastlines don't align — rotate-to-ocean only hides it, so we
do the proper fix. STATUS: ✅ DONE (2026-06-09) — ALL PHASES 0–6 SHIPPED. The planet is
longitude-periodic end-to-end and the globe is seamless (base + drilled, screenshot-
confirmed at the antimeridian). Commits: Phase 0–4 `d3d5af1`/`5980418`/`44d9694`/`d2ccb07`/
`e204daf`; Phase 5 plumbing `443b3df`, THE FLIP `f426c31`, colony-tag guard `e3c2ebc`;
Phase 6 (render/web) `4f2e46a`. Goldens re-anchored + native↔wasm re-verified (5/5).
This supersedes the rotate-to-ocean option in docs/BACKLOG.md "SEAM (#1)". -->

# Longitude-Periodic World Generation (Seamless Globe) — Phased Implementation Plan

## 0. Ground truth corrections to the stage maps (read this first)

The per-stage maps were largely accurate on *mechanism* but wrong on two facts that reshape the cost and the scoping strategy. Both were verified against the code:

- **Continental goldens are 1024×640, not 2048×1280.** `pipeline_spec::fixed_params(42)` and `testsupport::reference_params` are `1024×640, 4000 cells, 12 plates`. The 2048×wide numbers in the maps are the *runtime* presets (`GenerateParams::default`/`planet`), not what the goldens hash. Re-anchor reasoning must use the test params.
- **The planet preset is NOT golden-free.** A planet golden already exists — `crates/mapgen-world/tests/golden/seed9_planet_full.blake3.txt` (added Jun 7, schema v22) — pinned by `pipeline_spec::planet_seed9_far_shore_golden_hash`, with a native↔wasm twin in `mapgen-wasm/tests/cross_platform.rs`. Several stage maps asserted "no golden covers planet-scale"; that is stale.

And the cost the maps entirely missed:

- **The Sundered Lanes seed lists are empirically-observed behavioral contracts at planet scale.** `mapgen-testsupport` defines `CROSSING_SEEDS = [11,19,7,4]` (with documented crossing counts 11→4, 19→3, 7→2, 4→1), `SUNDERED_SEEDS = [23,42]`, `COLONIZE_SEEDS = [2,5,9,11,18]`. A whole suite (`trade_claims`, `diffusion_claims`, `faith_crossing_claims`, `cultures_spec`, `history_spec`) asserts these. Crucially, `naming::connected_bodies` (naming.rs:466-480) walks `mesh.neighbors` to define "landmass" — so **once neighbors wrap, any continent straddling x=0/x=width merges into one body.** X-periodicity does not merely shift these seeds' numbers; it can **invert the sundered/crossing taxonomy** (a sundered pair becomes land-connected across the seam; a crossing seed loses a sea lane). These seeds must be **re-derived from the Step-0 lane probe**, not blindly re-anchored. This is a first-class cost, larger than the single seed9 golden.

The consequence for scoping (Section 3): **do not couple periodicity to the `planet()` preset during bring-up.** Make it an independent flag, prove it on a *new* golden + an invariant test, and flip `planet()`→periodic only as a final, isolated commit.

---

## 1. Topology approach (the foundation everything depends on)

**Recommendation: ghost-point neighbor extraction with a zero-iteration rebuild, used for adjacency only. Real-cell sites and Lloyd relaxation stay byte-identical to today.**

The cell adjacency in this codebase comes entirely from voronoice's Delaunay triangulation of the input sites (`mesh.rs:140-145`, `iter_neighbors()`). voronoice has no toroidal mode and its `BoundingBox` is a hard rectangle. The clean, well-defined way to get a *wrapped* Delaunay without writing our own triangulator is the standard ghost-copy trick: duplicate the sites within a seam margin to `x ± width`, triangulate, and read off which real cells become neighbors *through* a ghost.

The trap the stage maps did not surface is **Lloyd relaxation**. `lloyd_iterations = 2` runs *inside* voronoice over whatever sites you pass it. If ghosts participate in relaxation, each ghost drifts independently of its original across iterations and silently destroys the periodicity you set up (the seam stops being a mirror of itself). So the build is split into two distinct voronoice calls:

1. **Relaxation pass (unchanged):** run `VoronoiBuilder` exactly as today, `lloyd_iterations = 2`, no ghosts. Read back the relaxed real-cell sites. On the non-periodic path this is the *only* pass and is bit-for-bit identical to today.
2. **Ghosted adjacency pass (periodic only):** take the relaxed real sites, append ghost copies of every real site whose `x < margin` (as `x + width`) and `x > width - margin` (as `x - width`), with a `margin` of a few cell diameters (`~3 * min_dist`). Rebuild with `lloyd_iterations = 0`. For each real cell, walk `iter_neighbors()`; if a neighbor index is a ghost, map it back to its real owner and add that real cell id to the neighbor list (deduplicated, skipping self). Discard the ghost voronoi.

Properties that make this the right choice:

- **Real-cell sites and indices `0..n` are unchanged.** Only `MeshData.neighbors` gains a few cross-seam entries at seam cells. Per-cell golden arrays (elevation, plate_id, etc.) therefore change *only* at seam-adjacent cells — a thin column — and the continental path is provably byte-identical (Section 4).
- **Latitude does not wrap.** No ghosts in y; the poles stay hard edges. This is a cylinder, not a torus — correct for a globe.
- **Symmetry is preserved by construction** if you add the wrapped edge to *both* endpoints (the existing `neighbor_symmetry` test in `mesh.rs` and `world_data.rs` will guard this).

Rejected alternatives: a hand-rolled periodic Delaunay (correct but a large, high-risk new geometry kernel — not worth it for an x-only wrap); periodic Lloyd that relaxes ghosts in lockstep (more code, perturbs real sites → breaks continental byte-identity for no benefit); pure post-hoc "stitch sites near x≈0 to sites near x≈width by distance" without a triangulation (fragile — picks wrong pairs, can't reproduce true Delaunay adjacency, fails the continuity invariant).

---

## 2. Which stages wrap for free vs. need explicit change

Periodicity has two independent concerns: **topology** (who is adjacent) and **coordinate metric** (raw-x deltas used for direction/sign/sort). Adjacency-driven stages inherit topology for free; coordinate-driven stages need explicit minimum-image dx.

**Wrap for free (adjacency only — zero code change, but add a proving test):**
- **Erosion** (`erosion.rs`) — flow routing, accumulation, lateral deposition all iterate `neighbors[i]`. No coordinates. RNG param is unused.
- **Hydrology** (`hydrology.rs`) — coast detection, depression fill, flow directions, river extraction all neighbor-driven, tie-break by cell index. No coordinates.
- **Continents / landmass partition** (`naming::connected_bodies`) — neighbor BFS; wraps automatically. (This is the double-edged one: it *correctly* merges seam-straddling continents, which is what re-derives the seed taxonomy — see Section 4.)
- **Cultures / polities / sea_lanes / biomes' core** — keyed by cell id and habitat/terrain lookups, not coordinates.

**Need explicit minimum-image dx change (coordinate-based):**
- **Plates** (`plates.rs`): (a) plate-assignment metric `sq_dist` (line 66/130) must wrap the x-delta: `dx = min(|dx|, width - |dx|)`; (b) stress normal `nx = si[0] - sj[0]` (lines ~92) must use the wrapped dx, or stress sign inverts across the seam (a plate pair at x=2047 and x=1 currently yields `nx≈2046`, sign-wrong). Plate-center seeding (line 30) stays raw `0..width`, no mirroring — periodicity is in the metric, not the seed locations. **RNG stream is unperturbed** (all draws precede assignment).
- **Noise** (`noise.rs`): the only stage where wrapping is a *coordinate-space remap*, not a dx fix. Sampling `height.get([sx+wx, sy+wy])` with raw x makes x=0 and x=width different inputs → guaranteed seam discontinuity. The fix is a **cylinder**: map longitude to a circle and feed 3D noise. **The stage map's `get([cos(lon), lat])` is a bug** — cos is even, so θ and 2π−θ collide and you get a mirror-image planet. Verified: the `noise` 0.9 crate's `OpenSimplex`/`Fbm` implement `NoiseFn<f64,3>`, so the correct call is `height.get([R*cos(θ), R*sin(θ), lat])` with `θ = (sx+wx)/width * TAU`. All trig must route through `mapgen_core::fmath` (not `std`) or wasm drifts from native. Pick `R` so the along-seam frequency matches the interior (tune `R ≈ width / TAU` so arc-length ≈ original x); validate visually.
- **Climate precipitation upwind march** (`climate.rs:105-147`, duplicated in `climate_seasonal.rs:124-147`): two sub-problems. (a) the upwind-neighbor dot and the global sort key use raw `sites[..][0]` — wrap the dx in the upwind dot (line 124) and be aware the global sort key `sites[0]*w[0]` becomes multi-valued on a cylinder. (b) **The real hazard**: the single-pass march assumes an open upwind boundary (`upwind=None ⇒ saturated ocean at domain edge`, line ~135). On a cylinder east-edge feeds west-edge, creating a **cyclic dependency** — a single sorted pass reads stale moisture at the seam → a visible biome seam (the exact failure mode we're eliminating). Fix options, in order of preference: (i) designate one meridian as a "seam cut" that keeps the open-boundary assumption (cheapest, one acceptable hairline of discontinuity hidden inside an ocean band), or (ii) iterate the march to moisture convergence at the seam (cleanest, costs passes). Both `climate.rs` and `climate_seasonal.rs` must change in lockstep or seasons desync at the seam.
- **Ocean currents** (`ocean.rs:60`): gyre east/west sign uses raw `sites[j][0] - sites[i][0]`. Wrap that dx, else the gyre limb (warm vs cold boundary current) flips at the seam.

---

## 3. Preset scoping — exact flag and branch point

**Add an independent `periodic: bool` to `GenerateParams` (lib.rs:30), defaulting `false`, threaded into `MeshBuildParams` (mesh.rs:12) as a new field.** This is the single global switch; once set at `Mesh::build`, the topology is fixed for all downstream stages, which is correct (you cannot gate periodicity per-stage — adjacency is global).

The dx-wrapping stages (plates, climate, ocean) read the period from `mesh.width` and branch on the flag (or, equivalently, infer `periodic` from the threaded flag; do **not** infer from aspect ratio — the maps suggested `width==2*height`, but that silently couples two unrelated concepts and would mis-fire if a future preset shares the aspect).

**Do not set `planet()`→periodic in the foundational phases.** The maps recommended exactly this; it is the trap. If `planet()` flips early, Phase 0 simultaneously moves the `seed9_planet_full` golden *and* perturbs ~20 behavioral claim assertions (CROSSING/SUNDERED/COLONIZE), and you lose the ability to tell "my ghost-stitch code is wrong" from "geography legitimately moved." Instead:

- Phases 0–4 add the capability behind `periodic`, defaulted off everywhere. Continental goldens, the existing `seed9_planet_full` golden, and all claim seeds stay byte-identical (proven by the byte-identity test in Section 4).
- Periodicity is exercised during bring-up via a **new, dedicated periodic fixture** (e.g., a `periodic_planet(seed)` params builder, or `planet()` + explicit `periodic:true` in tests) with its **own new golden**, plus the seam-continuity invariant.
- The flip of the *shipping* `planet()` preset to `periodic: true` is its own final commit (Phase 5) whose entire diff is: set the flag, re-anchor `seed9_planet_full` + its wasm twin, and **re-derive** CROSSING/SUNDERED/COLONIZE from the Step-0 probe. One reviewable PR isolates the expensive, judgement-heavy churn.

**Sub-region meshes stay non-periodic.** `Mesh::build_region` (mesh.rs:91, used by `scale::refine_sector`) tiles a sub-rectangle; a drilled sector is a continental view even from a planet parent. Hardcode the region path non-periodic (no flag threaded). This keeps periodicity root-only and drilling-invariant.

---

## 4. Determinism / golden strategy

**RNG impact is zero on the non-periodic path and structural (not stream-perturbing) on the periodic path.** The ghost rebuild draws no RNG (it reuses already-relaxed sites). Plates/ocean/climate dx-wraps are pure functions of position behind an `if periodic` — they reorder nothing. So determinism reduces to two guarantees:

**(A) Continental byte-identity (the safety net).** Add a test: generate `reference_params(42)` with `periodic:false` (the default) and assert the world's blake3 equals the committed `seed42_full` golden — i.e., the change is provably a no-op when off. This is the single test that lets reviewers trust that no continental/planet golden moved during Phases 0–4. Pair it with the existing `neighbor_symmetry` test (extend it to run on a periodic mesh too).

**(B) Cross-platform determinism for the new ops.** Every new transcendental/branch — wrapped-min dx, cylinder `cos/sin` — routes through `mapgen_core::fmath`, never `std`. The three-branch `min(|dx|, width-|dx|)` can wobble across `x=width/2` on wasm vs native; extend `mapgen-wasm/tests/cross_platform.rs` to hash a periodic world on both targets.

**Goldens that move (only at Phase 5, the deliberate flip):** `seed9_planet_full` and its wasm twin re-anchor wholesale (wrapped topology cascades through every neighbor-coupled stage). The continental goldens (`seed42_full/phase2/sector`) never move — they are 1024×640, never periodic. Bump `SCHEMA_VERSION` only if a `MeshData` field is added; the cleanest no-schema-bump route is to thread `periodic` as a *param* and not store it in `MeshData` (the topology is already baked into `neighbors`). If you do store it, use `#[serde(default)]` so laneless/pre-periodic worlds load unchanged.

**Behavioral seeds that must be re-derived (not re-anchored) at Phase 5:** CROSSING_SEEDS, SUNDERED_SEEDS, COLONIZE_SEEDS and their documented counts. Re-run the Step-0 lane probe on periodic planet worlds, observe the new crossing/sundered partition, and update `mapgen-testsupport` constants + their doc comments. Expect the partition to change because seam-straddling continents merge (`connected_bodies` wraps) and sea lanes at the seam reconnect.

**The periodicity proof — the seam-continuity invariant test (new, lives near `mesh.rs`/`pipeline_spec`):** this is the test that proves the feature, and per the false-green lesson it must assert a signal only periodicity can produce:
1. **Adjacency:** there exists a cell `i` with `site[i].x < ε` whose `neighbors` contains a cell `j` with `site[j].x > width - ε`. (Fails on a clamped/non-periodic mesh.)
2. **Geometric continuity:** for such a seam pair, the *wrapped* x-distance is small and the elevation (post-noise) differs by less than a threshold — i.e., geography is continuous across x=0/x=width, not merely topologically linked. Assert `|elev[i] - elev[j]|` is comparable to a typical interior neighbor delta, *and* that the same generation with `periodic:false` produces NO such x≈0↔x≈width neighbor (a differential assertion — the seam link exists only when the flag is on).
3. **Flow crosses the seam (free-via-adjacency, but worth a signal):** assert at least one river/flow edge in `hydrology` links a seam pair — proves the "erosion/hydrology wrap for free" claim rather than assuming it.

---

## 5. Phasing (ordered, each independently shippable & testable)

**Phase 0 — Mesh ghost topology behind the flag. [HIGH]**
Add `periodic` to `GenerateParams` + `MeshBuildParams`; implement the two-pass ghost build (Section 1). Ship with: the continental byte-identity test (A), the extended `neighbor_symmetry` on a periodic mesh, and the seam-**adjacency** half of the invariant test. Nothing downstream changes yet. This is the foundation; everything else depends on it.

**Phase 1 — Coordinate-metric wraps in plates + ocean. [LOW–MED]**
Wrapped dx in `sq_dist` and stress-normal (`plates.rs`), wrapped gyre dx (`ocean.rs`), all behind `if periodic`. Test: a plate pair straddling x=0 has sign-continuous stress; gyre limb sign is continuous across the seam. Continental byte-identity still holds (flag off).

**Phase 2 — Noise cylinder. [LOW–MED, gated on the 3D-input verification (done: `NoiseFn<_,3>` confirmed) + fmath].**
Replace raw-x sampling with `[R*cos(θ), R*sin(θ), lat]` via `fmath`. Extend the seam-continuity invariant to assert the **geometric** (elevation) half. This is where the visible seam in the *base field* disappears.

**Phase 3 — Climate upwind march periodicity. [MED–HIGH, honestly the hardest].**
Implement the seam-cut (preferred) or convergence-iteration fix in both `climate.rs` and `climate_seasonal.rs` in lockstep; wrap the upwind/gyre dx. Test: precipitation/biome is continuous across the seam (no hard band at x=0). This is non-local and algorithmic, not a coordinate tweak — budget accordingly.

**Phase 4 — (no code) Erosion/hydrology proof. [LOW].**
Add the flow-crosses-seam test (invariant #3). No production change; this converts a "free" claim into a guarded signal.

**Phase 5 — Flip the shipping `planet()` preset + re-anchor. [MED, judgement-heavy]. ✅ DONE (`443b3df`/`f426c31`/`e3c2ebc`).**
Set `GenerateParams::planet` → `periodic:true`. Re-anchored `seed9_planet_full` and its wasm twin (native↔wasm re-verified 5/5). Re-derived from a CALIBRATED probe (it reproduced the old flat constants exactly before being trusted on periodic worlds): CROSSING `[11,19,7,4]→[11,19,26,30]`, SUNDERED `[23,42]→[23,10]` (42 became a crossing seed), COLONIZE `[2,5,9,11,18]→[18,27,32]`. **The marquee 3-strand far shore no longer occurs naturally** (no seed in 0..120 puts faith+colony+sword on one shore) → `shore.rs` weave pinned SYNTHETICALLY; the colony place tag got a new real-world guard (`colony_far_shore_claims.rs`) since the synthetic fixture can't catch a carrier regression. Also moved fixtures whose premise changed: faith_crossing/shore laneless → a SUNDERED seed, RIVER_SEED 11→26, trade_prosperity 19→11.

**Phase 6 — Render + web follow-on (Section 6). [MED]. ✅ DONE (`4f2e46a`).**
`fadeMapEdges`→`fadePoleCaps` (seam bands dropped, poles kept); `lod.ts` longitude window WRAPS modularly (+ `sectorNearestDir` minimum-image); e2e fixtures re-derived (drill seed 4→8, exclave seed 15→26). Drilled-streaming-across-the-seam confirmed end-to-end (live patch columns straddle sx=0 and sx=span-1; screenshot). **Implementation notes vs the plan below:** (a) the graticule meridian needed no change — it's the pre-world placeholder, not the seam; (b) `refine_sector` correctly stays `periodic:false` — the seam x=0≡x=width is ALWAYS a sector boundary at every level, so no individual sector contains it in its interior (the cross-language flag concern is moot: `planet()` is unconditionally periodic, so Rust and wasm agree by construction).

---

## 6. Render + web follow-on

These are *consumers* and must be gated to the periodic planet (via world metadata / a client flag) so continental and drilled-sector views keep their current clamping behavior.

- **Remove the seam fade for periodic worlds.** `globe.ts:178-213 fadeMapEdges` paints opaque sea bands at the left/right texture edges *specifically to mask* the flat world's discontinuous coastlines. On a periodic world those coastlines now meet — the fade would mask legitimate detail. Keep the **pole** bands (the equirect pinch is geometric and inherent); conditionally skip only the **left/right seam** bands when the world is periodic. (`edgeFadeBands` returns `{seam, pole}` — drop the two `band(... seam ...)` calls, keep the two `band(... pole ...)` calls under the periodic flag.)
- **Wrap, don't clamp, the LoD longitude window.** `sector.ts:39 sectorAt` and `lod.ts` clamp `sx` with `Math.max(0, Math.min(span-1, ...))`. For periodic worlds replace with modulo `((sx % span) + span) % span` so the visible-sector window crosses the seam instead of stopping at it. `sectorNearestDir`'s clamp into the world rect (lod.ts) needs the same treatment for sectors straddling x=0.
- **Graticule meridian seam.** `planet.rs render_graticule` iterates meridians linearly across `[vx, vx+w]` with no wrap; emit the seam meridian at **both** x=0 and x=width so the antimeridian projects continuously (reads as one line, not a gap).
- **Cross-language consistency.** If `lod.ts` wraps but the Rust patch renderer does not (or vice-versa), the browser drills to a wrapped sector while the CLI renders a non-wrapped SVG → mismatch (this repo's "same sector in browser and CLI" promise). Gate both off the *same* world-level periodic flag carried in metadata, and add a smoke check that a seam-crossing sector renders identically.

---

## 7. Top risks and de-risking

1. **Re-deriving the behavioral seed taxonomy (highest, easy to under-budget).** Seam-straddling continents merge via `connected_bodies`, so SUNDERED/CROSSING/COLONIZE can flip. *De-risk:* treat Phase 5 as a re-derivation (re-run the Step-0 probe), not a re-anchor; isolate it in one PR; expect and document the new partition. Don't ship the flip until the probe output is reconciled with the claim tests.
2. **Lloyd relaxing the ghosts (silent correctness bug).** Ghosts in relaxation drift independently and break periodicity invisibly. *De-risk:* the explicit two-pass split (relax without ghosts; rebuild ghosted at 0 iterations) — and the seam-continuity invariant test catches a regression empirically.
3. **The noise cos-only bug from the stage map.** Would ship a mirror-image planet that *passes a naive seam test* (x=0 and x=width match because they're forced equal) while the geography is wrong. *De-risk:* 3D cylinder input, and validate the invariant against an interior-comparable delta plus a visual artifact, not just "edges equal."
4. **Climate cyclic march discontinuity.** The one genuinely algorithmic change; a naive dx-wrap leaves a hard seam. *De-risk:* seam-cut or convergence, tested for continuity; do both `climate.rs` and `climate_seasonal.rs`.
5. **Cross-platform float wobble in wrapped-min / cos-sin.** *De-risk:* `fmath` everywhere, extend `cross_platform.rs` to a periodic world.
6. **Continental regression during bring-up.** *De-risk:* the byte-identity test (Section 4A) gates every phase; if it goes red, the false-branch was touched.

---

## Key file references

- Topology: `crates/mapgen-geom/src/mesh.rs:43-129` (build/build_region), `crates/mapgen-geom/src/poisson.rs` (seeding stays unchanged on this approach).
- Flag plumbing: `crates/mapgen-world/src/lib.rs:30-68,100-132` (`GenerateParams`, `generate`), `crates/mapgen-geom/src/mesh.rs:11-17` (`MeshBuildParams`).
- Coordinate stages: `crates/mapgen-world/src/plates.rs:60-70,90-92,130` (sq_dist + stress), `crates/mapgen-world/src/noise.rs:42-64` (cylinder), `crates/mapgen-world/src/climate.rs:105-147` + `climate_seasonal.rs:124-147` (upwind march), `crates/mapgen-world/src/ocean.rs:55-77` (gyre dx).
- Free-via-adjacency: `crates/mapgen-world/src/erosion.rs`, `hydrology.rs`, `naming.rs:466-480` (`connected_bodies`).
- Goldens & seeds: `crates/mapgen-world/tests/pipeline_spec.rs:13-60`, `crates/mapgen-world/tests/golden/{seed42_full,seed42_phase2,seed42_sector,seed9_planet_full}.blake3.txt`, `crates/mapgen-testsupport/src/lib.rs:29-89` (REFERENCE_SEED, CROSSING/SUNDERED/COLONIZE_SEEDS, params builders), `crates/mapgen-wasm/tests/cross_platform.rs`.
- Render/web consumers: `crates/mapgen-render/src/style/planet.rs` (render_graticule), `web/src/lod.ts:64-80`, `web/src/sector.ts:30-42` (sectorAt clamp), `web/src/globe.ts:178-213` (fadeMapEdges).
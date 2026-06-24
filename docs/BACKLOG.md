# Backlog — deferred work

Items move here when removed from the active plan but worth preserving the
design intent and the reasoning behind the deferral. The active plan is
`docs/ARCHITECTURE.md`; anything not in the active plan that we've discussed
seriously lives here.

**Entry shape** (each item):

- **Title** — one-line description.
- **Why deferred** — the case for not doing this now.
- **Trigger for revival** — the concrete signal that would move it back into
  the active plan. If we can't name a trigger, the item probably shouldn't be
  on the backlog at all.
- **Cost** — rough order of magnitude (hours, days, weeks).
- **Origin** — where this was last discussed (commit hash, doc section, or
  research brief).

Items roughly grouped by category. Within a category, ordered by my current
guess at value-per-day. Re-prioritize freely.

---

## Up next — resume here (2026-06-11)

**⛰️ RELIEF R3 SHIPPED + REVIEWED + CLEANED (branch `claude/resume-pngmai`, pushed).** The oblique-camera
increment of the relief arc: tilt camera (Shift/right-drag, clamped to maxPitch — a swept POSE INVARIANT),
pitch-aware dynamic near, oblique LOD (lookDir window-centering), and patches-first picking (deferred from
R2). A high-effort code review then drove two fixes + a cleanup, all pushed:
- **Live terrain-clearance ceiling** (`f38691e`): a verifier found raw elevation is NOT bounded ≤1.0 —
  erosion + fill_depressions push peaks to ~1.05 (5/48 seeds), so the hardcoded R_TER=1.024 understated the
  real ceiling. maxPitch/nearFor now take a live `rTer = PATCH_BASE_RADIUS + reliefMaxSeen`; R_TER survives
  as the pinned nominal fallback (a VERT_EXAG retune now trips a unit test).
- **Gesture-leak guard** (`f38691e`): pointer capture + pointercancel + an `e.buttons===0` guard so a
  tilt/pan released off-canvas can't keep moving the camera on bare hover.
- **Cleanup** (`c30e9c6`): shared `norm`, single `resetOverviewNear()` helper (also fixed stale
  data-cam-near on hide).

**⛰️ R4 PARTIAL (2026-06-11):** the quality hunt MEASURED the seam-shade residual at **0.177** max
|Δlambert| (a visible bright/dark line at every sector seam, ≫ the 0.005 tolerance) and BUILT the
contingency — `lambert_grid` zeroes the cross-edge gradient at tile boundaries → 0.0 bit-identical;
`seed42_relief_grid` golden re-anchored + wasm twin green (fmath-pure). Also the texBytes→gpuBytes rename.
This R4 work TOUCHES RUST (mapgen-render lambert + the golden), unlike R3. DEFERRED (no GPU in this
sandbox): the named screenshots, oblique under-coverage at MAX_PITCH (frustum-footprint follow-up), depth
banding, and the 3 R3 review nits. Commits: R3 `b906ad8` → review `f38691e` → cleanup `c30e9c6` → state
`60d44d7` → R4 (this push). Gate: cargo test --workspace + clippy -D + wasm twin + 97 web unit + 18 globe
e2e green. **NEXT: R4 close-out (needs a GPU)** — capture screenshots, decide the frustum selector, mark
the addendum fully SHIPPED. Older: 1f/ST-5 and the periodic-globe work below are all DONE.


Picking this branch up on a fresh clone / new machine? Start here. Branch
`claude/fantasy-map-generator-1du5B`.

**State (all shipped + green: `just check`, web `tsc`/`vitest`/bundle-guard/e2e,
`wasm-pack test --node`).**

- **The Sundered Lanes** inter-continental arc — Phase 1 carriers + Phase 2
  mechanics (diffusion, faith replay, trade, embargo) + Phase 3 surfacing
  (prosperity heatmap, trade lens, chronicle weaving, first-contact arc, landmass
  place tag, multi-strand far-shore weave, cross-lens correlation) — COMPLETE, plus
  its inc-4 review test gaps closed (CLI `auto-shore` e2e + a laned cross-platform
  `far_shore` golden, seed 9).
- **3D globe** (`Scale: Globe`) — base shipped (richer equirect parchment+political
  texture, pole fade — the seam fade is gone now the world is periodic, time-lapse
  scrub, lens parity, fly-to drill) and now being EXTENDED into the continuous-LOD
  navigation arc below.
- **Cartographic generalisation** (ADR 0001 §3, the detail-DECREASING direction) —
  river + coastline Visvalingam–Whyatt simplification on the overview shipped
  (`mapgen-render::simplify`). The high-value slice is done. The remaining increments
  (Töpfer scale-rank selection, per-level stylesheet) are LOW-ROI and now DEFERRED
  behind the active arc (see the "Scale-dependent render fidelity" entry for them).

**🚧 ACTIVE ARC (2026-06-08): Globe→ground continuous-3D navigation.** The user
reported that drilling the globe DROPPED to a flat 2D sector, and chose (knowingly,
maximal) a full **continuous-LOD streaming globe** ("detail follows everywhere") plus
roadmapped relief + region-name billboard labels. Locked design (judge-panel +
red-team workflows): **`docs/design/globe-ground-3d-navigation.md`** (foundation +
continuous-LOD streaming addendum). Honest scope call baked into the design:
continuous-follow during fast motion is physically infeasible on the single-worker +
main-thread-rasterize substrate, so **v1 = "detail follows on SETTLE"** (continuous-
follow gated behind a 1-day OffscreenCanvas spike). Build order: foundation **1a
(free-fly camera) — DONE 2026-06-08** → 1b → 1c → streaming ST-1…ST-4 → gated ST-5
(spike) / relief.

- **1a — free-fly globe camera — DONE 2026-06-08.** Drilling the globe now STAYS in
  3D (no 2D handoff); a custom globe-flight camera frames the region in its local
  east-north-up frame (sidesteps the world-+Y polar-clamp singularity) with pan+zoom
  wired (pitch/heading pure but unwired). New pure `web/src/camera.ts` (pinned by a
  raycast round-trip through the trusted `uvToWorld` path) + flight mode in `globe.ts`
  + `navTo` change in `main.ts`. A 4-dimension adversarial review found 10 defects
  (1 blocker, 5 major) — ALL fixed + regression-tested (stale-flight wedge on
  re-entry; rolled-overview on return; intermediate-crumb 2D leak; unpinned up
  direction; pan-sign; doc/getCameraState deferral). `getCameraState` deferred to
  ST-1 with its consumer. Render-only, no Rust/golden change. CLAIMS rows registered.
- **1b (patch) — DONE 2026-06-08.** A globe drill now also refines the sector and lays
  its fontless `"globe"` render onto a curved partial-sphere patch over the base globe
  (`showPatch`/`patchTexture` in `globe.ts`: `SphereGeometry(1.001,…)`, `depthTest=false`
  +`renderOrder`, no-fade `ClampToEdge`, dedicated `data-patch-textures`; `main.ts`
  forces `style:"globe"` + routes `"refined"`→`showPatch`, raster at the sector aspect).
  Pure `sectorPatchParams` (`camera.ts`) pinned by raycast (centering + N/W orientation).
  A 4-dim review found 7 (1 major) — fixed: async `showPatch` **nav-guard** (a stale
  patch could weld onto the overview after a return during the rasterize window), narrate
  re-enabled on return, orientation raycast added. Deferred polish (1f): altitude framing
  tightening + fly-to→flight entry-ease (the patch is dead-centred but loosely framed).
  **HONEST CEILING (CLAIMS):** `data-patch`/textures prove ROUTING only — texture
  orientation + visibility are screenshot-verified (a mandatory review artifact).
  Render-only, no Rust/golden change.
- **1b-ii (region-name billboard) — DONE 2026-06-08.** The drilled region now shows its
  GROUNDED landmass name on an HTML billboard anchored to the region centre (a fixed unit
  vector) and tracked each flight frame by projecting it through the camera (hidden past
  the horizon). `continent_at` gained a read-only `name` (nearest-centroid match to the
  named `world.continents` — RNG-free, golden-neutral; not serialized), threaded
  `continentInfo`→`regionName`→`enterRegion`. Rust test pins name correctness (non-empty,
  real, ≥2 distinct); e2e pins the consumer (label visible+named on drill, hidden on reset);
  the projection/positioning is screenshot-verified (a sepia pill for contrast). CLAIMS row
  added.
- **1c (deeper 3D drilling) — DONE 2026-06-08.** Clicking the high-detail patch drills one
  level finer and rebuilds a smaller patch, staying in 3D (verified L0→L2→L3 with the chained
  breadcrumb + the billboard persisting). New pure `patchUvToWorld` (`sector.ts`; patch
  `hit.uv` → world point) pinned by corner/containment units + a `camera.test.ts` raycast
  round-trip; `globe.ts` got `onPatchPick` + a pointerup restructure (drilled click → patch
  raycast → deeper drill; overview click → the 1a base-sphere flyTo); `main.ts` wires
  `onPatchPick`→`patchUvToWorld`→`childSectorAt`→`navTo`. **First drill is now L3, so deeper
  goes L3→L4 (not L2→L3).** ⚠️ 1c shipped a LATENT double-fire: `#globe-canvas` is a child of
  `#map`, so a drilled click bubbled to `#map`'s pointerup → a SECOND `drillAt` — but it was
  masked because 1c's drill set `busy` (the old `requestRefine`), and `drillAt` early-returns on
  `busy`. ST-1 (below) dropped that busy-setting refine for non-blocking streaming, UNMASKING it
  (one click jumped two levels to a pole sector). Fixed in ST-1: the `#map` drill is inert for
  ALL globe levels (`if (globeScale) return`, not just `nav.level===0`). Render-only. CLAIMS row added.
- **Streaming engine ST-1 (multi-patch cache) — DONE 2026-06-08.** "Scroll around after zooming
  and keep the map data" — the user's contract. A bounded multi-patch cache over the base globe:
  pure `desiredSectors` selector (`lod.ts`: N×N window around the sub-point, horizon-culled,
  clamped to the patch cap — consumes the 1a-deferred `getCameraState` hook) + pure `reconcile`
  cache policy (`patchcache.ts`: count cap 24 + byte cap 96MB BOTH bound `toLoad`; in-view never
  evicted; out-of-view LRU). `worker.ts` gained `refineTile` (refine+render ONE tile WITHOUT
  touching the persistent `sector` slot — non-blocking, no `busy`); `globe.ts` got the multi-patch
  `Map` + settle detection (`onSettle` fires 140ms after the flight camera stills) + `markSeen`/
  `evictPatch`/`liveEntries`; `main.ts` `streamReconcile` drives worker+cache on every settle
  (drill/pan/zoom). Contract e2e + pure units for the selector & cache (red-first: caps mutation-
  verified). Also fixed the 1c double-fire (above). Render-only, no Rust/golden change. CLAIMS rows added.
- **ST-1 ADVERSARIAL REVIEW (45 agents, 7 dims; judge-panel + double-skeptic verify) — 8 confirmed,
  fixed 2026-06-08.** TWO real MAJOR correctness bugs the first pass shipped: (1) `desiredSectors`
  culled each sector on its CENTRE → at L3 + a wheel-zoom to MIN_ALT the detail set went EMPTY where
  the user was looking; fixed to cull/rank on each sector's NEAREST point to the sub-point (the
  sub-point's own sector clamps to itself → never dropped). (2) the CONTRACT e2e was FALSE-GREEN —
  `data-refines > refines0` was satisfied by the drill's still-in-flight tiles, so a pan that streamed
  NOTHING still passed; fixed by exposing the live sector-key SET (`data-patch-keys`) and asserting a
  genuinely NEW key after the pan (mutation-verified: reds when pan-streaming is disabled). Plus:
  `pendingTiles` held until the raster lands + cleared on lifecycle resets (an `error` reply can't
  brick a sector); byte-cap-on-load + nearest-keeps-rank tests added (mutation-pinned); WHERE-YOU-CLICK
  shared-`""` sentinel; **C1 (pre-existing, separate subsystem):** globe wheel/drag bubbled to #map's
  PanZoom and corrupted the hidden 2D transform → fixed with `stopPropagation` at the globe canvas.
  **WON'T-FIX (intended):** patches are `depthTest:false` (anti-z-fight); a far-hemisphere patch can
  bleed through during ONE long uninterrupted drag, self-correcting on settle — `depthTest:true` would
  reintroduce z-fight on every patch (net regression). Recurring lesson: the stale `globeScale &&
  nav.level === 0` guard appeared in THREE places (drillAt, setLayerState, doRestyle) — grep the class.
- **1e (lens-on-patch while drilled) — DONE 2026-06-08.** The review-confirmed MAJOR no-op: a lens
  toggle while DRILLED silently did nothing (`setLayerState`'s globe branch was gated `nav.level===0`,
  falling through to the hidden 2D SVG). Fixed: broadened to `if (globeScale)` → `retextureGlobe()`
  (base sphere) + `refreshGlobePatches()` (evict the live patches + clear `pendingTiles` + re-stream,
  so each re-rasterizes with `withLayerClasses` under the new lens — the wash group is in the refined
  sector's `render_globe_texture`). Brief coarse-base flash while the new patches land — the accepted
  interaction-model tradeoff. e2e pins it (drilled lens toggle bumps `data-textures` + `data-refines`),
  mutation-verified RED on the stale guard. `doRestyle`'s twin stale guard is harmless (style select
  disabled on the globe — review-refuted as a defect). Render-only, no Rust/golden change. CLAIMS row added.
- **ST-2 (prefetch ring) — DONE 2026-06-08** (user-requested: "scroll without waiting for render").
  `STREAM_CFG.window` 1→2 (5×5 = in-view + a one-sector ring beyond the view edge, horizon-culled
  to the visible hemisphere); `desiredSectors` now returns NEAREST-FIRST so the worker rasterizes
  under-camera sectors before the ring (breadth-first). Caps raised to hold the ring (MAX_LIVE_PATCHES
  24→32, ~128MB est / ~67MB real). The CONTRACT e2e bound now tracks the cap. LIMIT: one worker, one
  tile at a time (~100ms) — a fast fling past the ring still lags; full during-motion streaming is the
  ST-5 OffscreenCanvas spike. Render-only.
- **Globe breadcrumb — jumped ancestors muted 2026-06-08** (user: "breadcrumbs jump from globe to L3").
  A globe click lands at L3 in one hop; the shallower L1/L2 ancestors (quadtree path stops, never a
  distinct globe view) now render muted (`.crumb.via`) so the bar reads "Globe › ⟨zoom path⟩ › L3".
  Labels/clicks unchanged (up-nav + crumb-hop invariant intact). A LIGHT pass — if the *jump itself*
  should feel more gradual, options are fewer levels (accept L2 curvature) or continuous altitude-LOD.
- **✅ SEAM (#1) — DONE 2026-06-09: PERIODIC WORLD GEN SHIPPED, ALL PHASES 0–6. The globe is seamless
  (base + drilled, screenshot-confirmed at the antimeridian).** Rotate-to-ocean was DROPPED. Full plan +
  final status: **`docs/design/periodic-world-generation.md`**. The mesh is a CYLINDER (wrap x at lon
  ±180°, clamp y at the poles); adjacency stages wrap FREE, coordinate stages use minimum-image dx.
  Commits: P0–P4 `d3d5af1`/`5980418`/`44d9694`/`d2ccb07`/`e204daf`; **P5 plumbing `443b3df`, THE FLIP
  `f426c31`, colony-tag guard `e3c2ebc`; P6 render/web `4f2e46a`.** The GROWN COST (re-deriving the
  `CROSSING/SUNDERED/COLONIZE` taxonomy because seam-straddling continents merge) was paid via a
  CALIBRATED probe (reproduced the old flat constants exactly first): CROSSING `[11,19,7,4]→[11,19,26,30]`,
  SUNDERED `[23,42]→[23,10]`, COLONIZE `[2,5,9,11,18]→[18,27,32]`. **3-strand far-shore regression →
  RECOVERED:** periodicity + the flat-tuned naming threshold left the planet's medium continents unnamed,
  and carriers tag only named shores, so the natural 3-strand chronicle vanished (no seed 0..200). Fixed by
  re-calibrating naming `MIN_CONTINENT_DIVISOR 40→100` (≈1% — the periodic planet was genuinely
  under-labeled); seed 27 reaches shore "Zuk" by all 3 strands (proven in `lore_cli`). shore.rs weave LOGIC
  stays synthetic (drift-immune); a calibration guard + colony-tag guard added. See `docs/CLAIMS.md`
  "Periodic world generation".
  - **✅ Phase 0 (mesh ghost topology) — DONE 2026-06-08, byte-identical.** `MeshBuildParams.periodic`
    + a two-pass ghost build (`mesh.rs::periodic_seam_edges`: relax real sites unchanged → re-triangulate
    with seam ghosts at x±width, lloyd=0 → keep opposite-edge pairs within a few cell-widths → symmetrise).
    The voronoice triangulation is NOT mirror-symmetric across the seam, so cross-seam edges are added to
    BOTH endpoints (spike-proven). Differential test pins it (periodic → seam-crossing neighbours; flat → 0).
    Production caller hardcodes `periodic: false` → ALL goldens (continent + planet `seed9`) byte-identical.
  - **✅ Phase 1 (plates + ocean dx-wrap) — DONE 2026-06-08, byte-identical.** `MeshData.periodic` carrier
    (skip-serialized when false) + shared `plates::wrap_dx` (signed minimum-image, pure f32). Plates'
    nearest-plate metric + boundary-stress normal and ocean's gyre limb delta wrap at the seam. Tests:
    wrap_dx unit + a differential (periodic vs flat plate assignment differs — mutation-verified red).
  - **✅ Phase 2 (noise cylinder) — DONE 2026-06-08, byte-identical.** `noise::warped_height` maps longitude
    to a circle (R=width/TAU), sampling 3D `[R·cosθ, R·sinθ, y]` (NOT cos-only — mirrors the planet) so the
    elevation field (the visible coastline seam) is continuous at x=0↔x=width. Seam-continuity test,
    mutation-verified red. (All goldens — continent + planet seed9 — still byte-identical through P0–P2.)
  - **✅ Phase 3 (climate upwind march) — DONE 2026-06-08, byte-identical.** The one algorithmic seam
    change. Upwind selection extracted to a shared seam-aware `climate::upwind_neighbor` (dedups the
    climate.rs/climate_seasonal.rs duplication; wraps the x-delta) + a `done`-flag SEAM CUT: the per-band
    march start's unprocessed cross-seam upwind reads saturated ocean inflow (like the flat west edge),
    not stale dryness. Residual hairline at the cut is masked by the ocean-seam placement (P5) + sea
    re-saturation; full land-at-seam continuity (convergence pass) deferred, unneeded with an ocean seam.
    Test: `upwind_neighbor_wraps_the_seam` (mutation-verified). Lockstep in both climate modules.
  - **✅ Phase 4 (erosion/hydrology seam-flow) — DONE 2026-06-08.** No production change (adjacency wraps
    free since P0); a differential guard test proves flow crosses the seam ONLY on a periodic mesh.
  - **✅ Phase 5 (THE FLIP) — DONE 2026-06-09 (`443b3df`/`f426c31`/`e3c2ebc`).** `planet()`→`periodic:true`;
    `seed9_planet_full` golden + wasm twin re-anchored (native↔wasm 5/5 — the seam path is fmath-clean);
    taxonomy re-derived via a calibrated probe (above). Seed 7 fell out of CROSSING (its lone seizure became
    an isolated exclave → vacuous legibility check); seed 14 excluded from COLONIZE (colonizes an unnamed
    body, carries no tag). 3-strand weave → synthetic; `colony_far_shore_claims.rs` added as the real-world
    colony-tag guard; faith_crossing/shore laneless fixtures → a SUNDERED seed; RIVER_SEED 11→26;
    trade_prosperity fixture 19→11. All mutation-verified.
  - **✅ Phase 6 (render/web) — DONE 2026-06-09 (`4f2e46a`).** `fadeMapEdges`→`fadePoleCaps` (seam bands
    dropped, poles kept; `RepeatWrapping` joins the now-continuous edges); `lod.ts` longitude window WRAPS
    modularly (+ `sectorNearestDir` minimum-image), latitude still clamps; e2e fixtures re-derived (drill
    seed 4→8, exclave seed 15→26). Drilled-seam stitch confirmed end-to-end. The graticule needed no
    change (it's the placeholder, not the seam); `refine_sector` correctly stays `periodic:false` (the seam
    is always a sector boundary, so no sector contains it in its interior).
- **✅ 1f entry-ease — DONE 2026-06-09.** A drill now GLIDES from the camera's current pose into the
  flight pose over 350ms (pure `camera.ts::lerpPose`, nlerp'd up; `globe.ts entryEase` interpolates toward
  the LIVE flight pose so a pan mid-glide still converges) instead of the deferred 1a/1b one-frame snap.
  Also smooths the 1c deeper-drill re-frame. Unit-tested (endpoints/midpoint/clamp, 3 tests) +
  `data-entry-eases` e2e signal (mutation-verified: signal never written → drill e2e red). The "globe
  GROWS as you zoom" half was already shipped (the altitude formula scales ~1.4× the sector's angular
  span per level — see main.ts:830); remaining feel-tuning is screenshot/judgment-bound.
- **✅ ST-5 OffscreenCanvas spike — DONE 2026-06-09, verdict NO-GO; then THE UNLOCK SHIPPED 2026-06-10.**
  The spike (`docs/design/globe-ground-3d-navigation.md`): `createImageBitmap(svgBlob)` is unsupported in
  Chromium 148 (main thread AND worker — PNG control works, so the gap is specifically SVG ImageBitmap
  decode); the old main-thread path cost ~312ms `drawImage` per tile. The unlock was precisely known —
  **worker-side wasm rasterization** — and is now BUILT: `WorldHandle::render_rgba` (resvg/usvg/tiny-skia,
  fontless, `default-features = false`) renders globe tiles to RGBA IN THE WORKER and transfers the bytes;
  the main thread only blits + uploads (`data-last-blit-ms < 50` vs ~300 ms). A pre-build spike measured
  ~135 ms/tile off-thread and proved usvg HONORS the lens class selector (so `with_root_class` bakes the
  lens, no per-variant render). On that foundation, **continuous-follow + velocity-predictive prefetch
  SHIPPED** (during-motion pump + in-flight budget + discard-stale, bounded by the cache firewall). Wasm
  +1 MB (banded by `check-bundle.mjs`). DEFERRED follow-up: **base-globe off-thread** — the base sphere
  still rasterizes on main on enter/lens-toggle/year-scrub (OFF the navigation hot path; needs its own
  year-aware `render_rgba_at_year` + an `enterGlobeView`/`yearFrame` round-trip). **Revival trigger:** the
  ~300 ms `retextureGlobe` hitch on a lens-toggle or year-scrub becomes a top complaint, or year-scrub
  animation jank is profiled as the dominant globe stall. See `docs/CLAIMS.md`
  "Planet & globe presentation" (the off-main-thread + continuous-follow rows).
- **✅ DRILL PRECISION + SECTOR SEAMS — DONE 2026-06-10 (user-reported, both reproduced + measured).**
  (1) "Zooming into the wrong spots": the camera centred on the containing SECTOR's centre, not the click
  — measured 25.2° off at L3 (fly to your click, then lurch to the sector centre; a label floating over
  open ocean with the clicked land at the screen edge). Fixed: `navTo(target, focus)` threads the click's
  world point to the camera (sector keeps level/breadcrumb/altitude only) + picks use the exact `hit.point`
  (`unitToUv`) instead of the barycentric `hit.uv` (~4° off mid-triangle). Standing e2e precision contract:
  land within 1° (mutation-verified red at 22.5°). (2) "Sharp straight lines where nature shifts": TWO
  mechanisms — (a) DOMINANT: the 5×5 stream window was smaller than L3's visible cap, so the crisp-patch /
  blurry-base boundary cut across the view → window 3 (7×7), caps 52/208MB; (b) DATA: climate fields were
  unpinned across sector seams (elevation was) → biomes quantized the disagreement into straight-line class
  swaps → `scale.rs::pin_climate_to_parent` (87%→92% worst-seam agreement, mutation-verified;
  `seed42_sector` golden re-anchored, wasm 5/5). Screenshot-verified continuous terrain. RESIDUAL (named):
  half-texel edge fringes at patch boundaries (LinearFilter+ClampToEdge, no bleed gutter) — hairlines only;
  revival trigger: visible grid lines at high zoom after the above two fixes.
- **✅ CROSS-LEVEL CONTENT CONTRACT — DONE 2026-06-10 (user-reported: "content of cells do not align as
  you zoom — patterns do not represent the same content at different fidelity").** Third member of the
  continuous-UX defect family (after click-precision + seams). MEASURED: on the globe path (planet 8
  @2000, L3) drilling kept only 55–85% of land/sea signs and 34–58% of biomes — zooming rewrote the map.
  ROOT CAUSE: refine_sector anchored to the PRE-erosion base and ran its OWN erosion (fictional terrain
  matching neither the parent render nor the projected parent rivers). FIX: `parent_anchor_elevation` —
  the child REFINES the parent's FINAL eroded elevation (nearest-parent + 2 Jacobi smoothing passes) +
  zero-mean detail noise; child erosion REMOVED. After: 97–100% land/sea, 85–90% biome, MAD 0.15→0.01.
  Coarsening contract TIGHTENED (0.96/0.05 from 0.90/0.15) + new standing planet-path test
  (mutation-verified red at 55%). Visual: mid-streaming vs settled drill screenshots near-identical
  (content sharpens in place); ornate 2D drill keeps mountains/forest/coast character. `seed42_sector`
  golden re-anchored; wasm 5/5; tiles also stream FASTER (no per-tile erosion).
- **✅ QUALITY-HUNT BATCH (13 findings) — DONE 2026-06-10.** Systematic audit (26-agent hunt + empirical
  probes, all adversarially verified; 7 candidates refuted) then fixed ALL verified findings: (1) temporal
  snap — drilling from a scrubbed past year re-textures to the present (tiles render the present; mixed-era
  content + lying label before); (2) antimeridian DRILLED seam — parent sampling is wrap-aware
  (min-image x + unclamped sampling halo; 0%→≥85% biome agreement at the wrap); (3) polar cap — rows
  centred beyond ±75° never stream (wedge-streak tiles → faded base instead); (4) CI perf gate (NEW
  ci.yml job, PERF_BUDGET_SCALE=2.0; first run caught the intrinsic periodic+naming planet-render
  19→36ms — re-anchored with causes); (5) visual-regression floors → recorded bands; (6) timeout-audit
  (suite has ONE justified bare sleep; ready-race fixed by #8); (7) patch aspect tolerance → derived
  quantization bound (~3%, was 0.25); (8) "Globe ready" only after the texture lands (race-free e2e read);
  (9) re-drill round-trip contract e2e; (10) drill progress affordance ("Loading detail… N tiles" +
  `data-pending-tiles`, drains-to-zero e2e); (11) lens "Applying lens…" affordance; (12) end-to-end
  periodic continuity covered by the wrap-seam test (12≈#2); (13) nested-refinement reproducibility = (9).
  All mutation- or review-verified; goldens: `seed42_sector` unchanged (flat parents byte-identical).
- **✅ WASM RASTERIZER + CONTINUOUS-FOLLOW — DONE 2026-06-10 (the ST-5 unlock, taken).** Globe tiles
  rasterize off the main thread (`render_rgba`, resvg/usvg/tiny-skia in the worker → RGBA transferable);
  the 312 ms paint-thread hitch is gone (`data-last-blit-ms < 50`). On that foundation: continuous-follow
  (fill DURING motion) + velocity-predictive prefetch + in-flight budget + discard-stale, bounded. A
  13-agent adversarial review caught a real blocker (the budget starved the STATIC-drill fill — fixed
  with a per-completion refill, mutation-verified) + 7 others, all fixed. See the ST-5 entry above + the
  `docs/CLAIMS.md` rows. **Deferred follow-up:** base-globe off-thread (off the hot path; own arc).
  **Post-ship hardening (same day, from a 25-agent review of the landed diff + a 17-agent review of
  the fix itself):** the in-flight budget had converted "one orphaned reservation = one coarse sector"
  into "3 orphans = the whole stream freezes" — every tile-work failure now answers the
  sector+lens-carrying `tileFailed` (refineSector inside the try; `!root` routed through it; lens-echo
  guard mirrored), and — the fix-review's catch — a PERSISTENTLY failing sector is BENCHED after
  MAX_TILE_RETRIES=3 instead of retry-looping (naive release+refill re-requests a doomed NEAREST
  sector forever; measured: it monopolises the budget head and the fill starves at 0). Recovery
  contract pinned end-to-end with REAL injected wasm failures (`?failTiles=N` → w=0 → `Pixmap::new`
  rejects, persistent per-key), mutation-verified both ways (both starve at 0). `worker.onerror` is a
  STICKY defensive backstop (degraded flag stops re-reserving + status clobber, unwedges busy/overlay)
  honestly documented as structurally unreachable for tile work. Opacity asserts tightened to per-texel
  `min_alpha==255` (the premultiplied→putImageData color-correctness invariant, mutation-verified at
  `min alpha 161`). A `predictedAhead` radius-renorm was tried and REVERTED by the fix-review: it
  turned a fast zoom-in (radius more than halved between samples flips `2·pos − prev` backwards) from
  a self-culling overshoot (measured 0 sectors) into confident ANTIPODAL prefetch (measured 16 garbage
  sectors); the horizon inflation stays as a documented self-correcting artifact. CLAIMS rows updated.
- **NEXT candidates:** the **"storied globe" surfacing arc** (the vision-gap audit's top finding: 28
  event kinds + characters/dynasties/arcs/mythic ages are generated but ~none experienceable in the
  app; the just-recovered 3-strand chronicle is CLI-only) — surface events/chronicles/settlements in
  the primary globe view; **relief displacement — IN PROGRESS (user-elected full arc 2026-06-11; the evidence gate was lifted by choice). R1 SHIPPED:** seam-banded relief grids (the Increment-G seam assumption was spike-REFUTED — 0/65 edge nodes bit-identical from a tile's own pinned field; edges now sample the shared ROOT field at endpoint-pinned bitwise coords, antimeridian-canonicalized) + worker-baked deterministic hillshade (the visibility floor; flat sea a byte-no-op, alpha untouched) + a dedicated heights∥lambert golden with a wasm twin + the failStage=relief recovery contract. Design: the Relief addendum in `globe-ground-3d-navigation.md`. R2 SHIPPED: displaced patch geometry from the seam-banded grid (pure relief.ts, 6 unit contracts), depthTest:true, data-relief-max running witness, honest patchGpuBytes, AND the altitude half of the dynamic near pulled forward (measured black-screen at MIN_ALT — nearFor + e2e witness; screenshot-verified fixed). R3 SHIPPED: tilt camera (Shift/right-drag pitch, PITCH_SPEED 0.005, clamped to maxPitch(alt), cap 50° — the spike's envelope) made a POSE INVARIANT (camRadius ≥ R_SAFE swept off-GPU); pitch-aware dynamic near COMPLETED (the nadir-only false-green made executable — a `1+alt` formula breaches, the real-radius one never does); oblique LOD (CamState.lookDir centres the window on the look anchor, not the nadir ~26° behind it; horizon cull keeps posUnit; predictedAhead drops lookDir); and patches-first picking (deferred from R2) — `pick.test.ts` discriminator measures 0.0001° displaced vs 1.29° base-sphere on the same pitch-50° ray. data-cam-pitch/near witnesses; all 4 camera-reset sites covered. **R3 REVIEW (high-effort, 8 angles + verifier; `f38691e`/`c30e9c6`):** the verifier empirically caught that raw elevation is NOT bounded ≤1.0 (erosion + fill_depressions push peaks to ~1.05, 5/48 seeds) — so the hardcoded R_TER understated the real ceiling; FIXED by deriving the live ceiling `PATCH_BASE_RADIUS + reliefMaxSeen` (R_TER kept as a VERT_EXAG-pinned nominal). Also FIXED a gesture-leak (pointer capture + pointercancel + e.buttons guard). Cleanup: shared `norm`, single `resetOverviewNear()` (also fixed stale data-cam-near on hide). 97 web unit + tsc + bundle-guard + 27 e2e green; render-only, no Rust/golden change. R4 PARTIAL: the quality hunt MEASURED the seam-shade residual at **0.177** max |Δlambert| (a visible line at every seam, ≫ 0.005) → BUILT the contingency (`lambert_grid` zeroes the cross-edge gradient at tile boundaries → 0.0 bit-identical; `seed42_relief_grid` golden re-anchored + wasm twin green) + the texBytes→gpuBytes rename. DEFERRED (no GPU in-sandbox): oblique under-coverage at MAX_PITCH (frustum-footprint selector follow-up), depth banding at near 0.002, the named screenshots, and the 3 R3 review nits (per-pump lookDir recompute, atBound hysteresis, tilt-as-motion prefetch). NEXT (R4 close-out, needs a GPU): capture the oblique/set-edge/near-plane/seam screenshots, decide the frustum selector, then mark the addendum fully SHIPPED; **base-globe off-thread**
  (the deferred rasterizer follow-up — needs a year-aware `render_rgba_at_year`).

**Fresh-machine setup.** `just web-setup` (Node + wasm-pack + npm deps + first wasm
build; see `web/README.md`). Then `mapgen planet --seed 11` for the planisphere, or
`just web-dev` for the browser frontend. **Gotchas:** (1) **the 3D globe needs
`three` installed** — if it shows a blank / "failed to load" canvas, run
`npm install` (or `just web-setup`); the globe chunk lazy-loads three and a stale
`node_modules` is the usual cause. (2) Playwright ships browser builds per-Ubuntu-
version and lags new releases — on a too-new distro (e.g. 26.04) run the e2e with
`PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64` (noted in `just web-e2e`).

**Historical build logs below** (the Sundered Lanes + planet zoom-out records are
kept for context; all items marked DONE).

**Planet zoom-out, increment 2** (shipped — historical), in priority order:

1. ~~**Frontend zoom-out.**~~ DONE 2026-05-31 — planet usable in the browser
   (Scale control, `Generation.planet`, breadcrumb-root, click-to-drill).
2. ~~**Grounded continent / ocean names.**~~ DONE 2026-06-01 — `name_world` step 8
   flood-fills land/sea into major bodies and names each in its dominant
   culture's language (schema v17 `world.continents`/`oceans`); `style/planet.rs`
   reads them.
3. ~~**Continent-aware drill.**~~ DONE 2026-06-01 — a root click snaps to the
   clicked landmass (`continent_at` → `continentAt` wasm query → re-center +
   depth-size), instead of the quadtree quadrant. Re-center, not tight bbox
   framing (entry below explains why framing was deliberately skipped).
4. ~~**Planet-render perf budget.**~~ DONE 2026-06-01 — `PLANET_RENDER_BASELINE`
   row in `perf_baseline.rs` + `docs/perf_baseline.md` (18k-cell planisphere
   renders in ~13 ms, budget 20 ms; cheapest render path despite most cells).
   **Increment 2's tractable items are now all shipped** — what remains is the
   "harder / later" set below.
5. **Harder / later:** ~~planet-scale history viz~~ DONE 2026-06-01; ~~globe edge
   projection~~ DONE 2026-06-01 (Mollweide oval, drill kept correct); **inter-
   continental society & history — IN PROGRESS** (full multi-week arc, all-in incl.
   Phase 3; design + de-risk done, building now — see `docs/inter_continental_design.md`
   "The Sundered Lanes"); tight continent framing (entry below, advised against).

**The Sundered Lanes — build progress.**
- ~~Phase 1 Step 1 (substrate skeleton)~~ DONE 2026-06-01 — schema v18
  `WorldData::sea_lanes`, `Stage::SeaLanes`, `PipelineStage::SeaLanes` (after
  Biomes, before Cultures), no-op `chart` stub.
- ~~Phase 1 Step 3a (isotropic substrate)~~ DONE 2026-06-02 — `sea_lanes::chart`
  builds the lane graph: multi-source Dijkstra "watershed" from every coastal
  cell, one cheapest crossing per landmass pair (sea–sea + sea→land pinch scans),
  each gated by a fixed global `min_naval_for_cost` curve (slope = roster-max
  naval 40 / Step-0 gap 100, a *pure function of cost* — not a per-world quantile).
  Verified across canonical seeds: 11/19/4/7 carry both crossable straits and
  open-ocean walls; 23/42 are legitimately sundered. Endpoints are coastal *land*
  cells (seizable by a later carrier as a `BorderChange`).
- **Deferred within the arc (revival triggers):**
  - *Lake-bridging robustness.* `chart`'s navigable mask is `elev <= 0`, which
    also admits inland lakes / sub-sea-level basins; a lake touching two
    landmasses would bridge them for free. **Now PINNED** (not just noted) by
    `sea_lanes_spec::only_the_open_ocean_bridges_landmasses_no_inland_pool_does`:
    on every canonical seed the only sea component adjacent to ≥2 sizable bodies
    is the dominant ocean, so the hazard is latent, not live — and the tripwire
    fires the moment that stops being true. The algorithmic fix stays deferred on
    purpose: a strait and a bridging-lake are topologically identical in this code
    (both are a sea pocket touching two bodies, and the synthetic fixture models a
    legit strait as a *disconnected* pool), so "exclude non-ocean pools" would
    kill straits. **Revive when** the tripwire fires, OR when a Rust equirect
    renderer / real ocean-connectivity primitive lands that can tell open ocean
    from enclosed water (then restrict the mask and redo the strait fixture).
  - *`perf_baseline` row for the SeaLanes stage* (design lists it under
    Determinism + perf). The stage is empirically fast (6 planets probed in
    well under a second). **Add at end of Phase 1**, once 3b's anisotropic cost
    has settled the per-lane math (no point calibrating a row that 3b moves).
- ~~Phase 1 carriers~~ DONE 2026-06-02/03 — the beachhead carrier (earned
  cross-water conquest: a polity adjacent to a charted lane seizes the far
  landing as a `BorderChange`, `62cee5e`) + the `from:None` colonization carrier
  (settles unclaimed far shores and updates Turchin capacity, `53e4d3b`). Pinned
  in both directions on every crossing seed; legible exclave colors (conqueror ≠
  victim) keep the slider readable (`79f98af`).
- ~~Phase 1 surfacing — realms in the planisphere DOM~~ DONE 2026-06-03
  (`bbbede5`) — the planet wash groups each realm `<g class="realm" data-polity>`
  and an overseas exclave as `.realm.exclave`; the e2e scrubs min→present to make
  the earned exclave appear.
- ~~Phase 2 Diffusion — a faith provably crosses water~~ DONE 2026-06-03
  (`b018323`) — `LoopId::Diffusion` (double-buffered naval + land spread;
  religions confined to their home landmass until a lane carries them across),
  pinned by `diffusion_claims`.
- ~~Phase 2 surfacing — the Faith lens~~ DONE 2026-06-03 (`7f1f7bd`) — a per-cell
  faith wash colored by `religion_id` (planet `planet-faith` + SW legend, ornate
  `layer-faith`), a `faith` overlay + preset, swapped in under `on-faith`. Render
  test pins the planet wash surfaces every faith that crossed water; e2e pins the
  continental display swap. Both mutation-verified.
- ~~Phase 2 replay — the Faith time-slider~~ DONE 2026-06-03 (`639f7ae`+`424a73b`)
  — the Diffusion loop records a `faith_changes` timeline (schema v20, byte-
  invisible on seed42 so the goldens held as a non-perturbation proof);
  `WorldData::religion_at_year` reconstructs the past faith map; wasm `renderAtYear`
  swaps it in alongside control, and `replay_year_span` widens the slider to cover
  faith. Scrubbing with the Faith lens on now animates the spread (data + replay +
  render tests, all mutation-verified; e2e `planet Faith slider …` on seed 9 @ 2000).
- ~~Phase 2 trade diffusion~~ DONE 2026-06-03 (`82d924d`) — a `Trade` loop
  (`LoopId::Trade`, last in ORDER) opens an inter-continental route the first year
  two distinct realms hold a crossable lane, emitting `TradeRouteOpened` and
  lifting BOTH partners' Turchin capacity (`effective_capacity` = territory +
  `trade_bonus`, re-composed at every conquest/colonization recompute so a border
  change can't erase it). No schema bump — SimState-only state, byte-identical
  no-op on seed42. Data + loop-targeting + composition tests, all mutation-verified
  (`trade_claims.rs` + `loops/trade.rs`). Surfacing deferred to Phase 3.
- ~~Phase 2 embargoes~~ DONE 2026-06-03 (`5a8f2f3`) — the dual of trade: when a
  trade pair goes to war (`mearsheimer::resolve_war` → `SimState::belligerents`),
  the Trade loop SEVERS their route, reversing the exact bonus and emitting
  `EmbargoImposed`. Severs iterate the open routes (not a lane re-scan), so a
  beachhead-monopolized lane is still caught; `trade_routes` is now a map → exact
  reversal. Data (`EmbargoImposed` on every crossing seed, `embargo ≤ trade`) +
  loop targeting test, mutation-verified. No schema bump.
- **Phase 2 mechanics COMPLETE** — Diffusion, Faith replay, trade, embargo.
- ~~Phase 3 surfacing — increment 1~~ DONE 2026-06-03 (`73659b4`+`b38f7ef`+`e892a49`
  feats, `28638bf` review fixes) — built in PARALLEL by 3 worktree-isolated agents
  (a `Workflow` fan-out), then integrated (cherry-pick + conflict resolution) and
  adversarially reviewed (a second Workflow: 4 finders + per-finding verify → 4
  confirmed findings, all fixed + mutation-verified). Shipped: **prosperity heatmap**
  (`Nation::prosperity` channel, schema v21 + goldens re-anchored; per-realm
  choropleth on planet + ornate), **trade-route lens** (crossable lanes drawn on the
  planisphere + ornate), **chronicle weaving** (`auto-contact` focal selector). See
  the "Phase 3 surfacing" section in `docs/CLAIMS.md`.
- ~~Phase 3 surfacing — increment 2 (first-contact arc)~~ DONE 2026-06-04
  (`feat(history,lore)` + `docs(backlog)`) — the narrator now weaves a sea route's
  whole life into ONE Sundered-Lane chronicle. **History:** the `EmbargoImposed`
  that severs a route cites the exact `TradeRouteOpened` that bore it (opening
  `EventId` threaded through scratch `SimState::trade_routes` — no schema bump, no
  re-anchor; seed42 laneless → byte-identical), so the lore causal closure is a
  clean two-beat `{opening, embargo}`. **Lore:** `template.rs` frames the arc — a
  birth beat ("two peoples met who never had before"), a severance beat ("sundered
  once more", only if the route was actually severed), and a "Sundered Lane" title;
  a war chronicle stays plain. Advisor-reviewed (caught + dropped a faith-strand
  false-green: the diffusion faith-crossing is gated on religion-unconverted, NOT
  realm control, so it crosses a *different* pair than any trade route — stapling it
  in would assert presence, not same-pair) + a 4-dimension adversarial Workflow
  (16 findings, 7 no-fix confirmations + 4 actionable fixes: CLI help, comment, an
  open-only-branch test, cause-link exclusivity). 6 mutations verified. See the
  "Phase 3 surfacing" rows in `docs/CLAIMS.md`.
- ~~Phase 3 surfacing — increment 3 (landmass place tag + faith beat)~~ DONE
  2026-06-04 (`feat` + `docs`) — the place half of the landmass-centric arc, plus
  its first consumer. **Substrate (schema v22):** `Event::far_shore` (the index into
  `world.continents` an inter-continental event reached; `skip_serializing_if`-elided,
  byte-invisible on laneless seed42), a new `EventKind::FaithCrossed`, and `SeaLane`
  continent tags (`serde(skip)` build-time scratch). The far continent is resolved at
  **Naming** (back-filled onto each lane via the SAME `connected_bodies` +
  `MIN_CONTINENT_DIVISOR` filter that orders `world.continents`, purely additive — no
  classifier refactor). **Carrier:** the Diffusion sea-crossing emits `FaithCrossed`
  on a faith's first crossing to a named continent, deduped per (faith, continent) via
  a scratch `SimState::crossed_faiths` `BTreeSet`. **Consumer (the point — avoids the
  consumer-less-substrate trap):** `select_focal("auto-faith")` + the template names
  the far shore from `far_shore` — "The Faith Comes to Hovelni" — a continent name that
  reaches the chronicle ONLY because the narrator read the tag (the summary says only
  "a far shore"). Determinism: non-perturbation proof held (3 goldens byte-identical at
  v21 with all v22 code in place) → v22 re-anchor for the version byte alone. Design
  panel (3 approaches → 3 judges) + advisor (caught the consumer-less trap, steered the
  faith consumer in + the beachhead out) + a 4-dim adversarial Workflow (24 findings, 0
  defects — index-order correct, NER-safe, no realm/beachhead leak, no RNG perturbation).
  8 mutations verified; one vacuous dedup test caught + replaced with a synthetic unit
  test. See the "Landmass place tag (inc. 3)" rows in `docs/CLAIMS.md`.
- ~~Phase 3 surfacing — increment 4 (multi-strand far-shore weave)~~ DONE
  2026-06-04 (`feat` + `docs`) — the strand half of the landmass-centric arc: a far
  shore reached "by trade, by faith, by sword" woven into ONE chronicle. **Tags (no
  schema bump — `Event::far_shore` already v22):** the overseas colony (`CityFounded`,
  `loops/colonization.rs`) carries the colonized far anchor's continent; the beachhead
  conquest (`Siege`, `loops/mearsheimer.rs`) carries the seized loser-anchor's continent,
  set ONLY when a beachhead is actually taken — both read off the existing
  `lane.continent_a/b` (no new RNG, no schema/golden move). **Consumer (the point):**
  `mapgen-lore` `select_shore` groups tagged events by continent and picks the most
  strand-diverse shore (max distinct strands, tie→event count→lowest index);
  `narrate_shore` (`--event auto-shore`) weaves one gated beat per present strand, each
  NAMING the shore, and degrades gracefully to a single strand. On seed 9 this is "The
  Annal of the Reaching of Duv" — faith (yr 15), colony (yr 31), two beachheads (yr 72,
  417) — where "Duv" reaches the page ONLY via the tags (every event summary says just "a
  far shore"). The `finalize` helper is now shared by `narrate` + `narrate_shore`.
  Determinism: non-perturbation (seed42 laneless → no tag fires → 3 goldens byte-identical,
  no re-anchor). A per-kind probe (advisor-required) confirmed seed 9 is the only
  canonical 3-strand shore and that `auto-shore` never lands on an all-faith shore.
  Advisor (steered the colony strand IN with its OWN observable assertion, not a generic
  ≥2-strand count that faith+sword would satisfy vacuously) + a 5-dim adversarial Workflow
  (0 defects; 3 risk/partial coverage gaps, all deferred — see Next). 3 mutations verified
  (drop colony tag, drop sword tag, unconditional beat). See the "Multi-strand far-shore
  weave (inc. 4)" rows in `docs/CLAIMS.md`.
- ~~Phase 3 surfacing — increment 5 (cross-lens correlation)~~ DONE 2026-06-04
  (`feat` + `docs`) — the Trade lens now tints each crossable sea lane by the prosperity
  of the realms it connects (the AVERAGE of the two endpoint realms, through the SAME
  `prosperity_color` / `PROSPERITY` ramp the Prosperity wash uses — one colour source), so
  a lane binding rich shores reads deeper than one binding poor shores: the
  trade→prosperity causal loop, legible across lenses. Both `render_trade_routes` paths
  (planet `planet-trade` + ornate `layer-trade`) tinted; a lane with no controlled
  endpoint keeps the fallback carmine. **Render-only — no schema, no golden move**
  (`WorldData` untouched). Probe-picked fixture seed 19 (4 lanes spanning avg prosperity
  ~0.13→0.97); the render-path test asserts the rendered `<line>` strokes (rich deeper than
  poor) for BOTH styles, not the per-lane prosperity (which would re-test the data,
  vacuous). Advisor done-checkpoint caught the ornate tint shipping untested-but-claimed →
  parameterized the test over both styles. 3 mutations verified (planet no-tint, planet
  invert, ornate invert). See the "Cross-lens correlation (inc. 5)" row in `docs/CLAIMS.md`.
- ~~Phase 3 surfacing — hardening (inc. 4 review gaps closed)~~ DONE 2026-06-06
  (`feat` + `docs`) — the two deferred test gaps from inc. 4's review, both closed; no
  schema bump, no golden move (the existing three goldens are untouched — a NEW laned
  golden is added, born-anchored). **Gap (i) — CLI auto-shore integration test:**
  `mapgen-cli/tests/lore_cli.rs` drives the binary end-to-end — `generate --planet --seed 9`
  (a NEW `--planet` flag on `generate`, mirroring `refine --planet`; persists
  `GenerateParams::planet` to json.gz, which the CLI previously could NOT do — `planet`
  only rendered SVG, `generate` only continental) → gzip → `lore --event auto-shore` →
  asserts the three-strand weave NAMES the shore ("The Annal of the Reaching of Duv").
  Non-vacuous: the same seed CONTINENTAL fires only the faith strand, so the colony+sword
  beats are load-bearing on `--planet` (this also corrected the working assumption that
  "continental is always laneless" — true for seed 42, not seed 9). **Gap (ii) — laned
  cross-platform golden:** seed 9 planet (the canonical three-strand shore, so `far_shore`
  fires from every carrier) added to BOTH the native `pipeline_spec.rs` and the wasm
  `cross_platform.rs` against one golden `seed9_planet_full.blake3.txt` — the `far_shore`
  tag-firing native↔wasm byte-identity is now pinned by a real golden (was structural only,
  every prior golden being seed42-laneless). Verified green under `wasm-pack test --node`
  HERE, not just CI; `just check` green. See the two "Hardening (inc. 4 review …)" rows in
  `docs/CLAIMS.md`.
- **Next: Phase 3 continued** — increments 4 (multi-strand weave) and 5 (cross-lens
  correlation) shipped above, and **temporal surfacing is also DONE** — it shipped via the
  Replay thread (wasm `render_at_year` swaps `control_at_year` + `religion_at_year`, driven
  by the frontend time-slider, with e2e coverage of both the political and faith sliders;
  the earlier "temporal surfacing" item here was stale). What remains: (a) **realm tagging**
  (`far_realms` on the events) — still DEFERRED for lack of a consumer; the chronicle
  **trade-route strand** is architecturally OUT (the lore engine cannot map a realm to a
  continent at load time — `connected_bodies` isn't a lore dep and the lane continent tags
  are `serde(skip)` scratch), so "by trade" is carried by the colony/settlement strand. (b)
  **prosperity timeline** — the slider animates control + faith, but prosperity is
  end-state only; a per-year prosperity view needs a population-history substrate that hits
  the hashed path (schema bump + golden re-anchor), so it's a larger increment, not a quick
  follow-on. **Test gaps from inc. 4's review — both NOW CLOSED** (the hardening increment
  above, 2026-06-06): (i) the CLI auto-shore integration test landed (`lore_cli.rs`, enabled
  by the new `generate --planet`), and (ii) the laned cross-platform golden landed (seed 9
  planet, native + wasm against `seed9_planet_full.blake3.txt`) — the `far_shore` tag-firing
  native↔wasm byte-identity is no longer structural-only. **So the Sundered Lanes arc's
  Phase 3 surfacing is feature-complete AND its review debt is paid.** What still remains is
  larger or trigger-gated, not a quick follow-on: the **prosperity timeline** (per-year
  prosperity in the slider — needs a population-history substrate on the hashed path: schema
  bump + golden re-anchor), and the deferred **realm tagging** (`far_realms`, still no
  consumer). Aside still open: Phase 1 Step 3b (wind-aware anisotropic lane cost) — revive
  only if lanes look too symmetric.

See **World / planet scale (zoom out)** and **Toggleable map layers + data
overlays** below for full context.

---

## Optimization & meta

### Refinery optimization loop (SA-style auto-tuning)

- **Why deferred.** Both devil's-advocate reviews (2026-05-17) concluded SA on
  20+ continuous knobs against a piecewise objective with 5-30s eval cost is
  effectively random search. Goodhart's law dominates against any rule
  matching Earth statistics — produces statistical-sludge worlds that hit
  metrics but don't look real.
- **Trigger for revival.** The first ornate render reveals classes of
  realism gap that targeted property tests can't catch — e.g., spatial
  correlations across the whole map, multi-rule trade-offs that need
  exploration. *Or* I find myself hand-tuning the same parameter for the
  fourth time and the sweep CLI isn't sufficient.
- **Cost.** 2-3 weeks once revived (the original "4-6 days" estimate was
  optimistic by a factor of two per scope-DA review).
- **Origin.** ARCHITECTURE.md §5.5 v1 (commit `a7f0ead`); cut in `cf6230f`.

### Time-resolved epoch restructure

- **Why deferred.** Renames `Stage` to `Epoch`, adds tick loops and
  awakening logic, but the existing pipeline already encodes causal order.
  Tech-DA called this "a thesaurus pass with cache-invalidation bugs as a
  side dish."
- **Trigger for revival.** A real need to run different stages at
  meaningfully different time resolutions — e.g., glaciation as a
  multi-tick simulation, history sim as in-world annual updates. Today's
  pipeline does fine with one-shot stages.
- **Cost.** 1-2 weeks.
- **Origin.** ARCHITECTURE.md §5.5 v1.

### Composite-score audit CLI (`mapgen audit`)

- **Why deferred.** Depends on the Refinery's rule registry. Property tests
  in `cargo test --workspace` already print pass/fail per rule.
- **Trigger for revival.** The set of measurable realism criteria grows past
  ~15 rules and the user wants a single score number for a world.
- **Cost.** Half a day after Refinery lands.
- **Origin.** ARCHITECTURE.md §5.5 v1.

### Auto-Rule generation from caught bugs

- **Why deferred.** Speculative — we don't yet know if "every bug becomes a
  rule" produces a usefully growing rule set or a tangled one.
- **Trigger for revival.** After we manually author 10-15 realism property
  tests, look at the patterns and see if a meta-generator makes sense.
- **Cost.** Unknown until trigger.
- **Origin.** ARCHITECTURE.md §5.5 v1.

---

## Scale & level-of-detail

The generator works at one scale today: a single continent at ~15k cells
(a cell is ≈ tens of km across). This category is about generating the
*same* world at other zoom bands — a planet-wide view above, and local
urban/rural views below — plus the machinery that keeps them mutually
consistent.

The unifying idea is **nested deterministic refinement**: each finer level
is generated on demand, conditioned on its parent's boundary values, with a
child seed derived from the parent seed + sector id, such that *coarsening
the child reproduces the parent*. Get that contract right once and every
scale composes; skip it and each zoom level is an unrelated random map that
contradicts the one above it.

**The ladder** (current scale in bold): World → **Continental / regional**
→ [Provincial] → [District / hinterland] → Local-urban / Local-rural. The
two bracketed intermediates are the answer to "are layers between these
useful?" — they're real (the hinterland is the bridge that connects a city
interior to the surrounding countryside), but the recommendation is to treat
them as *zoom depths within the framework*, not separate generation passes,
until a concrete need forces otherwise. See the framework entry's design
note.

### Nested multi-scale refinement framework — DONE (Phase 7, 2026-05-25)

- **Shipped.** `mapgen_world::scale::refine_sector(parent, Sector{level,sx,sy},
  RefineParams)` recomputes the shared base field from the *root* seed, adds
  coordinate-addressed sector detail (`StageRng::sector` → splitmix64-nested
  `sector_seed(level,sx,sy)`), runs the physical pipeline over a haloed
  sub-mesh, then projects the parent society + hydrology. On-demand and
  stateless — a sector is a pure function of `(seed, plates, level, sx, sy)`;
  the planet is never persisted at local resolution. Purely additive: level-0
  output stays byte-identical (the `MeshData.region` field elides to `None`),
  and the refine path carries its own native↔wasm golden.
- **Boundary contract.** `scale::pin_edges_to_shared` blends terrain back toward
  the shared base field at sector edges (smoothstep); rivers + society are
  projected from the parent, so they're globally consistent and therefore
  seam-consistent. Intermediate layers (provincial, district) are refinement
  *depths*, not distinct pipelines — one mechanism, not five.
- **Origin.** 2026-05-24 multi-scale request; built in Phase 7.

### 3D globe view — DONE (2026-06-02)

A user-requested interactive 3D globe: **Scale: Globe** mounts a three.js
sphere textured with the world, rotatable (drag) + zoomable (scroll), click a
point to drill into that region. Built in 5 gated increments (design: an
8-agent panel + advisor chose plain three.js, lazy-loaded). Lives in
`web/src/globe.ts` (all three.js) + pure drill math in `web/src/sector.ts`.

- **three.js is dynamic-`import()`ed** into its own lazy chunk so the default
  SVG page stays ~20KB (three is ~130KB gz). Committed guard
  `web/scripts/check-bundle.mjs` (CI-wired, mutation-verified) fails if a static
  import re-merges three into the entry chunk.
- **Texture:** the flat `biomes` equirectangular render of the planet world,
  rasterized (shared `rasterizeSvg`) to a 2048×1024 `CanvasTexture`. Unlit
  `MeshBasicMaterial` — a paper globe, not a shaded Earth.
- **Drill:** the raycaster's intrinsic surface UV → `uvToWorld` (Vitest-pinned,
  mutation-verified; the `1-v` flipY convention validated end-to-end by the
  click e2e) → the SAME `continentAt` + refine the 2D planet uses → hands off to
  the 2D SVG sector. `?scale=globe` permalink + "Globe" breadcrumb root.
- **Headless WebGL** works in CI via SwiftShader launch flags
  (`playwright.config.ts`); e2e keys off a `data-rendered` first-paint signal +
  `data-textured`, never pixels.
- ~~**Antimeridian seam + pole pinch**~~ DONE 2026-06-06 — the flat non-periodic
  grid's left/right coastlines don't meet at lon ±180° and the poles pinch.
  Resolved client-side (cheaper than the once-envisioned Rust edge-fade render):
  `globe.ts::fadeMapEdges` fades the texture's four edge bands to the open-ocean
  colour, so the antimeridian reads as a sea strip (both edges become water) and
  the poles as clean ocean caps. The geometric pinch is inherent; the visible
  artifact is gone.
- ~~**Richer parchment globe skin**~~ DONE 2026-06-06 — replaced the flat `biomes`
  v1 texture with `Style::GlobeTexture` (`style/planet.rs::render_globe_texture`):
  an equirectangular, FONTLESS render (the new `Proj::equirect` identity
  projection) of the parchment biome fill + depth-shaded sea + political control
  wash + coast + major rivers. Fontless is the point — the labelled styles
  rasterize unreliably as an `<img>`, which is why the globe was stuck on biomes.
- ~~**Per-year re-texture (time-lapse on the sphere)**~~ DONE 2026-06-06 — the
  time-slider now shows at the globe root; scrubbing re-textures the sphere via
  `renderAtYear("globe", y)` (the political wash is in the texture, so empires
  rise/fall ON the globe). **Scrub perf:** profiling found the per-frame cost was
  ~2.4 s, dominated by `fadeMapEdges` sampling the sea colour via `getImageData`
  (a GPU→CPU readback per call). Fixed by fading to the known deep-sea constant
  (the globe is always the `globe` style) + skipping mipmap generation — ~10× on
  the fast path (≈0.2 s floor; the rest is the inherent SVG rasterize).
- ~~**Lens toggles on the globe**~~ DONE 2026-06-07 — Faith/Prosperity/Trade washes
  on the sphere (parity with the 2D planisphere, which the globe lacked). The
  globe texture (`render_globe_texture`) now emits the lens wash groups + the same
  swap CSS the planisphere uses; the frontend injects the active `on-<lens>` root
  class into the cached `globe` SVG before rasterizing (`withLayerClasses`), and a
  lens toggle in globe mode re-textures the sphere (`setLayerState` → `retextureGlobe`,
  no worker hop). Composes with the time-slider (the lens applies to each year frame).
- ~~**Cinematic fly-to on drill**~~ DONE 2026-06-07 — a click on the globe now
  animates the camera to swing the clicked point to face the viewer and zoom partway
  in (~600 ms ease-in-out, `globe.ts` RAF loop), THEN drills to the 2D sector,
  instead of an instant cut. Reads `hit.point` (world space), so it's independent of
  the UV hemisphere convention; the drag-rotates-doesn't-drill behaviour is unchanged.
- **Buttery scrub via base-layer caching (render only the per-year political wash
  over a cached static base) — ADVISED AGAINST (2026-06-07); do cartographic
  generalisation instead.** Decision from a vision-vs-optimization assessment.
  *Why not:* it optimises the symptom (one giant per-frame SVG), not the cause —
  the globe is the zoom-OUT *overview* yet renders the full ~18k-cell world, detail
  a 2048×1024 sphere texture can't even resolve. It also fights "one mechanism, not
  five": it adds a second render+composite path, and since the planet/globe SVG is
  deliberately NOT golden-pinned (`planet.rs`: "the planet SVG is never hashed"),
  that path could silently drift from the canonical `render()` (the wash is alpha-
  blended at 0.40 over the fill; compositing it as a separate layer changes the
  blend math) with nothing to catch it. And it speeds ONLY scrub — initial texture,
  lens toggles, and the fly-to first paint stay full-cost.
  *Do instead:* **cartographic generalisation** (ADR 0001 §3 — per-level
  stylesheets, Visvalingam–Whyatt coastline/river simplification, Töpfer feature
  budgets, rank-selected settlements). It shrinks the overview SVG at the source,
  so EVERY globe op gets cheap (not just scrub), keeps ONE canonical render path,
  serves planet/continental/regional alike, and is the missing detail-DECREASING
  half of the already-shipped detail-INCREASING render fidelity (shared per-level
  stylesheet). It makes the cache moot — a small overview SVG re-renders fast enough
  that splitting base from wash buys nothing.
  *Revival trigger (build the cache only if ALL hold):* (1) globe time-scrub becomes
  a central, heavily-used interaction (e.g. a "play history" auto-animation at many
  fps); AND (2) generalisation has already shipped and scrub STILL misses its frame
  budget; AND (3) a visual-equivalence guard (pinned snapshot diff of base+wash vs
  single-pass `render()`) closes the "never hashed" gap. Cheap interim before then:
  throttle scrub frames + show the nearest already-rasterised year instantly (no new
  render path).

### World / planet scale (zoom out) — increment 1 DONE (2026-05-25)

- **Resolved design question.** Plate positions are sampled in `[0, width)`, so
  *enlarging* the canvas to wrap the current world in a coarser planet re-rolls
  the whole layout — there is no cheap "level −1". The architecture-fitting form
  is therefore: **the planet IS the root (level 0)**, generated multi-continent,
  and continental maps are its refined sectors (zoom-in is the consistent
  direction the framework already gives). No multi-continent bias was needed —
  more plates over a 2:1 aspect already yields several continents in an
  encircling sea.
- **Shipped (increment 1).**
  - `GenerateParams::planet(seed)` — 2:1 aspect, 32 plates, 18k cells → a
    multi-continent world (the root).
  - `Style::Planet` (`style/planet.rs`) — an antique *planisphere*: biome-tinted
    continents over a depth-shaded sea, a lat/long graticule, major rivers
    (Strahler ≥ 4) + the largest ranges, flood-filled continent labels (antique
    Latin `TERRA SEPTENTRIONALIS…` by position) + a `MARE OCEANVM` ocean label,
    reusing the ornate parchment/typography/compass/cartouche (now `pub(crate)`).
    Drops per-cell forest/settlement clutter; vignette without the ink-stains.
  - `mapgen planet --seed` (generate + render the overview) and `mapgen refine
    --planet` (drill a sector of the same globe — same `refine_sector`, so
    planet → continent → region is one mechanism). Proven by
    `scale_spec::planet_root_refines_into_a_continental_sector` +
    `visual_regression::planet_render_rasterizes_to_a_sane_image`.
- **Shipped (increment 2, item 1 — frontend zoom-out, 2026-05-31).** A "Scale"
  control (Continent · Planet) in the web UI; `Generation.planet(seed, cells,
  nations)` in `mapgen-wasm` (same pipeline on the planet preset); the worker
  branches the constructor on a `scale` field; the planet is the breadcrumb
  *root* ("Planet" vs "World") and click-to-drill refines a continental sector
  via the existing machinery; `?scale=planet` permalink. Pure nav helpers
  `crumbLabel` / `navStyle` are unit-tested (`sector.test.ts`) and an e2e
  (`smoke.spec.ts`) generates a planet then drills a continent.
- **Shipped (increment 2, item 2 — grounded continent/ocean names, 2026-06-01).**
  `name_world` step 8 flood-fills land/sea into major bodies (`connected_bodies`)
  and names each in its dominant culture's language (`dominant_culture`, stable
  lowest-id tiebreak), stored as schema-v17 `world.continents`/`oceans`
  (name + centroid + cell_count). `style/planet.rs` reads them instead of the
  positional `latin_quarter`/`MARE OCEANVM`. Contract pinned in `continents_spec`
  (flood-fill disjoint/coverage, tiebreak, grounding via a two-culture fixture,
  determinism); thresholds in `docs/tuning_log.md`.
- **Shipped (increment 2, item 3 — continent-aware drill, 2026-06-01).** A root
  click snaps to the clicked landmass: `continent_at` (nearest cell → land body
  → 2.5% threshold) → `continentAt` wasm query → the frontend re-centers the
  drill on the centroid (`sectorAt`) and sizes its depth (`continentDrillLevel`).
  Re-center, not tight framing — see the "Tight continent framing" entry below.
- **Open follow-ups (increment 2+):** planet-render perf budget (**the immediate
  next item**); tight continent framing (entry below); projection / distortion at
  the planetary edge; inter-continental society/history (trade, migration) —
  currently society is generated per-world.
- **Origin.** This session, 2026-05-25, "let's do zoom out" → chose the
  level-above-0 hierarchy.

#### Planet-scale history visualization — DONE 2026-06-01

- **Shipped.** `Style::Planet` now washes in political control (per-cell tint by
  realm colour at 0.40 opacity over the biome fill) with a SW-corner REALMS
  legend; `refreshTimeslider` no longer gates on `!planetScale`. Because
  `render_at_year` swaps `control` before re-rendering, scrubbing animates
  empires rise/fall on the planisphere for free. Pinned by a render test
  (founding era ≠ present) + an e2e that scrubs and asserts the SVG changed.

#### Globe edge projection — DONE 2026-06-01 (Mollweide)

- **Resolved design (6-agent workflow + advisor DA).** Chose **Mollweide**
  (equal-area oval) over orthographic (rejected — only a hemisphere, breaks the
  drill for the far side), Robinson (no closed-form inverse), and sinusoidal
  (sharp petal poles). Whole world in one view, closed-form inverse for the
  drill, the classic antique oval-on-parchment silhouette, low warp.
- **Shipped.** `style/planet.rs` projects every world coordinate onto the oval
  via a `Proj` struct (Mollweide is separable — `sx = cx0 + (wx−cx0)·cosθ(wy)`,
  `sy = f(wy)` — so it's a per-row table lookup, no per-vertex trig); the
  graticule is now curved projected polylines; compass/cartouche/legend/vignette
  stay in screen space. Drill kept correct: `sector.ts` `mollweideUnproject`
  (closed-form) inverts a planet-root click before `continentAt`; corner clicks →
  null (inert); `projectedBounds` frames the coarse zoom. TS↔Rust projections
  pinned to shared reference points in both test suites. Perf re-anchored 13→19ms.
- **Follow-up — guard strengthened 2026-06-01.** The forward Mollweide
  projection is necessarily duplicated in Rust (render) and TS (drill/framing) —
  different runtimes, no shared code. The drift guard is now a committed
  single-source vector grid (`crates/mapgen-render/tests/mollweide_vectors.txt`,
  17 exact points spanning the oval) asserted by BOTH suites; a drifted constant
  fails its side (verified by mutation). A true single implementation would need
  the frontend to call the wasm projection on the main thread (a second wasm
  instance + FFI in the drill hot path) — judged not worth it; the vector pin
  covers the real risk. Revisit only if a third consumer needs the projection.

#### Toggleable political wash on the planisphere (pure-physical view)

- **Why deferred.** The planet political wash is always-on, so there's no way to
  get the clean biome-only planisphere back. The ornate style makes its political
  overlay a toggleable layer; the planet's isn't yet wired into the layers system.
- **Trigger for revival.** Someone wants the pure-physical planet view (no
  empires) — or the wash reads as clutter often enough to want it off by default.
- **Cost.** ~2h (wrap `render_political`/`render_nation_legend` in an
  `on-<name>` layer class + add it to the web layers manifest, defaulting on).
- **Origin.** Planet-scale history viz, 2026-06-01 (advisor: ship always-on,
  backlog the toggle).

#### Continent-aware drill — DONE 2026-06-01 (re-center)

- **Shipped.** A root click no longer snaps to whatever quadtree quadrant it
  lands in; it snaps to the *continent under the cursor*. `continent_at(world,
  x, y)` (nearest cell → land body → same `connected_bodies` + 2.5% threshold
  the naming stage uses) returns the landmass centroid + cell_count; the wasm
  `continentAt` exposes it; the frontend re-centers the drill on the centroid
  (`sectorAt`) and sizes its depth from the landmass (`continentDrillLevel`).
  Over sea / a speck it falls back to the quadtree drill. No per-cell
  `continent_id` map was needed — the drill recomputes per click (its only
  consumer), so no schema bump.
- **Resolved design (from the advisor DA).** *Re-center, not frame.* True
  bbox-framing was rejected: a continent isn't square, and with 32 plates
  continents straddle quadtree midlines constantly, so "frame the extent" either
  does nothing for straddlers (a midline continent only fits level 0) or requires
  re-keying `refine_sector`'s `(level,sx,sy)` RNG — re-opening a working,
  invariant-tested subsystem (the Refinery pattern). Re-center lives within the
  grid and behaves identically for every continent. See the follow-up below.

#### Tight continent framing (the bbox version)

- **Why deferred.** The shipped drill re-centers on the clicked continent's mass
  but still lands in a square quadtree sector — it does not crop tightly to the
  continent's outline. Tight framing needs an arbitrary-rect refine, which means
  re-keying `refine_sector`'s RNG away from `(level,sx,sy)` and re-proving the
  seam/tiling/reproduction invariants (`scale_spec`). That is a substantial
  rework of a load-bearing subsystem for a framing nicety.
- **Trigger for revival.** Re-center proves insufficient in practice — users
  consistently want the continent cropped to its coastline, not centered in a
  square — *and* the seam/tiling invariants can be preserved (or consciously
  relaxed) under rect-addressed refinement.
- **Cost.** ~3-5 days (the refine RNG re-key + invariant re-proof dominate).
- **Origin.** Continent-aware drill DA, 2026-06-01 — the option the advisor
  steered away from as "the Refinery in new clothes."

### Local rural / hinterland maps (zoom in, countryside)

- **Why deferred.** Framework prerequisite; and the regional map's settlement
  dots + biome fills are enough until someone needs to *stand inside* a
  region.
- **Design note.** Refine one non-urban sector to field-and-farmstead
  resolution: open-field strips vs. enclosures vs. terraces vs. paddies vs.
  vineyards keyed to parent biome + culture + era; hamlets and farmsteads;
  mills on the streams; fords and bridges where roads cross water; lanes and
  tracks branching off the parent road; woodlots, pasture, and the local
  stream network refined from the parent river's entry/exit points; plus any
  `LorePatch` features in the sector (sacred groves, ruins, mine mouths). The
  "similar detail to urban" mandate means this gets the *same* render-polish
  vocabulary the city map gets (labels, glyphs, hatching, contour/hachure
  relief, edge-burn) — countryside is not a green blob.
- **Trigger for revival.** A region or settlement needs a travel-map or
  VTT-usable local view; or lore references a specific village / ford / grove
  that should be drawable.
- **Cost.** 1–2 weeks (fine-terrain refinement + field- and
  settlement-scatter algorithms + render at the new zoom).
- **Origin.** This session, 2026-05-24, multi-scale request.

### Local urban maps (settlement interiors)

- **Why deferred.** Framework prerequisite; and it's a different algorithm
  family from terrain generation — procedural city layout (road networks,
  parcel subdivision, walls), i.e. substantial new work, not reuse. The
  settlement glyphs on the regional map suffice until users want to "enter" a
  city.
- **Design note.** Refine a settlement cell to street-and-district
  resolution: street network (organic-medieval vs. orthogonal grid vs. radial,
  keyed to culture/era, via tensor-field or agent road growth); districts /
  quarters; walls + gates + towers sized to population; citadel / keep /
  temple-precinct; market squares; river or harbor frontage; extramural
  suburbs; cemeteries — with the approaches stitched to the hinterland map's
  roads. Population, culture, religion, and polity already live on the parent
  settlement, so the city is *earned* by the regional sim rather than dropped
  in. Same render vocabulary as the rural map, so the atlas reads as one work
  at different zooms.
- **Trigger for revival.** A specific city needs an interior map for a game
  or chronicle illustration; or the web frontend wants a "zoom into a
  settlement" interaction.
- **Cost.** 2–3 weeks. Procedural urban generation is its own discipline;
  culture/era variants multiply it.
- **Origin.** This session, 2026-05-24, multi-scale request.

### Scale-dependent render fidelity (per-feature level-of-detail) — DONE (2026-05-25)

- **Origin.** User request, 2026-05-25: "zooming in should also change the
  fidelity level of the details — those trees should become more detailed
  forests."
- **DONE (2026-05-25).** `ornate_antique` derives `detail = world_width /
  view_width` (1 at level 0, 2/4/8… per sector) and draws scale-aware glyphs,
  all gated on zoom so the level-0 render is byte-identical:
  - `detail ≥ 2`: richer **forests** (trunk + layered conifer / lobed broadleaf
    crowns, denser canopy); **mountains** gain a subordinate ridge peak;
    **coastline** ripples scale by 1/detail so the finer coast isn't drowned by a
    bloated haze.
  - `detail ≥ 4` (regional zoom): **settlements** bloom into a town footprint — a
    building cluster around the landmark glyph, dashed wall ring for capitals,
    hamlet clusters for villages.
  - `detail ≥ 8` (local zoom): **full town plans** — an irregular wall enclosure
    (capitals), a street network keyed to the founding culture's architecture
    (chord grid for planned cultures, radial spokes + ring road for organic),
    quarters of buildings, a market plaza, the landmark glyph as the central
    citadel, and a **harbour** (piers + moored boats) for coastal towns.
- **Detail-decreasing generalisation direction (ADR 0001 §3) — STARTED 2026-06-07.**
  The complement to the detail-increasing fidelity above, now that the planet/globe
  overview exists above level 0. Increment 1 (DONE): **Visvalingam–Whyatt river
  simplification on the overview** — a new `mapgen-render::simplify::visvalingam`
  (pure, unit-tested) drops sub-cell river wiggle the planisphere can't resolve,
  applied in `style/planet.rs::render_major_rivers` in WORLD space before projection
  (projection- and seam-independent), so both the Mollweide planisphere and the globe
  texture get cleaner, lighter rivers (seed 11: 183→125 points, ~31% fewer, shape
  preserved). Render-only — the planet SVG is never hashed, so no golden moves.
  Pinned by `simplify` unit tests + `generalisation.rs` (drawn < raw). Tolerance in
  `docs/tuning_log.md`. Increment 2 (DONE 2026-06-07): **coastline simplification** —
  the coast is traced into continuous land/sea loops (reusing the SHARED
  `extract_coastline_polylines`, now `pub(crate)`, that the ornate ripples use — one
  coastline extraction) and each loop Visvalingam-simplified in world space, replacing
  the former per-coastal-cell polygon outlines (a crenellated band of full Voronoi
  hexagons) with one clean drawn coast line; per-cell biome fills preserved beneath.
  Both planisphere + globe benefit (shared `render_coast`). seed 11: 27 loops, ~1089
  points (well under the boundary-edge count). Pinned by
  `generalisation.rs::overview_coastline_is_traced_and_simplified`.
  **Remaining — LOWER value for the current structure:** scale-rank settlement/label
  selection (the overview already rank-selects — top-8 ranges, top-6 labels, Strahler≥4
  rivers — with working constants; Töpfer-keying them is marginal while the planet is
  the only level using this render path) and a per-level stylesheet (an architectural
  refactor that pays off mainly when ONE render path spans many levels — not the current
  planet-vs-ornate split). The two HIGH-value generalisation wins (rivers + coastline)
  are shipped. Optional further city detail: named districts/wards.
- **The gap.** Phase 7 refinement gives a drilled-in sector more *cells* (so more
  tree glyphs, finer rivers/coastline), but every feature still renders with the
  same whole-world glyph vocabulary — a forest is just a denser sprinkle of the
  small world-scale tree marks, not a richer forest. At a closer scale, features
  should gain *detail*, not merely count.
- **What it should become (examples).**
  - Forests: world = scatter of small tree marks → regional = larger individual
    trees + canopy texture → local = tree clusters with trunks/shadows/varied
    species marks.
  - Mountains: scaled triangles → ridgelines / hachures / contour-like strokes.
  - Coastlines: finer crenellation / roughr detail budget that grows with zoom.
  - Settlements: single glyph → town plan / street hint at urban scale.
- **What it needs.** A per-level render "stylesheet" keyed off the sector level /
  `mesh.region` size (the renderer already knows its viewport via
  `MeshData::view_rect`): choose feature glyph variants + detail budgets by scale.
  This is the *detail-increasing* complement to ADR 0001 Q3's generalisation
  (which *decreases* detail when zoomed out); the two share the per-level
  stylesheet mechanism. Pairs naturally with the "per-level cartographic
  generalisation" follow-up listed under the navigation item below.
- **Trigger.** Now that drill-in works (Phase 7), this is the most visible next
  uplevel for the atlas. Largely a `mapgen-render` change; no schema impact.

### Seamless inter-scale navigation — research + MVP DONE (2026-05-25); generalisation/prefetch deferred

- **Research pass DONE (6.3, 2026-05-24)** → **`docs/adr/0001-multiscale-navigation.md`**.
- **Framework + MVP navigation DONE (Phase 7 / 8.4, 2026-05-25).** Shipped:
  `mapgen-world/src/scale.rs` (`Sector` + `refine_sector` — deterministic
  on-demand sector refinement through the physical pipeline; coarsening contract
  tested at 94–98% coastline agreement), `mapgen-wasm::refineSector`, the
  `mapgen refine` CLI, and the browser drill-in (`web/src/{worker,main,panzoom}.ts`
  — click to zoom in, coarse-first focus, clickable breadcrumb). See the ADR's
  "Implementation status" for the done/deferred split.
- **Seam-pinning DONE (2026-05-25).** `scale::pin_edges_to_shared` blends each
  sector's terrain back to the shared base field toward its edges, so adjacent
  sectors agree along their seam (elevation MAD < 0.04 along a shared edge).
  River-crossing continuity across seams is the remaining piece.
- **Remaining follow-ups (each its own future item):** per-level cartographic
  generalisation (Töpfer budgets + Visvalingam simplification + scale-rank
  labels); rank-driven background prefetch; per-sector society (settlements/
  roads/local history); true cross-fade + raster pyramid (trigger-gated).
- **Original deferral rationale (kept for context).** Depended on the refinement
  framework plus at least one
  local scale existing — there's nothing to navigate *between* yet. And the
  right interaction model is itself an open question that wants a research
  pass before any code: an ornate hand-drawn atlas is traditionally a set of
  discrete plates with inset cross-references, not a continuous slippy
  surface, so forcing Google-Maps-style continuous zoom may fight the
  aesthetic — that tension needs deciding, not assuming.
- **The problem.** Two seams have to disappear for movement between scales to
  feel earned rather than like flipping between unrelated pictures. (1) The
  **temporal seam** as the user zooms: representations must transition without
  "pop-in," labels must fade/declutter sensibly, and which features appear
  must change with scale (a continent shows mountain ranges; a district shows
  individual hills). (2) The **spatial seam** between two independently
  generated adjacent sectors: their shared edge must agree on terrain height,
  river crossings, and road continuation. The generation-side half of the
  spatial seam is the refinement framework's boundary-condition contract; this
  entry owns the *navigation, rendering, and streaming* half.
- **Deep-research instructions.** When picked up, run a focused research pass
  that answers each question below with cited prior art, and ends in a short
  ADR-style recommendation (interaction model + render pipeline + transition
  technique + trade-offs) *before* implementation:
  1. **Interaction model.** Continuous geometric/semantic zoom (slippy map)
     vs. discrete atlas-plate drill-in vs. overview+detail / focus+context.
     Which fits an ornate atlas *and* an on-demand backend that costs seconds
     (not milliseconds) per sector? Study: Shneiderman's mantra (overview
     first, zoom & filter, details on demand); focus+context (fisheye,
     DOITrees); Google/Mapbox slippy zoom; Dwarf Fortress world → embark →
     local-map drill-in; 4X strategic-vs-tactical view swaps.
  2. **LOD transition / anti-popping.** How to morph between representations
     without a visible jump. Study: terrain LOD geomorphing (geometric
     clipmaps, chunked LOD, geomipmapping / ROAM); Mapbox GL vector-tile
     cross-fade; the CSS-scale-then-swap trick between integer zoom levels.
  3. **Cartographic generalization** — what to show / hide / simplify /
     aggregate per scale. Study: Töpfer's Radical Law (feature count vs.
     scale); the generalization operators (selection, simplification,
     aggregation, displacement, typification); Douglas–Peucker and
     Visvalingam–Whyatt line simplification; scale-dependent stylesheets
     (Mapbox GL style-spec zoom expressions).
  4. **Labels across zoom** — fade in/out, per-level collision/declutter,
     anchored persistence. Study: Mapbox GL label collision + fade; Imhof's
     label rules (we already use his SA placement); priority / scale-rank
     labeling.
  5. **Spatial-seam consistency** between adjacent generated sectors. Study:
     constrained boundary generation, ghost/halo cells, Wang tiles / corner
     tiles, blue-noise tile stitching, marching-squares contour continuity
     across tile borders. Cross-reference the framework entry's
     boundary-condition contract.
  6. **Streaming / prefetch.** Our sectors cost seconds to generate, so
     generate-ahead matters far more than for millisecond tile fetches.
     Study: slippy-map tile prefetch (adjacent + next-zoom), velocity-
     predictive loading, a background WebWorker generation queue, and
     progressive coarse-first rendering (show the upscaled parent instantly,
     swap in the refined child when ready).
  7. **Vector vs. raster pipeline.** Our renders are ornate SVG. Decide:
     render vector per sector on demand, or bake a raster tile pyramid
     (resvg → PNG tiles) for fast pan/zoom and render vector only at the
     active focus? Study: vector tiles (MVT) vs. raster tile pyramids, hybrid
     approaches, and in-browser SVG performance ceilings.
  8. **Determinism of the journey.** The same pan/zoom path must yield the
     same intermediate states. Confirm the framework's
     `child_seed = blake3(parent_seed, level, sector_id)` gives stable
     sectors regardless of the path taken to reach them, and define how
     fractional zoom resolves (snap to nearest generated level + interpolate,
     or generate a true intermediate).
- **Trigger for revival.** The framework + at least one second zoom band have
  landed and we want the web frontend to let users move between scales
  *interactively*, rather than generating each scale as a standalone export.
- **Cost.** Research pass: 2–3 days to produce the recommendation.
  Implementation: scoped by that recommendation — a discrete drill-in is
  days; a continuous geomorphing slippy renderer over on-demand generation is
  weeks.
- **Origin.** This session, 2026-05-24; multi-scale request, follow-up on
  cross-scale continuity ("how they flow together as the user moves between
  scales").

---

## Geography & geology

### Hot-spot tracks + abyssal-age subsidence

- **Why deferred.** Visual variety but lower priority than fixing climate /
  hydrology. The current plate model produces recognizable continents
  without it.
- **Trigger for revival.** Worlds need Hawaiian-style volcanic island chains
  for narrative purposes (Pacific-like seeds). Or the rendered sea floor
  needs to vary by age (deep abyss vs shallow ridge).
- **Cost.** 1-2 days. Each plate gets a drift vector; iterate 20 ticks of
  50My; deposit hotspot bumps at the current cell above each plume; depth =
  2500 + 350·√age (GDH1 model).
- **Origin.** Research brief in commit `44ec18f`.

### Glaciation one-shot pass

- **Why deferred.** Most fantasy worlds get away without explicit glaciation
  scars. We get tundra biomes for free at high latitudes.
- **Trigger for revival.** Need fjords (drowned glacial valleys) for
  Norse-coast aesthetics. Or U-shaped valleys / drumlins / moraine ridges
  for visual interest in high-latitude terrain.
- **Cost.** 1-2 days. Compute ice mask (lat > 50° OR elev > snowline),
  scour inside, raise moraine at boundary, drop fjord cells where mask edge
  meets coast.
- **Origin.** Research brief in commit `44ec18f`.

### USDA simplified soil orders — DONE (6.1.4, 2026-05-24)

- **Shipped.** `crates/mapgen-world/src/soils.rs` classifies every land cell
  into one of the USDA orders from climate (temp/precip/seasonality), drainage
  (`hydrology.flow`) and local relief; stored as `ClimateData::soil: Vec<u8>`
  (schema v14), classified at the head of the Biomes stage. Andisol is never
  assigned (no volcanism model) — documented. Calibrated across seeds 1/7/42/99/
  123 (no order >75% of land; aridity matched to `koppen` via `p_annual`).
- **Soil-driven biome refinement (the item below) shipped with it:** waterlogged
  Histosols become the new `biomes::WETLAND` (id 15) — a real biome the
  pure-climate palette couldn't express. Colors added to both render styles.
- **Now available to cultures** for habitat/agricultural scoring (the original
  trigger) — a follow-up can weight settlement suitability by soil order.
- **Origin.** Research brief in commit `44ec18f`.

### Volcanic point classification

- **Why deferred.** Decorative; no functional dependency.
- **Trigger for revival.** Render style wants volcano icons; or fire-giant
  archetype needs volcanic patches as habitat.
- **Cost.** Half a day. Shield (at hotspots / ridges) vs stratovolcano (at
  subduction zones) vs caldera (rare).
- **Origin.** Research brief in commit `44ec18f`.

### Continental shelf shoulder

- **Why deferred.** Current sea is uniform-depth at the abyssal value.
  Visually fine for now; cartographically uninteresting.
- **Trigger for revival.** Render needs a recognizable shelf-vs-abyss
  bathymetry. Or coastal cells need to track shelf width for fishing
  resources (cultures stage).
- **Cost.** Hours. Push deep cells from -0.3 to -0.7; define shelf as
  -0.05 < elev < 0.
- **Origin.** Research brief in commit `44ec18f`.

### Multi-continent worlds

- **Why deferred.** MVP is single-continent. Multi-continent adds
  scale/projection issues (which continent is rendered, distortion at
  edges).
- **Trigger for revival.** User wants to generate a world map showing all
  continents, not just a continent.
- **Cost.** A week. Mesh + plate model already supports it; the work is in
  rendering and labeling.
- **See also.** Scale & level-of-detail → "World / planet scale" — that
  entry is the zoom-out *view*; this is the *generation* of >1 continent it
  builds on.
- **Origin.** ARCHITECTURE.md §2 (deferred from MVP).

---

## Hydrology

### Strahler stream order — DONE (6.1.5, 2026-05-24)

- **Shipped.** `extract_rivers` computes per-cell Strahler order over the flow
  network (`HydrologyData::strahler`) and each `River::strahler` (mouth order).
  Sources = 1; a confluence increments only when ≥2 equal-order streams meet.
- **Caveat (documented in tuning_log).** At 4k cells the order tops out at ~2–3
  (small networks), so it's *not* a good render-width signal — √flow stays
  smoother and wider-ranging — and a naming gate on order ≥ 3 would strip all
  river names on low-order worlds. It's kept as the standard classification
  attribute (creek/stream/river) and the rank signal the multi-scale atlas will
  use for LOD river selection (per ADR 0001 §3). Grows expressive at higher cell
  counts.
- **Origin.** Rivers commit `bd30c9b`.

### Distinct headwater origin types

- **Why deferred.** Today's headwaters are all "elev ≥ 0.30 OR adjacent to
  lake." Real headwaters split into snowmelt / spring / glacier / lake-
  outlet.
- **Trigger for revival.** Seasonal-flow regime (next item) needs to know
  source type. Or cultures stage settlement-suitability wants to favor
  perennial springs.
- **Cost.** Half a day. Snowmelt if cold winter; glacier if elev > snowline;
  spring on impermeable-rock cells with high flow_acc; lake-outlet trivial.
- **Origin.** Rivers commit `bd30c9b`.

### Delta extraction at river mouths

- **Why deferred.** Iconographic for major rivers (Nile, Mississippi,
  Ganges) but cosmetic. Single channel reaches the sea fine in current
  render.
- **Trigger for revival.** Ornate render style wants delta fans visible.
  Or settlement placement needs to favor delta cells.
- **Cost.** Half a day. For each river with mouth flow > threshold,
  subdivide last 2-3 cells into 2-3 distributary paths with halved width.
- **Origin.** Rivers commit `bd30c9b`.

### Endorheic basin marking

- **Why deferred.** Already have lakes (24 on seed 42). Rivers ending in
  lakes work but aren't tagged as endorheic.
- **Trigger for revival.** Lore engine wants Caspian / Aral / Lake Chad
  patterns (drying-lake civilizations). Or render style wants to color
  endorheic basins differently.
- **Cost.** Hours. Walk each river; if terminus is in a lake (not sea),
  mark `River.endorheic = true`.
- **Origin.** Rivers commit `bd30c9b`.

### Seasonal river regime — DONE (6.1.5, 2026-05-24)

- **Shipped.** `hydrology::classify_river_regimes` (run at the end of the climate
  stage) tags each `River::regime` as Perennial / Summer-monsoon / Winter-rain /
  Nival / Ephemeral, from the catchment's seasonal precipitation balance and
  headwater winter temperature. The ornate render draws **ephemeral** rivers with
  a dashed line (the standard intermittent-stream convention). Seed 42 yields a
  varied mix (Ephemeral 20, Perennial 10, Nival 5, Monsoon 2, Winter-rain 2).
- **Future hooks.** History-sim drought/famine tied to ephemeral rivers failing
  (the original trigger) and seasonal river labels remain open follow-ups.
- **Origin.** Rivers commit `bd30c9b`.

### Meander geometry in flat country

- **Why deferred.** Rivers follow Voronoi cell-corner paths; in flat
  country they zig-zag rather than wiggle smoothly. Subtle aesthetic gap.
- **Trigger for revival.** Ornate render reveals stiff river polylines as
  un-natural.
- **Cost.** A day. Per cell-pair, add Perlin-noise lateral offset to the
  polyline scaled by inverse-slope.
- **Origin.** Rivers commit `bd30c9b`.

### Drainage-divide visualization

- **Why deferred.** Useful for political-border heuristics in Phase 3
  (real borders often follow watersheds) but not visible yet.
- **Trigger for revival.** Polities stage uses watershed boundaries to
  draw political borders. Or render wants explicit divide lines.
- **Cost.** Half a day. Group cells by ultimate downstream terminus;
  divide = edge between cells in different groups.
- **Origin.** Rivers commit `bd30c9b`.

### Karst, groundwater, oxbow lakes

- **Why deferred.** Specialized features. Karst (underground rivers) is
  cute but visually invisible. Oxbow lakes emerge naturally from meanders
  if we model those.
- **Trigger for revival.** Specific narrative need (subterranean dungeons
  follow karst; an in-world river relocates leaving an oxbow as a
  landmark).
- **Cost.** A day each.
- **Origin.** Research brief in commit `44ec18f`.

---

## Climate

### Stronger precipitation variance

- **Why deferred.** Current model has p10 land precip ≈ 0.04, p90 ≈ 0.07
  — factor of 2 variance. Real Earth: Atacama 1mm/yr to Cherrapunji
  12000mm/yr, factor-of-12 minimum. The tight variance is why "desert"
  cells are rare even with reasonable thresholds.
- **Trigger for revival.** Property test fails because desert fraction
  stays stuck below Earth's. Or render reveals the world is "everywhere
  the same humidity."
- **Cost.** 1-2 days. Stronger orographic shadow (release coefficient
  scales harder with uplift); weaker baseline release on flat land;
  longer cumulative dry-out across deep continents.
- **Origin.** Realism audit in commit `117eafe`.

### Per-band wind speed (Coriolis)

- **Why deferred.** Wind direction varies by band; wind *speed* is
  treated as unit-magnitude. Real trades blow harder than westerlies.
- **Trigger for revival.** Sailing-civilization narratives need trade-wind
  routes; render wants weather-wave indicators.
- **Cost.** Hours. `wind_vector` returns magnitude as well as direction;
  apply to moisture transport rate.
- **Origin.** Realism audit in commit `117eafe`.

### Monsoon system

- **Why deferred.** ITCZ shift in seasonal model produces some
  summer-wet/winter-dry pattern but doesn't show the dramatic monsoon
  reversal (India, Sahel).
- **Trigger for revival.** Specific tropical-civilization narratives. Or
  property test demands a clearly monsoonal cell on most seeds.
- **Cost.** A day. Wind direction reverses by season in tropical bands
  near a major coastline.
- **Origin.** Research brief in commit `44ec18f`.

### Ocean upwelling zones

- **Why deferred.** Current ocean model handles gyre warm/cool but not
  upwelling. Atacama-style coastal deserts (cold upwelling next to hot
  land) need this for the truly dry coastline pattern.
- **Trigger for revival.** Driest cells in Köppen come from this
  mechanism; expanding desert fraction toward Earth's 20% needs it.
- **Cost.** Half a day. Where wind blows alongshore + Ekman drives
  surface offshore, suppress SST and convection.
- **Origin.** Research brief in commit `44ec18f`.

---

## Biomes

### Wider biome palette

- **Why deferred.** Four Köppen classes (Cfa/Cfb/Dfa/Dfb) all collapse
  into TEMPERATE_FOREST in the 14-biome enum. Forest ends up
  over-represented (48% on seed 42) because of this collapse.
- **Trigger for revival.** Property test fails on biome distribution
  width. Or render needs to distinguish humid-subtropical (SE-US-style)
  from oceanic-temperate (NW-Europe-style) visually.
- **Cost.** A day. Add 4-6 new biome IDs (HUMID_SUBTROPICAL, OCEANIC,
  CONTINENTAL_COLD, etc.); update palettes; remap Köppen.
- **Origin.** Realism audit in commit `117eafe`.

### Soil-driven biome refinement — DONE (6.1.4, 2026-05-24)

- **Shipped with USDA soil orders (above):** waterlogged Histosol cells override
  to `biomes::WETLAND` (id 15). Conservative by design — the other orders mostly
  agree with Köppen, so only the genuinely-additive wetland case overrides. A
  richer substrate→biome coupling (e.g. Vertisol favouring grassland over forest
  in seasonal subtropics) remains a future option if it proves non-redundant.
- **Origin.** Research brief in commit `44ec18f`.

### Reach unused biome IDs from the Köppen path

- **Why deferred.** `mapgen-world::koppen::to_biome` has no `KoppenClass`
  mapping into TEMPERATE_GRASSLAND (id 4) or TROPICAL_DRY_FOREST (id 9),
  so those palette entries never render on a world built with seasonal
  climate. The Whittaker fallback covers both, but only fires when
  seasonal data is absent. Caught by
  `mapgen-render/tests/svg_invariants.rs::always_present_biome_colors_emitted_on_reference_world`,
  which was deliberately relaxed to assert only the 10
  always-Köppen-reachable colors rather than all 15. Distinct from
  "Wider biome palette" — that item adds *new* IDs (humid-subtropical,
  oceanic, etc.); this item routes existing-but-orphaned IDs.
- **Trigger for revival.** First ornate render where the steppe /
  tropical-dry-forest visual gap actually shows (today's renders are
  rare enough that no one's noticed). Or when "Wider biome palette" is
  picked up — fold this into that pass to avoid two passes over the
  Köppen map. Or someone wants the renderer test tightened to "all 15
  palette colors emit" without first widening the palette.
- **Cost.** Half a day. Candidate mapping: `BSk → TEMPERATE_GRASSLAND`
  (cold steppe IS prairie/grassland in real ecology); `Aw → TROPICAL_DRY_FOREST`
  when annual precip is high enough, else SAVANNA. Both need a
  property test that the new assignments don't displace the Earth-fit
  distribution `realism_spec` relies on.
- **Origin.** Phase 2.5 renderer test, this session.

---

## Cultures (Phase 3 work — partially deferred)

### Sound-change rules across language families

- **Why deferred.** MVP cultures stage uses a single phonotactic
  generator + Markov fallback. Tolkien-style cognate generation
  (Quenya/Sindarin sharing roots with divergent sound shifts) is later.
- **Trigger for revival.** Place names need to feel related across
  neighboring cultures, with divergent sound shifts indicating
  separation time.
- **Cost.** 2-3 days. Per language family: phonotactic + ordered
  rewrite rules (`p > f / _V`); fork per daughter; apply different
  rule sets.
- **Origin.** Cultures research brief; ARCHITECTURE.md Phase 3 backlog.

### Religions beyond minimum

- **Why deferred.** Phase 3 ships 1-3 religions per world tied to
  cultures with basic spread. Pantheon depth, schism modeling, holy-
  site detail are later.
- **Trigger for revival.** Lore engine wants to generate religious
  conflict narratives. Or render wants distinct shrine icons per
  pantheon pattern.
- **Cost.** 2-3 days. Full schema (pantheon pattern + tone + doctrine
  + sacred sites + antagonist religions + schism state).
- **Origin.** Cultures research brief.

### Artifacts beyond minimum

- **Why deferred.** Phase 3 ships LorePatch + Artifact + cause_event as
  types but populates only via simple cataclysm events.
- **Trigger for revival.** Lore engine writes chronicles citing
  artifacts; rendered map shows megalith icons.
- **Cost.** 1-2 days. Megalithic monuments, ruined cities, underground
  complex entrances, demon prisons — each as a LorePatch class with a
  spawn rule.
- **Origin.** Cultures research brief.

### All 14 race archetypes

- **Why deferred.** Phase 3 ships 4-5 archetypes (~one human, one elf,
  one dwarf, one orc, halfling). The other 9 land as data extensions.
- **Trigger for revival.** User wants a specific archetype that isn't
  in the MVP roster. Or world variety demands giants / lizardfolk /
  sea-folk for visual differentiation.
- **Cost.** Half a day per archetype (habitat curve + settlement icon
  + architecture + magic style).
- **Origin.** ARCHITECTURE.md §5.5; cultures research brief.

### Cataclysm clock (Sanderson-style Desolations)

- **Why deferred.** A *recurring, world-scale* catastrophe on a fixed
  mythic clock (Desolation every N centuries, with a build-up the
  chronicle anticipates) was never part of Phase 4. Phase 4 shipped all
  six causal loops — including the four originally deferred here
  (Khaldun, succession, schism, hero) at commits `52c709f`/`4c2348e`/
  `858e9d4`/`26df4a3` — plus *local, emergent* cataclysm-flavored events
  (`Megabeast`/`Plague`/`Famine`/`Drought`) and `HistoryData.ages`
  (fixed-window mythic ages). What's still missing is the *clock*: a
  scheduled, escalating, civilization-resetting Desolation that ages
  partition around rather than being framed by quartiles.
- **Trigger for revival.** Phase-4 loops are live and mature (✓); the
  lore engine (Phase 5) wants a recurring apocalyptic beat to narrate
  toward, or the user asks for Sanderson-style epoch resets.
- **Cost.** 2-3 days. A 7th loop (or a driver-level scheduler) that
  injects a periodic high-salience cataclysm, resets affected polity
  `SimState`, and re-anchors `ages` to the Desolation cadence.
- **Origin.** ARCHITECTURE.md §2 (deferred from MVP).

### Resources & economy (trade, commodities)

- **Gap (identified 2026-05-25 review).** The world has soils (6.1.4), biomes,
  rivers, and roads, but nothing *consumes* them economically: no resource
  deposits (ore / timber / fertile land / fisheries), no commodity flows, no
  trade networks beyond the bare road graph. Settlement placement already weighs
  habitat but not resource access; the history sim has no economic driver beyond
  the Turchin fiscal loop's abstract surplus. ARCHITECTURE §-level mentions
  "trade networks" as a concept but no concrete model exists.
- **Trigger for revival.** Wanting settlement/wealth distribution to *read* as
  resource-driven (a mining town in the mountains, a granary on river-valley
  Mollisols, a port trading hub); or the history sim wanting trade-war / blockade
  causes. Soils + Strahler rivers are the substrate it would build on.
- **Cost.** Multi-day. A resource layer (per-cell deposits from geology + soil +
  biome), settlement-economy scoring, and a trade-route pass over the road graph.
- **Origin.** Surfaced in the 2026-05-25 missing-features review.

---

## Render

### Toggleable map layers + data overlays — foundation DONE (2026-05-25)

- **Shipped.** The ornate render emits every component as a
  `<g class="layer-NAME">` group plus a `<style>` block keyed off root-`<svg>`
  classes: `off-NAME` hides a feature, `on-NAME` reveals a data overlay.
  Toggling is pure CSS on the root element — instant, no re-render, and it
  survives restyle / drill-in / year-scrub SVG swaps. Default rasterization is
  byte-stable (overlays hidden via a `display="none"` attribute resvg honours),
  so the visual-regression and refine goldens are unaffected. Frontend toggle
  logic lives in `web/src/layers.ts` (pure, unit-tested); the layer + preset
  *lists* are the single Rust source (`mapgen_render::layers`), serialized to a
  committed `web/src/layers.manifest.json` the frontend imports (a Rust test
  fails if it drifts — see Hardening below).
  Overlays shipped: **political territory** (per-cell nation tint) and
  **temperature**, **elevation/relief**, and **precipitation** scalar
  choropleths, each with a per-world-normalized gradient legend keyed off its
  own `on-NAME` class. These established the reusable **scalar-choropleth +
  legend** substrate in `ornate_antique.rs` (`fill_cells`, the `ramp` over a
  `Stops` table, `field_range`, `relief_color`, `render_overlay_legend` + the
  `#thermal`/`#hypso`/`#precip` gradient defs) that the remaining data overlays
  below can clone in well under ½ d each. Six presets ship: Antique, Political,
  Physical, Climate, Relief, Rainfall.
- **What this unlocks — data overlays** (each is the same `on-NAME` mechanism
  over a per-cell field the pipeline already computes):
  - **Climate**: temperature + precipitation choropleths — DONE (2026-05-25).
    Still open: Köppen-zone bands (categorical, already computed).
  - **Relief / hypsometric**: elevation tint + bathymetry — DONE (2026-05-25).
    Still open: drainage **basins** coloured by outlet (pairs with the deferred
    drainage-divide viz).
  - **Soil / fertility**: the USDA soil orders as an agronomic overlay → feeds
    a future population/agriculture layer.
  - **Cultural**: culture regions and (when religions land) faith spread —
    reuses the `cultures` style's per-cell assignment as an overlay tint.
  - **Economy** (needs the resources/economy item): resource deposits, trade-
    route intensity, population density heatmap.
  - **Tectonic / hazard**: plate boundaries, volcano/quake risk, wind &
    upwelling flow arrows.
- **What this unlocks — composite usages:**
  - **Layer presets / "lenses"**: one-click bundles — DONE (2026-05-25):
    Antique / Political / Physical / Climate / Relief / Rainfall. Open: a
    Cultural lens once a culture overlay lands.
  - **Thematic atlas export**: render one world under N preset sets → a multi-
    page world bible (this is exactly the `mapgen atlas` item below, now trivial
    to express — the presets *are* the page list).
  - **Animated political history**: time-slider × political overlay already
    reads cleanly; extend to a play/scrub GIF/film of territory shifting.
  - **Per-layer SVG export** (just rivers, just labels) for external compositing;
    **GM vs player** layer sets; **opacity sliders** and **legends** per overlay;
    **hover tooltips** (each layer is a hit-testable group).
- **Hardening pass — DONE (2026-05-25)** (from an engineering review of the
  above):
  - **Single source of truth.** Layer/preset lists no longer duplicated across
    Rust + TS; `mapgen_render::layers` is canonical, emits
    `web/src/layers.manifest.json` (which TS imports), and `web_manifest_is_in_sync`
    fails on drift. The `antique` preset is pinned to *exactly* the feature
    layers by a test, so a new feature can't silently vanish from the default.
  - **One colour source.** The `#thermal`/`#hypso`/`#precip` legend gradients are
    generated from the same `ramp` stop tables the tints sample, so legend and
    map can't diverge.
  - **Visual regression.** `every_preset_rasterizes_to_a_sane_image` rasterizes
    all six presets and asserts opaque / non-uniform / multi-coloured / on-band —
    catching a broken tint/ramp/prune that structure tests miss.
  - **Single-overlay invariant.** `toggleLayer` turns other overlays off when one
    is enabled, so tints don't stack and the one bottom-left legend never collides.
  - **Polish.** `fill_cells` takes `f32` opacity; `render_overlay_legend` args
    bundled into `LegendSpec` (no `#[allow(too_many_arguments)]`); bake's
    intentional string-transform coupling documented + guarded by the atlas test.
  - **Documented, not coded** (deliberate calls): overlays normalize per-world,
    so a refined sector's colour scale differs from the world's — a cross-scale-
    stable variant would thread an explicit range from the parent (product
    decision). Still-open debt: atlas font de-dup (~4.5 MB repeated TTFs);
    interactive panel wiring lacks tests (awaits the Playwright smoke); layer
    state isn't persisted to the permalink/reload.
- **Trigger for next slice.** Thematic atlas export shipped (see the `mapgen
  atlas` item below). Highest-leverage remaining: (a) a **soil or cultural**
  overlay (clone the choropleth substrate — adds a Cultural atlas plate too),
  or (b) wire the active layer state into the frontend's **PNG/SVG export** so a
  chosen lens can be downloaded (export currently emits the default state).
- **Origin.** "Implement overlays to toggle components on/off" (2026-05-25).

### Alternative render styles

- **Why deferred.** MVP ships only `ornate_antique` (and `greyscale` /
  `biomes` debug styles). `clean_modern`, `political`, `physical` come
  later.
- **Trigger for revival.** User wants to re-render a world in a
  different style for a different purpose (game manual vs novel
  illustration vs poster).
- **Cost.** A day per style.
- **Origin.** ARCHITECTURE.md §4 Phase 6.

### PDF export

- **Why deferred.** SVG covers most needs. PDF means going through a
  raster intermediate (`resvg`).
- **Trigger for revival.** User wants printable output.
- **Cost.** Hours after `resvg` is wired for PNG.
- **Origin.** ARCHITECTURE.md §2 (deferred from MVP).

### Hand-authored `docs/target_aesthetic.svg`

- **Why deferred.** ARCHITECTURE.md §Phase 3e recommends authoring a
  reference SVG on Day 1 to anchor tuning decisions for the
  generative renderer. We shipped the generative renderer first
  (parchment + coastline ripples + mountains + forests + roads +
  glyphs + typography + compass + cartouche + edge-burn) without
  the reference. Authoring it retroactively would be busywork unless
  we're starting a new variant.
- **Trigger for revival.** Starting work on an alternative ornate
  style (e.g., a "political map" or "physical map" variant) where
  having a reference SVG up front would prevent the same drift
  pattern. Or doing a major redesign of `ornate_antique` (e.g.,
  swapping to a watercolor aesthetic).
- **Cost.** ~2h of hand-drawing in Inkscape / Affinity / etc.
- **Origin.** ARCHITECTURE.md §Phase 3e "Day 1" recommendation;
  noted as never done in session-state.

---

## Infrastructure / tooling

### Save-game version migration

- **Why deferred.** Schema version field is set (currently **v13**); no
  migrations yet. Schema has bumped freely (v2→v13) on the accepted policy that
  old `.json.gz` worlds become un-loadable across a breaking change.
- **Trigger for revival.** First time we want to keep an old world
  across a breaking schema change.
- **Cost.** Half a day to add migration runner; cost-per-migration
  scales with schema delta.
- **Origin.** ARCHITECTURE.md §2 (deferred from MVP).

### Cross-platform byte-identical golden hashes (native ↔ wasm32) — DONE (6.2, 2026-05-24)

- **Shipped.** `crates/mapgen-wasm/tests/cross_platform.rs` runs the full
  pipeline compiled to wasm32 in **Node** (`wasm-pack test --node`, no headless
  Chrome needed) and asserts byte-identity with the native seed-42 golden (one
  shared golden file). Wired into CI (`build-web` job) and `just test-wasm`.
- **Caught real bugs on first run** (the contract had never actually executed
  cross-platform): a `usize`-width `gen_range` in `poisson` (mesh) and a std
  `.exp()` in `climate::band_precip` (climate) both diverged; `cultures` `.powf`
  and `patch` `.hypot` were latent. All routed through `fmath`; both goldens
  re-anchored. See `docs/tuning_log.md` § Cross-platform determinism.
- **Preventative.** The `fmath` purity guard now scans every crate's `src/`
  (was mapgen-history-only), so a future raw transcendental fails `just check`.
- **Origin.** ARCHITECTURE.md §3; planned for Phase 5, delivered in 6.2.

### CLI argument-parser regression tests

- **Why deferred.** `mapgen-cli` is covered end-to-end by
  `tests/roundtrip.rs`, `tests/sweep_roundtrip.rs`, and
  `tests/ornate_antique_stub.rs`, but the individual `generate` / `render`
  / `sweep` flag definitions (defaults, type bounds, mutually-exclusive
  combinations) have no unit-level coverage. A clap default change, a
  rename of `--cells`, or a silent removal of a knob from `--knob` would
  only surface when an end-to-end test happens to exercise the affected
  path.
- **Trigger for revival.** First time an arg-parser regression slips past
  the integration tests and reaches a user, *or* the CLI grows a
  fourth subcommand and the surface stops fitting in one head.
- **Cost.** Half a day. Add a `parse_cli` helper that returns the parsed
  `Cli` struct without running it, write table-driven cases per
  subcommand covering: default values, every flag explicitly set,
  malformed values, and `--help` rendering.
- **Origin.** Pre-Phase-3a hygiene audit, 2026-05-17.

### Repository documentation top-tier polish pass

- **Why deferred.** Current docs (`AGENTS.md`, `CONTRIBUTING.md`,
  `docs/SESSION.md`, `README.md`) are at "solid hobby project" or
  "internal team" bar — significantly better than the prior gitignored
  `.local/` files, but well short of top-tier OSS Rust repos (tokio,
  serde, ripgrep, clap). The gap is real but the marginal value at
  one-developer scale is near zero, and the maintenance overhead is
  real and recurring.
- **Trigger for revival.** Any of:
  (a) onboarding a second contributor;
  (b) public OSS launch / external user adoption;
  (c) doc rot has been observed (someone hit a documented gotcha that
      was no longer accurate, or followed instructions that no longer
      worked);
  (d) the project starts taking external contributions / issues.
- **Cost.** Tiered:
  - **Option A — Internal team polish (~3–4h).** Strip session-
    specific anecdotes (e.g. "two commits this session were
    stopped"), add TOCs to longer files, restructure SESSION
    gotchas as Symptom / Cause / Fix, add `_last reviewed_` dates,
    add a stale-SESSION CI check that fails when SESSION.md's
    latest-commit line doesn't match HEAD, generalize CONTRIBUTING's
    "substage shape" beyond pipeline stages, add PR/review process
    section, add code style guide beyond `fmt + clippy`.
  - **Option B — Top-tier OSS bar (~1–2 weeks).** Everything in A
    plus: `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CHANGELOG.md`
    (manual or git-cliff), `.github/ISSUE_TEMPLATE/`,
    `.github/PULL_REQUEST_TEMPLATE.md`, `.github/CODEOWNERS`,
    `.github/dependabot.yml`, `docs/adr/` (Architectural Decision
    Records for Refinery rejection, schema versioning, font
    vendoring, roughr adoption), `docs/GLOSSARY.md` (Köppen,
    riparian, Christaller, etc.), README badges (CI / license /
    MSRV / docs.rs), `cargo-deny` + `cargo-audit` + `lychee`
    Markdown link checking in CI, mdBook docs site, `examples/`
    directory, criterion benchmarks. Also: consider merging
    `AGENTS.md` into `CONTRIBUTING.md` since the two-root-level-docs
    split is unusual.
  - **Mixed pick.** Select individual items from Option B even
    pre-trigger if a specific one provides defensive value (e.g.,
    `cargo-audit` + `dependabot` for security hygiene even at
    single-developer scale).
- **Origin.** 2026-05-22 doc-quality review against "would a PE write
  this for a top-tier OSS repo?" Discussed in session-ending review;
  AGENTS.md / CONTRIBUTING.md / SESSION.md shipped as Option-3-minus
  on this date with this entry filed for the gap.

## Gaps identified in the 2026-05-25 review (previously untracked)

Surfaced while reviewing Phases 6–7. Each is a real gap not otherwise on this
list; promote when its trigger fires.

### Frontend test suite — Vitest + Playwright smoke DONE (2026-05-25)

- **Vitest (pure logic).** The bug-prone nav geometry lives in a pure
  `web/src/sector.ts` covered by `sector.test.ts`; the layer/preset/toggle logic
  in `layers.ts` by `layers.test.ts` (24 tests total). Runs in CI + `just web-test`.
  Vitest is scoped to `src/**/*.test.ts` (vite.config) so it ignores the e2e specs.
- **Playwright smoke DONE.** `web/e2e/smoke.spec.ts` drives a real browser through
  load → engine-ready → generate → map paints → apply the Climate lens (asserts
  the root `<svg>` gains `on-climate`) → narration enables — the worker + wasm +
  DOM glue the unit tests can't reach. `vite preview` serves the built bundle;
  wired into the CI build-web job (`npx playwright install --with-deps chromium`
  then `npm run e2e`) and `just web-e2e`. CI-only by nature (needs a browser);
  the dev sandbox is Ubuntu 26.04 which Playwright's chromium doesn't support, so
  it's validated locally via `playwright test --list` (spec + config compile,
  test discovered) and executes for real on CI's Ubuntu 24.04.
- **Still thin:** the smoke is one happy-path flow; drill-in/breadcrumb and
  error paths aren't covered yet — extend the spec as the UI grows.

### Refine-path cross-platform golden — DONE (2026-05-25)

- **Shipped.** A fixed refined sector (L2 (1,1), seed 42) is hashed natively
  (`scale_spec::refined_sector_golden_hash`) and under wasm32
  (`cross_platform::refined_sector_golden_matches_native_under_wasm`), both
  against a shared `golden/seed42_sector.blake3.txt`. Proves the whole refine
  path (sub-region mesh, projection, seam-pinning) is byte-identical across
  targets; passed first run (no fmath bypass in the new arithmetic). In CI.

### Per-component perf budgets — DONE (2026-05-25)

- **Shipped.** `examples/perf_baseline.rs` now budgets `render` (4k/15k) and
  `refine_sector` (a level-2 tile) alongside `generate_full`, same convention
  (median of 3 ≤ baseline × 1.5; `just perf` exits non-zero on a breach).
  Render landed at ~22/78 ms — nearly the cost of generation — and a sector at
  ~68 ms, the latency the navigation ADR flagged. Baselines anchored to this box
  and mirrored in `docs/perf_baseline.md`.
- **Still open:** the history sim isn't isolated (it's covered inside the
  `generate_full` budget — isolating a single pipeline stage wasn't worth the
  plumbing); `just perf` remains a manual/local gate, not in CI.

### Atlas / world-bible export (`mapgen atlas`) — DONE (2026-05-25)

- **Shipped.** `mapgen atlas --seed N [--cells --plates --nations --out]`
  generates the world once, renders the ornate base once, and bakes each of the
  six layer presets into one self-contained, parchment-themed HTML file — a
  multi-page "world bible" (Antique / Political / Physical / Climate / Relief /
  Rainfall), each page captioned, with `@media print` page-breaks so it prints
  to PDF cleanly (covering the PDF-export want without a resvg→PDF pipeline). No
  scripts, no external assets. The native `mapgen_render::layers` module is the
  canonical preset/layer source (mirrored by `web/src/layers.ts`) and supplies
  `bake_layer_state` (toggle `display`) + `bake_layer_state_pruned` (also drop
  the hidden groups via balanced `<g>` matching — ~67% smaller files: a 6-plate
  seed-42 atlas is ~7.6 MB at the 6 000-cell default). Tested in
  `mapgen-render` (bake/prune mechanics) + `mapgen-cli/tests/atlas.rs` (every
  declared layer is actually emitted; each preset bakes a self-contained,
  pruned page).
- **Open follow-ups.** Richer pages: embed the chronicle + an entity glossary +
  drilled-in key sectors (Phase 7 refine) as additional plates; per-page font
  de-dup (the embedded TTFs repeat across plates — a lean-artifact win); a
  Cultural plate once that overlay lands; wiring the active frontend lens into
  the in-browser PNG/SVG export (still emits the default state).

### Borders that move with history — ALREADY DONE (corrected 2026-05-25)

- **Not a gap — my 2026-05-25 review mis-reported this.** The Phase-4 war loop
  (`mearsheimer::transfer_border_cells`) *does* mutate `society.control`: a won
  war reassigns the loser's frontier cells to the winner, conserving the total,
  and dissolves a realm reduced to 0 cells. The rendered map shows present-day
  (post-conquest) borders. The mis-read came from grepping only `lib.rs` (which
  reads control) and missing the loop that writes it.
- **Now also pinned** by `tests/history_borders.rs` (end-to-end: 7–17% of
  controlled cells change owner across seeds; total conserved) — previously only
  the loop's panic-safety was unit-tested.

### Map as a point in time (history time-slider) — DONE (2026-05-25)

- **Shipped.** Schema v16 records a territorial timeline
  (`HistoryData::border_changes`); `WorldData::control_at_year` reconstructs the
  borders at any past year; `WorldHandle::renderAtYear` + a top-centre frontend
  slider scrub the world map across the conquest years (range from
  `historyYears`). World-scale only (history is world-wide; sectors have no
  timeline). Verified: seed 42 spans years 7–492, 258 cells differ founding→present.
- **Caveat / follow-up.** The shift is *subtle* in the ornate style — it shows
  only in the thin dashed polity borders + realm labels. A faint **political
  territory tint** (wash each realm's cells in its colour) would make the slider
  dramatic and improve the static political read too; deferred because it changes
  the locked level-0 ornate aesthetic (needs sign-off + a visual_regression
  re-anchor). Settlements/names over time also remain static (founded at gen).

### Structured tracing + narration eval harness

- **Gap (two small).** (1) No `tracing` spans in the library crates — debugging
  on-demand sector gen / the serve sidecar is `eprintln`-only. (2) No offline
  narration-quality harness (NER pass-rate / retry-rate across seeds) to tune the
  stoplist against data. **Trigger.** Debugging pain / tuning the lore engine.
  **Cost.** ~½ d each.

---

## How items move

- **Backlog → Active plan.** Edit `ARCHITECTURE.md` to add the item to
  the appropriate section; delete from this file. Note in commit
  message which trigger fired.
- **Active plan → Backlog.** When deferring something from the active
  plan, add an entry here citing the deferral reason. Don't just delete.
- **Backlog → Cut.** When an item is unambiguously not worth doing
  even speculatively, delete it. Note in commit message why.

The point of this file is to make the cost of "no, not yet" visible and
the path back into scope explicit. If an item lacks a "trigger for
revival," it shouldn't be on the backlog — it should be cut.

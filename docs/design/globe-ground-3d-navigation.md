# Globe → Ground Continuous-3D Navigation — Locked Design

**Status:** LOCKED for implementation. Build increment by increment in the stated order.

**Spine:** Camera/controls retarget first (1a), partial-sphere patch as a planned fast-follow (1b). This is a deliberate change from the pre-red-team draft, which bundled retarget + patch into one increment — see [What the red-team changed](#what-the-red-team-changed).

**The defect, exactly:** `web/src/main.ts` `navTo` lines 678-689 — in globe mode, drilling calls `exitGlobeView()` (hides `#globe-canvas`) + `requestRefine()` → a flat 2D SVG sector in `#map` with panzoom. That is the "old 2D rendering logic" the user sees. The drill *loop* above it (`globe.onPick` → `continentAt` → `continentInfo` handler at main.ts 488 → `navTo`) is correct; only the terminal consumer drops 3D.

### File-path corrections folded in (verified this pass)

The pre-red-team draft cited `planet.rs` / `mapgen-core`. Verified actual locations:

- `render_globe_texture` lives at **`crates/mapgen-render/src/style/planet.rs:117`**, dispatched from **`crates/mapgen-render/src/lib.rs:47`** (`Style::GlobeTexture => render_globe_texture(world)`). `Style::from_str("globe" | "globe_texture") => GlobeTexture` is at `crates/mapgen-render/src/style/mod.rs:90`.
- `MeshData::view_rect()` (`crates/mapgen-core/src/world_data.rs:309`) returns **`[x0, y0, x1, y1]` corners**, not `[x,y,w,h]`. `render_globe_texture` derives `(w,h) = (vx1-vx, vy1-vy)` and writes `viewBox="{vx} {vy} {w} {h}"` with **nonzero vx,vy and a sector-true (non-2:1) aspect** (planet.rs:119-125). This is load-bearing for the patch UV-aspect fix in 1b.
- `wasm_bindgen` export is `refineSector` → Rust `refine_sector` (`crates/mapgen-wasm/src/lib.rs:55`). Worker calls `root.refineSector(level,sx,sy,SECTOR_CELLS)` at `web/src/worker.ts:133`.

---

## Target experience

- **Globe (level 0):** unchanged. The parchment paper-globe sphere spins and zooms (OrbitControls, target origin), click to drill.
- **Continent/region (drill once):** the existing 600ms fly-to dives the camera toward the clicked region; **the canvas stays shown** and dragging now **orbits that region** (the camera arcs around the drilled point; the planet does not roll under you). Scroll dollies within local limits.
  - *After 1a:* the region is shown on the existing whole-world sphere texture (coarse — a zoomed slice of the 2048×1024 skin). This is the make-or-break "stay 3D, move freely" fix, isolated.
  - *After 1b:* a curved high-detail **segment** of the planet replaces the coarse slice — the horizon visibly bows, the rest of the globe still curves away at the edges, and drilling reveals *more cartography* (finer coast/rivers/wash from the refined sector mesh).
- **Deeper (drill again, 1c):** clicking a feature flies further down; the curved segment is replaced by a smaller, finer one to `MAX_LEVEL=6`. Curvature flattens as you near the ground (geographically correct) — see the named [curvature-at-depth cost](#named-costs-accepted-and-foregrounded).
- **Up (breadcrumb, 1d):** the camera pulls back and the parent segment — or the whole spinning globe — reassembles, no regenerate.

At every scale it is the SAME 3D scene; never a page-swap to a flat picture.

---

## Scene model

ONE persistent `Scene` + `PerspectiveCamera` + `WebGLRenderer` + `OrbitControls` + `Raycaster` (the existing `globe.ts` singletons at lines 173-199 — context reuse is what headless tests depend on; never re-created).

A new top-level **`pivot` Group** wraps the scene's drillable content. Pivoting the whole group (not just a patch) is what makes the +Y polar clamp correct for any sector — see [Controls](#controls-re-targeting-the-make-or-break).

- **`sphere`** (persistent): the existing `Mesh(SphereGeometry(1,64,48), MeshBasicMaterial)` (globe.ts 184-187), textured with the whole-world equirectangular `globe` render. **Always present and visible** — zooming back out always reveals the rest of the planet for context.
- **`patch`** (1b onward; null at root and after 1a): built on drill as `new SphereGeometry(1.001, segW, segH, phiStart, phiLength, thetaStart, thetaLength)` — a partial sphere occupying exactly the drilled sector's lon/lat span on the same unit sphere. **Drawn unconditionally over its base region** via `material.depthTest=false` + `renderOrder=1` (this removes the SwiftShader depth-precision dependency entirely — there is nothing left to z-fight; see [Risk register](#risk-register)). `MeshBasicMaterial` mapped to the rasterized refined-sector `globe` SVG.

**Hard line against the killed Refinery:** AT MOST ONE patch mesh exists at a time (plus the base sphere). Drilling deeper disposes the old patch and builds one smaller patch. No quadtree of live meshes, no streaming, no per-frame LOD selection, no prefetch, no tile pyramid, no fractional zoom. State is just `nav: Sector`; the patch is a pure function of `nav`.

---

## Drill flow (per level)

**Level 0 → N (the core fix):** unchanged through `globe.onPick(u,v)` (globe.ts 281 `onArrive`→`pickCb`) → `uvToWorld` → `continentAt` → `continentInfo` handler (main.ts 488) → `navTo(target)`. **Note: a single root click lands at `continentDrillLevel(cell_count,total_cells,MAX_LEVEL)` depth, which is pinned at 3 and 6 in `web/src/sector.test.ts` — NOT necessarily level 1.** The CHANGE is in `navTo`'s globe branch (main.ts 678-690):

- It NO LONGER calls `exitGlobeView()` on drill. The canvas stays shown.
- `requestRefine()` (1b+) runs but **forces `style:"globe"` when `globeScale`** (the [blocking texture graft](#texture--elevation-strategy)).
- After 1a, drill retargets controls on the base sphere. After 1b, the `"refined"` handler (main.ts 473) routes to `globe.showPatch(...)`.

**Transition reorder (UX major, fixed):** the root hop's target is the continent **centroid** (`cx,cy`), which arrives *async* from `continentInfo` and is NOT the clicked point. The pre-red-team "single coherent motion" claim was false — the camera would fly to the click, then jump to the centroid. **Fix:** for the root→drill hop, resolve `continentAt` and compute the landing sector's `centerDir` BEFORE committing the fly's target, so the camera flies directly to the centroid with the pivot quaternion lerping toward +Y in one motion. Deeper-drill and up-nav use `childSectorAt`/`ancestors` synchronously and are already coherent — the reorder is only for the root hop.

**Level N → N+1 (deeper, 1c):** a click on the `patch` mesh (raycast targets the active patch when one exists) → `hit.uv` (three.js intrinsic 0..1 across the partial sphere) → pure `patchLocalUvToWorld(localU, localV, nav, worldW, worldH)` → absolute world (x,y) inside `sectorRect(nav)` → `childSectorAt(...)` → `navTo(child)` → refine (`style:"globe"`) → `showPatch` rebuilds the smaller patch.

**Two code seams this closes (verified):**
- **navTo patch-to-patch fall-through (lines 692-708):** the globe branch only handles `nav.level===0 || target.level===0`; an L1→L2 nav falls through to the 2D `panzoom.focusContentRect` path. Increment 1c MUST extend the globe branch to handle `nav.level>0 && target.level>0` (swap the patch) — otherwise a deeper drill reverts to 2D.
- **onPick guard:** the root `onPick` fires from the fly-to's `onArrive` and `continentInfo` is gated on `nav.level !== 0` (main.ts 492). Deeper drilling needs a patch-raycast `onPatchPick` callback that fires at level≥1.

Capped at `MAX_LEVEL=6` exactly as today.

---

## Controls re-targeting (the make-or-break)

This is the literal "move around freely" fix and has a real gotcha: **OrbitControls measures `maxPolarAngle` from world +Y, not from the patch's local surface normal.** If we leave the drilled region at its true lon/lat and only set `controls.target` to its surface point, a `maxPolarAngle` clamp does NOT mean "stay above the local horizon" — for an **equatorial** sector the camera can orbit straight down through the planet (the user's exact complaint, rotated off the pole). Capping `maxPolarAngle` alone is therefore insufficient; the pivot is the only correct treatment.

**The treatment (pivot the whole scene):**
1. Rotate the `pivot` Group so the drilled region's CENTER direction lands on world **+Y** (local-up == world +Y). The base sphere is inside the same pivot, so the surrounding globe stays aligned and visible.
2. `controls.target = pivotedCenterWorldPos` (≈ `(0,1,0)`).
3. `controls.maxPolarAngle ≈ 75°` — now genuinely means "stay above the local horizon" for ANY sector, equatorial included.
4. Rescale `controls.minDistance/maxDistance` to the region's angular size for bounded local orbit+zoom.
5. `enablePan` stays **false**. "Move freely" = orbit around the region center + zoom within it.

**Pivot ↔ fly-to reconciliation:** the current fly-to does `camera.lookAt(0,0,0)` the whole way (globe.ts 216). The single coherent motion lerps the `pivot` group's quaternion (identity → pivotRot) while lerping the camera to a framing position above +Y, with `lookAt`/target tracking the region center (≈`(0,1,0)`); on arrival hand off to OrbitControls with the +Y-based clamp and tightened distances.

On return to level 0: pivot resets to identity, `controls.target → origin`, globe distances + `maxPolarAngle` restored. **The minimal `returnToGlobe` teardown ships in 1a** (not deferred to a later increment) so no intermediate ship leaves a stale pivot/target — see [What the red-team changed](#what-the-red-team-changed).

**Verification is mandatory on an EQUATORIAL sector** (worst case for the +Y clamp), not a polar one, and the test asserts a GEOMETRIC invariant, not an attribute's presence — see [Recommended first increment](#recommended-first-increment).

**Pan-across-the-surface to a neighbour sector without re-drilling is DEFERRED** (a distinct interaction; risks "move freely" silently expanding into a streaming engine).

---

## Texture & elevation strategy

**Texture (1b+) — reuse the exact existing pipeline:** the worker's `refineSector → render("globe") → refined` SVG (worker.ts 123-143), rasterized by `rasterizeSvg` (main.ts 839) into a `CanvasTexture` mapped onto the partial sphere (whose intrinsic UVs run 0..1 across the sector).

**BLOCKING graft — force `style:"globe"` at EVERY level in globe mode.** `requestRefine()` (line 668) sends `effectiveStyle()` → `navStyle()` (sector.ts 97, no globe branch) → returns the user's **labelled** style at level≥1, which rasterizes unreliably as an `<img>` (the documented reason the root globe is fontless). The one-line fix: send `"globe"` when `globeScale`.

**Why drilling reveals MORE cartography (verified):** `Style::GlobeTexture` → `render_globe_texture` (planet.rs 117) uses `Proj::equirect(view_rect())` — fontless, no text, coast + major rivers + ranges + political wash. Called on a **refined sector handle**, `view_rect()` returns the sector's lon/lat **sub-rect** (world_data.rs 309-311), so the SVG viewBox is the sector's lon/lat box → patch UVs align 1:1, AND `refineSector(...,4000 cells)` produces a finer mesh → finer coast/rivers/wash. Lock `"globe"` for the whole arc; a dedicated fontless detailed style is **out** (Open Q2 dismissed — `view_rect()` already gives finer cartography).

**UV-aspect fix (1b, UX minor — verified, fold in):** `render_globe_texture` writes `viewBox="{vx} {vy} {w} {h}"` with `(w,h)=(vx1-vx,vy1-vy)` — a sector's true (non-2:1) aspect. The existing `textureGlobe` hardcodes `rasterizeSvg(svg, 2048, 1024)` (main.ts 143), which is correct only for the 2:1 whole world. **`setPatchTexture` MUST rasterize at the sector's aspect** (e.g. a fixed long-edge × the `w/h` ratio from the SVG viewBox), or the cartography is anisotropically squashed on the patch. Pin `segW/segH` and the texture aspect in Vitest against `sectorPatchParams`' `phiLength/thetaLength` ratio so a mismatch goes red off-GPU.

**No-fade `setPatchTexture` (1b):** the existing `globe.setTexture` always runs `fadeMapEdges` (globe.ts 303) — whole-world antimeridian/pole logic that would paint sea bands into a continent interior. The patch needs a separate `setPatchTexture` that skips `fadeMapEdges` and uses `ClampToEdgeWrapping` both axes (a patch is not periodic).

**Counter contract (1b — SCOPE major, fixed):** `setTexture` bumps a single closure-scoped `textureCount` → `data-textures`/`data-textured` (globe.ts 201, 314-315). `setPatchTexture` MUST bump a **dedicated `data-patch-textures`** counter, set ONLY by `setPatchTexture`. Pinned in 1b (the increment that introduces the function), consumed by the 1e lens test. A shared counter would let the lens test false-green on any incidental base re-texture.

**Elevation: NONE in the core arc.** 3D comes from genuine sphere curvature (1b) + the re-targeted orbital camera (1a) + the visible surrounding globe. Relief is the single deferred increment below. The patch geometry is built segment-rich (segW/segH scaled to angular size, pinned in Vitest **justified by curvature legibility alone**, not "for relief").

### Named costs (accepted and foregrounded)

- **1a is a detail regression vs today's 2D drilled sector** (a coarse, zoomed slice of the whole-world skin) traded for the 3D / move-freely gain the user explicitly prioritized. That coarseness is the signal that justifies building 1b. Self-consistent: ship 1a, confirm the complaint is resolved, then restore detail with 1b.
- **The patch is fontless → TOTAL label absence (UX major, reframed honestly):** `render_globe_texture` carries ZERO place/continent labels. The 2D sector it replaces renders `.continent-label` (asserted live at smoke.spec.ts 175). This is a TOTAL wayfinding loss on the patch, not "softer text." Deferred: crisp on-patch labels (HTML billboards) — revive when label loss is the top complaint. Decision for v1: **accept unlabeled patches** (Open Q3), with the top-level region name as the first candidate if a billboard increment is pulled forward.
- **Curvature vanishes at depth (UX major, named not hidden):** a level-6 sector spans ~5.6° of longitude; its sagitta on the unit sphere is ~0.001 — geometrically a flat plane. At deep zoom the patch is a near-flat curved segment you orbit slightly. **We do NOT depth-cap the drill** (that fights "move freely / drill all the way down"). Instead the relief increment's revival gate is made concrete: *if deep near-flat patches empirically read as flat 2D, ship relief* (increment G below). The core arc is honestly "stays 3D and orbitable at every level; the 3D *cue* weakens toward the ground, with relief as the named remedy."

---

## WASM surface & determinism impact

**Core arc (1a-1e): ZERO wasm changes. The golden does NOT move.** No new Rust function, no `#[wasm_bindgen]` export, no change to `lib.rs` / `worker.ts`'s refine branch / the `WorkerResponse` shape. All new code is TypeScript scene/geometry/texture math + pure helpers in `sector.ts`. The cross-platform blake3-over-ciborium-`WorldData` golden (`crates/mapgen-wasm/tests/cross_platform.rs`) is untouched: no RNG drawn, no `WorldData` field added, no mutation; the render SVG is never hash-checked.

**Deeper 3D drilling reuses the golden-pinned refine verbatim (determinism minor, fold in):** the L1→L2→L3 path still calls `root.refineSector(level,sx,sy,SECTOR_CELLS)` (worker.ts 133) — the same call pinned by `cross_platform.rs`'s refined-sector golden. The 3D arc adds a new SVG **consumer**, not a new or modified refine. The safety comes from not touching it; a future edit to the deeper-drill path is therefore golden-relevant and must re-run that golden.

**Deferred relief increment (2): ONE read-only getter — and the "golden stays green" proof is REMOVED as vacuous (determinism major, fixed).** The pre-red-team draft claimed re-running the `WorldData` golden GREEN proves `sectorHeightfield` is golden-neutral. That is vacuous: the getter is never on the hashed path, so the golden is green no matter how broken it is. The real guarantee is **structural**:

- `WorldHandle::sectorHeightfield(grid_w, grid_h) -> Vec<f32>` is `&self`, allocates a fresh `Vec`, draws **no RNG**, and adds **NO `#[derive(Serialize)]` field** to `WorldData`/`MeshData`/`TerrainData`. Because no serde field is added, the ciborium byte stream is identical → the `WorldData` golden is genuinely neutral **for that structural reason**, not because re-running it "passes."
- **Proof that replaces the vacuous one:** (a) a serde round-trip / serialized-byte-equality assertion on a `WorldData` before vs after a getter call (must be byte-identical); (b) a code-level statement that the getter adds no serialized field.
- **Native↔wasm coverage for the getter's OWN output (determinism minor, fixed):** since the output is never hashed, the existing harness gives it zero coverage. Add a `wasm_bindgen_test` that calls `sectorHeightfield` on the seed-42 sector and asserts the returned `Vec<f32>` hashes identical to a committed **native golden of the same call** — a cheap dedicated getter golden. Spec `sample_grid` as pure index/comparison nearest-site selection (**no transcendentals**); if a distance is unavoidable, route it through `mapgen_core::fmath` and say so.

Verified fields exist and are public: `MeshData.sites: Vec<[f32;2]>`, `MeshData::view_rect()`, `TerrainData.elevation: Vec<f32>` (world_data.rs).

---

## Deeper levels, breadcrumb/up, lenses, time-slider

- **Deeper levels (1c):** patch raycast → `patchLocalUvToWorld` → `childSectorAt` → `showPatch`. Capped at `MAX_LEVEL=6`.
- **Breadcrumb/up (1d):** the existing `updateBreadcrumb` / `ancestors` / `crumbLabel` machinery is reused verbatim (main.ts 626-654), with the "Globe" root override (line 637). `navTo`'s globe branch changes only its terminal step: `target.level===0` → `globe.returnToGlobe()` (reset pivot→identity, target→origin, restore globe clamps, dispose patch; base sphere already present; NO regenerate); `0 < target.level < nav.level` → refine + `showPatch` rebuilds the parent patch. Canvas stays visible throughout.
- **Lenses on the patch (1e — close the dead path):** `setLayerState` (main.ts 893-898) only re-textures when `globeScale && nav.level===0`; at level≥1 in patch mode it falls through to `applyLayers(contentEl.querySelector("svg"))` — but there is no live SVG in `contentEl`, so a lens toggle is a **DEAD no-op**. Fix: cache the active patch's sector SVG (mirroring `lastGlobeSvg`), and on toggle re-rasterize it with `withLayerClasses` injected and call `setPatchTexture`. **Until 1e lands, disable the lens panel (or show a "lenses apply at the globe root" hint) when `globeScale && nav.level>0`** so a dead toggle is never presented (UX minor, fold in). If 1e is descoped → recorded as KNOWN GAP, never listed as "preserved."
- **Time-slider:** PRESERVED automatically. `refreshTimeslider` (main.ts 766-779) gates on `nav.level===0`, so it shows only at the globe root (where per-year sphere re-texture already works) and hides at every patch level. Named explicitly as intended (sectors have no timeline — matches the old 2D sector), not a silent loss.

---

## Transitions

The existing 600ms `flyTo` easeInOut lerp (globe.ts 207-222) carries every scale change, extended to lerp the pivot quaternion + `controls.target` alongside `camera.position`. **Coarse-first is free:** the base sphere's own texture already shows that region under where the patch lands, so during the ~100ms refine + rasterize there is never a blank gap. Feel: **continuous camera, discrete geometry** — matching ADR 0001's "snap to nearest generated level, animate the camera between," now in 3D. The root-hop target is resolved before the fly commits (see [Drill flow](#drill-flow-per-level)) so there is no fly-then-jump-to-centroid jank.

---

## Increment sequence

> **Structural call:** the complaint has two halves — *stay-3D* and *move-freely* — and BOTH are resolved by the camera/controls retarget on the EXISTING textured sphere, with zero new geometry. So **1a is the camera retarget alone** (the minimal, sufficient fix, isolating the single make-or-break property — the equatorial +Y clamp). **1b is the partial-sphere patch** (restores the per-drill detail today's 2D sector already shows). 1b is a **planned fast-follow, not hard-gated** — it is regression-recovery of existing detail, a different class from relief (which is net-new). Only relief is hard-gated on evidence.

### Increment 1a — Drill stays on the globe, orbitable region (THE core fix) — RECOMMENDED FIRST

> **SUPERSEDED to the FREE-FLY variant (2026-06-08, user chose continuous-LOD streaming).**
> 1a ships as the free-fly globe-flight camera (fixed globe, moving camera, pivot
> identity), NOT the pivot-orbit-one-region model the prose below describes.
> OrbitControls stay at level 0; the globe-flight camera is the drilled-mode camera
> only (per advisor — camera STATE is controller-independent). Built in steps: (1)
> drill stays on the globe (no 2D handoff, no refine) — the reported-bug fix; (2)
> the free-fly camera with pan+zoom wired (pitch/heading math pure but unwired).
> The `getCameraState()` streaming hook is DEFERRED to ST-1, where its consumer
> (the LOD selector) lands — shipping it in 1a would be an unconsumed hook (the
> consumer-backed-substrate rule). See "Foundation preconditions" in the streaming
> addendum for the eventual contract.

- **Scope:** Pure `sectorPatchParams(sector, worldW, worldH) → {phiStart, phiLength, thetaStart, thetaLength, centerDir}` in `sector.ts` (centerDir is the unit vector to the sector center; used to compute the pivot rotation). In `globe.ts`: a `pivot` Group wrapping the sphere; `globe.focusRegion(centerDir, angularSize)` that rotates the pivot so `centerDir → +Y`, sets `controls.target`, clamps `maxPolarAngle≈75°`, rescales `minDistance/maxDistance`; sets `data-region="1"` and writes a per-frame `data-cam-above` = sign of `dot(camera.position − target, centerDirWorld)`; and `globe.returnToGlobe()` (pivot→identity, target→origin, restore clamps). Extend the fly-to to lerp the pivot quaternion + target. In `main.ts`: `navTo` globe branch stops `exitGlobeView()` on drill, retargets via `focusRegion`; the root hop resolves `continentAt` before committing the fly's target; the `target.level===0` branch calls `returnToGlobe()`. **NO patch, NO refine, NO new texture** — the region is shown on the existing whole-world sphere skin.
- **Drill-initiation gate (a NEW rough edge 1a introduces — verified):** 1a is the FIRST increment where the canvas stays live at level≥1, so a second click now reaches `globe.ts`'s pointerup → origin-centric `flyTo` (`lookAt(0,0,0)`) → `onArrive` → `pickCb` → `continentInfo` which **bails** at `nav.level !== 0` (main.ts 492). That strands the camera (flew under origin-centric math) with a stale `controls.target` ≈ (0,1,0) and a conflicting `lookAt(0,0,0)` that snaps when controls re-enable — a visible break inside 1a's own deliverable. **Fix (one line):** a `pickable` flag `main.ts` sets false at `nav.level≥1` (re-enabled in 1c when `onPatchPick` is wired); `globe.ts`'s pointerup checks it before committing a `flyTo`. In 1a a click while drilled is INERT (deeper drill lands in 1c); the canvas still freely orbits the region.
- **Deliverable:** Clicking the globe to drill keeps the 3D canvas visible, retargets the camera so dragging **orbits the drilled region** (not the planet core), on an equatorial sector as well as a polar one; the Globe breadcrumb returns cleanly to a freely-rotating whole globe.
- **TDD spec — geometric, not attribute-presence (the load-bearing fix):**
  - **Vitest (primary, off-GPU, deterministic, false-green-proof floor):** (1) `sectorPatchParams(ROOT)` → full sphere ranges; a known level-2 sector → its exact lon/lat sub-ranges; `centerDir` cross-checked against the xyz of `uvToWorld(sector center)` (catches a flipped 1−v/north convention, mirroring sector.test.ts corner anchors). (2) The pivot quaternion built from an **equatorial** sector's `centerDir` sends that `centerDir` to ≈`(0,1,0)` within epsilon (Vector3 comparison, not a written attribute). (3) The analytic invariant: with target on +Y and polar ≤ 75°, the camera locus stays above the region's tangent plane.
  - **E2e (integration, equatorial sector):** drill, drive OrbitControls to the polar limit, and assert the per-frame `data-cam-above` attribute (derived from real `camera.position`, written each frame — NOT on code-path entry) stays `"1"` (camera above the local horizon). Plus: `#globe-canvas` visible AND `#map-content` hidden AND breadcrumb matches `/L[1-6]/` (NOT a literal `"1"` — a single click lands at `continentDrillLevel` depth 3-6). **Drill-gate assertion:** while drilled, a second click does NOT change the breadcrumb level and does NOT strand the camera (canvas still visible, `data-region` still `"1"`, breadcrumb unchanged) — proves the inert-click gate, since without it the click fires a `flyTo` that bails and snaps. **Return assertion strengthened:** after clicking "Globe", `data-region` is ABSENT AND a subsequent drag rotates the WHOLE globe (assert `controls.target` back at origin / pivot identity via `page.evaluate`) — the pre-red-team 391-394 assertions false-green over a stale-state globe.
- **Est:** 1.5 days
- **Risk:** The world-+Y polar clamp on an EQUATORIAL sector — covered by the geometric Vitest + e2e above (the whole point of isolating 1a).

### Increment 1b — Curved high-detail patch (restores per-drill cartography)

- **Scope:** Pure `sectorPatchParams` already gives the partial-sphere params. In `globe.ts`: `showPatch(rasterCanvas, params)` — build the partial `SphereGeometry(1.001,...)` inside the pivot with `depthTest=false` + `renderOrder=1`, no-fade `setPatchTexture` (skips `fadeMapEdges`, `ClampToEdgeWrapping`, bumps a dedicated `data-patch-textures`), texture rasterized at the **sector's aspect** (not 2048×1024), set `data-patch=<level>`. `returnToGlobe`/intermediate-up dispose the patch. In `main.ts`: `navTo` globe branch runs `requestRefine()` forcing `style:"globe"`; the `"refined"` handler routes to `showPatch` when `globeScale && nav.level>0`.
- **Deliverable:** The drilled region resolves into a curved, finer-cartography segment over the still-visible globe (horizon bows; coast/rivers/wash sharper than the overview).
- **TDD spec:** Vitest — segW/segH and the texture aspect pinned against `sectorPatchParams`' `phiLength/thetaLength` ratio (catches the UV-squash off-GPU); segment count justified by curvature legibility. E2e — after drill, `data-patch` is a digit in `[1-6]` AND `data-patch-textures>=1` AND `#globe-canvas` visible AND `#map-content` hidden.
- **Est:** 2 days
- **Risk:** Patch overlay correctness under SwiftShader is **NOT data-* gate-coverable** (per the pixel-free test rule) — required as a named SwiftShader visual artifact (per the verify-before-commit memory). `depthTest=false` + renderOrder removes the depth-precision dependence, so the artifact is checking texture/aspect, not z-fighting.

### Increment 1c — Deeper drilling in 3D (L1→2→3)

- **Scope:** Pure `patchLocalUvToWorld(localU, localV, sector, worldW, worldH)` in `sector.ts`. In `globe.ts` add a patch raycast + `onPatchPick` callback firing at level≥1. In `main.ts` wire `onPatchPick` → `patchLocalUvToWorld` → `childSectorAt` → `navTo(child)`; **close the navTo fall-through** so `nav.level>0 && target.level>0` swaps the patch instead of reverting to 2D.
- **Deliverable:** From a continent patch, clicking a feature flies further down to a smaller, finer segment — 3D persists to MAX_LEVEL.
- **TDD spec:** Vitest — `patchLocalUvToWorld(0.5,0.5,...)` → sector world center; corners → sector-rect corners; result through `childSectorAt` → a level-2 child INSIDE the parent (reuses sector.test.ts containment). E2e — a second click bumps `data-patch` 1→2 AND keeps `#globe-canvas` visible AND `#map-content` hidden (the unclosed fall-through would reveal `#map-content`).
- **Est:** 2 days
- **Risk:** `hit.uv` on a partial SphereGeometry — pin the local-UV→world mapping in Vitest so a three.js UV-convention surprise is caught off-GPU.

### Increment 1d — Up-navigation + return in 3D (intermediate crumbs)

- **Scope:** `returnToGlobe()` already lands in 1a; this adds the camera-pullback animation and the intermediate-crumb (L2→L1) **patch rebuild** path: `0 < target.level < nav.level` → refine + `showPatch` rebuilds the parent patch.
- **Deliverable:** The breadcrumb navigates UP in 3D — to a parent patch or all the way back to the spinning globe — no regenerate, no flat-SVG flash.
- **TDD spec:** E2e — after drilling to L2, clicking an intermediate L1 crumb keeps the canvas visible, breadcrumb returns to `/L1/`, `#map-content` stays hidden the whole time, and status never returns to "Generating" (proves no regenerate).
- **Est:** 1 day
- **Risk:** Pivot/controls must fully reset on full return (already asserted in 1a's strengthened return test).

### Increment 1e — Lenses on the patch (close the dead path)

- **Scope:** In `setLayerState`: when `globeScale && nav.level>0`, re-rasterize the cached patch sector SVG with `withLayerClasses` injected and call `setPatchTexture`. Cache the patch's last SVG (mirror `lastGlobeSvg`). Remove the 1b interim "lenses apply at the root" disable.
- **Deliverable:** Faith/Prosperity/Trade lens toggles re-skin the curved patch.
- **TDD spec:** E2e mirroring the globe-root lens test (smoke.spec.ts ~461): drill to a patch, toggle the Prosperity lens (always has data), assert `data-patch-textures` increments while `data-patch>0` and the canvas stays visible — proving the patch re-textured (the **dedicated** counter, not shared `data-textures`, so the assertion is the feature's own signal).
- **Est:** 1 day
- **Risk:** None structural. If descoped, recorded as KNOWN GAP.

### Increment 1f — Polish (named, optional)

- **Scope:** Distance/`maxPolarAngle` feel-tuning (logged in `tuning_log.md`); optional patch-opacity cross-fade on swap. Confirm time-slider gating across the drill.
- **Deliverable:** The arc feels continuous globe→ground.
- **TDD spec:** No new feature signal (feel-tuning) — visual artifact per the verify-before-commit memory; assert time-slider hidden at patch level + visible after the Globe crumb.
- **Est:** 1 day
- **Risk:** Feel-sensitive; visual artifact required.

### Increment G (DEFERRED, evidence-gated) — Relief displacement at deep zoom

- **Revival trigger (concrete):** ship ONLY if user testing shows deep near-flat patches read as flat 2D (the curvature-at-depth cost manifesting), where curvature provably cannot carry the 3D cue.
- **Scope (consumer-backed in ONE increment — per MEMORY):** ship the read-only `WorldHandle::sectorHeightfield(gw,gh)` getter **AND** its displaced-patch consumer together (never plumbing-alone). Sampler is a pure native fn `sample_grid(mesh, elevation, gw, gh)` over `view_rect()`, **no transcendentals** (pure nearest-site index/comparison; if a distance is needed, route `mapgen_core::fmath`), grid capped ~128² and/or sites spatially bucketed. Build the patch from a denser grid; displace vertices radially by normalized per-cell elevation × `VERT_EXAG` (sea clamped flat).
- **TDD spec:** Native Rust unit on `sample_grid`: a 2-cell mesh (left +1, right −1) → grid ~+1 left half, ~−1 right half (a signal only correct nearest-site sampling produces) PLUS a mutation check (flip the comparator to argmax → directional test fails). **Native↔wasm:** a `wasm_bindgen_test` asserts `sectorHeightfield` on the seed-42 sector hashes identical to a committed native getter golden. **Golden-neutrality:** a serde byte-equality assertion on `WorldData` before vs after the getter call (NOT "re-run the WorldData golden green" — that proof is vacuous and removed). JS: the displaced patch has non-constant vertex radii.
- **Est:** 3 days
- **Risk:** A second geometry model; off the critical path; `VERT_EXAG` has no ground truth (tuning_log.md); coastline aliasing between relief geometry and SVG skin. Ship only on evidence.

---

## Risk register

| Risk | Severity | Where it lives | Mitigation / verification |
|---|---|---|---|
| Equatorial +Y polar clamp lets the camera orbit through the planet core (the reported bug, rotated to the equator) | **Blocker** | 1a controls | Pivot the whole scene so `centerDir → +Y`; Vitest asserts the quaternion lands an equatorial `centerDir` at ≈(0,1,0) and the analytic above-tangent invariant; e2e drives OrbitControls to the limit and asserts the per-frame `data-cam-above` (real camera state) stays `> 0`. **No attribute-presence checks.** |
| Patch overlay/depth correctness under SwiftShader is invisible to the data-* gate (pixel-free test rule) | Major | 1b | `depthTest=false` + `renderOrder` draws the patch unconditionally over its region (removes the z-fight dependency); the residual visual property is a NAMED SwiftShader screenshot artifact, not a data-* assertion. |
| Curvature is the only 3D cue and vanishes at deep zoom (~level 6 sagitta ≈ 0.001) | Major | core arc | Named honestly as a known cost; relief (increment G) is the concrete remedy behind an evidence gate. No depth-cap (it fights "drill / move freely"). |
| Lens-on-patch toggle is a silent dead no-op between 1b and 1e | Minor | 1b→1e | Disable the lens panel (or show a root-only hint) at `globeScale && nav.level>0` until 1e lands; 1e keys its test off the dedicated `data-patch-textures`. |
| Root-hop fly-to jank (fly to click, then jump to async centroid) | Major | 1a root hop | Resolve `continentAt` and the landing `centerDir` BEFORE committing the fly's target. |
| Patch texture squashed (sector aspect ≠ 2:1, `view_rect` is corners) | Minor | 1b | `setPatchTexture` rasterizes at the sector's `w/h` aspect; pinned in Vitest. |
| New wasm getter (relief) silently diverges native↔wasm (output never hashed) | Minor | inc 2 | `sample_grid` transcendental-free (or `fmath`); dedicated getter golden via `wasm_bindgen_test`. |
| Relief getter perturbs the `WorldData` golden | — (non-risk) | inc 2 | Structural: `&self`, no RNG, no serde field → ciborium bytes identical. Proven by a serde byte-equality test, NOT by re-running the (vacuous-for-this) `WorldData` golden. |
| Intermediate increment ships a stale pivot/patch (false-green return) | Major | 1a | `returnToGlobe` teardown lands in 1a; the return e2e asserts `data-region` absent AND post-return drag rotates the whole globe. |

---

## What the red-team changed

1. **Split increment 1 into 1a (camera/controls retarget, no geometry) + 1b (the patch).** 1a is the minimal sufficient fix for the literal complaint and isolates the make-or-break +Y clamp; 1b restores per-drill detail as a planned fast-follow (NOT hard-gated — it is regression-recovery, a different class from relief). [SCOPE major]
2. **Replaced the false-green make-or-break test.** `data-max-polar present` proved only that a line ran. Now: a Vitest geometric assertion (quaternion lands equatorial `centerDir` at +Y; analytic above-tangent invariant) + an e2e that drives OrbitControls past the clamp and asserts a per-frame `data-cam-above` derived from real `camera.position`. [DETERMINISM blocker, UX major]
3. **Pulled `returnToGlobe` teardown into 1a and strengthened the return assertion** (`data-region` absent + post-return whole-globe drag), so no intermediate ship leaves a stale pivot/patch under a green suite. [UX major, SCOPE major]
4. **Fixed the drill-depth assumption:** assert `data-patch` ∈ `[1-6]`, not `"1"` (a single root click lands at `continentDrillLevel` depth 3-6). [UX major]
5. **Dedicated `data-patch-textures` counter** for `setPatchTexture`, pinned in 1b, consumed by 1e — the lens test now keys off the patch's own signal, not the shared `data-textures`. [DETERMINISM major, SCOPE major]
6. **UV-aspect fix:** `setPatchTexture` rasterizes at the sector's true aspect (`view_rect` returns corners; viewBox is non-2:1), not hardcoded 2048×1024. [UX minor]
7. **Removed the vacuous relief golden proof.** "Re-run the `WorldData` golden green" cannot fail (getter not on the hashed path). Replaced with a structural argument (no serde field → identical ciborium bytes) + a dedicated getter golden via `wasm_bindgen_test` for native↔wasm coverage; `sample_grid` spec'd transcendental-free. [DETERMINISM major + minor]
8. **z-fight de-risked to non-existence:** patch drawn with `depthTest=false` + `renderOrder` (unconditional overlay), and the residual visual property named as a SwiftShader artifact, not a data-* assertion. [DETERMINISM major]
9. **Curvature-at-depth named as a known cost** with a concrete relief revival gate (no depth-cap). [UX major]
10. **Transition reorder:** resolve `continentAt`/`centerDir` before the fly commits (kills fly-then-jump-to-centroid jank). [UX major]
11. **Lens dead-path interim:** disable the lens panel at patch level until 1e. [UX minor]
12. **Label loss reframed** as TOTAL absence (not "softer text"); HTML billboards deferred with the region name as the first candidate. [UX major]
13. **Folded in:** deeper drilling reuses the golden-pinned `refineSector` verbatim (one sentence flagging it golden-relevant for future edits). [DETERMINISM minor]

**Dismissed:**
- *segW/segH "for relief" is premature abstraction* — DISMISSED as moot: the split moves segments to 1b, justified by curvature legibility alone; 1a has none.
- *Dedicated fontless detailed style (Open Q2)* — DISMISSED: `render_globe_texture` on a refined-sector handle already yields finer cartography via `view_rect()`; lock `"globe"`.
- *Hard-gate the patch like relief* — DISMISSED: the patch is regression-recovery of detail today's 2D drill shows, not net-new enrichment; only relief (a genuinely new cue) is gated.

---

## Reuse

- `rasterizeSvg(svg, w, h)` (main.ts 839) — refined sector SVG → untainted CanvasTexture, called with the sector's aspect in `setPatchTexture`.
- Worker `refine` branch + `refined` WorkerResponse (worker.ts 123-143) — unchanged; the patch is a new CONSUMER of the SVG it already returns.
- `refineSector` (lib.rs 55) → `render("globe")` → `render_globe_texture` (mapgen-render planet.rs 117, `Proj::equirect` over `view_rect()`) — no change; the sector globe SVG already carries the sector viewBox.
- The single persistent renderer/scene/camera/OrbitControls/raycaster lifecycle + `data-rendered`/`data-textured`/`data-textures` e2e signals (globe.ts).
- The 600ms fly-to lerp (globe.ts 207-222) — extended (pivot quaternion + target), not rewritten.
- Pure sector geometry: `sectorRect`, `childSectorAt`, `sectorAt`, `uvToWorld`, `continentDrillLevel`, `ancestors`, `crumbLabel` (sector.ts) — new helpers sit alongside and cross-check `uvToWorld`.
- The breadcrumb + `navTo` + `updateBreadcrumb` up/over machinery (main.ts 626-708).
- `withLayerClasses` (main.ts 132) — re-applies lens CSS before rasterizing.
- three 0.184.0 partial-sphere params + `Material.depthTest`/`renderOrder` (verified present).
- The blake3-over-ciborium golden harness — reused as context for the relief getter's structural-neutrality argument (NOT as the proof).

---

## Explicitly deferred (with revival triggers)

- **Relief / elevation displacement (increment G):** revive only if deep near-flat patches empirically read as flat 2D. Getter+consumer ship together; golden-neutral by structure.
- **Crisp on-patch place labels (HTML billboards):** revive when the fontless-patch label loss becomes the top complaint; region name first.
- **Pan-across-the-surface to a neighbour sector without re-drilling:** revive only with a concrete UX case; risks expanding into a streaming engine. Bounded pan-within-the-current-sector is the smaller variant if orbit-only tests as too constrained.
- **OUT permanently (Refinery-in-new-clothes):** continuous-LOD / quadtree-terrain streaming, >1 live patch, camera-distance auto-LOD, raster tile pyramid, geomorphing, fractional zoom, base-layer caching for scrub.

---

## Open questions for the user

1. **"Move freely" = orbit+zoom only, or also bounded pan within the region?** Default: orbit+zoom (the plain reading; the safe scope line). Bounded pan-within-the-sector is a cheap follow-on if orbit-only feels cramped.
2. ~~Dedicated fontless detailed sector style?~~ **Resolved: lock `"globe"`** (refined-sector render already gives finer cartography).
3. **Label loss on the patch is TOTAL** (the patch is fontless) vs the 2D sector's crisp vector labels. Accept unlabeled patches for v1 (defer HTML billboards), or pull the region-name billboard forward?
4. **Relief (increment G) is deferred behind an evidence gate.** Content to ship curvature-only first and decide on relief after feeling deep zoom, or put relief on the roadmap from the start?

---

## Recommended first increment

**Build Increment 1a — "Drill stays on the globe, orbitable region" — first.**

**Exact scope:** camera/controls retarget on the EXISTING base sphere, with the minimal `returnToGlobe` teardown. NO patch mesh, NO refine, NO new texture.
- `sector.ts`: pure `sectorPatchParams(sector, worldW, worldH) → {phiStart, phiLength, thetaStart, thetaLength, centerDir}`.
- `globe.ts`: a `pivot` Group wrapping the sphere; `focusRegion(centerDir, angularSize)` (rotate pivot so `centerDir → +Y`, set `controls.target`, `maxPolarAngle≈75°`, rescale min/max distance, write per-frame `data-cam-above`); `returnToGlobe()` (pivot→identity, target→origin, restore clamps); extend `flyTo` to lerp the pivot quaternion + target.
- `main.ts`: `navTo` globe branch stops `exitGlobeView()` on drill and calls `focusRegion`; root hop resolves `continentAt`/`centerDir` before committing the fly; `target.level===0` → `returnToGlobe()`.

**TDD spec (the load-bearing geometric floor — write the failing tests first):**
- **Vitest:** (1) `sectorPatchParams` ranges + `centerDir` cross-checked against `uvToWorld(center)`'s xyz. (2) The pivot quaternion from an **equatorial** sector sends its `centerDir` to ≈`(0,1,0)` within epsilon. (3) Analytic: with target on +Y and polar ≤ 75°, the camera locus stays above the region's tangent plane.
- **E2e (equatorial sector):** drill → drive OrbitControls to the polar limit → assert the per-frame `data-cam-above` (from real `camera.position`) stays `"1"`; `#globe-canvas` visible, `#map-content` hidden, breadcrumb `/L[1-6]/`; after "Globe" crumb, `data-region` absent AND a post-return drag rotates the whole globe.

**Why 1a first:**
1. It is the **minimal sufficient fix for the literal complaint** ("stay 3D and move freely") — both halves are resolved by retargeting controls on the already-textured sphere, with zero new geometry.
2. It **isolates the single make-or-break property** — the equatorial +Y polar clamp (the reported bug rotated to the equator) — and proves it with a real geometric test BEFORE any patch/texture/depth investment, so the hardest correctness question is settled in the smallest, cleanest unit.
3. It lets the **user confirm the fix** on a coarse-but-3D, freely-orbitable region before we spend 2 days on the patch apparatus (1b) — exactly the evidence-gating discipline the project applies to relief, applied one tier earlier.

This doc is LOCKED. Implement 1a against the spec above.

---

# Continuous-LOD streaming addendum (lock-ready)

> **Status: design, ready to lock.** This addendum is the user's deliberate
> opt-in to a continuous-LOD streaming layer, and it **supersedes the
> single-patch "no streaming, AT MOST ONE patch mesh" hard line above** (lines
> ~41, ~82, ~264-265) *for this layer only*. It sits ON TOP of foundation
> increments 1a (free-fly camera), 1b (single partial-sphere patch), 1c (deeper
> drill) and realizes `docs/adr/0001-multiscale-navigation.md` (coarse-first Q2,
> along-vector prefetch Q6, snap-to-discrete-level Q8). It does **not** redesign
> the camera, the per-sector refine, or the generation pipeline. Synthesized from
> four proposals (S1=267, S4=266, S3=241, S2=193), then hardened against a
> three-lens red-team (DETERMINISM, PERF/TESTABILITY, SCOPE/SEAMS) — every
> blocker and major is folded in below with an inline `[red-team: …]` note.

## Spine decision (and the corrected rationale)

**Exactly ONE discrete level is live at any instant** — a set of same-level
partial-sphere patches over the always-present coarse base sphere. One level +
a hard cap = bounded, never a pyramid. This is ADR 0001's "snap to the nearest
GENERATED level," generalized from one drilled plate to a camera-following set.

**Why one level beats multi-level + skirts — the HONEST rationale**
`[red-team SCOPE+DET, blocker+major: the original "textural seams FREE" claim
is FALSE for coastlines and must not be the spine rationale]`:

1. **Geometric seams are genuinely free** — adjacent live patches are always
   same-level, so their `SphereGeometry` parametric edges coincide exactly at
   radius 1.001 (no T-junction, no skirt geometry). This is real and
   pure-testable.
2. **It AVOIDS the *worse* cross-level coast mismatch** — multi-level adjacency
   forces neighbours generated at different cell densities AND different
   simplify tolerances to meet, a guaranteed coarse-vs-fine coastline step.
   One-level deletes that failure class by construction.
3. **It is NOT because same-level coast seams are free.** They are not (see
   Inter-patch seams). The one-level spine reduces the coast-seam problem to its
   minimum residual; it does not eliminate it.

**THE CORE V1 SCOPE DECISION — "detail follows on SETTLE," not "everywhere
during motion"** `[red-team PERF, blocker: the anti-stutter story is falsified
by the project's own code]`. Verified: a refine is ~100 ms serialized on the
single worker, and `rasterizeSvg` (`main.ts:842-850`: `new Image()` +
`await img.decode()` + `drawImage`) plus the upload (`globe.ts:303-308`:
`fadeMapEdges` pixel pass + `new CanvasTexture` + `needsUpdate` →
synchronous GL `texImage2D`) **all run on the main paint thread.** A microtask
boundary is still the main thread. Sustained fill is therefore ~10 patches/s
with a main-thread hitch per tile — **"detail follows everywhere during fast
pan" is a physical impossibility on this substrate, and this doc does not
promise it.**

The honest, decisively-recommended v1 is **detail follows on settle**: while the
camera moves, the user flies over the always-present coarse base sphere (never
blank); when the ~10 Hz settle-gate fires (camera moved < ε since last tick), the
in-view set fills in over ~1-2 s. This ships the genuinely-new value — multi-patch
at a fixed level (ST-1) and altitude→level snap (ST-2) — **without promising
motion the engine cannot sustain.** True continuous-follow and prefetch are
**gated behind one unlock**: off-main-thread rasterize via OffscreenCanvas /
`createImageBitmap`-from-SVG-blob, **which requires a spike to confirm it works
under headless SwiftShader before commitment** (not reliably supported
cross-engine). This is the same evidence-gated, anti-Refinery shape as the
worker pool — see Recommended scope reduction.

**In scope (v1):** view-driven LOD selection, a bounded patch cache,
settle-driven scheduling with single-worker serialization, inter-patch seams
(geometric free, coast measured + contingently stitched), pixel-free
testability. **Gated behind the OffscreenCanvas spike:** continuous-follow,
prefetch. **Out of scope (foundation / named / rejected):** the free-fly camera
controller (1a precondition), the 1b overlay (precondition), multi-level live
set, edge skirts, SSE metric, geomorphing, fractional zoom, the worker pool.

---

## LOD selection (the pure function)

A **pure** function in a new `web/src/lod.ts` — no three.js object crosses the
boundary, unit-testable off-GPU exactly like `sector.ts`:

```
desiredSectors(cam: CamState, worldW, worldH, cfg) -> Sector[]

CamState = {
  posUnit:  [x,y,z],    // camera position in the UNIT-SPHERE frame (pivot identity)
  forward:  [x,y,z],    // look direction (sub-point + prefetch vector)
  vpMatrix?: number[16],// view-projection — OPTIONAL; absent → N×N window fallback
  fovY, aspect, near, far,
}
altitude = |posUnit| - 1     // height above the unit sphere (radius 1)
```

Five pure, composable, individually-testable steps:

### 1. altitude → level (discrete snap — the hard line)
`levelForAltitude(altitude) ∈ [0, MAX_LEVEL=6]`, a fixed monotonic threshold
ladder, `clamp`ed. Thresholds are TUNING (`tuning_log.md`, sagitta/cells-per-pixel
argument), not free. **One global level per frame**, from `|posUnit|−1`. No
fractional levels, no geomorph. Testable invariant: monotonic, clamped `[0,6]`,
snaps at exact boundary altitudes.

### 2. Candidate footprint — frustum-corner rays + coarse-level enumeration arm
The target includes tilt-to-oblique, so a fixed N×N window under-covers the
oblique swath. Candidate generator = **frustum-corner ray projection**:
- Cast the 4 frustum-corner rays + central ray from `posUnit` through inverse
  `vpMatrix`; ray-sphere-intersect the unit sphere; drop rays that miss the near
  hemisphere.
- `dir → lon/lat → world(x,y)` via the inverse of `sector.ts` `uvToWorld`, then
  `sectorAt(wx,wy,level,...)` at the snapped level.
- Candidate set = unique sectors spanning the convex hull of hits, dilated by one
  neighbour ring (**clamp longitude AND latitude at the grid edge — see Seams**).
- **High-altitude arm:** when the whole globe disk is in view, corner rays miss
  into space (only near-center rays hit) → under-production leaves the visible
  limb on bare base. Fix: at coarse levels (level ≤ ~2) ENUMERATE all sectors at
  that level and let the horizon cull (step 3) select the visible hemisphere.
  Few sectors exist at coarse levels, so this is cheap. Graceful sharpening, not
  a correctness gate (base sphere covers any residual).
- **`vpMatrix`-absent fallback:** an N×N window around the sub-point (documented
  degradation, not the primary path).

### 3. Horizon / back-face cull (pure)
Keep a candidate iff its `centerDir d` (from `sectorPatchParams`) is above the
camera's tangent horizon: `dot(d, posUnit/|posUnit|) >= 1/|posUnit|`. Drops the
entire far hemisphere — the dominant bound on the live set.

### 4. Frustum cull (pure)
Project each survivor's 4 `sectorRect` corners (→ sphere points) through
`vpMatrix`; keep if any corner falls inside NDC `[-1,1]²` (1-sector margin so edge
patches load before they are strictly on-screen).

### 5. Rank + clamp to K (the cap, enforced IN THE SELECTOR)
Sort survivors by angular distance of `centerDir` from `forward`, take the first
**K**. Steps 2–4 each bound the count and step 5 truncates → **`|desired| ≤ K`
by construction** (S1's boundedness-twice). Returns a `Sector[]` sorted by key
for test stability.

> **Accepted cost (Open Q):** a single global level under steep oblique tilt
> over-details the far swath and under-details the near. Per-swath level would
> reintroduce cross-level adjacency. Accept for v1.

---

## Patch cache & lifecycle (caps + eviction + dispose)

A `PatchCache` owning the three.js objects + a **pure** policy module
`web/src/patchcache.ts`:

```
reconcile(live, desired, footprint, cfg) -> { toLoad: Sector[], toEvict: key[] }
```

Key = `${level}:${sx}:${sy}:${style}`
`[red-team DET, major: style/year omitted from the key → a lens toggle serves a
STALE patch for the patch's whole cache lifetime, not a transient frame]`.
`style` is in the key AND echoed in the `tile` response for verify-before-apply
(discard if the active lens changed mid-flight, like discard-stale). **Year is
NOT a key dimension — the time-slider is scoped OUT of the streaming globe in
v1** (mutually-exclusive modes; named limitation — re-refining K patches per
scrub during flight is pointless). `PatchMeta = { sector, mesh, material,
texture, texBytes, lastSeen, status, textured }`. The cache lives in `globe.ts`
(the only module that owns three.js).

### Load (on enter-set)
A desired-but-uncached sector enqueues a refine. On the `tile` response it is
built by the **generalized 1b `showPatch` pattern** *(per the 1b foundation spec
— see Foundation preconditions; 1b's overlay is not yet in shipped code)*:
`SphereGeometry(1.001, segW, segH, phiStart, phiLength, thetaStart, thetaLength)`
from `sectorPatchParams`, `depthTest=false`, **`renderOrder = baseOrder + level`**,
no-fade `setPatchTexture` (`ClampToEdgeWrapping`, `generateMipmaps=false`,
rasterized at sector aspect). The base sphere shows that region underneath until
the texture lands (coarse-first — free).

### Double-buffered swap (anti-flash, from S3)
Build the mesh + `CanvasTexture` **fully off-scene**, then in **one tick** add it
to the pivot AND dispose the stale one. A swap is never a half-textured flash.

### Level transition — coverage-gated keep-until-ready (anti-pop), bounded
`[red-team PERF+SCOPE, blocker×2 CONVERGED: the "every incoming .textured" gate
livelocks against the cap, and nested oscillating flips blow the cap or pop —
ONE unified fix]`

When the global level flips, outgoing and incoming patches cover the **same
footprint, stacked** (never adjacent — the one-level invariant makes this a
compositing-order problem, not a cross-level seam). Four rules make it pop-free,
double-image-free, AND provably bounded:

1. **Compositing order:** `renderOrder = baseOrder + level` → the finer incoming
   level composites **on top** of the coarser outgoing level; with
   `depthTest=false` and opaque patches there is no z-fight and partial fill
   reads cleanly.
2. **Coverage gate, NOT a textured-count gate:** outgoing patches are pinned
   until **the loaded-incoming set COVERS the current view footprint** (a
   coverage predicate the selector already computes), then disposed.
   Gating on "every incoming `.textured===true`" would livelock — the cap can
   refuse an incoming add, so that set may *never* complete; coverage cannot be
   starved the same way.
3. **Incoming-during-transition has STRICT eviction priority over pinned
   outgoing.** When the cap is hit, the cache evicts pinned-outgoing *before* it
   ever refuses an incoming add. The cap can therefore never starve the incoming
   set into a stuck transition.
4. **Overlap is capped at ONE generation.** A second level flip beginning before
   the first completes immediately disposes the oldest-outgoing level (does not
   pin a third). At most 2 levels are ever pinned → the count cap holds under
   rapid altitude oscillation. The cost is a **rare, bounded single-frame pop on
   fast oscillation — a NAMED accepted cost**, not a livelock.

### Evict (on leave-set / overflow / transition-complete)
Disposal mirrors `globe.ts` `dispose`/`setTexture` teardown *(per the shipped
`globe.ts:309,323-325` base-sphere teardown — generalized per-patch; the 1b
patch teardown is part of the 1b foundation spec)*:

```
pivot.remove(mesh);
mesh.geometry.dispose();
material.map.dispose();
material.dispose();
// then Map.delete(key); texBytes -= entry.texBytes; bump data-evictions
```

### Hard caps (two-dimensional, enforced before any add)
- **COUNT cap `MAX_LIVE_PATCHES = 24`** (tuning knob). Typical in-view 4–12; 24
  is ~2× headroom for the ≤2-level (one-generation) transient. Overflow evicts
  LRU among out-of-view non-pinned entries, then pinned-outgoing (rule 3 above);
  if the cap *still* cannot be honored, **refuse the add, leave that region on
  the base sphere** (graceful degrade, never crash).
- **GPU-MEMORY cap `MAX_TEXTURE_BYTES ≈ 96 MB`** (tuning knob). Each patch ≤4 MB
  (1024px long-edge, RGBA, no mipmaps) → ≤96 MB at 24. `texBytes` summed on load,
  subtracted on every dispose; can force eviction before the count cap.

### Eviction policy
LRU among out-of-view entries; **never evict** (a) a current desired-set member,
(b) a prefetch entry, (c) a pinned-outgoing patch *unless* an incoming-during-
transition add needs the slot (rule 3). `reconcile` is pure over plain cache
state — unit-tested off-GPU; three.js dispose is the only GL-touching glue. The
cache holds **zero wasm handles** (the worker frees them).

---

## Scheduling & anti-stutter (settle-driven, single worker)

A **pure** `web/src/scheduler.ts` (the worker `send` is injected) + the queue
drive. The single worker is the serialization point and the design **embraces**
it.

### In-flight budget = 1
At most one refine outstanding (one worker; WASM refine is synchronous,
**cannot be preempted**). No refine, rasterize, or geometry build in the render
loop — the loop only reads camera state and recomputes the desired set on the
settle throttle.

### Worker change — ONE new `refineTile` branch (resolved conflict)
`[red-team DET, confirmed: reusing `refine` clobbers a live consumer]`. Verified:
`worker.ts:17` `view() = sector ?? root`; `worker.ts:132-134` `refine` writes the
module-global `sector` slot; `render`/`renderYear` read `view()`; `main.ts`
posts `render` AND `yearFrame`-driven `renderYear` **while the globe is active**
(main.ts:503-509 re-textures the live sphere). So a streaming refine on the
`refine` message would re-texture the whole-globe base to a stray patch.
Resolution — the minimal change:
- New message `{ type:'refineTile', seqId, level, sx, sy, style }`.
- Handler: `const tile = root.refineSector(level,sx,sy,SECTOR_CELLS);
  const svg = tile.render(style); tile.free();` — **free the temp handle
  immediately, never touch the `sector` slot.** Posts
  `{ type:'tile', seqId, level, sx, sy, style, svg }` (`style` echoed for
  verify-before-apply).
- The legacy `refine`/`render`/`renderYear` slot path is **untouched** → the
  shipped 2D drill + globe lens/year re-render keep working.

Golden-neutral (the `refineSector` CALL is verbatim).

### Priority queue (pure)
`prioritize(desired, cached, inflight, cam) -> Sector | null`:
in-desired ∧ not-cached ∧ not-in-flight, ranked **coarse-first → nearest-to-
view-center → along-flight-vector**. Coarse-first: a not-yet-loaded fine sector
is already covered by the base sphere (and, during a transition, the outgoing
coarser patch) → no "gap" is ever enqueued.

### Cancellation (worker can't be preempted → discard + don't-start)
- **Discard-stale:** when a `tile` arrives, if its `{level,sx,sy}` is no longer
  desired **or** its `style` ≠ the active lens, drop the SVG **without
  rasterizing/uploading**. Bumps `data-refines-discarded`/`data-coalesced`.
  Bounded waste: ≤ one stale ~100 ms refine per sharp turn.
- **Don't-start-if-stale:** the scheduler re-prioritizes from the LIVE desired
  set each settle tick, so a queued-but-unsent sector that left view is never
  sent.

### Coalescing
The queue is keyed by sector — re-requesting an already-queued/in-flight/cached
sector updates priority, never duplicates.

### Anti-stutter budget (honest about the main-thread cost)
`[red-team PERF, blocker: do NOT claim "<2ms/frame" or "off-RAF removes the
hitch" — the rasterize+upload is the dominant cost and lands on the paint
thread]`:
1. **Coarse-first base sphere** → never a blank gap.
2. **Double-buffered swap** → never a half-textured flash.
3. **Settle-gated fill (~10 Hz):** `desiredSectors` + the rasterize+upload pump
   run only when the camera has *settled* (moved < ε since the last tick), so the
   per-tile main-thread hitch lands while the camera is still, not mid-pan.
4. **≤1 rasterize + upload per pump tick** → the honest ceiling is **one
   rasterize+decode+upload hitch per N ticks** while settled; N is the tuned
   knob, and the base sphere covers everything not yet filled. ST-4 ships a
   measured `data-upload-tick-ms` signal, NOT a budget asserted on paper.
5. **Discard-stale** → bounds wasted refines to one ~100 ms.

The honest worst case: a fast traverse reads as **coarse base sphere with the
in-view set filling in once you settle** — graceful, never a frame stall. The
refine RATE + main-thread upload, not the frame rate, is the inherent ceiling.

### Prefetch — GATED behind the OffscreenCanvas unlock (was the last increment)
Finite-difference camera velocity (JS-ephemeral, NOT persisted), advance
`posUnit` one tick, compute the desired set there, enqueue at **idle** priority
(only when the in-view queue is empty). Conservative, rank/vector-driven (ADR
0001 Q6, not the speculative adjacent-tile flood). **Moved behind the
off-main-thread-rasterize gate** — on the single-worker + main-thread-rasterize
substrate, prefetch competes for the same serialization point and main-thread
hitch as the in-view fill, so it cannot deliver "instant arrival" until that
ceiling is lifted.

### Worker pool — REJECTED for v1, gated to a final increment
`root` (the opaque WorldHandle) cannot be transferred, so a pool needs each
worker to re-`generate` a byte-identical `root` (doubling init + memory) —
speculative infra, the killed-Refinery shape. v1 is single-worker; a 2-worker
pool is **evidence-gated** (revival: profiling shows the queue persistently
starves the active sector AND a second root init is affordable).

---

## Inter-patch seams

### Geometric — FREE (proven, not hoped)
Same-level adjacency → `sectorPatchParams` derives `phi/theta` from `sectorRect`,
which tiles exactly (pinned by `sector.test.ts` "four children tile it exactly"),
same level → identical `segW/segH` → boundary vertices are positionally identical
at radius 1.001. No runtime stitching, no skirts.
**Pure-testable (ABSOLUTE anchoring, not just relative adjacency)**
`[red-team PERF, major: a shared offset/flip error passes a relative-adjacency
test]`: assert `sectorPatchParams(sector).phiStart/thetaStart` equal the values
**independently computed from `sectorRect` via `uvToWorld` corner anchors** (the
`sector.test.ts` cross-check discipline), AND that E/W neighbours' shared edge
coincides, AND equal `segW`. Relative-only would false-green on a uniform flip.

### Textural — NOT free; a measured residual, contingently stitched
`[red-team SCOPE+DET, blocker+major: same-level coast seams are NOT free, and the
phi/theta unit test is false-green for the coast risk]`. Verified against
`scale_spec.rs:262-299` and `scale.rs:328`:
1. `pin_edges_to_shared` blends each sector's elevation toward the SHARED base
   field; at the seam `d_in→0 ⇒ smoothstep weight w→0 ⇒ elevation = shared
   field EXACTLY` (edges/corners are **maximally** pinned, not weakest). Test:
   elevation **MAD < 0.04**, land/sea agreement **> 90%**, river-presence
   **≥ 34/40 (85%)**. Reused, not re-solved.
2. **The residual is a REAL coast seam, not a sub-pixel hairline.** Land/sea is a
   threshold on `elev > 0.0` (confirmed throughout the codebase). The ~10% of
   seam cells where the shared elevation sits near 0 can flip land↔sea across the
   sub-0.04 residual → one patch draws coast where the neighbour draws open
   water. Separately, each patch extracts its coast on its **own independent
   Voronoi layout** (`refine_sector` builds a fresh sub-mesh per `(level,sx,sy)`),
   and Visvalingam removal order is global-to-chain → even where both sides agree
   land/sea, the simplified polylines run through different vertices → a notch
   where a coastline crosses the seam. **Corrected wording:** coast chains agree
   "to within a hairline because the elevation field is pinned; the polylines are
   NOT bit-identical."
3. **Rivers:** projected from the parent global network (`project_hydrology`), so
   both sides **agree on river presence at the seam ~85% of the time, with
   sub-cell positional jogs where a river crosses** (the per-patch
   `nearest_sector` snap). Reuse, not identity — corrected from the over-strong
   "identically."

**The coast-seam unit test (CONTINGENT — feasible only if coast chains are
extractable off-GPU).** Coast extraction currently lives in the SVG render path,
NOT a standalone data module. **If** the seam coast chains can be sampled as data
off-GPU, add a pure unit: the two neighbours' simplified coast chains, sampled
along the shared edge, stay within a lateral-deviation tolerance band
(< N world units), so a regression in `pin_edges`/simplify fails a pure test.
**If they cannot** (reachable only through SVG), the screenshot is the sole
check — stated honestly, not promised. **Demote the phi/theta/segW unit** to
what it is: a regression guard on already-pinned *geometry*; it does NOT cover
the textural seam.

**The named coast-seam screenshot is LOAD-BEARING** — its job is to **measure
the severity of the unavoidable coast seam**, not to confirm seamlessness. If it
shows a visible crack, the fix is the gated **contingent ST-1.5 Rust
coast-reconciliation** increment (re-extract the seam coast chain from the shared
field so both sides emit the identical polyline). ST-1 must ship first *to even
see the crack* → the Rust stitch is necessarily a gated fast-follow, not a
pre-ST-1 increment.

**Optional texture-bleed mitigation — a NEW RENDER ENTRYPOINT, never a `region`
widen** `[red-team DET, major: the halo is OUTSIDE the serialized `region`
(scale.rs:191 `mesh_data.region = Some(rect)`); widening `region` to expose
bleed pixels MOVES the seed42_sector golden]`. The bleed, if shipped, is a
render-time wider-viewBox **argument** (e.g. `render_globe_texture_bleed(world,
grow_frac)` projecting over a grown rect while the halo cells, already in
`mesh.sites`, fall inside it). It **MUST NOT mutate the stored `MeshData.region`.**
It hides a *sampling* hairline; it **cannot reconcile a land/sea topological
disagreement** (growing the viewport just overlaps A's coast onto B's water) — so
it is not the coast-seam fix, only a hairline polish. See Hard lines.

### Antimeridian / poles — CLAMP, do not wrap
`[red-team SCOPE, major: wrapping loads both ±180° columns as adjacent patches,
and `fadeMapEdges` is base-sphere-only — patches use the no-fade `setPatchTexture`
path, so the fade does NOT mitigate the loaded case]`. The generated world is a
flat, **non-periodic** grid (its ±180° edges are different coastlines; the poles
pinch). **Decision: the selector CLAMPS longitude (like latitude), so the ±180°
columns never load as adjacent patches.** The antimeridian/pole region then
degrades to the `fadeMapEdges`-faded base sphere — **the same accepted cost as
the shipped globe**, never a hole, never a hard loaded-patch seam. The "fade
mitigates a wrapped, loaded antimeridian patch" claim is **deleted** — it was
false.

### REJECTED: S2 edge skirts
Skirts solve a cross-level adjacency that only exists in a multi-level live set.
The one-level spine has no cross-level pair → skirts are pure added geometry with
no problem to solve. Deferred, revivable only if a multi-level variant is ever
forced.

---

## Foundation preconditions (what 1a + 1b must hand the streaming layer)

The streaming layer consumes camera **state**, never the control model — this
decoupling is what makes LOD selection pure.

**1a free-fly reconciliation:** the locked 1a is **pivot-orbit-one-region**
(`enablePan=false`, orbit ONE region). Streaming requires **free-fly** (pan
across the whole surface, tilt, zoom). The streaming layer assumes the
**free-fly variant of 1a** (fixed globe, moving camera, pivot identity) with a
`getCameraState(): CamState` hook. **This is a BLOCKING precondition — see Open
Q1.**

**1b overlay precondition** `[red-team: globe.ts today has only
`SphereGeometry(1,64,48)` — 1b's `showPatch`/`setPatchTexture`/`depthTest=false`/
`ClampToEdge`/`renderOrder` overlay is the 1b SPEC, not yet shipped code]`. All
patch-build specifics in this addendum are **per the 1b foundation spec**, not
verified-against-shipped-code; they presuppose 1b has landed. **Also a BLOCKING
precondition — see Open Q1.**

Hooks `globe.ts` must expose: `getCameraState()` (packs `camera.position` in the
unit-sphere frame, `forward`, `vpMatrix = matrixWorldInverse × projectionMatrix`,
`fovY/aspect/near/far`); the single persistent `Scene`/`Camera`/`Renderer`/
`Raycaster`/`OrbitControls` + base `sphere` + pivot `Group` (reused, never
recreated — headless SwiftShader depends on context reuse); the 1b
`showPatch`/`setPatchTexture` overlay **generalized to `showPatchSet`** (N keyed
patches), not rewritten. The selector NEVER imports three.js.

---

## Determinism & WASM/golden impact

**The golden does not move.** The blake3-over-ciborium-`WorldData` golden
(`cross_platform.rs`, incl. `refined_sector_golden_matches_native_under_wasm`,
seed-42 sector) hashes serialized `WorldData`; the render SVG is **never** hashed.
Three negatives:
- **(a) NO new RNG draw** — the layer orchestrates `refineSector` calls (already-
  pinned `StageRng::sector` seeds); it draws nothing.
- **(b) NO new serialized field** — cache, queue, velocity estimate, counters,
  `seqId`, `style` are all JS/main-side. The optional texture bleed is a
  render-time viewBox arg (never hashed) and **must not** widen `region`.
- **(c) NO mutation of the `refineSector` CALL** — each patch is a new SVG
  CONSUMER of `root.refineSector(level,sx,sy,SECTOR_CELLS)`, verbatim. The
  `refineTile` branch changes the worker *state model around* the call (frees the
  temp handle, never writes `sector`), not the call.

**THE SINGLE NEW DETERMINISM RULE — broadened to ALL refine inputs**
`[red-team DET, minor: SECTOR_CELLS is correct but `halo_fraction` and
`detail_octaves` ride on `RefineParams::default()` and would equally break
cache-coherence if a future increment varied them]`: **ALL `RefineParams` fields
(`target_cells`/SECTOR_CELLS, `halo_fraction`, `detail_octaves`) must be a pure
function of the sector key.** Today they are `RefineParams::default()` (4000 /
0.25 / 3) + the `SECTOR_CELLS` const — all constant, so a cache hit is
byte-identical to a fresh refine (already proven by
`scale_spec.rs::refinement_is_deterministic` + the wasm golden). Any future
per-level variation of ANY of them must fold into the key **and re-anchor the
seed42_sector golden.** The risk is future drift only.

**Why this layer is the most dangerous under the safety model (honest framing):**
the golden hashes WorldData, never the render — so the streaming engine's entire
*visible* output (seams, lens/year consistency, stale-vs-fresh textures,
antimeridian/pole degradation) lives in the one region the determinism harness is
structurally blind to. **"Golden-neutral" means "the harness cannot see it,"
NOT "verified."** This layer trades determinism-harness coverage for screenshot
+ pure-unit coverage — so the named screenshot artifacts are **load-bearing
infrastructure, not afterthoughts.**

**Golden re-anchor required:** NONE for the streaming increments. The gated
worker-pool increment adds a `worldJson` root-equality assertion across workers
(a real assertion on the new thing), not a golden move.

---

## Testability (pure units + data-* signals + named screenshot artifacts)

### Pure / off-GPU units (Vitest — the false-green-proof floor)
- `levelForAltitude` — monotonic, clamped `[0,6]`, snaps at boundaries.
  Mutation: flip an inequality → a boundary altitude lands on the wrong level.
- `desiredSectors` — an **oblique tilt** cam yields the expected swath at the
  snapped level, **cross-checked vs `uvToWorld` corner anchors** (catches a
  flipped 1−v); a FAR-side sector is ABSENT (mutation: drop the horizon cull →
  it appears); an out-of-frustum sector ABSENT; an antimeridian cam **CLAMPS**
  sx (mutation: wrap → both ±180° columns appear); a high-altitude whole-disk cam
  covers the full horizon-visible hemisphere (mutation: drop the enumeration arm
  → limb sectors absent); `|result| ≤ K` for a dense oblique frame.
- `reconcile` — `toLoad`/`toEvict` diff; HARD CAP (feed 40 desired → live never
  exceeds 24 AND byte-sum ≤ cap; mutation `cap-1` → an extra eviction);
  LRU evicts oldest non-pinned; **the converged transition test:** feed
  `desired+outgoing > cap` AND two overlapping flips, assert live ≤ 24
  throughout AND ≤ 2 levels pinned AND the transition COMPLETES (mutation: pin
  outgoing above incoming priority → live > 24 OR transition never completes —
  kills both the livelock and the cap-blow false-greens at once).
- `prioritize` — coarse-first → nearest → along-vector. Mutation: reverse the
  distance comparator → far sector chosen.
- discard predicate — a `tile` whose sector left `desired` **or** whose `style` ≠
  active lens is flagged discard. Mutation: drop the check → stale applied.
- seam **geometry** — `sectorPatchParams` ABSOLUTE-anchored to `uvToWorld`
  corners + shared-edge coincidence + equal `segW` (a regression guard on
  already-pinned geometry — **NOT** coast-seam coverage).
- (CONTINGENT) coast-seam lateral-deviation — only if coast chains are
  extractable off-GPU; else the screenshot is the sole coast check.
- (ST-3) `desiredSectors` over a SEQUENCE of panned cam states returns a sliding
  window whose membership tracks the moving sub-point (mutation: freeze the
  sub-point → the window stops moving → the assertion fails) — proves the
  *follow* is real off-GPU before the e2e.
- (ST-5, gated) `predictDesired(cam, velocity, dt)` returns the set one tick
  ahead (mutation: zero velocity → prediction equals the current set).

### data-* e2e signals (headless WebGL via SwiftShader, NEVER pixels)
Written each frame from REAL engine state onto `#globe-canvas.dataset`. **Stated
per signal whether it adds coverage over the unit or is a redundant smoke check**
`[red-team PERF, minor: don't pad the list with vacuous signals]`:
- `data-live-patches` — cache size; asserted **`≥ 2 AND ≤ K`** (the `≥ 2` is the
  false-green guard; needs the real GL loop — genuine e2e coverage).
- `data-lod-level` — min-max RANGE (S2 graft): `"4-4"` normally (the one-level
  invariant observed), `"5-6"` only during a transition. Genuine.
- `data-evictions` — monotonic; INCREASES on a traverse while `data-live-patches`
  stays capped (proves bounded lifecycle — needs the real worker+GL loop, a pure
  test cannot produce it). Genuine.
- `data-refines` — monotonic completed-and-applied tiles. Genuine (real loop).
- `data-upload-tick-ms` — **measured** rasterize+upload cost per pump tick (NOT a
  paper budget). The honest perf signal.
- `data-refines-inflight` — asserted ≤ 1. **Redundant** with the pure budget=1
  invariant; kept as a cheap smoke check, not claimed as added coverage.
- `data-coalesced`/`data-refines-discarded` — driven by a **deterministic
  scripted teleport** (jump the camera between two non-adjacent sub-points in one
  tick so the in-flight tile is provably stale), NOT a "fast pan" that races the
  clock. If even the teleport can't make it deterministic headless, demote to the
  pure discard-predicate test and drop the e2e claim.
- `data-prefetched` (gated) — increments when the ahead-sector goes live before
  the view-center reaches it.

### Named screenshot artifacts (per verify-before-commit — LOAD-BEARING)
Two, both captured at ST-1, both eyeballed (not optional):
1. **Patch placement** `[red-team PERF, major: a flipped-UV / wrong-orientation
   patch still counts as a live patch — NO data-* counter catches it]`: a single
   SwiftShader shot of a known sector at a known sub-point, eyeballed that the
   patch lands where `uvToWorld` predicts and is not flipped.
2. **Coast-seam severity:** the two-adjacent-patch coastline boundary, eyeballed
   for crack severity (NOT z-fight — `depthTest=false` removes that). This
   **measures** the residual coast seam and decides whether contingent ST-1.5
   Rust stitching is needed.

---

## Perf budget (concrete, honest)

| Bound | Value | Enforced by |
|---|---|---|
| Render-loop CPU (camera read + throttled selector + scheduler poll) | small; **excludes** rasterize+upload | NO refine/rasterize/build in the loop |
| Rasterize + decode + upload | **main-thread hitch, ~tens of ms/tile** | lands while SETTLED (not mid-pan); ≤1 per pump tick; `data-upload-tick-ms` measures it |
| In-flight refines | **1** | single worker, ~100 ms/refine, synchronous |
| Live-patch count cap | **24** (hard) | pure `reconcile`; typical 4–12 |
| Texture-memory cap | **~96 MB** (hard) | 1024px × RGBA × no-mipmaps ≤4 MB × 24 |
| Selector + fill pump | **~10 Hz, settle-gated** | recompute only when settled (moved < ε) |
| Refine fill rate | **~10 patches/s** | one worker; coarse-first hides it |

**Honest stutter bound:** coarse-first base → never blank; double-buffered swap →
never a half-textured flash; settle-gate → the per-tile main-thread hitch lands
while the camera is still. Worst case on a fast traverse is **coarse base sphere
with the in-view set filling in once you settle** — graceful, never a stall.
**The refine rate + main-thread upload, not the frame rate, is the ceiling; the
OffscreenCanvas unlock + the gated worker pool are its only true fixes.**

---

## Increment sequence

After foundation 1a (**free-fly variant**), 1b, 1c. Prefixed **ST-** to avoid
colliding with proposal IDs S1–S4.

### ST-1 — Multi-patch cache at the drilled (fixed) level
- **Deliverable:** drilling shows the few in-view patches as a tiled set over the
  base sphere; orbiting brings new patches in and disposes those that leave,
  within the cap. User-visible value on increment one. Pure `desiredSectors`
  (frustum-corner + horizon + frustum + clamp-to-K, **longitude CLAMPED**),
  `reconcile`, the three.js `PatchCache` (`showPatchSet` per the 1b spec, dispose
  mirroring the base teardown), and the worker `refineTile` branch
  (frees temp handle, never touches `sector`; `style` in key + echoed).
- **TDD spec:** Pure Vitest — `desiredSectors` oblique swath cross-checked vs
  `uvToWorld` corners; horizon cull drops the far hemisphere; high-altitude
  whole-disk → full horizon-visible hemisphere (mutation: drop the enumeration
  arm → limb absent); antimeridian cam CLAMPS sx (mutation: wrap → both columns);
  `reconcile` cap (mutation `cap-1` → extra evict); `sectorPatchParams`
  ABSOLUTE-anchored to `uvToWorld` corners + shared-edge + equal `segW`. E2e:
  after drill `data-live-patches ≥ 2 AND ≤ 24`; after an orbit drag
  `data-evictions` INCREASES while `data-live-patches` stays ≤ 24 and
  `data-rendered=1`. **Two named screenshots:** patch placement (not flipped) +
  coast-seam severity.
- **Est:** 3 days. **Risk:** GL leak if dispose incomplete → pure-tested
  `reconcile` + dispose-is-the-only-glue + `data-evictions` rising without
  `data-live-patches` growing. The `refineTile` branch must not regress the
  legacy slot path → re-run the existing drill + globe lens/year e2e green.

### ST-1.5 (CONTINGENT) — Rust coast-seam reconciliation
- **Trigger:** ST-1's coast-seam screenshot shows a visible crack. **Deliverable:**
  re-extract the shared seam coast chain from the shared base field so both
  neighbours emit the identical polyline (and/or force identical elevation in the
  pin band so the land/sea threshold cannot flip). **Golden impact:** if it
  touches serialized terrain → re-anchor seed42_sector; if render-only → neutral.
  **Est:** 2–3 days (gated). Not built unless the screenshot demands it.

### ST-2 — Altitude → discrete level snap + coverage-gated keep-until-ready
- **Deliverable:** climbing/descending snaps to the next discrete level with NO
  blank flash and NO pop — coarse-first per level, finer-on-top compositing,
  the **coverage-gated, one-generation-capped** transition. Drives
  `data-lod-level` (range).
- **TDD spec:** Pure Vitest — `levelForAltitude` monotonic + clamped + snaps
  (mutation: flip inequality); **the converged transition test** (desired+outgoing
  > cap AND two overlapping flips → live ≤ 24, ≤ 2 levels pinned, transition
  completes; mutation: pin-above-incoming → blows cap or never completes). E2e:
  descend → `data-lod-level` increases AND `data-live-patches` never drops to 0
  mid-transition AND `data-lod-level` briefly shows `"5-6"` then collapses.
- **Est:** 2 days. **Risk:** thresholds have no ground truth (`tuning_log.md`);
  the invariant is monotonic+snap. Rapid oscillation may show a bounded
  single-frame pop — a named accepted cost.

### ST-3 — Settle-fill follow (re-scoped from "continuous follow everywhere")
- **Deliverable** `[red-team PERF, blocker: the single-worker + main-thread-
  rasterize substrate cannot sustain continuous follow]`: flying across the
  planet keeps the **settled** in-view region at the altitude-warranted level,
  refining the new region and disposing behind once the camera stops moving.
  While moving, the coarse base sphere covers everything. **NOT** detail during
  fast motion. Reuses ST-1's selector; binds it to free-fly pan over the WHOLE
  surface.
- **TDD spec:** **Pure Vitest floor (the marquee increment gets its own
  false-green guard)** — `desiredSectors` over a SEQUENCE of panned cam states
  returns a sliding window whose membership tracks the moving sub-point
  (mutation: freeze the sub-point → the window stops moving → fail). E2e: pan,
  settle, pan, settle across several sectors and assert `data-evictions` AND
  `data-refines` BOTH increase across settles while `data-live-patches` stays in
  `[2,24]`.
- **Est:** 2 days. **Risk:** fill only on settle is the named v1 behavior, not a
  bug; continuous-follow is the OffscreenCanvas-gated follow-on.

### ST-4 — Scheduler: priority queue + budget=1 + double-buffered swap + discard-stale + measured upload cost
- **Deliverable:** refines spent nearest-needed first; double-buffered swaps;
  discard-stale (regions the camera left, or a stale lens); **a MEASURED
  `data-upload-tick-ms`** (not a paper budget).
- **TDD spec:** Pure Vitest — `prioritize` picks the nearest in-desired-uncached
  (mutation: reverse comparator → far sector); discard predicate (sector left
  OR style changed). E2e: a **deterministic scripted teleport** drives
  `data-refines-discarded` up (stale dropped); `data-refines-inflight` never > 1.
- **Est:** 2.5 days. **Risk:** mis-prioritization shows as slower fill, not a
  crash.

### ST-5 (GATED behind the OffscreenCanvas spike) — Prefetch + true continuous-follow
- **Gate:** a spike confirms `createImageBitmap`-from-SVG-blob rasterize works in
  a worker under headless SwiftShader (it is not reliably supported cross-engine).
  Until then, do NOT build. **Deliverable once unlocked:** off-main-thread
  rasterize lifts the per-tile main-thread hitch → continuous-follow during
  motion becomes feasible, and idle-priority prefetch along the flight vector
  makes steady flight feel instant.
- **TDD spec:** the OffscreenCanvas spike result is itself the first artifact;
  then Pure Vitest `predictDesired` (mutation: zero velocity → prediction =
  current set); E2e `data-prefetched` increments AND the ahead-sector is live
  BEFORE the camera center crosses into it.
- **Est:** spike 1 day + 2 days (gated). **Risk:** the spike may fail headless →
  prefetch/continuous-follow stay deferred, settle-fill remains the shipped v1.

### ST-6 — (GATED) 2-worker pool — do NOT build speculatively
- **Revival trigger:** profiling shows the queue persistently starves the active
  sector AND a second root init is affordable. `inFlightBudget → 2`.
- **TDD spec (revival-gated):** Vitest scheduler honors budget=2; a `worldJson`
  byte-equality root-equality assertion across workers (the real new assertion,
  NOT a vacuous golden re-run); `data-refines-inflight` may reach 2.
- **Est:** 2 days (gated). **Risk:** doubles wasm init + root memory; `root`
  can't be transferred. Gated — the Refinery lesson.

---

## Risk register

| # | Risk | Severity | Mitigation / status | Residual |
|---|---|---|---|---|
| R1 | Main-thread rasterize+upload hitch caps sustained fill at ~10/s | **blocker (accepted)** | v1 re-scoped to settle-fill; hitch lands while settled; `data-upload-tick-ms` measures it; continuous-follow gated behind OffscreenCanvas (ST-5) | Fast motion reads as coarse base until settle — NAMED v1 behavior |
| R2 | Same-level coast seam (land/sea threshold flip + independent Voronoi) | **blocker (managed)** | spine demoted to "geometric-free, avoids worse cross-level mismatch"; coast-seam screenshot LOAD-BEARING; contingent ST-1.5 Rust stitch | A measured coast seam; stitched only if the shot demands |
| R3 | Keep-until-ready livelock + nested-flip cap blow | **blocker (fixed)** | coverage gate (not textured-count) + incoming-priority-over-outgoing + one-generation overlap cap; converged pure test | Bounded single-frame pop on rapid oscillation — named cost |
| R4 | Stale lens/year patch served for the patch's whole cache life | **major (fixed)** | `style` in the cache key + echoed in `tile` + verify-before-apply; time-slider scoped OUT of the streaming globe (year not a key dim) | Globe + time-slider are mutually-exclusive modes in v1 |
| R5 | Texture-bleed silently widens serialized `region` → moves the golden | **major (fixed)** | bleed = render-time wider-viewBox ARG, never mutates `region`; hard line added | Bleed is a hairline polish only, not the coast fix |
| R6 | Antimeridian: wrapped loaded patches show a hard unfaded seam | **major (fixed)** | CLAMP longitude (don't wrap) → degrades to faded base, same as shipped globe | Antimeridian region stays coarse — accepted, same as today |
| R7 | False-green geometry test masks the real (placement + coast) risks | **major (fixed)** | geometry test demoted to a regression guard; absolute-anchoring added; TWO load-bearing screenshots (placement + coast) | Placement + coast are pixel-checked, by design |
| R8 | `refineTile` regresses the legacy `sector`-slot drill/lens/year path | **major (fixed)** | new branch frees its temp handle, never writes `sector`; re-run legacy e2e green in ST-1 | None if the legacy e2e stays green |
| R9 | Future per-request refine-param drift breaks cache coherence | **minor (ruled)** | ALL `RefineParams` fields must be a pure function of the key; any variation re-anchors the golden | Future-drift only; constant today |
| R10 | OffscreenCanvas-from-SVG unsupported under headless SwiftShader | **minor (gated)** | ST-5 gated behind a spike; failure leaves settle-fill as shipped v1 | Continuous-follow may never unlock headless |
| R11 | GL-context leak from incomplete dispose | **minor (managed)** | dispose is the only GL glue, mirrors the proven base teardown; `data-evictions` rises without `data-live-patches` growth | None if the eviction e2e stays green |

---

## What the red-team changed

Every blocker and major was code-verified before folding in; **no finding was
dismissed** (all were grounded in the actual code). Changes, by lens:

**PERF/TESTABILITY (1 blocker, 3 majors):**
- **[blocker] The anti-stutter story was falsified by the code** (rasterize+upload
  are main-thread; `<2ms/frame` and "off-RAF removes the hitch" were false). →
  **The biggest change in the doc:** v1 re-scoped to **settle-fill**;
  `data-upload-tick-ms` replaces the paper budget; continuous-follow + prefetch
  gated behind an OffscreenCanvas spike (ST-5). The original ST-3 "continuous
  follow everywhere" and ST-5 "prefetch on single worker" are **removed as
  shippable-on-this-substrate.**
- **[major] Patch placement is pixel-only** (a flipped/wrong-orientation patch
  still counts as live). → Added a **second load-bearing screenshot** (placement)
  and **absolute-anchored** the geometry unit to `uvToWorld` corners.
- **[major] Keep-until-ready livelocks against the cap.** → Converged with the
  SCOPE nested-flip finding into one fix (coverage gate + incoming priority +
  one-generation cap + a converged pure test).
- **[minor] Vacuous e2e signals.** → Stated per-signal whether it adds coverage;
  `data-coalesced/discarded` driven by a deterministic teleport or demoted.

**SCOPE/SEAMS (2 blockers, 2 majors, 2 minors):**
- **[blocker] Same-level coast seams are NOT free.** → The spine rationale was
  rewritten (geometric-free + avoids-worse-cross-level, NOT coast-free); coast-seam
  screenshot made load-bearing; contingent ST-1.5 Rust stitch added; "identical
  coast chains" softened to "hairline, not bit-identical."
- **[blocker] Nested-transition cap blow.** → Converged with the PERF livelock
  (above).
- **[major] Antimeridian wrap-vs-fade contradiction.** → **CLAMP** longitude
  (don't wrap); deleted the false "fade mitigates the loaded case" claim.
- **[major] River "identically" over-claimed.** → Softened to "~85% presence
  agreement, sub-cell jogs."
- **[minor] Stray `S3` increment ref.** → All increment refs are `ST-…`; grep-clean.
- **[minor] ST-3 had no pure floor.** → Added the sliding-window pure unit.

**DETERMINISM/GOLDEN (3 majors, 1 minor):**
- **[major] Texture bleed would move the golden via `region`.** → Bleed is a
  render-time wider-viewBox ARG, never mutates serialized `region`; hard line
  added.
- **[major] Stale lens/year cached patches.** → `style` in the key + echoed +
  verify-before-apply; time-slider scoped out of the streaming globe.
- **[major] False-green seam test.** → Geometry unit demoted to a regression
  guard; contingent off-GPU coast-deviation unit (feasible-if); coast screenshot
  load-bearing.
- **[minor] SECTOR_CELLS rule too narrow.** → Broadened to ALL `RefineParams`
  fields as a pure function of the key.

Plus the honest framing folded throughout: **"golden-neutral" means the
determinism harness is blind to this layer, NOT that it is verified** — so the
named screenshots + pure units are load-bearing infrastructure.

---

## Recommended first streaming increment

**Build ST-1 (Multi-patch cache at the drilled fixed level) first.** Why:

1. **User-visible value on increment one, not plumbing.** The very first ST-1
   build shows several in-view patches tiled over the base sphere — the user
   *sees* "more than one detailed plate at once," which neither the foundation
   1b (one patch) nor the shipped 2D drill delivers. This is the anti-Refinery
   test: it ships a capability, not an engine.
2. **It settles the hardest correctness questions in the smallest unit** — the
   pure selector (`desiredSectors`: horizon cull, frustum-corner footprint,
   clamp-to-K, **longitude clamp**) and the bounded cache (`reconcile`: the cap,
   LRU, dispose) are both pure and false-green-proofed *before* any altitude
   snap, transition, follow, or scheduler complexity is added on top.
3. **It is the only increment that can EXPOSE the load-bearing coast seam.** The
   two-adjacent-patch coast screenshot only exists once a multi-patch set
   renders — ST-1 is the increment that decides whether the contingent ST-1.5
   Rust stitch is needed. Building anything else first defers the one
   architecture-validating measurement.

**ST-1 TDD spec (write the failing tests first):**
- **Pure Vitest (the false-green-proof floor):**
  - `desiredSectors` over a known oblique cam returns the expected sector swath,
    each sector cross-checked against `uvToWorld` corner anchors (catches a 1−v
    flip); a far-hemisphere sector is ABSENT (mutation: drop the horizon cull →
    it appears); an antimeridian cam CLAMPS sx (mutation: wrap → both ±180°
    columns appear); a high-altitude whole-disk cam covers the full
    horizon-visible hemisphere (mutation: drop the coarse-enumeration arm → the
    limb is absent); `|result| ≤ K`.
  - `reconcile(live, desired, footprint, cfg)` respects the count + byte caps
    (feed 40 desired → live ≤ 24 AND byte-sum ≤ cap; mutation `cap-1` → an extra
    eviction fires); LRU evicts oldest out-of-view non-pinned.
  - `sectorPatchParams` ABSOLUTE-anchored to `uvToWorld` corners + E/W shared-edge
    coincidence + equal `segW` (a geometry regression guard — explicitly NOT
    coast-seam coverage).
- **E2e (SwiftShader, no pixels):** after a drill, `data-live-patches ≥ 2 AND
  ≤ 24` (the `≥ 2` is the multi-patch false-green guard); after an orbit drag,
  `data-evictions` INCREASES while `data-live-patches` stays ≤ 24 and
  `data-rendered=1` (base fallback never blank). Re-run the existing 2D drill +
  globe lens/year e2e **green** (the `refineTile` branch must not regress the
  legacy `sector`-slot path).
- **Two named, load-bearing SwiftShader screenshots:** (1) patch placement —
  a known sector lands where `uvToWorld` predicts, not flipped; (2) coast-seam
  severity — the two-adjacent-patch coastline boundary, eyeballed for crack
  severity (decides ST-1.5).
- **Est:** 3 days.

---

## Recommended SCOPE REDUCTION (honesty over completeness — read this)

Two independent red-team lenses concluded the same thing, and I **agree and
recommend it to the user**: the single-worker + main-thread-rasterize substrate
**physically cannot** sustain "detail follows the camera everywhere during fast
motion." That is not a tuning gap — it is ~100 ms serialized refine + a
main-thread decode+upload hitch per tile, ~10 patches/s, on the paint thread,
all verified in the project's own code.

**Recommended v1 = "detail follows on SETTLE":** ship **ST-1 (multi-patch fixed
level)** and **ST-2 (altitude→level snap)** — the genuinely new, demonstrable
value — plus **ST-3 settle-fill** and **ST-4 scheduler**. While the camera moves,
the user flies over the always-present coarse base sphere (never blank); the
in-view set fills in over ~1-2 s once the camera settles. **Defer ST-5
(continuous-follow + prefetch) behind a one-day OffscreenCanvas/`createImageBitmap`
spike** — only if off-main-thread SVG rasterize works under headless SwiftShader
(unproven cross-engine) does true continuous-follow become physically feasible.
ST-6 (worker pool) stays profiling-gated.

This is **the user's target preserved and delivered incrementally**, not
abandoned: settle-fill is the same Google-Earth experience minus instant
fast-pan detail; the fast-pan piece is gated behind the one unlock that makes it
real, with the same evidence-gated discipline that (correctly) killed the
Refinery and gates the worker pool. Building ST-3/ST-5 "continuous-follow
everywhere" on a single-worker, main-thread-rasterize substrate would be exactly
the "infrastructure ahead of shippable value" shape this project rejects — it
would promise an experience the engine cannot keep.

---

## Open questions for the user

1. **(BLOCKING — two preconditions) Confirm 1a ships as the FREE-FLY variant AND
   1b's patch overlay is built first.** 1a-as-locked is pivot-orbit-one-region,
   incompatible with the pan-across-surface streaming requires — streaming has no
   valid camera input without the free-fly variant (fixed globe, moving camera,
   pivot identity) + the `getCameraState()` hook. Separately, **globe.ts today
   has only the base `SphereGeometry(1,64,48)` — 1b's `showPatch`/`setPatchTexture`
   overlay is the 1b SPEC, not shipped code** — every patch-build specific here
   presupposes 1b has landed. Confirm both, or an extra retarget/overlay increment
   precedes ST-1.

2. **(SCOPE — the big one) Accept the "detail-follows-on-SETTLE" v1?** The
   single-worker + main-thread-rasterize ceiling makes continuous-follow-during-
   fast-motion physically impossible on this substrate (verified). Recommendation:
   YES — ship ST-1…ST-4 settle-fill; gate continuous-follow + prefetch (ST-5)
   behind the OffscreenCanvas spike. See Recommended scope reduction.

3. **OffscreenCanvas spike (gates ST-5):** authorize a one-day spike to confirm
   `createImageBitmap`-from-SVG-blob rasterize works in a worker under headless
   SwiftShader? It is the unlock for continuous-follow + prefetch and is not
   reliably supported cross-engine.

4. **Single global level under steep oblique tilt** — over-details the far swath,
   under-details the near. Per-swath level reintroduces cross-level adjacency +
   the rejected skirts. Recommendation: accept for v1.

5. **Lens-on-globe consistency** — `style` is in the cache key + echoed for
   verify-before-apply, so a lens toggle is correct (stale patches are discarded,
   not served). Should a toggle **re-refine** all live patches (~K worker hops) or
   only re-style subsequently-loaded ones (cheaper)? Recommendation: defer
   re-refine-all to a follow-on.

6. **Coast-seam stitch (gates ST-1.5)** — accept ST-1's coast-seam screenshot as
   the trigger: build the Rust seam-coast reconciliation only if the shot shows a
   visible crack. Recommendation: yes, contingent — don't pre-build it.

7. **The tuning knobs** — `MAX_LIVE_PATCHES=24`, 1024px long-edge, ~96 MB,
   `levelForAltitude` thresholds, the settle-ε and pump-N (`tuning_log.md`).
   Confirm 24/1024/96 MB as starting points, or start tighter (16/768/57 MB)?

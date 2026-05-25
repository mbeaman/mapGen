# ADR 0001 — Multi-scale atlas: navigation, rendering & seam model

- **Status:** Accepted (design anchor for the Phase-7 refinement framework)
- **Date:** 2026-05-24
- **Context tasks:** Roadmap 6.3; `docs/BACKLOG.md` → "Seamless inter-scale
  navigation"; designs `docs/ROADMAP.md` Phase 7 (the nested refinement
  framework) and Phase 8.4 (interactive navigation).

## Context

The committed goal is a **multi-scale atlas**: one deterministic world viewable
at every zoom (planet → continental → local urban/rural), generated **on demand**
by nested refinement where coarsening a child sector reproduces its parent. Two
hard constraints shape every decision below and separate this project from a
conventional web map:

1. **Sectors cost *seconds*, not milliseconds.** A sector is a full
   generate-pipeline run at finer resolution — Voronoi mesh, erosion, hydrology,
   climate, society. This is the dominant constraint: it rules out anything that
   assumes cheap, continuous tile fetches.
2. **The output is an ornate, hand-drawn-style SVG**, and an ornate atlas is
   traditionally a set of **discrete plates with inset cross-references**, not a
   continuous slippy surface. Forcing Google-Maps continuous zoom would fight
   both the cost model and the aesthetic.

Determinism is already in hand: `child_seed = blake3(parent_seed, level,
sector_id)` (the Phase-7 contract) makes every sector a pure function of its
coordinates, regardless of the path taken to reach it. fmath keeps it
byte-identical native↔wasm (pending the 6.2 cross-platform golden).

This ADR answers the eight research questions from the backlog brief and ends in
the concrete model the framework + frontend will build to.

## Decision (summary)

**A discrete, atlas-plate drill-in (overview+detail) with progressive
coarse-first rendering, vector SVG rendered per sector on demand, deterministic
per-sector seeds, Töpfer/Visvalingam generalisation per level, and halo-cell
boundary stitching.** We explicitly choose the *discrete* model over continuous
slippy zoom because seconds-per-sector generation and the ornate aesthetic both
demand it. A raster tile pyramid is deferred until in-level pan performance
proves it necessary.

---

## The eight questions

### 1. Interaction model → **discrete drill-in + overview+detail**

Studied: Shneiderman's mantra (overview first → zoom & filter → details on
demand); focus+context (fisheye, DOITrees); Google/Mapbox slippy zoom; Dwarf
Fortress's world → region → embark → local-map drill-in; 4X strategic-vs-tactical
view swaps.

**Decision.** Discrete levels the user *drills into* (click a continental sector
→ generate & open the regional plate; click a settlement → the urban plate),
with a persistent **overview+detail** frame (a minimap of the parent showing the
current sector, plus breadcrumb up-navigation). This matches DF's proven model
and the ornate-plate aesthetic, and it is the only model compatible with
seconds-per-sector generation — continuous geometric zoom needs sub-frame tile
readiness we cannot provide. Focus+context (fisheye) is rejected as
aesthetically wrong for a hand-drawn map.

### 2. LOD transition / anti-popping → **progressive coarse-first**

Studied: terrain LOD geomorphing (geometric clipmaps, chunked LOD, geomipmapping/
ROAM); Mapbox GL vector-tile cross-fade; the CSS-scale-then-swap trick.

**Decision.** On drill-in, **immediately** upscale the parent sector's already-
rendered SVG as a blurred placeholder (CSS transform), then **cross-fade** to the
refined child SVG when generation completes. No true geomorphing between
continuous levels (we have none). The placeholder makes the seconds-long
generation feel responsive; the cross-fade hides the pop. This is the single most
important UX mechanism given the cost model.

### 3. Cartographic generalisation → **Töpfer's Radical Law + operator set**

Studied: Töpfer's Radical Law (feature count ∝ √(scale ratio)); the generalisation
operators (selection, simplification, aggregation, displacement, typification);
Douglas–Peucker and Visvalingam–Whyatt line simplification; Mapbox GL
zoom-expression stylesheets.

**Decision.** Each level has a **scale-dependent stylesheet** controlling which
feature classes appear (a continent shows mountain *ranges*; a district shows
individual hills) and a feature-count budget per Töpfer. Coastlines and rivers
are simplified with **Visvalingam–Whyatt** (better area-preservation than
Douglas–Peucker for natural lines). Settlements use **selection by rank**
(Christaller tier) at coarse levels, all of them at fine levels. This lives in
the render layer as per-level style parameters, not new generation.

### 4. Labels across zoom → **scale-rank priority + per-level Imhof SA**

Studied: Mapbox GL label collision + fade; Imhof's label rules (already used for
our SA placement); priority/scale-rank labelling.

**Decision.** Re-run the existing **Imhof simulated-annealing label placement per
level** (labels are cheap relative to generation), with a **scale-rank priority**
(realm names at continental scale; town/feature names only once drilled in) and
collision-driven declutter. Labels **fade** on the drill-in cross-fade. No new
algorithm — the SA placer we have is reused per plate.

### 5. Spatial-seam consistency → **halo cells + shared-edge contract**

Studied: constrained boundary generation, ghost/halo cells, Wang/corner tiles,
blue-noise tile stitching, marching-squares contour continuity across borders.

**Decision.** This is the **generation half**, owned by the Phase-7 boundary
contract; this ADR fixes the *rendering* expectations. A child sector is
generated with a one-cell **halo** of its parent's edge values (elevation
envelope, river entry/exit points, coastline crossings, biome, settlement
positions) as fixed Dirichlet constraints, so adjacent independently-generated
children agree on their shared edge by construction. Render-side: coastline/river
simplification (Q3) must be applied **identically** at a shared border (same
Visvalingam tolerance keyed to level, seeded deterministically) so the two plates'
lines meet. Cross-reference: framework boundary-condition contract.

### 6. Streaming / prefetch → **background WebWorker generation queue**

Studied: slippy-map tile prefetch (adjacent + next-zoom), velocity-predictive
loading, background generation queues, progressive coarse-first rendering.

**Decision.** A **background WebWorker generation queue** (extending the existing
worker that already runs generation off the main thread). On opening a plate,
**prefetch** the most likely drill-in targets — the highest-rank settlements /
most prominent features in view — at idle priority, so a click on them is
instant. Because generation is expensive, prefetch is **conservative and
rank-driven**, not the speculative adjacent-tile flood of a millisecond slippy
map. Coarse-first (Q2) covers the unprefetched case.

### 7. Vector vs. raster pipeline → **vector-per-sector on demand (hybrid deferred)**

Studied: vector tiles (MVT) vs. raster tile pyramids; hybrid approaches;
in-browser SVG performance ceilings.

**Decision.** **Render vector SVG per sector on demand** at the active focus —
each plate is one SVG, exactly as today, just at a finer level. We do **not** bake
a raster tile pyramid for the MVP, because the discrete drill-in model (Q1) means
we never pan continuously across a huge vector surface — the SVG performance
ceiling is hit by *continuous* slippy maps, not by viewing one plate at a time. A
raster pyramid (resvg → PNG tiles) is **deferred** with a clear trigger: in-level
pan/zoom of a single ornate plate becomes janky in the browser.

### 8. Determinism of the journey → **path-independent seeds, snap fractional zoom**

**Decision.** The framework's `child_seed = blake3(parent_seed, level, sector_id)`
already guarantees a sector is identical regardless of the navigation path that
reached it (no hidden state, no accumulation). Fractional/continuous zoom is **not
supported** — the discrete model snaps to the nearest generated level and uses the
coarse-first upscale (Q2) for the in-between. This sidesteps "generate a true
intermediate level" entirely and keeps every view reproducible from its
coordinates alone. The 6.2 cross-platform golden should be extended to pin a
child sector's hash once the framework lands.

---

## Consequences

**Positive.**
- The model is *achievable* under seconds-per-sector generation — the discrete
  drill-in + coarse-first placeholder is the only interaction design that doesn't
  fight the cost model, and it matches the ornate-plate aesthetic.
- Maximal reuse: the existing SVG renderer, Imhof SA labeller, and generation
  WebWorker are reused per level; the new work is the boundary contract,
  per-level stylesheets, simplification, and the drill-in/coarse-first UX.
- Every view is deterministic and regenerable from coordinates; nothing is
  persisted at local resolution.

**Negative / accepted trade-offs.**
- No continuous "infinite zoom" feel — a deliberate choice, not a limitation to
  apologise for. Movement is plate-to-plate, like turning atlas pages.
- Prefetch is conservative, so an un-prefetched drill-in shows the coarse
  placeholder for a few seconds. Acceptable; the cross-fade makes it feel
  intentional.
- The render layer gains per-level generalisation complexity (stylesheets,
  Visvalingam, scale-rank selection) that must stay consistent across shared
  seams (Q5) — the main rendering risk to test.

**Implications for Phase 7 (the framework).**
- The boundary-condition contract must expose the halo (Q5): a child is generated
  given its parent's edge cells as fixed constraints.
- Generation must be parameterisable by `(level, sector_bounds)` and remain
  stateless/on-demand (no global world assumption) — the pipeline-parameterisation
  work the framework audit flagged.
- The coarsening-contract property test (deferred Track 2.4) is the framework's
  acceptance criterion and should be TDD'd alongside it.

**Implications for Phase 8.4 (navigation impl).**
- Frontend: minimap/breadcrumb overview+detail frame; drill-in click handler;
  coarse-first upscale + cross-fade; rank-driven background prefetch queue.
- Render: per-level stylesheet + Visvalingam simplification + scale-rank label
  priority; raster pyramid only if in-level pan proves janky.

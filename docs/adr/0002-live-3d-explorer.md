# ADR 0002 — Live 3D explorer (mapgen-viewer crate)

- **Status:** Accepted (architecture amendment; triggers ARCHITECTURE.md unlock)
- **Date:** 2026-05-30
- **Branch:** `claude/wgpu-3d-explorer-1du5B`
- **Context tasks:** New work track; not previously in `docs/BACKLOG.md`. ADR
  0001 (multi-scale navigation) is the *static* sibling — discrete plates,
  vector SVG per sector. This ADR introduces the *interactive* sibling — a
  real-time 3D renderer over the same world data.

## Context

The committed product to date is a deterministic world generator producing
**SVG plates** (`mapgen-render`, `ornate_antique` style) viewed in a browser as a
drill-in atlas (ADR 0001). The SVG renderer is the brand: parchment, roughr-
perturbed coastlines, Tolkien mountains, Cinzel/IM Fell typography, Imhof
labels. It is the artifact a curious reader holds.

The user's directive (2026-05-30) is to add a **second render path**: a
real-time, fly-over, 3D-tilt **live explorer** rendered from the same
`WorldData`, with the explicit goal of preserving the ornate aesthetic in the
live view (not adopting a separate "video-game look"). The static SVG path is
NOT being deprecated — `mapgen-render` remains the artifact-of-record for
export, atlas, and screenshots.

This ADR amends the LOCKED architecture (ARCHITECTURE.md §1) to admit one
new workspace crate, `mapgen-viewer`, and fixes the architectural commitments
that protect the existing pipeline's invariants (determinism, seam-pinning,
schema, native↔wasm parity) while the new path is built.

The user explicitly chose this path after seeing three cheaper alternatives
(CSS-3D-tilt the existing SVG; WebGL terrain + SVG label overlay hybrid; defer
in favour of planet-zoom increment 2) and the steelman against doing it now
(displaces queued planet-zoom work; SVG aesthetic took most of Phase 3e to nail
and recreating it in shaders is a months-long research effort). The decision
is recorded as deliberate.

## Decision (summary)

**Add `crates/mapgen-viewer` to the workspace as a real-time 3D renderer over
`WorldData`. wgpu + winit substrate; single crate, both native and web
targets; consumes `mapgen-world` directly (no changes to its API); leaves
`mapgen-render` untouched and authoritative for SVG/PNG output; staged
delivery (0 → 5) with each stage independently shippable.** The new crate
does NOT modify any existing crate's public API for Stage 0; later stages may
expose read-only adapters from `mapgen-world` (e.g. a vertex-stream over the
existing mesh data) but never feed data backward.

---

## The nine questions

### 1. Why a new crate, not a new `Style` in `mapgen-render`?

Studied: the existing `Style` enum (`Greyscale | Biomes | Cultures |
OrnateAntique | Planet`) and the `render(world, style) -> String` boundary —
all SVG, all pure-function, all artifact-shaped.

**Decision.** A new crate, not a new `Style`. The reasons are structural,
not stylistic: (1) the output is a live framebuffer, not a `String`; (2) it
owns mutable state (camera, input, GPU resources, scene graph) inconsistent
with a pure-function renderer; (3) its dependency footprint (`wgpu`, `winit`,
`bytemuck`, `glam`, optional `wasm-bindgen`) is large and orthogonal to
`mapgen-render`'s text-output concerns; (4) keeping the boundary explicit
prevents future contributors from accidentally coupling the SVG pipeline to
GPU state. The `Style` enum is for *static* render variants over the same
output shape — that's a meaningful axis we preserve.

### 2. Substrate: wgpu, or WebGL via Three.js / a JS library?

Studied: Three.js / Babylon.js (JS) vs raw WebGL via wasm vs wgpu (Rust,
WebGPU-and-WebGL2-backed).

**Decision.** **wgpu.** The world generator is already Rust; introducing a JS
3D engine would force a second data-marshalling boundary across the
wasm/JS edge per frame (camera state, scene mutations, etc.) — expensive at
60 Hz and impossible to keep deterministic. wgpu compiles for both native
(`wgpu` on Vulkan / Metal / DX12) and web (`wgpu` on WebGPU primary, WebGL2
fallback) from one Rust source, sharing the entire renderer including
shaders (WGSL). The trade-off is a heavier wasm bundle (~1MB before our
code) and slower compile times — both acceptable given the architectural
unification this buys.

### 3. Native + web targets — one crate or split?

Studied: single-crate with `cfg(target_arch = "wasm32")` entry-point split,
versus separate `mapgen-viewer-native` + `mapgen-viewer-wasm` wrappers.

**Decision.** **One crate.** All renderer logic — camera, scene graph,
mesh assembly, render passes, shaders — lives in `mapgen-viewer/src/lib.rs`
target-agnostic. Two entry-point shells gate on `cfg(target_arch)`: a
`src/bin/native.rs` binary running a winit event loop on native; a wasm-
bindgen `start()` export on web binding to a `<canvas>`. Shared code is the
99% case; the entry shells are <100 LOC each. This keeps shader edits
(WGSL) and pipeline changes a single-edit affair.

### 4. Where in the dependency DAG?

Studied: the existing edges — `core ← geom ← world ← history ← render`;
`lore` depends on `core + history`; `cli` depends on everything; `wasm`
depends on everything **except `lore`**.

**Decision.** `mapgen-viewer` depends on `mapgen-world` (and transitively
`core` + `geom` + `history`). It does **not** depend on `mapgen-render`
(no shared output format) and `mapgen-render` does **not** depend on
`mapgen-viewer`. It does **not** depend on `mapgen-lore` (no narration in
the live view at MVP). `mapgen-cli` may grow a `mapgen viewer` subcommand
(native window over a generated world) — adds a `cli → viewer` edge,
consistent with `cli`'s existing "depends on everything" role.
`mapgen-wasm` does **not** gain a dep — the web viewer entry-point lives
in `mapgen-viewer` itself (gated by `cfg(target_arch = "wasm32")`) and
ships as its own wasm artifact, distinct from `mapgen-wasm`'s SVG-shaped
façade.

### 5. Relationship to scale.rs (Phase-7 refinement framework)?

Studied: `crates/mapgen-world/src/scale.rs` (`Sector` quadtree, `refine_sector`,
`pin_edges_to_shared`); the discrete-drill-in model of ADR 0001.

**Decision.** The viewer reads from the **same `WorldData`** the SVG path
reads — including parent + per-sector data when sectors have been refined.
At Stage 0 we render a single world at level 0; at Stage 2 we add **GPU
LOD streaming** that consumes the same `refine_sector` machinery as the
SVG drill-in — same seeds, same boundary contract, same `pin_edges_to_shared`
output. Critically: the viewer **must not introduce a parallel refinement
pipeline**. If a feature (rivers, borders, towns) wants different LOD
behaviour in 3D than in SVG, that's a *render-side* difference (mesh
density, billboard count) over the same underlying world data. Seam-
pinning's elevation MAD < 0.04 contract remains the single source of truth
for tile boundaries.

### 6. Camera model for 3D?

Studied: orbit (Maya/CAD), free-look (FPS), godview (Civ/AoE), Mapbox-style
constrained pitch-bear-zoom.

**Decision.** **Orbit-with-pitch around a focus point**, constrained pitch
(0° = top-down map view, ~70° = oblique aerial), zoom collapses focus
distance, pan slides the focus point across the world surface. This matches
the cartographic mental model (you're looking *down at* a place, never
inside it) and gives a clean Stage-2 transition from a top-down "map" feel
to a Stage-3 "fly over the terrain" feel by tilting. Free-look is out — it
loses orientation in a hand-drawn aesthetic.

### 7. Aesthetic: how do we preserve ornate in a real-time rasterizer?

Studied: Disco Elysium (painted illustration in real-time), Old World
(period-painting aesthetic), *Inkulinati* (medieval-manuscript style),
Mapbox's hill-shading + texture-atlas hybrid.

**Decision.** **Ornate-in-shaders, staged.** Stage 0 renders flat-shaded
biome colours (no aesthetic claim). Stage 5 introduces the ornate
treatment in shader form: a paper background as a screen-space texture
(with subtle parallax); coastline pen-jitter as fragment-SDF noise (the
roughr aesthetic, regenerated procedurally per-frame); hatched mountain
shading as procedural texture keyed to LOD; instanced billboard glyphs
for trees / settlements / mountains (deterministic placement seeded from
cell data, so the same world always scatters the same trees in the same
places). Cinzel/IM Fell text in 3D space lands in Stage 4 as SDF-bake
billboards. We accept the live view will look *worse* than the SVG export
until Stage 5 is mature — and possibly indefinitely. The SVG renderer
remains the artifact-of-record for screenshots.

### 8. Labels in 3D — algorithm?

Studied: Mapbox GL label collision in 3D space; SDF text rendering; the
existing Imhof SA label placement in `mapgen-render`.

**Decision.** **Camera-facing SDF billboards with tier-based fade and
greedy collision.** SDF atlas baked from the existing font stack
(Cinzel, IM Fell English, EB Garamond). Each label has a world-space
anchor (settlement / feature centroid); the billboard turns to face the
camera each frame; tier determines minimum zoom (capitals visible at all
zooms, hamlets only at close zoom). Collision is **greedy screen-space**
(sort by priority, accept if no overlap with already-accepted labels) —
explicitly NOT Imhof SA, which is too expensive per-frame. We accept
"good" collision, not Mapbox-grade. Curved river/range labels are
deferred to Stage 4.5 (after MVP labels work). Critically: the SVG
renderer's Imhof SA labelling remains unchanged — it owns the
artifact-quality bar.

### 9. Non-goals (what this ADR does NOT authorize).

- **`mapgen-render` is not deprecated.** The SVG path remains the
  artifact-of-record for export, atlas, sharing, and screenshots. Any
  user-facing claim that "the live view replaces the SVG" is out of
  scope for this ADR and would require a separate one.
- **No new world-data fields for rendering.** The viewer reads existing
  `MeshData`, `TerrainData`, `HydrologyData`, `ClimateData`,
  `SocietyData`, `entities`, `history`, etc. If a Stage discovers it
  *needs* a new field (e.g. per-cell normal vectors precomputed), that's
  a separate ADR + schema bump.
- **No changes to the determinism contract.** All randomness in the
  viewer (procedural billboard scatter, pen-jitter noise) seeds from
  cell IDs / world seed, not from frame counters. The same world always
  renders to a frame that is *structurally* identical (modulo camera).
- **No FFI to JS for rendering logic.** Web target uses wgpu's WebGPU /
  WebGL2 backend through wasm-bindgen; the JS side owns the canvas
  element and input event forwarding, nothing else.
- **No new Anthropic / Claude integration.** Live narration in the
  viewer is out of scope (and `mapgen-viewer` does not depend on
  `mapgen-lore`).

---

## Consequences

**Positive.**
- One renderer, two targets — native debug iteration (RenderDoc,
  fast rebuild) and web ship target share 100% of pipeline + shader
  code.
- Architectural symmetry with `mapgen-render`: parallel render paths,
  same input data, different output format. The DAG stays clean.
- The SVG renderer is protected — no shared mutable state, no
  feedback edges, no schema coupling.
- Seam-pinning, schema, determinism, native↔wasm parity all remain
  the single sources of truth they already are.

**Negative / accepted trade-offs.**
- Long payoff curve. Stage 0 (substrate hello-world) is days; Stage 5
  (ornate-in-shaders parity-or-near) is months. Until Stage 5 the
  live view looks worse than the SVG export.
- Wasm bundle grows. wgpu adds ~1MB. The existing 3 MB risk budget
  (ARCHITECTURE.md §8.2) needs re-anchoring; this is an explicit
  follow-up.
- Build-time and toolchain complexity doubles. `cargo build` (native)
  vs `wasm-pack build` (web) both need to stay green; CI needs a
  third matrix entry for the viewer's wasm build.
- Architecture is no longer single-rendering-path. Two paths means
  two sets of style choices to keep coherent if the project later
  wants them to feel like "the same product."

**Implications for the immediate roadmap.**
- The planet-zoom-out increment 2 work (BACKLOG "Up next") is
  **deferred** while this branch is active, OR runs in parallel on
  `claude/fantasy-map-generator-1du5B`. The two branches do not
  conflict; planet-zoom-2 is SVG-pipeline work, viewer is a separate
  crate.
- A new perf budget row for the viewer lands in Stage 0c (TBD targets
  — native ≥60 fps at 4k cells, web ≥30 fps).
- `just check` gains `cargo check -p mapgen-viewer` (native) at
  Stage 0; `just check` gains the wasm32 build at Stage 0b.

**Implications for future ADRs.**
- If the viewer's LOD streaming reveals scale.rs limitations (e.g.
  needs richer halo data, sub-second refinement), that's a 0003 ADR.
- If the ornate-in-shaders work succeeds enough to consider
  deprecating the SVG path, that's a 0004 ADR with its own steelman.

---

## Implementation status (2026-05-30)

Branch created (`claude/wgpu-3d-explorer-1du5B`); ADR written; no code yet.

**Stage 0 plan** (next work, gated on this ADR landing):

- **0a. Substrate hello-world (native only).** Add `crates/mapgen-viewer`
  to the workspace; `src/lib.rs` empty; `src/bin/native.rs` opens a winit
  window, initializes wgpu (instance, adapter, device, queue, surface),
  clears to a colour, closes cleanly on Esc. Verifies wgpu + winit
  toolchain on this box. ~150 LOC. Exit: `cargo run -p mapgen-viewer
  --bin native` opens a window.
- **0b. Web target.** Add `[lib] crate-type = ["cdylib"]` gated on
  `wasm32`; a `start()` wasm-bindgen export taking a canvas element; same
  wgpu init through the WebGPU/WebGL2 backend. `wasm-pack build` of the
  viewer crate succeeds; serving the wasm in `web/` (under a new
  `web/viewer.html` entry, separate from the SVG frontend) shows the
  same cleared canvas. Exit: web canvas clears.
- **0c. Render the world (flat).** Native binary takes
  `--seed <u64> --cells <n>`; generates a world via
  `mapgen_world::generate_full_with`; converts cells to a vertex
  buffer (biome colour per cell); orbit camera (glam mat4) with pan +
  zoom + tilt. No elevation extrusion yet — flat, viewed from above
  by default, tiltable. Exit: a recognizable biome-coloured continent
  rendered in a 3D window, tiltable to oblique view.

**Stages 1–5** match the original roadmap (terrain extrusion + lighting;
LOD streaming; vector overlays; labels; ornate-in-shaders). Each is
independently shippable to the branch and ADR-amendable if it reveals
something this ADR got wrong.

**Open follow-ups** (track in `docs/TASKS.md` when scaffolded):
- wgpu + winit dep version pinning (verify against late-2025 stable).
- Determinism of procedural billboard scatter (seed plumbing).
- WebGPU vs WebGL2 backend matrix — what works where in browsers.
- Stage 5 ornate aesthetic taste reference (analogous to the
  long-deferred `docs/target_aesthetic.svg`).

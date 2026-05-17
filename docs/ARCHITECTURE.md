# Fantasy Map Generator — Implementation Plan

> **Status: LOCKED 2026-05-17.** Reviewed by one constructive reviewer + two
> devil's advocates (technical, product/scope) on 2026-05-17. Both DAs
> converged on RADICAL SCOPE CUT against the originally proposed §5.5
> (Refinery + Time-Resolved Simulation). This document adopts their
> prescription: keep the conceptual content of §5.5 as design reference,
> defer the Refinery to Phase 6+, replace Phase 2.5 with a one-day
> property-tests + sweep-CLI pass, proceed directly to Phase 3.
>
> Unlocking requires explicit user approval and a triggering signal —
> e.g., the first ornate render reveals realism gaps that targeted
> property tests can't catch.

## Context

Greenfield Rust project at `/home/user/mapGen` on branch `claude/fantasy-map-generator-1du5B`. The user wants a fantasy map generator that produces lore-rich worlds with cartography that looks like it came from "an exotic map shop." This plan covers the **vertical-slice MVP**: one continent, 6–12 nations, 500 years of simulated history, and one ornately rendered SVG — proving the full pipeline end-to-end before expanding.

The architecture rests on five pillars:

- **Geography pipeline** — Voronoi mesh → plate tectonics → noise + erosion → hydrology → ocean currents → climate (3-cell, seasonal) → Köppen-Geiger → biomes. **Default = Earth-faithful baseline**; we use established models (3-cell atmospheric circulation, gyre/upwelling ocean currents, Köppen-Geiger climate, USDA soil orders, GDH1 oceanic-crust subsidence). Techniques from Azgaar's FMG, mewo2, Red Blob Games, Songs of the Eons.
- **Cultures pipeline (new)** — `biomes → cultures → polities → history`. The `cultures` stage decides *who lives where*: for each land cell, score the fitness of each candidate race-archetype against (biome, temperature, elevation, water-proximity, soil), assign primary culture via weighted Voronoi. Cultures carry: race (with sub-variant), language, religion, alignment (2D continuous law/chaos × good/evil), tech profile (era tier + per-axis skill in metallurgy/agriculture/naval/military/arcane), magic style, settlement icon, architecture vocabulary, diplomatic pattern, habitat preference function. Religions attach to cultures (not polities) and can spread across borders. The `polities` stage draws political borders on top of cultural ones; one polity can span multiple cultures (driving Mearsheimer-loop tension). Genre tropes (specific D&D races, specific pantheons) live in **data tables** — individual worlds can have no elves, three competing human cultures, or a giants-only post-cataclysm state. Robust universals (Christaller central places, Zipf rank-size, Glorantha's "every cult must have a present-day reason") live in engine code.
- **History engine** — Deterministic agent-based simulation produces a typed, append-only event log. Claude (via the Anthropic API) is the narrator only, never the canon authority. Strict NER-validated grounded retrieval prevents hallucinated names. Patterns drawn from Dwarf Fortress, Caves of Qud, CK3, Turchin's secular cycles, Mearsheimer's offensive realism, Tolkien/Sanderson/Martin worldbuilding. The six causal loops (Turchin, Khaldun, Mearsheimer, Succession, Schism, Hero) read culture fields as their per-actor inputs.
- **LorePatch overlay (foundational)** — Every scientific stage queries a `Vec<LorePatch>` at the end of its computation and applies per-cell deltas / multipliers / overrides with smoothstep falloff at polygon edges. Patches carry a `cause_event: Option<EventId>` so the history sim can answer "why is this here?" via causal lineage (Tolkien-style deep memory). Magic, cataclysms, divine intervention, and **land impact** (deforestation around human cities, mine networks around dwarf holds, sacred groves around elven sites, blighted earth around orc settlements) all deviate from physics *explicitly*, never accidentally. Cultures emit patches as a side-effect of long settlement (retroactive land memory). Cataclysm events emit patches that wipe out civilizations and leave **artifacts**: megalithic monuments, ruined cities, cataclysm scars, petrified armies, ancient roads, demon prisons. Mythic-ages model (3-5 ages per world, each ending in a Cataclysm event) provides clean narrative breakpoints over Civ-style continuous succession. Patterns synthesized from Dwarf Fortress evil regions, Eberron manifest zones, Mage reality verges, Glorantha runes, Sanderson highstorms, Numenera's Ninth World.
- **Ornate rendering** — Hand-authored SVG with `roughr` (Rust port of Rough.js, v0.12) for pen-jitter primitives, Tolkien triangular mountain icons, biome-aware tree scatter, Cinzel/IM Fell/EB Garamond typography, parchment + compass + cartouche + vignette overlay. Settlement glyphs derive from `Culture.settlement` × `Culture.architecture` (tower vs castle vs hall vs spire vs longhouse vs treehouse, gothic vs classical vs organic vs megalithic). Renderer is a pure function of world data; style modules are pluggable so the same world can be re-rendered in different styles without re-simulation.

Tech stack is locked: **Rust + WASM** for the simulation/render core, with a thin **vanilla TypeScript + Vite** frontend driving it through a WebWorker. Claude integration is **native-only** behind a small sidecar server — API keys never reach the WASM build.

The intended outcome: a single artist/researcher can type a seed, get back a beautiful labeled continent with believable kingdoms, and click one button to read a Claude-written chronicle of a major war from that world's 500-year history.

---

## 1. Cargo Workspace

```
mapgen/
├── Cargo.toml                    # [workspace], resolver=2, workspace.dependencies
├── rust-toolchain.toml           # pin stable + clippy/rustfmt
├── .github/workflows/ci.yml      # fmt + clippy + test (native + wasm32)
└── crates/
    ├── mapgen-core/      # IDs, entities, event schema, RNG harness, libm shim
    ├── mapgen-geom/      # Voronoi mesh, Poisson-disk, Lloyd relaxation
    ├── mapgen-world/     # Geography pipeline (plates → biomes → society)
    ├── mapgen-history/   # Sim loops + append-only event log
    ├── mapgen-render/    # SVG renderer + roughr primitives + style modules
    ├── mapgen-lore/      # Claude integration (native-only, cfg-gated)
    ├── mapgen-cli/       # Native dev binary (clap)
    └── mapgen-wasm/      # wasm-bindgen façade (no lore dep)
```

**Dependency edges (DAG):** `core ← geom ← world ← history ← render`; `lore` depends on `core + history`; `cli` depends on everything; `wasm` depends on everything **except `lore`**.

**Targets:** all crates compile for `wasm32-unknown-unknown` except `mapgen-lore` (native-only, gated by `#[cfg(not(target_arch = "wasm32"))]` at the crate root and enforced in CI) and `mapgen-cli` (native).

**Pinned workspace dependencies:** `voronoice 0.2`, `noise 0.9`, `rand_chacha 0.3`, `indexmap 2`, `smallvec 1`, `serde 1`, `serde_json 1`, `ciborium` (golden hashes), `svg 0.17`, `roughr 0.6` (verify API at pin time; vendor if churn), `resvg 0.43` / `usvg 0.43` / `tiny-skia 0.11` (PNG export, native-only), `clap 4`, `axum 0.7` (sidecar), `wasm-bindgen 0.2`, `serde-wasm-bindgen 0.6`, `libm 0.2`, `reqwest 0.12` (native), `proptest 1`, `insta 1`.

---

## 2. MVP — Definition of Done

**IN:**
- Single continent, ~15k cells, deterministic from a `u64` seed.
- 6–12 nations (default 8), placed via Christaller k=4 hierarchy with capital + 2–4 secondary settlements each.
- 500 years of simulated history with **two of six** causal loops live: **Turchin demographic-fiscal** (rise/collapse) + **Mearsheimer power-balance** (Thucydides-trap wars). Other four (Khaldun frontier, succession, schism, hero/megabeast) stubbed as no-op trait impls so they can be slotted in later without touching callers.
- Event log with ≥200 events; ≥3 tagged `salience >= 0.8` (major wars/collapses).
- One ornate SVG (`ornate_antique` style): parchment, perturbed coast (4 offset ripples), mountain icons, forest scatter, river network (√flow width), labeled nations + capitals, compass rose, corner cartouche, vignette aging.
- One Claude round-trip: CLI subcommand renders one major-war event into a ~300-word chronicle, NER-validated against the world bible, persisted back as a `Work` entity.
- Persisted artifact: single gzipped JSON (`world.json.gz`) holding `WorldData + EventLog + Works`. Re-renderable / re-narratable without re-sim.
- Web frontend: one HTML page — seed input, "Generate", SVG preview, "Download SVG", "Narrate major event" (POSTs to native sidecar).

**OUT (deferred):**
- Multiple continents, archipelagos as a feature.
- Remaining four causal loops, cataclysm clock.
- Sound-change rules across language families (MVP uses one phonotactic generator + Markov fallback).
- Alternative render styles (`clean_modern`, `political`, `physical`).
- Religions/artifacts/dynasties beyond the minimum the two MVP loops need.
- PDF export.
- Save-game version migration.

---

## 3. Data Flow & Boundary Types

**Trunk type** (`mapgen-core::world::WorldData`):
```rust
pub struct WorldData {
    pub meta: WorldMeta,         // seed, schema_version, generated_at
    pub mesh: MeshData,          // cell centers, neighbors, coast flags
    pub terrain: TerrainData,    // heightmap, plate IDs
    pub hydrology: HydrologyData,// flow accum, river polylines, lakes
    pub climate: ClimateData,    // precip, temp, biome per cell
    pub society: SocietyData,    // settlements, roads, nation territories
    pub entities: EntityStore,   // IndexMap<EntityId, Entity>
    pub events: EventLog,        // Vec<Event>, append-only
    pub works: Vec<Work>,        // accepted Claude chronicles
}
```

**Event** (frozen schema, append-only):
```rust
pub struct Event {
    pub id: EventId, pub year: i32, pub kind: EventKind,
    pub actors: SmallVec<[EntityId; 4]>, pub patients: SmallVec<[EntityId; 4]>,
    pub location: Option<CellId>, pub cause_ids: SmallVec<[EventId; 4]>,
    pub salience: f32, pub casus_belli: Option<CasusBelli>,
    pub summary_canonical: String,
}
```

**Determinism harness** (`mapgen-core::rng`): one master seed feeds per-stage `ChaCha8Rng` sub-streams via `set_stream(stage as u64)` so re-rolling a stage cannot perturb downstream stages.
```rust
enum Stage { Mesh=1, Plates=2, Noise=3, Erosion=4, Hydro=5, Climate=6,
             Capitals=7, Hierarchy=8, Roads=9, Names=10, History=11, Render=12 }
```
All float math routes through `mapgen_core::fmath` (a `libm` shim) — never `f32::sin` directly — so native and WASM produce bit-identical results. `BTreeMap` / `indexmap` only; never `HashMap` for iteration.

**Persistence:** single gzipped JSON per world. SQLite buys nothing at MVP scale; a file tree adds path coupling for a solo researcher. The unserialized `EventLog` indices (by year, by actor) rebuild on load.

---

## 4. Phased Delivery (each phase is independently shippable)

**Phase 1 — Mesh + heightmap + flat SVG** (3–5 days)
Workspace skeleton, `mapgen-core` types stub, `mapgen-geom` mesh (voronoice + Poisson + Lloyd ×2), `mapgen-world` plates + noise heightmap (no erosion yet), `mapgen-render` minimal greyscale SVG.
Exit: `cargo run -p mapgen-cli -- generate --seed 1 | mapgen render > out.svg` shows a recognizable grey continent. Tier-A golden hash test passes on both native and wasm32 — **this is non-negotiable for Phase 1**, fixing float-determinism bugs after erosion stacks on top is brutal.

**Phase 2 — Erosion, hydrology, climate, biomes** (5–7 days)
Droplet erosion + thermal sweep; Priority-Flood depression fill → steepest-descent flow graph → flow accumulation → rivers (corner-routed, √flow width); orographic precipitation (prevailing winds + rain shadow); Whittaker LUT biomes (`data/whittaker.csv` committed).
Exit: SVG shows rivers in blue, biome-colored cells. Property tests pass: rivers monotonically descend, elevation mass-conserved ±ε under erosion, no endorheic basin without lake.

**Phase 2.5 — Realism property tests + sweep CLI** (1–2 days, blocks Phase 3)
*Revised 2026-05-17 — both devil's-advocate reviews recommended killing the
Refinery design. The minimal replacement does the regression-net job at a
fraction of the cost. See §5.5 for the full reasoning.*

*2.5a. Property tests.* Encode every realism failure caught this session
(and anticipated classes) as `#[test]` invariants in
`crates/mapgen-world/tests/realism_invariants.rs`. Asserts on
distribution-level properties: biome %, river density, ocean-current
sign, latitudinal banding correlation. Runs in CI; <1s per test.

*2.5b. Sweep CLI.* `mapgen sweep --seed 42 --knob <param>
--range <lo>..<hi> --steps <n> --out <dir>` renders N maps in a grid
for eyeball selection. Half a day of work. Replaces the Refinery's
practical use case (finding good defaults) with manual selection.

*2.5c. Pin tunable defaults.* For each parameter where we ran a sweep,
commit the chosen value with a comment citing the seed and the metric
used to pick it. The chosen-defaults log is `docs/tuning_log.md`.

Exit: every realism failure from sessions 1–3 has a corresponding
property test that fails on its bug pattern. `mapgen sweep` works end
to end on at least one knob (`erosion_rate` recommended).

**Phase 3 — Cultures + polities + naming + ornate render** (10–14 days) — **aesthetic payoff phase**
Now split into substages, each with its own spec file:

*3a. Cultures stage* (`mapgen-world/src/cultures.rs`). Per-cell habitat-fitness scoring across a roster of 5-8 race-archetypes seeded from `params`. Weighted Voronoi assignment writes `culture_id` per cell. Cultures own: race, language, religion, alignment, tech profile, magic style, settlement icon, architecture, diplomatic pattern. Seed roster includes random drop of races so seeds vary (some worlds have no elves, some have giants-only post-cataclysm). Tests: every land cell has a culture, no culture's average habitat-score below 0.3, distribution is non-trivial.

*3b. Religions stage* (`mapgen-world/src/religions.rs`). Found 1-3 religions per world; each tied to a culture; spread by alignment compatibility. Pantheon pattern (mono/poly/dual/animism/ancestor/cosmic) picked by founder culture's tech tier and magic style. Sacred sites placed at high-magic-field cells preferred by the religion's pattern. Tests: every religion has a founder, no religion has zero adherents, sacred sites are on the right biome.

*3c. Polities stage* (`mapgen-world/src/polities.rs`, formerly society). Suitability-weighted Poisson capitals filtered by `Culture.settlement_preference`; Christaller k=4 hierarchy; A* roads with reuse discount (trunk-and-branch). One polity can span multiple cultures. Tests: every settlement reachable from its capital, polity capital is in a high-suitability cell, road network connected.

*3d. Naming* (`mapgen-world/src/names/`). Phonotactic generator per language family + Markov fallback. Each `Language` entity stores phoneme inventory, syllable structure, sound-shift rules. Cognate generation (Quenya/Sindarin model) deferred to post-MVP.

*3e. Ornate render* (`mapgen-render/src/style/ornate_antique.rs`). Parchment background, roughr-perturbed coastlines with 4 offset ripples, Tolkien triangular mountain icons, scatter-tree forests keyed by biome, settlement glyphs keyed by `Culture.settlement × Culture.architecture`, Cinzel/IM Fell/EB Garamond typography bundled in `<defs>`, compass rose, corner cartouche, vignette + edge burn. Hand-author `docs/target_aesthetic.svg` Day 1 as the visual reference.

Exit: the screenshot you show people — a continent with named regions, distinctively-drawn settlements per culture (dwarven gates in the mountains, elven spires in old-growth forest, human castles on rivers), pilgrimage roads marked, sacred-site icons at appropriate biomes, a single ruined-city patch from a Cataclysm event, ornately framed.

**Phase 4 — History sim + event log** (7–10 days)
`mapgen-history` event log; Turchin demographic-fiscal loop; Mearsheimer power-balance loop; 500-year run; nation borders evolve and the final-year snapshot drives the render. Other four loops stubbed.
Exit: `worlds/w42.json.gz` contains ≥200 events; `mapgen events --in world.json.gz --major` lists major wars; rendered map shows the *post-history* political map.

**Phase 5 — Claude integration + WASM frontend** (5–7 days)
`mapgen-lore` with three-layer prompt structure: cached WORLD_BIBLE (built once per world, prompt-cached at top) + ENTITY_CONTEXT (transitive closure of `event.cause_ids`) + focal EVENT_SLICE + VOICE_CARD. Strict JSON output schema with `references: [event_id]`. NER validator rejects any proper noun in `body` not present in supplied context; retry once with violation reported; second failure falls back to template. Accepted chronicles persisted as `Work` entities.
Native sidecar (`mapgen serve --port 7878`, axum, single `POST /narrate` endpoint) wraps the same `narrate()` function the CLI uses — API key strictly native, in `$ANTHROPIC_API_KEY`. `mapgen-wasm` exposes `generate_world(seed, nations) -> {world_json, svg_string}`. Minimal Vite + vanilla TS frontend with WebWorker for the 2–5s generate call.
Exit: the full verification recipe (§9) passes end to end.

**Phase 6 — Post-MVP polish** (optional, 3–5 days)
Remaining four causal loops (Khaldun, succession, schism, hero/megabeast); second render style (`clean_modern`); PNG export via `resvg`; simulated-annealing label placement.

---

## 5. Critical Files

```
/home/user/mapGen/Cargo.toml                                       # workspace + version pins
/home/user/mapGen/rust-toolchain.toml
/home/user/mapGen/.github/workflows/ci.yml

# Core schema + determinism harness
crates/mapgen-core/src/{ids,entities,event,rng,fmath,world_data}.rs

# Geography pipeline
crates/mapgen-geom/src/{mesh,poisson,lloyd}.rs
crates/mapgen-world/src/lib.rs                                     # generate(seed, params) -> WorldData
crates/mapgen-world/src/{plates,noise,erosion,hydrology,climate,biomes}.rs
crates/mapgen-world/src/society/{capitals,hierarchy,roads}.rs
crates/mapgen-world/src/names/{phonotactic,markov}.rs
crates/mapgen-world/data/whittaker.csv

# History simulation
crates/mapgen-history/src/lib.rs                                   # run_history(world, years)
crates/mapgen-history/src/loops/{turchin,mearsheimer}.rs           # MVP loops
crates/mapgen-history/src/loops/{khaldun,succession,schism,hero}.rs # stubs
crates/mapgen-history/src/templates.rs                             # summary_canonical templates

# Rendering
crates/mapgen-render/src/lib.rs                                    # render(world, style) -> String
crates/mapgen-render/src/style/ornate_antique.rs                   # MVP style
crates/mapgen-render/src/primitives/{roughr_wrap,mountains,forests,rivers,coast}.rs
crates/mapgen-render/src/labels/{placement,typography}.rs
crates/mapgen-render/src/decor/{compass,cartouche,aging}.rs
crates/mapgen-render/assets/fonts/                                 # Cinzel + IM Fell + EB Garamond WOFF2

# Claude
crates/mapgen-lore/src/lib.rs                                      # narrate(world, event_id) -> Work
crates/mapgen-lore/src/{bible,context,voice,schema,ner,client,server}.rs
crates/mapgen-lore/src/prompts/{system,chronicle}.txt

# Binaries + frontend
crates/mapgen-cli/src/main.rs                                      # clap: generate, render, lore, serve, works, events
crates/mapgen-wasm/src/lib.rs                                      # #[wasm_bindgen] generate_world, render_world
web/{index.html, src/{main,worker,api}.ts, package.json, vite.config.ts}

# Tests
tests/golden/seed42.cbor.sha256
tests/golden/seed42.svg.sha256
tests/property/{hydrology,events}.rs
docs/target_aesthetic.svg                                          # hand-authored visual reference
```

### Algorithms/libraries to reuse (don't reinvent)
- **Mesh:** `voronoice` crate (Lloyd built in) + Bridson Poisson-disk + coastal repack (Azgaar's technique).
- **Terrain:** `noise` crate's OpenSimplex2 fBM + Inigo Quilez domain warping (`f(p + g(p))`) + Job Talle / Sebastian Lague droplet erosion (~100k droplets) + thermal talus sweep.
- **Hydrology:** Priority-Flood (Barnes-Lehman-Mulla 2014) for depression fill — O(N log N); river width ∝ √flow (Amit Patel).
- **Climate:** prevailing-wind orographic precipitation + Whittaker biome LUT (Azgaar's 14-biome table is portable).
- **Society:** Christaller central-place theory (k=4 transport principle) for settlement hierarchy; A* on weighted cell graph with road-reuse discount for trunk-and-branch road networks.
- **Rendering:** `roughr` crate (Rough.js Rust port) for hand-drawn primitives — endpoint jitter + 2 intermediate Bézier control points + 2-pass overstroke is the canonical hand-drawn recipe.
- **Determinism:** `rand_chacha::ChaCha8Rng` master + `set_stream` sub-streams; `libm` for cross-platform float math.

---

## 5.5. Iteration: deferred. Property tests + sweep CLI suffice for now.

> **Revised 2026-05-17.** The original §5.5 proposed a full Refinery
> optimization loop (simulated annealing over ~20-40 tunable knobs against a
> ~20-rule scoring function) plus a code restructure of the pipeline into
> six named time-resolved epochs. Both devil's advocates (technical and
> product/scope) recommended a radical scope cut. The arguments that
> changed the design:
>
> - **SA on 20+ continuous knobs against a piecewise objective with 5-30s
>   eval cost is effectively random search.** Goodhart's law dominates:
>   any rule matching Earth statistics produces statistical sludge.
> - **The regression-net argument collapses to property tests.** Every
>   "Rule scoring 0.0 on biome distribution" *is* `assert!(deserts/land
>   < 0.50)`. Property tests catch the same bug class in <1s of CI time
>   vs 30min of search. The Refinery was being justified for a job that
>   property tests already do.
> - **The "time-resolved epochs" reframing is a thesaurus pass.** The
>   existing `Stage` enum already encodes causal order. Renaming to
>   `Epoch` adds vocabulary, not mechanism. Code restructure costs 1-2
>   weeks for zero new capability.
> - **Scope priority.** The user's stated goal is "exotic map shop"
>   aesthetic. After three sessions there's not yet a settlement icon
>   on a map. Building auto-tuning infrastructure before the first
>   ornate render is the textbook displacement-activity pattern.
>
> What's kept from the original §5.5:

### Causal precedence (kept as design reference)

Every detail emerges *because of* earlier details. The chain (read top to
bottom = chronological + caused-by):

```
plate kinds + drift vectors
  ↓
mountain belts + ocean basins + continental margins
  ↓
ocean currents (gyres along basin shapes)        terrestrial heightmap
  ↓                                                       ↓
climate bands (3-cell × ocean SST × continentality)       erosion patterns
                            ↓                                  ↓
                           biome distribution + soil orders ←─┘
                                       ↓
                           freshwater hydrology (lakes, rivers, deltas)
                                       ↓
                           habitat suitability per race
                                       ↓
                           cultural placement + migration corridors
                                       ↓
                           polities + religions + trade networks
                                       ↓
                           land impact (deforestation, mining, sacred groves)
                                       ↓                  ↑
                                       └──── feedback ────┘
```

The feedback edge — cultures change the land, which changes the biomes,
which changes the cultures — is what makes the resulting world feel
earned. We will model this with **a second biome pass** after cultures
emit `LorePatch` records during Phase 3, not by restructuring the
pipeline into time-resolved epochs.

### What races want (kept as Phase 3 design reference)

Each race archetype carries an implicit utility function that drives
where the cultures stage places them and what land impact they leave.

| Race archetype | Wants | Yields |
|---|---|---|
| Human (Mediterranean) | Trade harbors, fertile deltas, defensible cities | Coastal city clusters; cleared agricultural hinterland |
| Human (Norse) | Coastal access, timber, raiding range | Coastal villages; ship-burial mounds; cleared lowland forests |
| Human (Steppe nomad) | Grasslands for horses, mobility | Burial kurgans; expanded grassland (overgrazing); no scarring |
| Human (River-valley) | Alluvial floodplain, irrigation, dense population | Canalized rivers; intensive agriculture; flood-control mounds |
| Elf (High) | Pristine old-growth forest, leyline access, isolation | Forest preserved or magically enhanced; spire-towers in canopy |
| Elf (Wood) | Dense temperate/tropical rainforest, low cultural footprint | Old-growth preserved; almost invisible settlements |
| Dwarf (Mountain) | Mineral-bearing mountains, defensible hold, ore networks | Mine networks; gate-towers; scarred foothills with spoil heaps |
| Dwarf (Hill) | Iron-bearing foothills with arable land adjacent | Terraced hills; surface mining; visible quarries |
| Orc | Marginal lands OR conquered fertile lands | Burnt earth around settlements; ruined farms |
| Halfling | Fertile rolling grassland with streams, weak threats | Cultivated landscape (visually pristine, agriculturally intensive) |
| Lizardfolk | Swamp/wetland with abundant prey | Stilt-villages on water; preserved wetland |
| Sea folk | Shallow coastal waters, reefs, kelp forests | Sunken temples, offshore megaliths at low tide |
| Underdark | Deep cavern complexes, lightless rivers | Portal sites on surface only |
| Giant (Frost) | Glaciers, permafrost | Megalithic ice-halls; mammoth-bone middens |

The 14-archetype roster is **data, not code** — lives at
`crates/mapgen-world/data/race_archetypes.csv` so seeds can drop entire
races (no-elves world, giants-only world) without recompiling. Phase 3
implements 4-5 archetypes; the rest land as data extensions.

### What replaces the Refinery: property tests + sweep CLI

For every realism failure caught (now or in future), the discipline is:

1. Write a property test in `crates/mapgen-world/tests/realism_invariants.rs`
   (or a topic-specific file) that fails on the bug. This is the
   regression net.
2. Fix the code. Test goes green.
3. If the bug was about parameter calibration (not algorithm choice),
   add a sweep over that parameter to `examples/sweep_<knob>.rs` and
   record the chosen default in `docs/tuning_log.md`.

The CLI exposes:
```
mapgen sweep --seed 42 --knob <param> --range <lo>..<hi> --steps <n> --out <dir>
```
which renders N maps in a grid for eyeball selection. The artist picks
the best, commits the chosen value as the new default.

This is concretely cheaper than the Refinery, gets ~80% of its
practical value (finding good defaults), and is implementable in half a
day.

### Deferred items

All cut work moved to `docs/BACKLOG.md` with a "trigger for revival"
condition per item. The Refinery, time-resolved epoch restructure, audit
CLI, and auto-Rule generation all live there now under "Optimization &
meta." If the first ornate render reveals classes of realism gap that
property tests can't catch, those entries reactivate per their stated
triggers.

---

## 7. Claude Integration

**Architecture:** Single `narrate(world: &WorldData, event_id) -> Work` function in `mapgen-lore`. CLI calls it directly (`mapgen lore --in world.json.gz --event auto-major-war`). Native sidecar (`mapgen serve --port 7878`) wraps the same call behind `POST /narrate`. Frontend POSTs to the sidecar. **API key never leaves the native process.**

**Prompt structure:**
- *System (cached):* in-world chronicler rules — narrate only from supplied EVENT_LOG, refer only to entities in WORLD_BIBLE + ENTITY_CONTEXT, no new named persons/places/gods/artifacts/dates, mark gaps `[lacuna]`, output valid JSON.
- *User per call:* WORLD_BIBLE (stable, prompt-cached at top, ~5k tokens), ENTITY_CONTEXT (records for every entity referenced, ~1–3 KB), EVENT_SLICE (focal events + transitive `cause_ids`), VOICE_CARD (in-world author, register: saga/monastic-chronicle/hymn/courtly-letter/peasant-rumor, bias, length, optional Booker/Propp/Polti literary template), SCHEMA.

**Validation:** every `references[]` ID must exist in supplied events; every proper noun in `body` must appear in supplied context (NER check); on violation retry once with the violation reported; second failure falls back to a template-rendered summary so the pipeline never breaks.

**Budget:** WORLD_BIBLE prompt-cached → >90% in-session cache hit. Default config caps: `max_calls_per_world = 20`, `max_tokens_per_call = 4000`. MVP narrates one event on demand; per-world cost ceiling ~$0.30 at current Sonnet pricing with full caching.

---

## 8. Top Risks (with cheap probes)

1. **`roughr` API instability or ugly output.** Day-1 probe: 30-line standalone binary that draws one perturbed polygon. If broken, vendor the ~600 LOC of bezier-perturbation code. Decision deadline: end of Phase 1.
2. **WASM bundle > 3 MB.** End-of-Phase-1 probe: `wasm-pack build --release`, check size, profile with `twiggy`. Mitigations: `wee_alloc`, `wasm-opt -Oz`, feature-gate unused `noise` variants.
3. **Native ↔ WASM float drift despite `libm`.** Phase 1 must run the Tier-A hash on both targets the moment the heightmap exists. Fixing this after erosion + hydrology + climate stack is brutal.
4. **Ornate render is ugly, not beautiful.** Phase 3 Day 1: hand-author `docs/target_aesthetic.svg` as the comparison reference. Aesthetic risk needs a visual probe, not a code probe.
5. **Claude hallucinates proper nouns despite NER.** Phase 5 Day 1: 50-line test harness with a fake mini-world bible to tune the prompt before integration. Template fallback must be acceptable, not a failure mode.

---

## 9. Verification Recipe (MVP acceptance)

```bash
# 1. Tests green on both targets.
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --target wasm32-unknown-unknown

# 2. Generate world from seed.
cargo run --release -p mapgen-cli -- generate --seed 42 --nations 8 \
    --out worlds/w42.json.gz

# 3. Render ornate SVG.
cargo run --release -p mapgen-cli -- render --in worlds/w42.json.gz \
    --style ornate_antique --out maps/w42.svg

# 4. Narrate one major event via Claude.
export ANTHROPIC_API_KEY=sk-...
cargo run --release -p mapgen-cli -- lore --in worlds/w42.json.gz \
    --event auto-major-war --out worlds/w42.json.gz

# 5. Inspect the chronicle.
cargo run --release -p mapgen-cli -- works --in worlds/w42.json.gz --latest

# 6. WASM build.
wasm-pack build --release --target web crates/mapgen-wasm --out-dir ../../web/pkg

# 7. Native sidecar (background).
cargo run --release -p mapgen-cli -- serve --port 7878 &

# 8. Frontend.
cd web && npm install && npm run dev
# Open http://localhost:5173, enter seed 42, click Generate.
# Verify SVG matches maps/w42.svg byte-for-byte.
# Click "Narrate major event"; verify chronicle text appears, referencing only
# entities present in worlds/w42.json.gz.
```

Pass criteria: all commands exit 0, golden hashes match across native/WASM, browser SVG matches CLI SVG byte-for-byte, chronicle's proper nouns are all NER-validated against the world bible.

---

## Phase 0 — Day 1 Bootstrap Checklist (first 8 commits)

1. `chore: initialize workspace` — root `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `LICENSE` (dual MIT/Apache-2.0, Rust convention), `README.md` stub.
2. `chore: add CI workflow` — `.github/workflows/ci.yml` running fmt/clippy/test on linux + wasm32.
3. `feat(core): scaffold mapgen-core` — `ids.rs`, `fmath.rs` libm shim, `rng.rs` with `Stage` enum. Tier-A golden harness compiles, skipped.
4. `feat(geom): Bridson Poisson disk` — unit test on point count.
5. `feat(geom): voronoice mesh + Lloyd ×2` — unit test on neighbor symmetry.
6. `feat(world): plate uplift heightmap` — minimal `world.rs` produces `WorldData` with only `terrain` filled.
7. `feat(render): greyscale Voronoi SVG` — no style yet, just colored polygons.
8. `feat(cli): mapgen generate + render` — end-of-day demo: `cargo run -p mapgen-cli -- generate --seed 1 | mapgen render > out.svg`.

After these eight commits: workspace real, CI green, data-flow trunk exists, visible artifact. Nothing here gets thrown away in later phases.

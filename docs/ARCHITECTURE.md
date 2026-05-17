# Fantasy Map Generator — Implementation Plan

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

**Phase 2.5 — Refinery + Rules** (4–6 days, blocks Phase 3)
Land the iteration architecture *before* the cultures stage, so cultures-stage parameters get auto-tuned from day one and every future calibration regression is caught by a Rule. Substages:

*2.5a. Rule trait + composite scorer.* `mapgen-world/src/rules/` with the Earth-grounded rule set above. Each rule cites its real-world reference data in a doc comment.

*2.5b. Operation trait + tunable-knob registry.* `mapgen-world/src/operations/`. Every existing pipeline parameter (Köppen aridity threshold, erosion rate, base_precip, etc.) registers itself as a tunable knob with a range.

*2.5c. Refinery loop.* `mapgen-world/src/refinery.rs`. SA-style accept/revert with plateau detection. Determinism: each iteration's RNG is `splitmix64(master, iter_index)`.

*2.5d. CLI surface.* `mapgen generate --iterate 200 --target-score 0.85`. `mapgen audit --in world.json.gz` prints per-rule scores. `mapgen log --in world.json.gz` prints the refinement history (which operations the Refinery tried and accepted).

*2.5e. Time-resolved epochs.* Restructure `generate_full` into the named-epoch pipeline so the Refinery can schedule operations by epoch. Earlier epochs run once and cache; later epochs re-run when only their knobs are tuned. Cache key = hash of all upstream knobs.

Exit: `mapgen generate --iterate 50 --seed 42` produces a world whose composite score is above the current hand-tuned baseline. The audit command shows each rule's score; the log shows which knobs the Refinery converged on.

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

## 5.5. Iteration: Refinery + Time-Resolved Simulation

The generator is **not a one-shot pipeline**. It is a composition of two iterations:

### (A) In-world time simulation

The world isn't built by stages running once; it's built by **time advancing**. The same systems (geology, climate, hydrology, biology, society) run continuously, just at different time resolutions per epoch.

```
                 simulated in-world time →
T = −1Gy ── −100My ── −10My ── −100ky ── −10ky ── −3ky ── −1ky ── 0 (present)
  │           │         │         │         │       │        │       │
  │           │         │         │         │       │        │       │
 plates    erosion   coast     glaciation  climate cultures civilizations
 form      sculpts   stable                stabilizes form   wars/religions
                                                 │            │
                                                 ▼            ▼
                                            languages      polities
                                            religions      events
```

Each epoch advances every system that's awake at that time scale:

| Epoch | Time scale | Awake systems | Tick size |
|---|---|---|---|
| Deep time | −1Gy → −100My | plates, hotspots, volcanism, basin formation | 50 My |
| Sculpting | −100My → −10My | tectonic motion, mountain building, erosion at scale | 5 My |
| Glaciation | −100ky → −10ky | climate, glaciation cycles, sea-level, biome migration | 5 ky |
| Holocene | −10ky → −3ky | stable climate, soil, biomes, megafauna, early humans | 500 y |
| Cultural | −3ky → −1ky | language families form, cultures settle, religions emerge | 50 y |
| Civilizational | −1ky → 0 | polities, wars, dynasties, schisms, technology spread | 5 y |

This is **not** a separate "history sim" tacked on after a static pipeline — it's a single time arrow with system awakenings as it advances. Geology is awake at T=−1Gy; cultures wake up around T=−3ky; ornate render happens at T=now.

### (B) The Refinery — cross-run optimization

Outside the time simulation, a Refinery loop tries different parameters / operations / algorithms, runs the time sim, scores against rules, and accepts-or-reverts:

```
                ┌───────────────────────────────────────┐
                │  best world snapshot (highest score)  │
                └───────────────────────────────────────┘
                         ▲                       │
                  accept │                       │ revert
                         │                       ▼
   time-sim(params) ──► world ──► score(world) ──► operation? ──► params'
       ▲                                                            │
       └────────────────────────────────────────────────────────────┘
              new master seed (deterministic, indexed by iteration)
```

Operations (Add/Refine/Replace/Deepen) modify the **parameters** that the time sim runs against. The output is the best world found, not the last. Simulated-annealing acceptance permits temporary regressions to escape local optima; plateau detection signals convergence.

The Refinery may be **time-budgeted** (`--budget 30s`) or **iteration-budgeted** (`--iterate 200`), with the simulation's own runtime as the inner cost — early iterations are cheaper because the loop can short-circuit if a parameter choice scores poorly without running the full time sim.

### Causal precedence: details create details

Within the time sim, every detail emerges *because of* earlier details. The causal chain (read top → bottom = chronological + caused-by):

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
                           land impact (deforestation, mining, sacred groves, scars)
                                       ↓                  ↑
                                       └──── feedback ────┘
```

The feedback edge at the bottom is the crucial part: **cultures change the land**, which changes the biomes, which changes the cultures. After 3,000 years of human civilization, the land downwind of major cities is deforested; the soil is depleted; rivers have shifted. The "natural baseline" at T=now is not the same as the baseline at T=−3ky.

### What nature wants

Each natural system has an implicit utility function (its physics):

| System | Wants |
|---|---|
| Plate tectonics | Convert potential energy via mantle convection (drift, subduct, uplift) |
| Erosion | Reduce slope, transport mass downhill (entropy) |
| Hydrology | Water at lowest gravitational point |
| Climate | Distribute solar energy poleward |
| Biomes | Fill suitable habitat (selection pressure) |
| Ecosystems | Conserve nutrients in closed cycles |

Nature has no agency, but the *direction of its drift* is deterministic given parameters. The time sim ratchets toward each system's local equilibrium at its own time scale.

### What races want — agent utility functions

Cultures and races *do* have agency. Each carries a utility function the time sim agent layer maximizes:

| Race archetype | Wants | Yields |
|---|---|---|
| Human (Mediterranean) | Trade harbors, fertile deltas, defensible cities | Coastal city clusters; cleared agricultural hinterland |
| Human (Norse / Viking) | Coastal access, timber, raiding range | Coastal villages; ship-burial mounds; cleared lowland forests |
| Human (Steppe nomad) | Grasslands for horses, mobility, weak settled neighbors | Burial kurgans; expanded grassland (overgrazing); no permanent scarring |
| Human (River-valley) | Alluvial floodplain, irrigation, dense population | Canalized rivers; intensive agriculture; flood-control mounds |
| Elf (High) | Pristine old-growth forest, leyline access, isolation from humans | Forest preserved or magically enhanced; spire-towers in canopy |
| Elf (Wood) | Dense temperate or tropical rainforest, low cultural footprint | Old-growth preserved indefinitely; almost invisible settlements |
| Dwarf (Mountain) | Mineral-bearing mountains, defensible hold, ore networks | Mine networks (subterranean); gate-towers; scarred foothills with spoil heaps |
| Dwarf (Hill) | Iron-bearing foothills with arable land adjacent | Terraced hills; surface mining; visible quarries |
| Orc | Marginal lands or conquered fertile lands | Burnt earth around settlements; ruined farms |
| Halfling | Fertile rolling grassland with streams, weak threats | Cultivated landscape (visually pristine, agriculturally intensive) |
| Lizardfolk | Swamp/wetland with abundant prey | Stilt-villages on water; preserved wetland |
| Sea folk (merfolk / triton) | Shallow coastal waters, reefs, kelp forests | Sunken temples, offshore megaliths visible at low tide |
| Underdark (drow / duergar) | Deep cavern complexes, lightless rivers, fungal forests | Portal sites on surface only |
| Giant (Frost) | Glaciers, permafrost | Megalithic ice-halls; mammoth-bone middens |
| Dragon (territorial) | Hoard-defensible peaks, fear-radius | Depopulated zone for ~10 cells; treasure hoard at lair |

When wants conflict (two cultures want the same biome), the time sim has to resolve via **war** (Mearsheimer), **migration** (Khaldun frontier), **trade equilibrium**, or **extinction**. The history-sim causal loops we already stubbed (Turchin, Khaldun, Mearsheimer, Succession, Schism, Hero) are exactly the conflict-resolution mechanisms.

### What causes change

Per epoch, the active causes of change:

| Cause | Epoch | Effect |
|---|---|---|
| Mantle convection | Deep time | Plates drift, oceans open/close |
| Hot-spot tracks | All | Volcanic island chains, basaltic floods |
| Erosion + deposition | All | Heightmap smooths, valleys cut, deltas grow |
| Glaciation cycles (Milankovitch) | Glaciation | Sea-level drops, ice scours, refugia preserve genes |
| Climate forcing (volcanic, solar) | All post-glaciation | Crop failures, plague years, migrations |
| Species migration | Holocene+ | Megafauna corridors, biome boundary shifts |
| Cultural diffusion | Cultural+ | Language families spread along routes |
| Cultural conflict | Cultural+ | Wars, refugee patches, ruined cities |
| Religious schism | Civilizational | Crusades, holy-site contestation, doctrinal patches |
| Technology adoption | Civilizational | Land-use intensification, mining, deforestation |
| Cataclysms (rare) | Any | Patches with `cause_event`: Mournland, Burnt Lands, Desolations |
| Divine intervention | Any post-cultural | Patches: sacred groves, blighted earth, manifest zones |

Each is encoded as either a **physical update rule** (deterministic from parameters) or an **agent decision** (resolved by utility maximization) or a **rare event roll** (with cause_event provenance back into the chronicle).

### Composition: the two loops together

```
for refinery_iter in 0..budget:
    params ← propose(rules, history)
    seed ← splitmix64(master, refinery_iter)
    world ← TimeSimulation::run(params, seed)
        ├ epoch deep_time(50My ticks):   plates → mountains → basins
        ├ epoch sculpting(5My ticks):    erosion → coast
        ├ epoch glaciation(5ky ticks):   ice ages → refugia
        ├ epoch holocene(500y ticks):    biomes → species → early humans
        ├ epoch cultural(50y ticks):     languages → cultures → religions
        └ epoch civilizational(5y ticks): polities → wars → land impact
    score ← compose(rules.score(world))
    if score > best.score:
        best ← world
    elif SA_accept(Δscore, temperature):
        current ← world
    else:
        revert
    if plateau(history): break
return best
```

The same code that runs Phase 2 today is the **deep_time + sculpting + glaciation** epochs of the time sim. The "cultures stage" in Phase 3 is the **cultural epoch**. The Phase 4 history sim is the **civilizational epoch**. Phase 5's render visualizes the final state at T=now.

Re-framed this way, the project is more coherent: there is one time arrow, and our existing pipeline stages are *snapshots at different epochs*.

### Rules of compliance (what the Refinery scores)

Starter rule set, grouped by epoch. Each `Rule` reports a score in [0, 1] plus a weight and a per-rule "failing cells" set for targeted operations.

```
Deep-time epoch:
  R-G1  hypsometric_curve_matches_earth        elevation histogram vs Earth's
  R-G2  mountain_belts_along_plate_boundaries  spatial corr (peaks ↔ plate edges)
  R-G3  coastline_fractal_dimension            D-box ≈ 1.25 for Earth

Sculpting epoch:
  R-S1  drainage_basin_count_per_continent
  R-S2  river_density_5_to_15_pct
  R-S3  physical_headwater_origins

Glaciation epoch:
  R-Gl1 fjord_count_at_high_latitude_coasts    (when glaciation enabled)
  R-Gl2 refugium_count_inside_glacier_mask

Holocene epoch:
  R-C1  latitudinal_precip_correlation         3-cell pattern
  R-C2  ocean_current_sign                     east coast warmer @ mid-lat
  R-C3  koppen_distribution_within_earth_band
  R-B1  biome_distribution_within_earth_pct
  R-B2  latitudinal_biome_banding_correlation
  R-B3  riparian_corridor_present_in_arid

Cultural epoch:
  R-Cu1 every_culture_in_preferred_habitat
  R-Cu2 zipf_rank_size_city_distribution
  R-Cu3 cultures_diverge_with_geographic_isolation
  R-Cu4 race_habitat_competition_resolved

Civilizational epoch:
  R-Ci1 polities_have_capitals_on_suitable_cells
  R-Ci2 land_impact_visible_around_old_settlements
  R-Ci3 religion_sacred_sites_align_with_doctrine
  R-Ci4 history_events_have_causal_chains_to_geography
```

The composite score is a **weighted geometric mean**: any catastrophic-zero rule pulls the whole down, preventing the loop from over-optimizing one rule while another collapses.

### Validating "compliance increases over time"

Two distinct trajectories:

1. **Within a Refinery run**: score curve over iterations, with SA-style temporary regressions but monotone-best. The best-so-far snapshot is what gets returned.
2. **Across project sessions**: as we add new rules and operations, the achievable best score on the same seed should improve. We track `seed_42_baseline.json` as a regression dataset — if a change drops the best-achievable score, the change is suspect.

The discipline this enforces: **every realism fix lands as a Rule first**. If I miscalibrate ocean currents again, the corresponding Rule scores it 0.0; the next Refinery run automatically reverts the bad change. The user catching the desert-everywhere bug becomes a Rule that prevents the same class of bug forever.

---

Tests are the executable spec. No feature lands without its test landing first in the same or prior commit. Per phase, per module, per feature:

1. **Write the failing test** that encodes the invariant or behavior. Run it; confirm it fails for the expected reason (not a typo).
2. **Implement the minimum** to make the test pass.
3. **Refactor** with the test as safety net.
4. **Phase exit** = the phase's spec file (`tests/phaseN_spec.rs`) is green, not "I wrote some code and the unit tests passed."

Every phase begins by committing a `phaseN_spec.rs` file of failing tests that defines what "phase N done" means. Implementation then drives them green. This makes the spec auditable in the diff: the test file is the contract.

Three tiers of tests, all run in CI:

**Tier A — Byte-identical golden hashes.** Fixed seed 42, fixed params. Serialize `WorldData` as CBOR (stable byte order, unlike JSON); hash the CBOR and the rendered SVG; commit hashes under `tests/golden/`. Run separately on `x86_64-linux` and `wasm32-unknown-unknown` (via `wasm-bindgen-test` headless Chrome). Both targets must match. This is the test that catches `libm` regressions, iteration-order bugs, and noise-library updates. **Debug-mode only** — release float ordering can drift; document this. Pull this forward into Phase 2 (originally scheduled for Phase 5) so float-drift bugs surface before erosion + hydrology + climate stack on top.

**Tier B — `proptest` invariants.**
- Every river cell's downstream neighbor has strictly lower elevation.
- `sum(elevation_after_erosion) ≈ sum(elevation_before) ± 1e-3 * cells` (mass conservation modulo coast outflow).
- Every event's `cause_ids` reference prior events (`year(cause) <= year(event)`).
- Every event's `actors`/`patients` are live entities at `event.year`.
- No `Claim` outlives its `Polity` without being marked dormant.
- Every settlement is reachable from its capital via the road graph.

**Tier C — `insta` snapshot tests** on small-scale (1k-cell) SVG strings for fast renderer feedback.

**Local validation gate (run before every push, not just CI):**
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --target wasm32-unknown-unknown    # from Phase 2 onward
wasm-pack test --headless --chrome crates/mapgen-wasm     # from Phase 5 onward
```

Same commands run in CI. Discipline: the first commit of every phase is the spec file (red). The last commit of every phase makes it green plus refactors.

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

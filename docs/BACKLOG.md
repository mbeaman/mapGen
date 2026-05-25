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

### Nested multi-scale refinement framework

- **Why deferred.** It's the keystone every other scale entry depends on,
  and it's pure infrastructure with no visible output on its own. The MVP
  (Phase 4 history, Phase 5 Claude) isn't done; building scale machinery
  before the single-scale world is fully lored fails the "ships value vs.
  intellectually satisfies" test. This is post-MVP work.
- **Design note (the hard part).** Add a `level` axis alongside `Stage` in
  the RNG model: `child_seed = blake3(parent_seed, level, sector_id)`. Per
  stage, define a boundary-condition contract — a child sector inherits its
  parent cells' elevation envelope, river entry/exit points on its edges,
  coastline crossings, biome, and settlement positions as *fixed
  constraints*, then fills sub-cell detail so that down-sampling the child
  reproduces the parent within tolerance. On-demand and stateless: never
  persist the planet at local resolution; regenerate any sector from seeds.
  Intermediate layers (provincial, district/hinterland) are expressed as
  refinement *depths*, not distinct pipelines — one mechanism, not five.
  Must be purely additive: existing single-scale golden hashes must not
  change.
- **Trigger for revival.** The first concrete consumer of a second zoom band
  — the user wants a map at a scale other than the current continental one
  (a planet view, a city interior, or a countryside tile). Not before the
  Phase 4–5 MVP closes.
- **Cost.** 1–2 weeks for the framework alone. The boundary-condition
  contract and its property tests are the real work, not the seed plumbing.
- **Origin.** This session, 2026-05-24, multi-scale (world / regional /
  local) request.

### World / planet scale (zoom out)

- **Why deferred.** Needs the refinement framework above *and* multi-
  continent generation (see Geography & geology → "Multi-continent worlds").
  No narrative today spans more than one continent.
- **Design note.** The planet as the root level: multiple continents, ocean
  basins, planetary tectonic plates, and the global climate belts /
  ocean-current gyres the climate stage already models but currently only
  over one continent. The present continental map becomes one child sector of
  this root. Projection / distortion at the planetary edge is the new render
  problem the Multi-continent item already flags.
- **Trigger for revival.** User wants to seed / see the whole planet rather
  than one continent; or narratives need inter-continental trade, migration,
  or colonization; or several regional maps must share one consistent globe.
- **Cost.** ~1 week on top of the framework + multi-continent generation.
- **Origin.** This session; extends Geography & geology → "Multi-continent
  worlds" (that entry is the *generation* of >1 continent; this is the
  zoom-out *view* of it).

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

### Scale-dependent render fidelity (per-feature level-of-detail)

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
- **Still open:** the *detail-decreasing* generalisation direction
  (Töpfer/Visvalingam line simplification + scale-rank label declutter), which is
  really only meaningful once a coarser-than-world (planet / multi-continent)
  view exists above level 0 — see Phase 8.3 and the navigation item's follow-ups.
  Optional further city detail: named districts/wards.
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

### Seamless inter-scale navigation (research brief)

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

---

## Render

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

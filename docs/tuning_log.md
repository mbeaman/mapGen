# Tuning Log

Records every parameter sweep we ran and which value we picked. The
sweep CLI (`mapgen sweep --seed S --knob K --range lo..hi --steps n
--out dir`, see `crates/mapgen-cli/src/sweep.rs`) is the standard
intake; entries should cite the seed they were swept on, the audit
artifact that justified the pick, and the commit that landed the value.

> **First-entry caveat — values below are retroactive.** The current
> defaults landed before the sweep CLI existed (commits `117eafe`,
> `bd30c9b`, etc.); they were chosen by *audit* (user-driven map
> inspection that caught compounding realism bugs), not by sweep. From
> Phase 3 onward, every tuning change should leave a forward-looking
> entry: a `mapgen sweep` command, the picked value, the artifact
> directory, the reasoning.

## Format per entry

```
### <param_name>

* **Current:** <value>
* **Why this value:** <one-paragraph justification>
* **Method:** sweep | audit | derived | reference
* **Source:** <commit-hash | sweep-dir | citation>
```

## Climate — `mapgen-world::climate::ClimateParams`

### `base_precip`

* **Current:** `0.7` (fraction of saturated open-ocean baseline)
* **Why this value:** At `0.5` the typical-land `p_annual` distribution
  centered around `~0.10`, pushing most cells through the aridity
  branch in Köppen → desert-everywhere maps. `0.7` lifts the
  distribution enough that mid-latitude wet bands escape aridity while
  the subtropics still hit the desert cutoff. Earth-fit ~30% arid land
  preserved.
* **Method:** audit
* **Source:** commit `117eafe` ("Fix five compounding realism bugs that
  produced desert-everywhere maps")

### `lapse_rate`

* **Current:** `0.45` (normalized °C per unit elevation)
* **Why this value:** `0.5` was too aggressive — mid-latitude land
  cells crossed the freezing/tropical boundary too easily, and
  moderately elevated tropical cells (Kenyan-highlands analog at
  ~1500 m) dropped out of A-class Köppen. Real wet-adiabatic lapse is
  ~6.5 °C/km; that maps to ~`0.45` on our normalized scale at typical
  elevations.
* **Method:** derived (calibrated against real adiabatic lapse) + audit
* **Source:** commit `117eafe`; doc comment in `climate.rs:42-49`

### `axial_tilt`

* **Current:** `23.5°.to_radians()` ≈ `0.4101524`
* **Why this value:** Earth value; used by `climate_seasonal::run` to
  derive `itcz_shift = sin(tilt) * 0.5`, which controls how far the
  ITCZ moves between perihelion and aphelion passes (~0.26 of a
  hemisphere on Earth).
* **Method:** reference
* **Source:** Earth fact; commit `b4cd84d` (Köppen-Geiger seasonal
  climate landing)

### `equator_temp` / `polar_temp`

* **Current:** `1.0` / `-0.4`
* **Why this value:** Normalized scale anchored so freezing = 0 and
  tropical sea-level mean ≈ 1.0. Polar `-0.4` keeps high-latitude
  bands below freezing year-round without immediately collapsing all
  high-lat cells into EF (ice cap).
* **Method:** derived
* **Source:** commit `609b312` (Phase 2 green — climate landing)

## Erosion — `mapgen-world::erosion::ErosionParams`

### `erosion_rate`

* **Current:** `0.04`
* **Why this value:** Stream-power coefficient `k`. At default
  `iterations = 8`, this rate produces visible V-shaped valleys at
  river junctions without flattening continents. No targeted audit
  fix yet — value untouched since Phase 2 spec landed.
* **Method:** derived (default initial pick)
* **Source:** commit `609b312`

### `iterations`

* **Current:** `8`
* **Why this value:** Each iteration is `O(N log N)` (priority-flood
  + flow accumulation + erosion sweep). 8 passes is the lowest count
  that visibly differentiates basin topography on seed 42; higher
  counts run longer with diminishing returns.
* **Method:** derived
* **Source:** commit `609b312`

### `talus_angle` / `thermal_passes`

* **Current:** `0.10` / `0`
* **Why this value:** Thermal smoothing is OFF by default
  (`thermal_passes = 0`) — stream-power alone is producing the desired
  V-valley aesthetic. `talus_angle = 0.10` is set but unused at
  current `thermal_passes`. Revisit if cliffs start looking too sharp.
* **Method:** derived
* **Source:** commit `609b312`

## Köppen-Geiger thresholds — `mapgen-world::koppen`

### `arid_threshold` formula

* **Current:** `0.10 + t_warm * 0.04`
* **Why this value:** Calibrated against the actual precipitation
  distribution our 3-cell + orographic model produces (typical land
  `p_annual` ranges from ~0.06 driest decile to ~0.20 wettest). The
  threshold sits near the 30th percentile so ~30% of land qualifies as
  arid — Earth's actual fraction.
* **Method:** audit + derived
* **Source:** commit `117eafe`; doc comment in `koppen.rs:103-108`

### `desert_cutoff` ratio

* **Current:** `arid_threshold * 0.75` (vs. Köppen's classical 0.5)
* **Why this value:** Our precipitation distribution is tighter than
  real Earth's (no Atacama-scale outliers because we don't yet model
  upwelling). The BW/BS cutoff sits proportionally higher so steppe
  doesn't swallow every arid cell.
* **Method:** derived (departs from real Köppen)
* **Source:** commit `117eafe`; doc comment in `koppen.rs:111-113`

### Tropical-vs-temperate threshold (`t_cold > 0.55`)

* **Current:** `0.55` (vs. real Köppen's 18 °C ≈ 0.72 on our scale)
* **Why this value:** Our lapse rate is calibrated for cell elevations
  on a 2048×1280 map; at real-Köppen's 0.72 cutoff, moderately
  elevated equatorial cells (Kenyan highlands) drop out of A-class.
  Relaxed to 0.55 so tropical-elevation cells stay tropical.
* **Method:** audit
* **Source:** commit `117eafe`; doc comment in `koppen.rs:128-134`

### E (polar/tundra) `t_warm < 0.40` threshold

* **Current:** `0.40` (~10 °C tree line)
* **Why this value:** Real-Earth treeline sits at warmest-month
  ≈ 10 °C, which maps to 0.40 on our normalized scale.
* **Method:** reference
* **Source:** commit `b4cd84d` (Köppen landing); doc comment in
  `koppen.rs:92-95`

## Hydrology — `mapgen-world::hydrology::extract_rivers`

### `flow_threshold` (third arg to `extract_rivers`)

* **Current:** `0.05` (passed in `lib.rs:60`)
* **Why this value:** `0.05` was originally too generous — produced
  rivers in every cell on seed 42. The fix was *not* to bump the
  threshold but to add three orthogonal filters in `extract_rivers`:
  (a) adaptive threshold scaled by the actual flow distribution,
  (b) highland-headwater rule (rivers start only where `elev ≥ 0.30`
  or adjacent to a lake), (c) minimum length. The bare `0.05` here
  is the starting probe; the real filtering happens inside.
* **Method:** audit
* **Source:** commit `bd30c9b` ("Fix river realism")

## Biomes — `mapgen-world::biomes`

### Alpine override (Köppen path): `elev ≥ 0.45 AND summer_t < 0.30`

* **Current:** `0.45` / `0.30`
* **Why this value:** Without this override, mid-elevation cells in
  the temperate band get misclassified as low-lying tundra (because
  Köppen E uses warmest-month-only, ignoring elevation context).
  `0.45` is roughly the "highlands" floor on our normalized elevation
  scale; `0.30` corresponds to ~5 °C summer mean.
* **Method:** derived
* **Source:** commit `117eafe`; doc comment in `biomes.rs:66-71`

### Riparian override

* **Current:** Applied to any river cell whose own biome OR any
  neighbor's biome is `{DESERT, SHRUBLAND, SAVANNA}`.
* **Why this value:** Models the Nile-through-Sahara /
  Colorado-through-Mojave effect — a river through forest stays
  forest, but a river through arid surroundings produces a visible
  green corridor. No tunable threshold.
* **Method:** audit
* **Source:** commit `bd30c9b`

## Cultures — `mapgen-world::cultures`

### `CulturesParams::target_cultures`

* **Current:** `5`
* **Why this value:** Phase 3a MVP roster is 5 archetypes (1 Human
  variant + 1 Elf + 1 Dwarf + 1 Orc + 1 Halfling per ARCHITECTURE.md
  §5.5 / TASKS.md). The target is effectively a ceiling — a seed
  where Mountain habitats don't exist culls the Dwarf entry, so the
  realized roster size is bounded above by this.
* **Method:** derived (mirrors CSV row count)
* **Source:** commit `1451d5f`

### `CulturesParams::min_habitat_fitness`

* **Current:** `0.3`
* **Why this value:** ARCHITECTURE.md §4 Phase 3a exit criterion is
  literal: "no culture's average habitat-score below 0.3." Cultures
  whose mean fitness falls below this threshold are culled and their
  cells reassigned. On seed 42 (4 000 cells) all 5 archetypes clear
  the floor; the culling loop is dormant.
* **Method:** reference (architecture-prescribed)
* **Source:** commit `1451d5f`

### `habitat_fitness` axis-blend (geometric vs arithmetic)

* **Current:** geometric mean (`(b·t·e·w).powf(0.25)`) across biome /
  temperature / elevation / water axes.
* **Why this value:** Arithmetic mean rewards generalists scoring
  `(0.5, 0.5, 0.5, 0.5)` the same as a specialist scoring
  `(1.0, 1.0, 0.2, 0.2)`. "Earned worlds" need specialists winning
  their niche — geometric mean punishes a single weak axis enough to
  produce coherent territories (dwarves in mountains, halflings in
  pastoral, river-valley humans on coasts).
* **Method:** derived (advisor pass on the C2 design)
* **Source:** commit `1451d5f`

### Biome-mismatch floor (in `habitat_fitness`)

* **Current:** `0.2` — a cell whose biome isn't in the archetype's
  preferred list still scores `0.2` (not `0.0`) on the biome axis.
* **Why this value:** Hard zero produces fragmented territories — a
  Dwarf settlement on a single non-mountain cell becomes unplaceable.
  `0.2` lets cultures bleed into adjacent non-preferred biomes when
  other axes (temperature / elevation / water) are strong, which is
  the realistic shape (Norse longhouses on the edges of fjord forest).
  Worth sweeping `0.0..0.5` step 5 once the ornate render exists to
  judge by visible territory coherence.
* **Method:** derived
* **Source:** commit `1451d5f`

### Tent falloff width (temperature / elevation ranges)

* **Current:** `0.2` — outside `[lo, hi]`, score falls linearly to 0
  over a `0.2`-wide buffer.
* **Why this value:** With temperature scale `[-0.4, 1.0]` and
  elevation `[0, 1]`, a falloff of `0.2` means 14-20% of the full
  range is the "marginal" zone where score interpolates. `0.1`
  produces sharper boundaries (more fragmentation); `0.4` lets
  cultures spread too far from their core habitat. Sweep candidate
  once ornate render exists.
* **Method:** derived
* **Source:** commit `1451d5f`

### Water-absence penalty multiplier

* **Current:** `0.6` — a cell without coast / river / lake adjacency
  scores `1.0 - 0.6 * archetype.water_weight` on the water axis.
* **Why this value:** River-valley Human has `water_weight = 0.9`, so
  inland cells score `0.46` on water → multiplied through geometric
  mean drags fitness ~25% lower. Dwarf with `water_weight = 0.1`
  barely penalizes inland placement (0.94). Calibrated so the
  per-archetype `water_weight` column in the CSV is the dominant
  knob, not this constant. Don't sweep alongside `water_weight` —
  they trade off; pick one.
* **Method:** derived
* **Source:** commit `1451d5f`

## Open tuning questions (next sweep candidates)

- **`erosion_rate`** — never audited; sweep `0.01..0.10` step 8 on
  seed 42 and pick the value that produces the V-valley aesthetic
  without flattening uplift.
- **Köppen `desert_cutoff` ratio** — currently `0.75`; if we expand
  arid → desert fraction toward Earth's ~20% / 10% split, this is the
  knob.
- **Riparian neighbor-search radius** — currently 1-ring only; a
  larger radius would extend green corridors further from the channel.
  Visual-only effect; needs an ornate render to judge.
- **Cultures biome-mismatch floor** — `0.0..0.5` step 5 against the
  ornate render. Affects territory fragmentation.
- **Cultures tent-falloff width** — `0.1..0.4` step 5. Sharpness of
  archetype range boundaries.
- **Per-archetype `water_weight` in `race_archetypes.csv`** — five
  values today; the River-valley Human `0.9` and Dwarf `0.1` are
  archetype-defining and shouldn't move much, but the middle three
  (Wood Elf `0.2`, Orc `0.2`, Halfling `0.4`) are mostly guesswork.

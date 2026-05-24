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

## Naming — `mapgen-world::naming`

### `MIN_NAMED_RIVER_CELLS` / `MIN_NAMED_LAKE_CELLS` / `MIN_NAMED_RANGE_CELLS`

* **Current:** `8` cells (rivers) / `3` cells (lakes) / `5` cells
  (mountain ranges — connected ALPINE/SNOW clusters).
* **Why this value:** Only *major* features earn a label, so the map
  isn't littered with names on every brook and pond. `8` river cells is
  roughly the long-river tier on seed 42 at 4k cells (verified: ≥1
  river clears it); `3` lake cells skips single-cell ponds. Below these
  the feature renders unlabeled.
* **Method:** derived (seed-42 spot check in `naming_spec`)
* **Source:** Phase-3e polish; `naming.rs` step 6

## History (Phase 4) — `mapgen-history`

### Turchin demographic backbone (4b) — `loops::turchin`: `GROWTH_RATE` / `FAMINE_STRESS` / drought + plague knobs

* **Current:** `GROWTH_RATE = 0.03` (logistic `r`); `FAMINE_STRESS = 0.85`
  (stress threshold for famine); `DROUGHT_BASE_PROB = 0.15` ×aridity,
  `DROUGHT_SEVERITY = 0.35`; `PLAGUE_DENSITY_PROB = 0.010` ×density,
  `PLAGUE_MORTALITY = 0.18`; `INITIAL_FILL = 0.35`, `ARID_REF_PRECIP = 0.08`,
  aridity floor `0.15`, `MIN_POPULATION = 0.5` (in `lib.rs`).
* **Why these values:** famine is the Malthusian regulator and should be the
  *common* crisis of Turchin's demographic backbone, so its threshold sits
  below unity (`0.85`) while plague stays a rare density shock (`0.010`). The
  first draft (`FAMINE_STRESS 0.92`, `PLAGUE_DENSITY_PROB 0.05`) produced a
  plague-dominated log (71 plague / 7 famine / 6 drought on seed 42 — wrong
  shape). Retuned to **56 famine / 49 drought / 24 plague** over 500 years,
  spread evenly across all 4 seed-42 polities (25–41 events each) — famine-led,
  believable. Discrete logistic (`r = 0.03`) approaches capacity without
  overshoot; the dramatic secular *cycles* (overshoot → collapse) arrive with
  the 4d fiscal/elite coupling.
* **Method:** tuned against canonical seed 42 at 4k cells (event-kind mix +
  per-polity spread).
* **Source:** Phase 4b; `loops/turchin.rs` + `SimState` in `lib.rs`.

### Agent layer (4c) — `agent.rs`: reign / family knobs

* **Current:** `FOUNDER_AGE = 30`; `REIGN_MIN = 16` / `REIGN_MAX = 44`;
  `MARRY_AFTER = 3` (years into reign); `CHILD_INTERVAL = 4`;
  `MAX_CHILDREN = 4`.
* **Why these values:** reigns of 16–44 years (mean ~30) give ~12–18 rulers per
  polity over 500 years — enough turnover for a readable king-list without
  trivializing each reign. Marriage 3 years in, then a child every 4 years up to
  4, means most reigns produce ≥1 heir, so dynasties usually persist with the
  occasional line-failure → new dynasty (welcome variety). Seed 42: 4 dynasties,
  241 characters, 68 coronations / 64 deaths / 169 births over 500 years — a
  coherent genealogy on the first cut, **no retune required** (contrast 4b's
  plague-spam). Heirs can be crowned young (no age/regency gate); that's a known
  4c simplification — succession crises + minimum-age/regency land in 4f.
* **Method:** derived; sanity-checked against seed 42 (king-list reads as a
  plausible dynastic history; lineage reciprocity + chains asserted in
  `history_spec`).
* **Source:** Phase 4c; `agent.rs`.

### Turchin fiscal + Khaldun asabiyyah (4d) — `loops/turchin.rs` + `loops/khaldun.rs`

* **Current:** instability gains `IMMIS_W = 0.020` (above `IMMIS_THRESH = 0.75`
  density) / `ELITE_W = 0.020` / `FISCAL_W = 0.010` / `DECADENCE_W = 0.012`,
  baseline venting `INSTAB_RELAX = 0.004`, `CRISIS_THRESHOLD = 0.65`; crisis
  losses `CRISIS_POP_LOSS = 0.30` / `CRISIS_ELITE_LOSS = 0.60`; elites
  `ELITE_GAIN = 0.012` / `ELITE_DECAY = 0.02` / `ELITE_SUSTAIN = 0.35`; treasury
  `TAX_YIELD = 0.10` / `ELITE_UPKEEP = 0.20`; `EXPAND_PROB = 0.02`. Khaldun:
  `ASAB_HIGH = 0.9`, `ASAB_DECAY = 0.9955`/yr (~0.5 after ~140 years).
* **Why these values:** tuned so each polity runs **~3 secular boom/bust cycles**
  over 500 years (Turchin's century-scale cadence), not a single fall or
  constant churn. First drafts: `CRISIS_THRESHOLD = 1.0` gave only ~1 crisis per
  polity (too rare); lowering to `0.65` and raising `DECADENCE_W` to `0.012`
  gave ~3. The Exile (elite purge) gate was relaxed to any elite surplus —
  crises here are decadence-driven and hit before elite overproduction peaks, so
  the stricter gate never fired. Seed 42: crises cluster ~y223–243 / ~347–359 /
  ~446–462; crash-relief eases famine (56→42 vs. 4b alone).
* **Method:** tuned against seed 42 (crisis count per polity + event-kind mix);
  Khaldun decay pinned monotonic + bounded in a unit test.
* **Source:** Phase 4d; `loops/turchin.rs` (fiscal half) + `loops/khaldun.rs`.

### Mearsheimer wars (4e) — `loops/mearsheimer.rs`

* **Current:** `WAR_BASE = 0.020` per (aggressor, neighbour)/yr × power pressure
  × diplomatic multiplier (`EXPANSIONIST 2.5` / `HONORBOUND 1.4` / `ISOLATIONIST
  0.3` / else 1.0); `WAR_COOLDOWN = 12` yr; `TRANSFER_CELLS = 6`; battle salience
  `0.58 + 0.4·min(1, (Pa+Pb)/STAKES_REF)` with `STAKES_REF = 300`;
  `CLAIM_PROB = 0.012`/polity/yr. Power = population × (0.5 + military/100).
* **Why these values:** tuned vs. seed 42 to **~49 wars / 20 sieges** over 500
  years (≈1/decade across 4 polities) — a turbulent-but-legible military history,
  not constant war. Two failure modes fixed: (1) without a cooldown the strongest
  realm warred every few years (rich-get-richer runaway → 103 wars); the 12-year
  cooldown breaks it. (2) the first salience formula `(Pa+Pb)/(Pa+Pb+4)` saturated
  to ~1 for every war (132 "major" events); rescaling against `STAKES_REF = 300`
  makes only the largest wars read ≥ 0.8 (20 of 49). Also caught an `i32::MIN`
  cooldown-sentinel overflow that silenced all wars. `STAKES_REF` is the knob to
  retune if the population scale changes (it tracks territory capacity).
* **Method:** tuned against seed 42 (war/siege counts, casus-belli mix,
  count of salience ≥ 0.8); border conservation + war well-formedness asserted in
  `history_spec`.
* **Source:** Phase 4e; `loops/mearsheimer.rs`.

### Succession crises (4f) — `agent.rs`

* **Current:** `MIN_RULE_AGE = 16` (adult-first heir preference); `CONTEST_PROB
  = 0.30` (chance a ≥2-adult-heir death becomes a war of brothers).
* **Why these values:** addresses review finding #5 (4c could crown a 13-year-
  old) via skip-to-next-eligible — prefer an adult child; crown a minor only
  when none exists. `CONTEST_PROB = 0.30` keeps most successions peaceful
  (primogeniture) while ~1 in 3 multi-heir deaths becomes a contested civil war,
  for drama without spam. Seed 42: 11 contested successions over 500 years.
  Residual minor accessions (9/71 coronations under 16, min age 12) are the
  no-adult-heir cases — realistic boy-kings; full *regency* (an adult governs for
  the minor) is deferred, as is foreign-claim usurpation.
* **Method:** tuned against seed 42 (contested-succession count); the
  ≥2-claimants + follows-a-Death contract asserted in `history_spec`.
* **Source:** Phase 4f; `agent.rs` (`succeed` / `contested_succession`).

### Religious schism (4g) — `loops/schism.rs`

* **Current:** `SCHISM_PROB = 0.003` per religion per year; `ALIGN_DRIFT = 0.3`
  (alignment shift on one axis for the splinter sect).
* **Why these values:** `0.003` × ~2–3 religions × 500 years ≈ a handful of
  schisms (seed 42: 4) — faiths fracture occasionally, not constantly. The
  splinter inherits pantheon + founding culture and drifts one alignment axis by
  ±0.3 (a doctrinal divergence the chronicler can read). Different-faith polities
  without a dynastic claim fight `ReligiousSchism` wars (verified on a
  multi-faith world; `fixed(42)`'s adjacency happens not to trigger them).
  Per-cell sect *spread* (converting adherent cells) is deferred — out of scope
  for the event/entity layer.
* **Method:** tuned against seed 42 (schism count); sect-inherits-pantheon +
  drifts-alignment + parent-reference asserted in `history_spec`.
* **Source:** Phase 4g; `loops/schism.rs` + `polity_religion` in
  `loops/mearsheimer.rs`.

### Hero / megabeast (4h) — `loops/hero.rs`

* **Current:** `PROPHECY_PROB = 0.020`/yr, `MEGABEAST_PROB = 0.025`/yr,
  `SLAY_PROB = 0.18`/yr per active beast, `HERO_AGE = 25`.
* **Why these values:** legendary events must be *rare* (the DA flagged 4h as the
  most speculative loop — ship minimal). These give seed 42 ~7 hero-sagas over
  500 years (rise → champion → slaying → artifact → prophecy fulfilled), not a
  flood. `SLAY_PROB = 0.18` ⇒ a beast rampages ~5–6 years before a champion ends
  it (occasionally slain the same year if one is ready). Prophecies slightly
  rarer than beasts so some lapse unfulfilled (Phase-5 lacunae). All world-scale
  (not per-polity).
* **Method:** tuned against seed 42 (saga count + rarity); foreshadow→payoff
  cause links asserted in `history_spec`.
* **Source:** Phase 4h; `loops/hero.rs`.

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

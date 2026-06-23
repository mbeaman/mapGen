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

### Orographic release + post-depletion floor (6.1.1)

* **Current:** land precip `= base_precip · band · (0.085 + uw_m·release)`,
  `release = (0.20 + uplift·6.0).min(0.95)`. Was `(0.10 + uw_m·(0.25 +
  uplift·4.0).min(0.9))`.
* **Why these values:** the prior `base_precip` fix over-corrected — by lifting
  the whole distribution it left almost no arid land (seed 42: **1% desert, 46%
  forest** — "everywhere the same humidity"). Three coupled changes widen the
  spread: a stronger orographic term (`·6`) wrings windward slopes harder and
  leaves leeward/deep-interior air depleted; a slightly lower flat-land baseline
  (`0.20`); and critically a lower post-depletion **floor** (`0.085`, was `0.10`)
  so rain-shadow and subtropical cells finally cross the Köppen aridity cutoff.
  The floor is a knife-edge (precip clusters at the threshold): `0.10`→1% desert,
  `0.07`→36%, `0.04`→68%. `0.085` lands the Earth-like band.
* **Effect:** seed 42 → desert 23% / steppe 26% / forest 27%. Across seeds
  1/7/42/99: desert 17–30%, forest 24–35%, steppe 23–33% (no degenerate worlds).
* **Method:** tuned against seeds 1/7/42/99 (land biome histogram); realism band
  specs (equatorial-wettest, subtropical-drier) still hold. Both goldens
  re-anchored. **Note:** the release formula is duplicated in `climate.rs` and
  `climate_seasonal.rs` (kept in lockstep) — a shared helper is a worthwhile
  follow-up refactor.
* **Source:** Phase 6.1.1; `climate.rs` / `climate_seasonal.rs`.

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

### `MIN_CONTINENT_DIVISOR` / `MIN_OCEAN_DIVISOR`

* **Current:** `100` (continents) / `20` (oceans) — a body earns a name
  when `cell_count * DIVISOR >= total_cells`, i.e. continents ≥ 1% of
  the world, oceans ≥ 5%. **RE-CALIBRATED `40`→`100` (2026-06-09) for the
  longitude-periodic planet.**
* **Why this value:** the original `40` (2.5%) reproduced the planisphere's
  former render-time speck-skip on the FLAT world, where one dominant
  landmass set the scale. The periodic planet (the now-primary globe view)
  packs several genuine continents into the same cell budget, each a smaller
  fraction, so the 2.5% bar left real medium continents (1–2.5% of the world)
  UNNAMED — under-labeling the globe and, downstream, starving the far-shore
  chronicle: every inter-continental carrier (faith/colony/sword) tags only
  NAMED shores, so the three strands could never converge on one continent
  (no natural 3-strand far shore existed in seeds 0..200). Re-calibrating to
  1% names the periodic planet's true continents (seed 9: 5→6 labels — one
  previously-unnamed medium continent), restores the marquee three-strand
  chronicle (seed 27 → shore "Zuk", proven end-to-end in `lore_cli`), and
  leaves single-landmass CONTINENTAL worlds unchanged (they have no bodies in
  the 1–2.5% band, so `seed42_*` goldens hold byte-identical — only the
  `seed9_planet` golden re-anchored). The carrier taxonomy is unaffected (it
  keys off `SIZABLE_BODY_MIN`, independent of this divisor). Oceans keep `20`.
* **Guard:** `continents_spec::the_periodic_planet_names_every_continent_down_to_one_percent`
  pins seed 9's named set to EXACTLY the bodies ≥1% of the world — reverting the
  divisor trips it. This guards seed-9 NAMING COMPLETENESS; it is NOT the
  chronicle's drift guard (seed 9's smallest named body is ~1.88%, so its named set
  is invariant for divisors ~[54, 1000] — a range that already collapses the
  3-strand chronicle, which rides seed 27's shore "Zuk" at ~1.05%). The chronicle's
  recovery is guarded by `mapgen-cli/tests/lore_cli.rs` (seed 27).
* **Grounding:** a continent is named in the language of the culture that
  owns the most of its cells (stable lowest-id tiebreak); an ocean by the
  dominant culture among its coastal-adjacent land cells. Uninhabited
  bodies fall back to language 0. Naming runs as `name_world` step 8, RNG
  drawn after the mountain-range pass so all earlier names are unchanged.
* **Method:** derived (the renderer's prior 2.5% threshold; grounding +
  determinism pinned in `continents_spec`)
* **Source:** grounded-continent-names substage; `naming.rs` step 8

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

### Salience calibration (4j) — `STAKES_REF` + per-kind salience floors

* **Current:** `STAKES_REF = 2000.0` (was `300.0`); rare marquee events lifted into
  the "major" band (salience ≥ 0.8): `Schism 0.6 → 0.85`, `ProphecyFulfilled
  0.72 → 0.86`, `MegabeastSlain 0.85 → 0.88`, `MegabeastRise 0.7 → 0.80`, hero
  `Ascension 0.7 → 0.82`. High-frequency churn (Birth/Marriage/Death/Coronation/
  WarDeclared/routine battles/famine) deliberately stays below 0.8.
* **Why:** the phase-end review of `mapgen events --major` on seed 42 found the
  highlight reel was **96 near-identical field battles all pinned at 0.98**, while
  the genre's rare turning points (schisms, fulfilled prophecies, hero deeds) sat
  *below* the major bar. Two causes: (1) `STAKES_REF = 300` was far under the real
  combined-power distribution (probe on seed 42: range ≈ 251–2197, median ≈ 634),
  so `stakes = (Pa+Pb)/REF` saturated at 1.0 for ~75% of battles → flat 0.98.
  Re-anchoring `REF` to the high end (≈ 2000) makes battle salience *spread* across
  the range so only the top tier of wars clears 0.8. (2) the rare world-shaping
  events were undervalued relative to ubiquitous battles. Guiding rule, now
  documented in code: **salience ≈ consequence × rarity** — rare, world-altering
  events headline; common churn does not.
* **Effect on seed 42:** major-event count 96 → 61, now a *gradient* (0.86–0.98)
  rather than a flat wall, and megabeast slayings / prophecies fulfilled / schisms
  appear in the reel. The residual battle density is one expansionist hegemon
  (Uedihi) genuinely dominating the late era — a real historical pattern, left
  intact. **Deliberately not built:** a tunable multi-factor salience optimizer
  (the plan's "Refinery anti-pattern"); a first-of-kind/novelty discount that would
  de-rank repeated identical battle summaries — deferred, would need the post-sim
  pass that 4i left out.
* **Method:** instrumented `resolve_war` with a throwaway combined-power probe to
  measure the real distribution, then chose `REF` ≈ its max; eyeballed the reel.
* **Source:** Phase 4j; `loops/mearsheimer.rs` (`STAKES_REF`), `loops/hero.rs`,
  `loops/schism.rs`.

### Review-driven uplevel (4k) — phrasing, novelty discount, arc/age weave

A deep multi-agent review after Phase 4 flagged the *presentation* layer
(repetitive reel, mislabeled arcs, monotone ages). The substrate (correctness,
determinism, schema, fmath) was clean. Changes, all keeping the loop *dynamics*
intact:

* **Battle/siege phrasing** (`mearsheimer.rs`) now varies with the victory margin
  (narrow → "narrowly bested"; rout → "crushed/routed/shattered") and reflects
  defender upsets ("repelled the invasion"); conquest size scales with
  decisiveness (4..=8 holdings, was a fixed 6). Kills the "X defeated Y in the
  field" monotony.
* **Salience novelty discount** (`lib.rs::refine_salience`, `REPEAT_DECAY = 0.6`):
  a verbatim-repeated summary is discounted by `0.6^prior`, so the first instance
  headlines and identical repeats recede. Keyed on the summary *text* (a hegemon's
  serial battles span many rulers but render identically); distinct events keep
  unique text. This is the post-sim pass 4j deferred. seed 42 major-event count
  96 → 61 (4j) → 39 (4k).
* **Arc weave** (`extract.rs` + `hero.rs` + `schism.rs`): the hero cassette is
  causally chained (rise→ascension→slaying→artifact→prophecy) so HeroSaga arcs
  form (were 0). HolyWar now requires an actual `Schism` event in the thread — a
  *war of religion* is orthodox-vs-sect (the schism→war cause edge weaves them),
  not any inter-faith border war, which dropped seed-42 HolyWars 4 → 1 (accurate).
  New `ArcKind::Conquest` catches plain expansion wars (was Chronicle). Arc bar:
  a thread must span years or be a ≥4-event cluster (no lone one-year skirmishes).
  Titles disambiguated with regnal numerals. seed 42: 52 → 20 well-classified arcs.
* **Mythic ages** (`extract.rs::frame_ages`): classified *relative to the
  timeline's own average* (above-mean catastrophe → Dark, above-mean heroism →
  Heroic, quiet → Golden). Absolute thresholds had pinned every age the same;
  "Dark" now counts only acute catastrophes (realms falling / plague / migration),
  not baseline war/famine. Golden is reachable (seed 1: Founding/Golden/Dark/Heroic);
  seed 42 reads all-Dark because it genuinely is a conquest-grim world.
* **Polish (built in a follow-up):** (a) hero-event phrasing variety — 3
  deterministic phrasings each for prophecy/rise/ascension/slaying/forging/
  fulfilment, so monster-heavy seeds don't read as one template; (b)
  `GREAT_BEAST_PROB = 0.30` — ~30% of risen beasts are "great wyrms" too mighty
  to be slain (never enter the slay queue), leaving standing menaces / Phase-5
  lacunae instead of a 100% kill rate (seed 11: 20 rises, 12 slain); (c)
  `age_name` picks among several epithets per motif by age index, so a grim seed
  reads "Founding / Shadow / Ruin / Woe" rather than "Strife" thrice.
* **Source:** Phase 4k; `loops/{mearsheimer,hero,schism}.rs`, `lib.rs`,
  `extract.rs`, `entities.rs`/`history.rs` (schema v12 Megabeast, v13 Conquest).

## Cross-platform determinism — both goldens re-anchored (6.2)

**Not a tuning change** — recorded here so the golden re-anchor at this commit
isn't mistaken for parameter drift. 6.2 wired the first real native↔wasm golden
(`crates/mapgen-wasm/tests/cross_platform.rs`: run the full pipeline in wasm32
via Node, assert byte-identity with the native golden). It immediately exposed
**three** silent divergences — the "byte-identical" contract had never actually
been executed cross-platform before:

1. **`poisson.rs` — `gen_range(0..active.len())`** over a `usize` range. `rand`'s
   integer sampler draws a different number of RNG bytes for `u64` (64-bit
   native) vs `u32` (wasm32), desyncing the whole stream from the first active
   pick → a *different mesh* (3612 vs 3631 points). Fixed by sampling over an
   explicit `u64` range (`0..active.len() as u64`). **No-op on native** (usize
   *is* u64 there), so this fix alone changed nothing.
2. **`climate::band_precip` — `.exp()`** (and a no-op `.sqrt()` in
   `wind_vector`). std `exp` resolves to the *system* libm natively but a
   *different* libm baked into the wasm binary; they disagree in the last ULP.
   Routed through `fmath::exp`. This **did** move the native precip values onto
   the shared libm result → **both goldens re-anchored** (phase2
   `39afce5e…`→`f2e8f6f9…`, full `0b29e65c…`→`2c9e034c…`). Distribution
   unchanged at the aggregate level (every realism/svg/visual test still passes).
3. **Latent (didn't bite seed 42, would bite others): `cultures` `.powf()`,
   `patch` `.hypot()`.** Routed through `fmath`. No golden impact for seed 42.

**`sqrt` is intentionally left raw** — IEEE-754 *requires* it correctly rounded,
so x86 SSE2 and wasm `f32.sqrt` are bit-identical (unlike sin/cos/exp/ln/pow).
The `fmath` purity guard (was mapgen-history-only) now scans **every crate's
`src/`** so a future raw transcendental fails fast in `just check`; the wasm
runtime golden runs in CI and via `just test-wasm`.

## USDA soil orders + WETLAND biome (6.1.4)

New per-cell soil layer (`soils.rs`, schema v14) classified from climate +
drainage + relief, plus a soil-driven `WETLAND` biome. Thresholds were
calibrated by sweeping the order distribution across seeds 1/7/42/99/123 (a
throwaway example), with these findings worth recording:

- **`flow` is raw accumulation counts, not normalized** (land median ~2, p90
  ~20, max ~500 on a 4k-cell continent). The Entisol floodplain rule keys off
  `flow > 25` (≈p90) with `relief < 0.08`; an early draft using `flow > 0.08`
  matched ~everything and made Entisol 56% of land.
- **Aridity must use `p_annual = p_summer + p_winter`, not `climate.precipitation`**
  (which is the half-year *mean*). Using the mean halved every threshold and put
  66% of land in Aridisol. With `p_annual` matched to `koppen::classify_one`,
  Aridisol = the true `BW` desert (`< 0.75×` the arid threshold); the wetter
  `BS` steppe band falls through to **Mollisol** (semi-arid grassland soils),
  which is where prairie/steppe soils actually form.
- **The tropical gate must use the *coldest* month** (`t_cold > 0.55`, Köppen's
  A criterion), not the warmest — temperate regions also have hot summers, so
  gating on `t_warm` leaked them into the tropical (Ultisol) branch and starved
  Mollisol/Alfisol entirely.
- **Resulting character (seed 42):** Mollisol ~33% + Aridisol ~31% + Inceptisol
  ~20% dominate — i.e. grassland + desert + mountain soils, which matches these
  dry, mountainous continents. Oxisol/Vertisol/Andisol are absent for lack of
  hot-wet-tropical lowland / a volcanism model (documented, not a bug). A guard
  test fails if any single order exceeds 75% of land.
- **WETLAND** (waterlogged Histosols: `relief < 0.035 && elev < 0.30 &&
  p_annual > 0.25`) lands at 0.0–0.8% of land across seeds — rare on arid worlds,
  scaling up on wetter ones (seed 99). Left deliberately conservative so marshes
  read as special rather than blanketing low ground.

## Strahler order + seasonal river regime (6.1.5)

New per-cell + per-river Strahler order and a per-river seasonal regime (schema
v15). Findings from sweeping seeds 1/7/42/99/123:

- **Strahler order tops out at 2–3 on 4k-cell continents** (even just 2 on seed
  123). The networks are small (10–39 rivers) and order increments only when two
  *equal*-order streams meet, which is rare with few tributaries. Consequences:
  - **Render width stays on √flow, not Strahler.** √flow spans ~0.6–5.0 px on
    seed 42 (flow up to ~530); order-based width would be a flat 1.2–2.6. So
    Strahler would *coarsen* the river hierarchy at this resolution — kept it as
    classification data instead.
  - **No Strahler naming gate.** Gating "name a river" on order ≥ 3 would strip
    every river name on seed 123 (max order 2). Naming stays length-based.
  - Strahler will be more expressive at 15k–30k cells and is the rank signal the
    atlas LOD selection wants (ADR 0001 §3), so it's shipped as data regardless.
- **Regime classifies well and *is* consumed:** thresholds — Ephemeral if
  catchment `p_annual < 0.12`; else Nival if the headwater's winter temp < 0
  (freezing → snowpack); else Summer-monsoon / Winter-rain if one half-year
  exceeds 1.5× the other; else Perennial. Seed 42 → Ephemeral 20 / Perennial 10
  / Nival 5 / Monsoon 2 / Winter-rain 2. **Ephemeral rivers render dashed** (the
  intermittent-stream cartographic convention) — the visible payoff of 6.1.5.

## Render generalisation — `mapgen-render` (cartographic LOD, ADR 0001 §3)

The detail-DECREASING knobs for the zoom-out overview (planet / globe). Render-only
(the planet SVG is never hashed), so these never move a golden — calibrate by eye.

### `RIVER_SIMPLIFY_TOLERANCE` (`style/planet.rs`)

* **Current:** `60.0` (world-units², a *doubled*-triangle-area threshold for
  Visvalingam–Whyatt on the overview's major rivers).
* **Why this value:** the planet preset spaces cells ~10 world-units apart, so a
  river vertex whose triangle with its neighbours is under ~½ a cell² is sub-scale
  wiggle the planisphere can't resolve. Dropping it cleans the line and shrinks the
  SVG without moving the river's course.
* **Measured (seed 11 — the only canonical planet seed with Strahler≥4 rivers):**
  12 major rivers, **183 raw cells → 125 drawn points (~31% fewer)**; per-river
  counts 5–18, so no river collapses to a straight segment (shape preserved). Lower
  for gentler simplification, raise for a barer overview.
* **Next.** A per-level tolerance — the planet render is always level-0 so a fixed
  value suffices here; the per-level stylesheet matters once the ornate render
  simplifies at depth.

### `COAST_SIMPLIFY_TOLERANCE` (`style/planet.rs`)

* **Current:** `40.0` (world-units², doubled-triangle-area for Visvalingam–Whyatt on
  the overview coastline). Lower than the river value because coast vertices sit ~a
  cell-edge apart (~5–10 world units) — closer than river cell-centres — so a smaller
  threshold drops the same sub-scale crenellation.
* **Why this approach.** The coast is traced into continuous land/sea loops (the
  SHARED `extract_coastline_polylines` the ornate ripples use — one coastline
  extraction) and each loop simplified in world space, replacing the former
  per-coastal-cell polygon outlines (a crenellated band of full Voronoi hexagons).
* **Measured (seed 11):** 27 coast loops, ~1089 drawn points (well under the land/sea
  boundary-edge count) — a clean single coast line, per-cell biome fills preserved
  beneath. Raise for a smoother (more stylised) coast, lower to hug the cells.

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

## ShadeParams (relief R1, 2026-06-11)
`strength: 14.0, ambient: 0.6, light: [-1,-1,1.25]` (NW, raster space). Ground
truth: the relief spike's screenshot matrix (hillshade alone carries ~90% of the
3D reading) + the R1 native artifact (planet-42 L3 (1,3): land models gently,
sea byte-identical). `ambient` must stay ≥ 0.5 — the flat-field ≡ 1.0 exactness
contract (`relief.rs::flat_field_shading_is_a_byte_noop`) relies on Sterbenz.
Feel-judged; revisit in the R4 quality pass with oblique screenshots.

## VERT_EXAG (relief R2, 2026-06-11)
`0.023` (web/src/relief.ts): peak raw land elevation ~0.86 (seed-42 stats) →
~2% of sphere radius — the spike's screenshot-verified displacement. Heights
are RAW per-world values (never normalized — seam bit-identity + one fewer
derived constant), so low-relief worlds displace proportionally less.
Skirt depth 0.004; grid 129×65. Feel-judged at the R4 pass.

## Tilt camera (relief R3, 2026-06-11)
`PITCH_SPEED 0.005` rad/px (web/src/globe.ts): a ~175 px Shift/right-drag spans
the full 0→50° envelope — fast enough to reach the limb in one gesture, slow
enough to frame. `MAX_PITCH 50°` (web/src/camera.ts): the relief spike's MEASURED
oblique envelope; 55° was an extrapolation and rejected (the set-edge/limb shading
cliff, §9.4, dominates past it). Terrain-clearance constants (all derived, NOT
feel-tuned): `R_TER 1.024` = patch base 1.001 + max raw elevation × VERT_EXAG;
`CLEAR_MARGIN 0.005` → `R_SAFE 1.029` (the pitch clamp's binding margin at the
steepest tilt); near `NEAR_FRAC 0.5`, `NEAR_MIN 0.002` (half the radial clearance,
floored at depth precision, capped at the legacy 0.1). These are POSE INVARIANTS
swept in camera.test.ts, not knobs to taste. Depth-precision banding at near 0.002
/ far 100 under deep tilt is the R4 screenshot check.

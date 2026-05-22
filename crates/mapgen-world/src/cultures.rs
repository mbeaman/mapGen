//! Cultures stage. Per-cell habitat-fitness scoring across a roster of 4-5
//! MVP race archetypes loaded from `crates/mapgen-world/data/race_archetypes
//! .csv` (data, not code — per ARCHITECTURE.md §5.5 "data not code"). Weighted
//! Voronoi assignment writes `culture_id` per land cell; sea cells stay
//! `None`.
//!
//! Pipeline position: runs after `biomes::classify` (needs biome + climate +
//! hydrology) and before any polities stage (which keys off culture).
//!
//! Spec: `crates/mapgen-world/tests/cultures_spec.rs`.
//!
//! Architecture: `docs/ARCHITECTURE.md` §3 (data flow), §4 Phase 3a (exit
//! criteria), §5.5 "What races want" (archetype-to-habitat utility table).

use mapgen_core::entities::{
    Alignment, Architecture, Culture, DiplomaticPattern, MagicStyle, Race, SettlementIcon, TechEra,
    TechProfile,
};
use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

/// Tunables for the cultures stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct CulturesParams {
    /// Target number of cultures per world. Subject to habitat availability —
    /// a world with no high-mountain cells produces no Dwarf (Mountain)
    /// culture even if the target permits it.
    pub target_cultures: usize,
    /// Minimum mean habitat-fitness for a culture's assigned cells. Cultures
    /// whose mean fitness falls below this threshold are discarded and their
    /// cells reassigned to the next-best candidate, with a floor of two
    /// surviving cultures (distribution-non-trivial invariant). Per
    /// ARCHITECTURE.md §4 Phase 3a: "no culture's average habitat-score
    /// below 0.3."
    pub min_habitat_fitness: f32,
}

impl Default for CulturesParams {
    fn default() -> Self {
        Self {
            target_cultures: 5,
            min_habitat_fitness: 0.3,
        }
    }
}

/// One row of `data/race_archetypes.csv`. The renderer/lore engine reads
/// `Culture` fields downstream; the cultures stage uses the habitat-preference
/// fields (`preferred_biomes` / `temp_range` / `elev_range` / `water_weight`)
/// to score per-cell fitness.
///
/// Public for the spec to introspect fitness against the same source of truth
/// the assignment uses.
#[derive(Clone, Debug, PartialEq)]
pub struct RaceArchetype {
    pub race: Race,
    pub archetype_name: String,
    pub culture_name: String,
    pub preferred_biomes: Vec<u8>,
    pub temp_range: (f32, f32),
    pub elev_range: (f32, f32),
    /// `0.0` = the culture doesn't care about water, `1.0` = strongly prefers
    /// coast/river/lake adjacency. Linear blend in fitness.
    pub water_weight: f32,
    pub alignment: Alignment,
    pub tech: TechProfile,
    pub magic: MagicStyle,
    pub settlement: SettlementIcon,
    pub architecture: Architecture,
    pub diplomatic: DiplomaticPattern,
}

impl RaceArchetype {
    /// Project the archetype's Culture-struct fields onto a fresh `Culture`.
    /// `name` is the per-world chosen culture display name; `archetype_id`
    /// is the row index in the loaded roster.
    pub fn to_culture(&self, archetype_id: u16) -> Culture {
        Culture {
            name: self.culture_name.clone(),
            race: self.race,
            archetype_id,
            language_id: 0,
            religion_id: None,
            alignment: self.alignment,
            tech: self.tech,
            magic: self.magic,
            settlement: self.settlement,
            architecture: self.architecture,
            diplomatic: self.diplomatic,
        }
    }
}

const ARCHETYPES_CSV: &str = include_str!("../data/race_archetypes.csv");

/// Parse the embedded `race_archetypes.csv` into a Vec.
///
/// Panics on malformed rows: this is a data file we ship with the binary, so
/// a parse failure is a bug, not a user error. Cheap enough (5-row file) that
/// we re-parse on every `populate` call; if perf becomes an issue, cache in a
/// `LazyLock`.
pub fn load_archetypes() -> Vec<RaceArchetype> {
    parse_archetypes(ARCHETYPES_CSV).expect("baked-in race_archetypes.csv must parse")
}

fn parse_archetypes(csv: &str) -> Result<Vec<RaceArchetype>, String> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| "race_archetypes.csv is empty".to_string())?;
    // Sanity check column count — guards against silent column reorderings.
    let expected_cols = 21;
    let actual_cols = header.split(',').count();
    if actual_cols != expected_cols {
        return Err(format!(
            "race_archetypes.csv header has {actual_cols} columns; expected {expected_cols}"
        ));
    }

    let mut out = Vec::new();
    for (i, line) in lines.enumerate() {
        let row_no = i + 2; // 1-indexed header + 1-indexed data row
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() != expected_cols {
            return Err(format!(
                "race_archetypes.csv row {row_no}: {} columns, expected {expected_cols}",
                cols.len()
            ));
        }
        out.push(parse_row(&cols, row_no)?);
    }
    Ok(out)
}

fn parse_row(cols: &[&str], row_no: usize) -> Result<RaceArchetype, String> {
    let race = parse_race(cols[0]).map_err(|e| format!("row {row_no} race: {e}"))?;
    let archetype_name = cols[1].to_string();
    let culture_name = cols[2].to_string();
    let preferred_biomes =
        parse_biome_list(cols[3]).map_err(|e| format!("row {row_no} preferred_biomes: {e}"))?;
    let temp_min = parse_f32(cols[4]).map_err(|e| format!("row {row_no} temp_min: {e}"))?;
    let temp_max = parse_f32(cols[5]).map_err(|e| format!("row {row_no} temp_max: {e}"))?;
    let elev_min = parse_f32(cols[6]).map_err(|e| format!("row {row_no} elev_min: {e}"))?;
    let elev_max = parse_f32(cols[7]).map_err(|e| format!("row {row_no} elev_max: {e}"))?;
    let water_weight = parse_f32(cols[8]).map_err(|e| format!("row {row_no} water_weight: {e}"))?;
    let law_chaos = parse_f32(cols[9]).map_err(|e| format!("row {row_no} law_chaos: {e}"))?;
    let good_evil = parse_f32(cols[10]).map_err(|e| format!("row {row_no} good_evil: {e}"))?;
    let era = parse_tech_era(cols[11]).map_err(|e| format!("row {row_no} tech_era: {e}"))?;
    let metallurgy = parse_u8(cols[12]).map_err(|e| format!("row {row_no} metallurgy: {e}"))?;
    let agriculture = parse_u8(cols[13]).map_err(|e| format!("row {row_no} agriculture: {e}"))?;
    let naval = parse_u8(cols[14]).map_err(|e| format!("row {row_no} naval: {e}"))?;
    let military = parse_u8(cols[15]).map_err(|e| format!("row {row_no} military: {e}"))?;
    let arcane = parse_u8(cols[16]).map_err(|e| format!("row {row_no} arcane: {e}"))?;
    let magic =
        parse_magic_style(cols[17]).map_err(|e| format!("row {row_no} magic_style: {e}"))?;
    let settlement = parse_settlement_icon(cols[18])
        .map_err(|e| format!("row {row_no} settlement_icon: {e}"))?;
    let architecture =
        parse_architecture(cols[19]).map_err(|e| format!("row {row_no} architecture: {e}"))?;
    let diplomatic = parse_diplomatic_pattern(cols[20])
        .map_err(|e| format!("row {row_no} diplomatic_pattern: {e}"))?;

    Ok(RaceArchetype {
        race,
        archetype_name,
        culture_name,
        preferred_biomes,
        temp_range: (temp_min, temp_max),
        elev_range: (elev_min, elev_max),
        water_weight,
        alignment: Alignment {
            law_chaos,
            good_evil,
        },
        tech: TechProfile {
            era,
            metallurgy,
            agriculture,
            naval,
            military,
            arcane,
        },
        magic,
        settlement,
        architecture,
        diplomatic,
    })
}

fn parse_race(s: &str) -> Result<Race, String> {
    match s {
        "Human" => Ok(Race::Human),
        "Elf" => Ok(Race::Elf),
        "Dwarf" => Ok(Race::Dwarf),
        "Orc" => Ok(Race::Orc),
        "Halfling" => Ok(Race::Halfling),
        "Lizardfolk" => Ok(Race::Lizardfolk),
        "SeaFolk" => Ok(Race::SeaFolk),
        "Underdark" => Ok(Race::Underdark),
        "Giant" => Ok(Race::Giant),
        other => Err(format!("unknown race {other:?}")),
    }
}

fn parse_tech_era(s: &str) -> Result<TechEra, String> {
    match s {
        "Stone" => Ok(TechEra::Stone),
        "Bronze" => Ok(TechEra::Bronze),
        "Iron" => Ok(TechEra::Iron),
        "Classical" => Ok(TechEra::Classical),
        "Medieval" => Ok(TechEra::Medieval),
        "Renaissance" => Ok(TechEra::Renaissance),
        other => Err(format!("unknown tech_era {other:?}")),
    }
}

fn parse_magic_style(s: &str) -> Result<MagicStyle, String> {
    match s {
        "None" => Ok(MagicStyle::None),
        "Highmagic" => Ok(MagicStyle::Highmagic),
        "LowMagic" => Ok(MagicStyle::LowMagic),
        "Wild" => Ok(MagicStyle::Wild),
        "Divine" => Ok(MagicStyle::Divine),
        "Ancestral" => Ok(MagicStyle::Ancestral),
        other => Err(format!("unknown magic_style {other:?}")),
    }
}

fn parse_settlement_icon(s: &str) -> Result<SettlementIcon, String> {
    match s {
        "Castle" => Ok(SettlementIcon::Castle),
        "Tower" => Ok(SettlementIcon::Tower),
        "Hall" => Ok(SettlementIcon::Hall),
        "Spire" => Ok(SettlementIcon::Spire),
        "Longhouse" => Ok(SettlementIcon::Longhouse),
        "Treehouse" => Ok(SettlementIcon::Treehouse),
        "Gate" => Ok(SettlementIcon::Gate),
        "Yurt" => Ok(SettlementIcon::Yurt),
        other => Err(format!("unknown settlement_icon {other:?}")),
    }
}

fn parse_architecture(s: &str) -> Result<Architecture, String> {
    match s {
        "Classical" => Ok(Architecture::Classical),
        "Gothic" => Ok(Architecture::Gothic),
        "Organic" => Ok(Architecture::Organic),
        "Megalithic" => Ok(Architecture::Megalithic),
        other => Err(format!("unknown architecture {other:?}")),
    }
}

fn parse_diplomatic_pattern(s: &str) -> Result<DiplomaticPattern, String> {
    match s {
        "Isolationist" => Ok(DiplomaticPattern::Isolationist),
        "Mercantile" => Ok(DiplomaticPattern::Mercantile),
        "Expansionist" => Ok(DiplomaticPattern::Expansionist),
        "Tributary" => Ok(DiplomaticPattern::Tributary),
        "HonorBound" => Ok(DiplomaticPattern::HonorBound),
        "Egalitarian" => Ok(DiplomaticPattern::Egalitarian),
        other => Err(format!("unknown diplomatic_pattern {other:?}")),
    }
}

fn parse_biome_list(s: &str) -> Result<Vec<u8>, String> {
    s.split('|')
        .map(|part| {
            part.parse::<u8>()
                .map_err(|e| format!("bad biome id {part:?}: {e}"))
        })
        .collect()
}

fn parse_f32(s: &str) -> Result<f32, String> {
    s.parse::<f32>().map_err(|e| format!("bad f32 {s:?}: {e}"))
}

fn parse_u8(s: &str) -> Result<u8, String> {
    s.parse::<u8>().map_err(|e| format!("bad u8 {s:?}: {e}"))
}

/// Habitat-fitness score in `[0, 1]` for placing `archetype` at `cell` in
/// `world`. Sea cells score 0. Geometric mean across four axes:
///
/// * **Biome match** — 1.0 if cell's biome appears in
///   `archetype.preferred_biomes`, else a floor of 0.2 (some scoring lets
///   cultures bleed into non-preferred biomes when other axes are strong).
/// * **Temperature tent** — 1.0 inside `archetype.temp_range`, linear
///   falloff to 0 over a 0.2-wide buffer outside.
/// * **Elevation tent** — same shape, against `archetype.elev_range`.
/// * **Water proximity** — 1.0 if the cell is on a coast / river / lake;
///   else `1.0 - 0.6 × archetype.water_weight` (a culture that doesn't care
///   about water pays no penalty; a river-valley culture pays heavily for
///   inland placement).
///
/// Geometric mean is the architecturally-relevant call: arithmetic mean
/// rewards generalists scoring (0.5, 0.5, 0.5, 0.5) the same as a specialist
/// scoring (1.0, 1.0, 0.2, 0.2). Phase 3a's "earned worlds" want specialists
/// winning their niche.
pub fn habitat_fitness(world: &WorldData, cell: usize, archetype: &RaceArchetype) -> f32 {
    // Public entry recomputes near-water on the fly (O(rivers + lakes)). Hot
    // paths inside `populate` use the cached variant `habitat_fitness_with`.
    let near_water = cell_is_near_water(world, cell);
    habitat_fitness_with(world, cell, archetype, near_water)
}

fn habitat_fitness_with(
    world: &WorldData,
    cell: usize,
    archetype: &RaceArchetype,
    near_water: bool,
) -> f32 {
    if world.terrain.elevation[cell] < 0.0 {
        return 0.0;
    }
    let biome = world.climate.biome[cell];
    let biome_score = if archetype.preferred_biomes.contains(&biome) {
        1.0
    } else {
        0.2
    };

    let temp_score = tent(
        world.climate.temperature[cell],
        archetype.temp_range.0,
        archetype.temp_range.1,
    );
    let elev_score = tent(
        world.terrain.elevation[cell],
        archetype.elev_range.0,
        archetype.elev_range.1,
    );
    let water_score = if near_water {
        1.0
    } else {
        1.0 - 0.6 * archetype.water_weight
    };

    let product = biome_score * temp_score * elev_score * water_score;
    product.powf(0.25).clamp(0.0, 1.0)
}

fn tent(value: f32, lo: f32, hi: f32) -> f32 {
    const FALLOFF: f32 = 0.2;
    if value >= lo && value <= hi {
        1.0
    } else if value < lo {
        (1.0 - (lo - value) / FALLOFF).max(0.0)
    } else {
        (1.0 - (value - hi) / FALLOFF).max(0.0)
    }
}

fn cell_is_near_water(world: &WorldData, cell: usize) -> bool {
    if cell < world.mesh.coast.len() && world.mesh.coast[cell] {
        return true;
    }
    let cell_u32 = cell as u32;
    for river in &world.hydrology.rivers {
        if river.cells.contains(&cell_u32) {
            return true;
        }
    }
    for lake in &world.hydrology.lakes {
        if lake.cells.contains(&cell_u32) {
            return true;
        }
    }
    false
}

fn precompute_near_water(world: &WorldData) -> Vec<bool> {
    let n = world.mesh.cell_count();
    let mut near_water = vec![false; n];
    for (i, &is_coast) in world.mesh.coast.iter().enumerate().take(n) {
        if is_coast {
            near_water[i] = true;
        }
    }
    for river in &world.hydrology.rivers {
        for &cell in &river.cells {
            let idx = cell as usize;
            if idx < n {
                near_water[idx] = true;
            }
        }
    }
    for lake in &world.hydrology.lakes {
        for &cell in &lake.cells {
            let idx = cell as usize;
            if idx < n {
                near_water[idx] = true;
            }
        }
    }
    near_water
}

/// Mean habitat-fitness across all land cells currently assigned to
/// `culture_idx` (an index into `world.cultures.cultures`). Returns 0.0 if
/// the culture has no assigned cells or if `culture_idx` is out of bounds.
///
/// The archetype lookup goes through `load_archetypes()` so the returned
/// value matches what `populate` would compute. Public for use by spec
/// tests (the deferred ARCHITECTURE.md §4 Phase 3a "no culture's average
/// habitat-score below 0.3" assertion).
pub fn mean_habitat_fitness(world: &WorldData, culture_idx: u16) -> f32 {
    let culture = match world.cultures.cultures.get(culture_idx as usize) {
        Some(c) => c,
        None => return 0.0,
    };
    let archetypes = load_archetypes();
    let archetype = match archetypes.get(culture.archetype_id as usize) {
        Some(a) => a,
        None => return 0.0,
    };
    let near_water = precompute_near_water(world);

    let mut sum = 0.0_f32;
    let mut count = 0usize;
    for (cell, slot) in world.cultures.culture_id.iter().enumerate() {
        if let Some(id) = slot {
            if *id == culture_idx {
                sum += habitat_fitness_with(world, cell, archetype, near_water[cell]);
                count += 1;
            }
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f32
    }
}

/// Populate `world.cultures` — roster + per-cell assignment.
///
/// Writes `world.cultures.cultures` (the roster) and
/// `world.cultures.culture_id` (per-cell back-reference, `Some(u16)` for land
/// cells the stage assigned, `None` otherwise).
///
/// Preconditions: `world.mesh`, `world.terrain`, `world.climate`, and
/// `world.hydrology` must be populated. Biome classification (`world.climate
/// .biome`) is required since archetype habitat scores key off biome.
///
/// The `rng` parameter is reserved for future tie-breaking and per-world
/// archetype-trait jitter; today's algorithm is fully deterministic from
/// world state alone.
pub fn populate(world: &mut WorldData, params: CulturesParams, _rng: &mut ChaCha8Rng) {
    let n = world.mesh.cell_count();
    if n == 0 {
        return;
    }
    let archetypes = load_archetypes();
    if archetypes.is_empty() {
        return;
    }

    let near_water = precompute_near_water(world);

    // 1. Pick a seed cell for each archetype: the unclaimed land cell where
    //    that archetype's fitness is highest. Processed in CSV order — first
    //    archetype grabs the globally best fit for itself, subsequent
    //    archetypes pick from what remains. Ties broken by lower cell index.
    let seeds = pick_seed_cells(world, &archetypes, &near_water);

    // 2. Multi-source BFS Voronoi fill. Each newly-frontier cell adopts the
    //    culture of the already-assigned neighbor whose archetype scores
    //    highest *at this cell*. Produces contiguous territories that
    //    respect per-cell fitness — the architectural shape from
    //    ARCHITECTURE.md §3.
    let assignment = voronoi_fill(world, &archetypes, &seeds, &near_water);

    // 3. Cull cultures whose mean habitat-fitness falls below the floor
    //    (ARCHITECTURE.md §4 Phase 3a: "no culture's average habitat-score
    //    below 0.3"). Iterates to fixed point or 5 passes, whichever
    //    comes first. Always keeps ≥ 2 survivors (distribution-non-trivial
    //    invariant).
    let (final_assignment, survives) =
        cull_to_threshold(world, &archetypes, assignment, &near_water, &params);

    // 4. Build the final roster — only surviving archetypes get a Culture.
    let mut final_cultures = Vec::new();
    for (i, archetype) in archetypes.iter().enumerate() {
        if survives[i] {
            let new_id = final_cultures.len() as u16;
            final_cultures.push(archetype.to_culture(i as u16));
            debug_assert!((new_id as usize) < final_cultures.len());
        }
    }

    world.cultures.cultures = final_cultures;
    world.cultures.culture_id = final_assignment;
}

fn pick_seed_cells(
    world: &WorldData,
    archetypes: &[RaceArchetype],
    near_water: &[bool],
) -> Vec<usize> {
    let n = world.mesh.cell_count();
    let mut seeds = Vec::with_capacity(archetypes.len());
    let mut taken = vec![false; n];
    for archetype in archetypes {
        let mut best_cell = 0usize;
        let mut best_fit = f32::NEG_INFINITY;
        let mut found = false;
        for cell in 0..n {
            if taken[cell] {
                continue;
            }
            if world.terrain.elevation[cell] < 0.0 {
                continue;
            }
            let f = habitat_fitness_with(world, cell, archetype, near_water[cell]);
            if f > best_fit {
                best_fit = f;
                best_cell = cell;
                found = true;
            }
        }
        if found {
            taken[best_cell] = true;
            seeds.push(best_cell);
        }
    }
    seeds
}

fn voronoi_fill(
    world: &WorldData,
    archetypes: &[RaceArchetype],
    seeds: &[usize],
    near_water: &[bool],
) -> Vec<Option<u16>> {
    let n = world.mesh.cell_count();
    let mut assignment: Vec<Option<u16>> = vec![None; n];

    // Seed each starting cell with its culture.
    for (culture_idx, &seed) in seeds.iter().enumerate() {
        assignment[seed] = Some(culture_idx as u16);
    }

    // Multi-source BFS. `frontier` holds cells added in the previous round
    // (sorted by cell index for deterministic neighbor-visit order).
    let mut frontier: Vec<u32> = seeds.iter().map(|s| *s as u32).collect();
    frontier.sort();

    while !frontier.is_empty() {
        let mut next: Vec<u32> = Vec::new();
        for &cell_u32 in &frontier {
            let cell = cell_u32 as usize;
            for &neighbor in &world.mesh.neighbors[cell] {
                let neighbor = neighbor as usize;
                if assignment[neighbor].is_some() {
                    continue;
                }
                if world.terrain.elevation[neighbor] < 0.0 {
                    continue;
                }
                // Among already-assigned neighbors, pick the culture whose
                // archetype scores highest on *this* cell. First seen wins
                // on tie (neighbors are mesh-deterministic).
                let mut best_culture: Option<u16> = None;
                let mut best_fit = f32::NEG_INFINITY;
                for &nn in &world.mesh.neighbors[neighbor] {
                    let nn = nn as usize;
                    if let Some(c) = assignment[nn] {
                        let f = habitat_fitness_with(
                            world,
                            neighbor,
                            &archetypes[c as usize],
                            near_water[neighbor],
                        );
                        if f > best_fit {
                            best_fit = f;
                            best_culture = Some(c);
                        }
                    }
                }
                if let Some(c) = best_culture {
                    assignment[neighbor] = Some(c);
                    next.push(neighbor as u32);
                }
            }
        }
        next.sort();
        next.dedup();
        frontier = next;
    }

    // Pickup pass: land cells that were unreachable through neighbor BFS
    // (e.g., surrounded only by sea cells in a degenerate mesh) get the
    // global argmax across all archetypes. Guarantees every land cell ends
    // up assigned per the contract.
    for cell in 0..n {
        if assignment[cell].is_none() && world.terrain.elevation[cell] >= 0.0 {
            let mut best_idx = 0u16;
            let mut best_fit = f32::NEG_INFINITY;
            for (i, archetype) in archetypes.iter().enumerate() {
                let f = habitat_fitness_with(world, cell, archetype, near_water[cell]);
                if f > best_fit {
                    best_fit = f;
                    best_idx = i as u16;
                }
            }
            assignment[cell] = Some(best_idx);
        }
    }

    assignment
}

fn cull_to_threshold(
    world: &WorldData,
    archetypes: &[RaceArchetype],
    mut assignment: Vec<Option<u16>>,
    near_water: &[bool],
    params: &CulturesParams,
) -> (Vec<Option<u16>>, Vec<bool>) {
    let roster_size = archetypes.len();
    let mut survives = vec![true; roster_size];
    let n = world.mesh.cell_count();

    // Cap iterations at 5 — convergence in practice happens in 1-2 passes;
    // 5 is a defensive bound against pathological cycles I haven't anticipated.
    for _pass in 0..5 {
        // Per-culture means over the current assignment.
        let mut counts = vec![0usize; roster_size];
        let mut sums = vec![0.0_f32; roster_size];
        for cell in 0..n {
            if let Some(id) = assignment[cell] {
                let id = id as usize;
                if survives[id] {
                    counts[id] += 1;
                    sums[id] +=
                        habitat_fitness_with(world, cell, &archetypes[id], near_water[cell]);
                }
            }
        }
        let means: Vec<f32> = (0..roster_size)
            .map(|i| {
                if counts[i] == 0 {
                    0.0
                } else {
                    sums[i] / counts[i] as f32
                }
            })
            .collect();

        // Cull candidates: surviving cultures below the threshold, sorted
        // worst-first.
        let mut cullable: Vec<usize> = (0..roster_size)
            .filter(|&i| survives[i] && means[i] < params.min_habitat_fitness)
            .collect();
        cullable.sort_by(|&a, &b| {
            means[a]
                .partial_cmp(&means[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Refuse to drop below 2 survivors (distribution-non-trivial floor).
        let n_surviving = survives.iter().filter(|&&s| s).count();
        let max_cull = n_surviving.saturating_sub(2);
        let n_to_cull = cullable.len().min(max_cull);
        if n_to_cull == 0 {
            break;
        }

        for &i in &cullable[..n_to_cull] {
            survives[i] = false;
        }

        // Reassign cells of just-culled cultures to the highest-fit survivor.
        for cell in 0..n {
            if let Some(id) = assignment[cell] {
                if !survives[id as usize] {
                    let mut best_idx = 0u16;
                    let mut best_fit = f32::NEG_INFINITY;
                    for (j, archetype) in archetypes.iter().enumerate() {
                        if !survives[j] {
                            continue;
                        }
                        let f = habitat_fitness_with(world, cell, archetype, near_water[cell]);
                        if f > best_fit {
                            best_fit = f;
                            best_idx = j as u16;
                        }
                    }
                    assignment[cell] = Some(best_idx);
                }
            }
        }
    }

    // Remap surviving archetype indices to contiguous culture_ids.
    let mut remap = vec![0u16; roster_size];
    let mut new_id = 0u16;
    for i in 0..roster_size {
        if survives[i] {
            remap[i] = new_id;
            new_id += 1;
        }
    }
    let remapped: Vec<Option<u16>> = assignment
        .iter()
        .map(|s| s.map(|id| remap[id as usize]))
        .collect();

    (remapped, survives)
}

// ──────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baked_archetypes_csv_parses_to_five_rows() {
        let roster = load_archetypes();
        assert_eq!(
            roster.len(),
            5,
            "Phase 3a MVP roster: 5 archetypes (1 Human variant + 1 Elf + 1 Dwarf + \
             1 Orc + 1 Halfling); got {}",
            roster.len()
        );
    }

    #[test]
    fn each_mvp_race_is_represented_exactly_once() {
        let roster = load_archetypes();
        // MVP coverage from ARCHITECTURE.md §5.5: one variant each of these
        // five high-level races. Compare as sorted discriminants so order in
        // the CSV doesn't matter. Add a row → bump the expected list.
        let mut got: Vec<u8> = roster.iter().map(|a| a.race as u8).collect();
        got.sort();
        let mut want: Vec<u8> = vec![
            Race::Human as u8,
            Race::Elf as u8,
            Race::Dwarf as u8,
            Race::Orc as u8,
            Race::Halfling as u8,
        ];
        want.sort();
        assert_eq!(got, want, "expected one of each MVP race; saw {got:?}");
    }

    #[test]
    fn temp_and_elev_ranges_are_well_ordered() {
        for a in load_archetypes() {
            assert!(
                a.temp_range.0 <= a.temp_range.1,
                "archetype {:?}/{:?} has temp_min > temp_max: {:?}",
                a.race,
                a.archetype_name,
                a.temp_range
            );
            assert!(
                a.elev_range.0 <= a.elev_range.1,
                "archetype {:?}/{:?} has elev_min > elev_max: {:?}",
                a.race,
                a.archetype_name,
                a.elev_range
            );
        }
    }

    #[test]
    fn weights_and_alignment_are_in_valid_range() {
        for a in load_archetypes() {
            assert!(
                (0.0..=1.0).contains(&a.water_weight),
                "{:?}/{:?} water_weight {} outside [0,1]",
                a.race,
                a.archetype_name,
                a.water_weight
            );
            assert!(
                (-1.0..=1.0).contains(&a.alignment.law_chaos),
                "{:?}/{:?} law_chaos {} outside [-1,1]",
                a.race,
                a.archetype_name,
                a.alignment.law_chaos
            );
            assert!(
                (-1.0..=1.0).contains(&a.alignment.good_evil),
                "{:?}/{:?} good_evil {} outside [-1,1]",
                a.race,
                a.archetype_name,
                a.alignment.good_evil
            );
        }
    }

    #[test]
    fn biome_ids_reference_known_biomes() {
        // 0..=14 covers SNOW through RIPARIAN per crate::biomes.
        for a in load_archetypes() {
            assert!(
                !a.preferred_biomes.is_empty(),
                "{:?}/{:?} has empty preferred_biomes; archetype with no biome \
                 preference scores 0.2 everywhere and the culture is unplaceable",
                a.race,
                a.archetype_name
            );
            for b in &a.preferred_biomes {
                assert!(
                    *b <= 14,
                    "{:?}/{:?} references biome id {b}, which is outside the \
                     defined palette (0..=14)",
                    a.race,
                    a.archetype_name
                );
                // Sea biomes (12, 13) on land-archetype preference lists would
                // never match — flag as a likely authoring error.
                assert!(
                    *b != crate::biomes::SEA_SHALLOW && *b != crate::biomes::SEA_DEEP,
                    "{:?}/{:?} prefers a sea biome ({b}); land archetypes can't \
                     occupy sea cells",
                    a.race,
                    a.archetype_name
                );
            }
        }
    }

    #[test]
    fn rejects_truncated_row() {
        let bad = "race,archetype,culture_name\nHuman,Foo,Bar";
        assert!(parse_archetypes(bad).is_err());
    }

    #[test]
    fn rejects_unknown_enum_value() {
        let bad = "race,archetype,culture_name,preferred_biomes,temp_min,temp_max,elev_min,\
             elev_max,water_weight,law_chaos,good_evil,tech_era,metallurgy,agriculture,\
             naval,military,arcane,magic_style,settlement_icon,architecture,\
             diplomatic_pattern\n\
             Unknown,X,Y,3,0,1,0,1,0.5,0,0,Iron,1,1,1,1,1,None,Hall,Classical,Mercantile";
        let err = parse_archetypes(bad).unwrap_err();
        assert!(
            err.contains("race"),
            "error should name the race column: {err}"
        );
    }

    #[test]
    fn to_culture_carries_every_field() {
        let roster = load_archetypes();
        let a = &roster[0]; // River-valley Human (Riverfolk)
        let culture = a.to_culture(0);
        assert_eq!(culture.name, "Riverfolk");
        assert_eq!(culture.race, Race::Human);
        assert_eq!(culture.archetype_id, 0);
        assert_eq!(culture.alignment, a.alignment);
        assert_eq!(culture.tech, a.tech);
        assert_eq!(culture.magic, a.magic);
        assert_eq!(culture.settlement, a.settlement);
        assert_eq!(culture.architecture, a.architecture);
        assert_eq!(culture.diplomatic, a.diplomatic);
    }
}

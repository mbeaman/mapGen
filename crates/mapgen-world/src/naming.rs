//! Naming stage. Phase 3d.
//!
//! Builds a per-culture `Language` (one per culture; MVP doesn't yet
//! model dialect splits or sound-change-rule cognates — see
//! BACKLOG.md), then generates phonotactic names for settlements,
//! polities, and religions. The CSV-authored `Culture.name` strings
//! (e.g., "Riverfolk", "Iron Hold") are kept as the *exonym* — what
//! outsiders call this culture — while the language-generated names
//! supply the in-world place / institution names.
//!
//! Pipeline position: runs after `polities::lay_out` (settlements need
//! to exist before they can be named) and before any history /
//! lore stage.
//!
//! Spec: `crates/mapgen-world/tests/naming_spec.rs`.
//!
//! Architecture: `docs/ARCHITECTURE.md` Phase 3d — "Phonotactic
//! generator + Markov fallback, per `Language`." The Markov fallback
//! is BACKLOGGED; MVP uses pure phonotactic.

use std::collections::VecDeque;

use mapgen_core::entities::{Language, Race};
use mapgen_core::{generate_name, Continent, MeshData, MountainRange, Ocean, WorldData};
use rand_chacha::ChaCha8Rng;

/// Tunables for the naming stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct NamingParams {
    /// Maximum number of attempts at generating a unique name before
    /// giving up and accepting a collision. Settlements with collisions
    /// would otherwise be indistinguishable.
    pub uniqueness_attempts: usize,
}

impl Default for NamingParams {
    fn default() -> Self {
        Self {
            uniqueness_attempts: 8,
        }
    }
}

/// Generate languages + rename settlements, polities, and religions
/// with their respective cultures' generated phonotactic names.
///
/// Preconditions: `world.cultures.cultures` populated, `world.society
/// .settlements` and `world.society.nations` populated (Phase 3c must
/// have run), `world.religions.religions` populated (Phase 3b must
/// have run).
pub fn name_world(world: &mut WorldData, _params: NamingParams, rng: &mut ChaCha8Rng) {
    if world.cultures.cultures.is_empty() {
        return;
    }

    // 1. Build per-culture languages (template-driven by Race; deterministic
    //    from world state alone — no RNG yet at this step).
    let languages: Vec<Language> = world
        .cultures
        .cultures
        .iter()
        .map(|c| language_for_race(c.race))
        .collect();

    // 2. Wire each culture to its language index.
    for (i, culture) in world.cultures.cultures.iter_mut().enumerate() {
        culture.language_id = i as u16;
    }

    // 3. Rename settlements. Iterate in `settlements` order for
    //    determinism. Each settlement's name uses the language of the
    //    culture that owns the polity (looked up via polity's capital
    //    cell — since 1 polity == 1 culture in MVP, this resolves
    //    uniquely).
    for settlement in world.society.settlements.iter_mut() {
        let polity = &world.society.nations[settlement.polity_id as usize];
        let culture_id = world
            .cultures
            .culture_id
            .get(polity.capital_cell as usize)
            .and_then(|x| *x);
        let language_idx = culture_id.map(|c| c as usize).unwrap_or(0);
        let lang = &languages[language_idx.min(languages.len() - 1)];
        settlement.name = generate_name(lang, rng);
    }

    // 4. Rename polities. Same language lookup as settlements.
    for polity in world.society.nations.iter_mut() {
        let culture_id = world
            .cultures
            .culture_id
            .get(polity.capital_cell as usize)
            .and_then(|x| *x);
        let language_idx = culture_id.map(|c| c as usize).unwrap_or(0);
        let lang = &languages[language_idx.min(languages.len() - 1)];
        polity.name = generate_name(lang, rng);
    }

    // 5. Rename religions. Founder culture's language.
    for religion in world.religions.religions.iter_mut() {
        let founder = religion.founder_culture_id as usize;
        let lang = &languages[founder.min(languages.len() - 1)];
        religion.name = generate_name(lang, rng);
    }

    // 6. Name *major* natural features — long rivers and sizeable lakes
    //    — so prominent water gets a label without littering every brook
    //    and pond. Features have no owning culture, so each is named in
    //    the language of the culture at its mouth (river) / first cell
    //    (lake), falling back to language 0. Language indices are
    //    resolved first (immutable culture borrow) and applied after, to
    //    satisfy the borrow checker. RNG is consumed strictly after the
    //    settlement/polity/religion passes above, so their names are
    //    unchanged by this addition.
    let last = languages.len() - 1;
    let river_langs: Vec<Option<usize>> = world
        .hydrology
        .rivers
        .iter()
        .map(|r| {
            (r.cells.len() >= MIN_NAMED_RIVER_CELLS).then(|| {
                let mouth = *r.cells.last().expect("non-empty by the length guard");
                culture_language(world, mouth, last)
            })
        })
        .collect();
    for (river, lang_idx) in world.hydrology.rivers.iter_mut().zip(river_langs) {
        if let Some(idx) = lang_idx {
            river.name = generate_name(&languages[idx], rng);
        }
    }

    let lake_langs: Vec<Option<usize>> = world
        .hydrology
        .lakes
        .iter()
        .map(|l| {
            (l.cells.len() >= MIN_NAMED_LAKE_CELLS).then(|| {
                let cell = *l.cells.first().expect("non-empty by the length guard");
                culture_language(world, cell, last)
            })
        })
        .collect();
    for (lake, lang_idx) in world.hydrology.lakes.iter_mut().zip(lake_langs) {
        if let Some(idx) = lang_idx {
            lake.name = generate_name(&languages[idx], rng);
        }
    }

    // 7. Name *major* mountain ranges — connected clusters of ALPINE /
    //    SNOW cells. Each range is named in the language of the culture
    //    at its highest peak (fallback language 0). RNG is consumed
    //    after the river/lake passes, so those names are unchanged.
    let clusters = cluster_mountain_ranges(world);
    let range_specs: Vec<(Vec<u32>, usize)> = clusters
        .into_iter()
        .filter(|c| c.len() >= MIN_NAMED_RANGE_CELLS)
        .map(|cells| {
            let peak = *cells
                .iter()
                .max_by(|&&a, &&b| {
                    world.terrain.elevation[a as usize]
                        .partial_cmp(&world.terrain.elevation[b as usize])
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .expect("non-empty by the length guard");
            (cells, culture_language(world, peak, last))
        })
        .collect();
    let ranges: Vec<MountainRange> = range_specs
        .into_iter()
        .map(|(cells, idx)| MountainRange {
            name: generate_name(&languages[idx], rng),
            cells,
        })
        .collect();
    world.mountain_ranges = ranges;

    // 8. Name *major* continents + oceans. Flood-fill the land/sea into
    //    connected bodies (the primitive the planisphere used to run at render
    //    time), keep the significant ones, and name each in the language of the
    //    culture that dominates it — a continent by its own owned cells, an
    //    ocean by the cultures along its coast — falling back to language 0 when
    //    uninhabited. RNG is consumed strictly after the range pass, so every
    //    earlier name is byte-unchanged; only the new fields move the golden.
    let (continents, oceans) = name_geographic_bodies(world, &languages, rng);
    world.continents = continents;
    world.oceans = oceans;

    // 8b. Tag each sea lane's anchors with the continent they sit on, so the
    //     history carriers (which run next, and cannot reach this crate) can stamp
    //     the far shore of an inter-continental event without recomputing landmass
    //     membership. PURELY ADDITIVE: it re-runs the SAME `connected_bodies` +
    //     `MIN_CONTINENT_DIVISOR` filter `name_geographic_bodies` used above, so the
    //     index it assigns to a body equals that body's position in
    //     `world.continents`; it reads only mesh/terrain and writes only the
    //     (`#[serde(skip)]`, empty-on-seed42) lane tags, so it perturbs no named
    //     field and no hashed byte. A no-op on a laneless world (seed42).
    if !world.sea_lanes.lanes.is_empty() {
        let continent_of: Vec<Option<u16>> = {
            let n = world.mesh.cell_count();
            let elev = &world.terrain.elevation;
            let is_land = |i: usize| elev.get(i).copied().unwrap_or(0.0) > 0.0;
            let mut map = vec![None; n];
            let mut idx: u16 = 0;
            for body in connected_bodies(&world.mesh, is_land) {
                if body.len() * MIN_CONTINENT_DIVISOR < n {
                    continue; // a speck, not a named continent — same filter as above
                }
                for &c in &body {
                    map[c] = Some(idx);
                }
                idx += 1;
            }
            map
        };
        let at = |cell: u32| continent_of.get(cell as usize).copied().flatten();
        for lane in &mut world.sea_lanes.lanes {
            lane.continent_a = at(lane.a);
            lane.continent_b = at(lane.b);
        }
    }

    world.languages = languages;
}

/// A body earns a name when `cell_count * DIVISOR >= total_cells`. 100 ≈ 1% of
/// the world for continents; 20 ≈ 5% for oceans, so secondary seas still get a
/// name. RE-CALIBRATED 40→100 for the longitude-periodic planet (2026-06-09): the
/// old 2.5% bar was tuned for the FLAT world, where one dominant landmass set the
/// scale. The periodic planet packs several genuine continents into the same cell
/// budget, each a smaller fraction, so 2.5% left real medium continents unnamed —
/// under-labeling the now-primary globe view and, downstream, starving the
/// far-shore chronicle (every carrier tags only NAMED shores, so the three strands
/// could never converge). 1% names the periodic planet's true continents while
/// still excluding islets (continental single-landmass worlds are unaffected —
/// they have no bodies in the 1–2.5% band). See docs/tuning_log.md.
const MIN_CONTINENT_DIVISOR: usize = 100;
const MIN_OCEAN_DIVISOR: usize = 20;

/// Flood-fill the land and sea into major bodies and name each via the lore
/// naming system, grounded in the dominant culture. Returns owned vectors so the
/// caller can write them onto `world` after this immutable borrow ends. Body
/// order (largest-first, stable) fixes the RNG draw order, so naming is
/// deterministic; centroids are plain f32 means in body-cell order (no
/// transcendental), so they stay byte-identical native↔wasm.
fn name_geographic_bodies(
    world: &WorldData,
    languages: &[Language],
    rng: &mut ChaCha8Rng,
) -> (Vec<Continent>, Vec<Ocean>) {
    let mesh = &world.mesh;
    let n = mesh.cell_count();
    let last = languages.len() - 1; // non-empty: caller guards cultures non-empty
    let culture_id = &world.cultures.culture_id;
    let elev = &world.terrain.elevation;
    let is_land = |i: usize| elev.get(i).copied().unwrap_or(0.0) > 0.0;
    let lang_of = |owner: Option<u16>| owner.map(|c| (c as usize).min(last)).unwrap_or(0);

    let mut continents = Vec::new();
    for body in connected_bodies(mesh, is_land) {
        if body.len() * MIN_CONTINENT_DIVISOR < n {
            continue; // a speck, not a continent
        }
        let lang = lang_of(dominant_culture(&body, culture_id));
        continents.push(Continent {
            name: generate_name(&languages[lang], rng),
            centroid: centroid(mesh, &body),
            cell_count: body.len() as u32,
        });
    }

    let mut oceans = Vec::new();
    for body in connected_bodies(mesh, |i| !is_land(i)) {
        if body.len() * MIN_OCEAN_DIVISOR < n {
            continue; // a minor inlet, not an ocean
        }
        let coast = coastal_land(mesh, &body, &is_land);
        let lang = lang_of(dominant_culture(&coast, culture_id));
        oceans.push(Ocean {
            name: generate_name(&languages[lang], rng),
            centroid: centroid(mesh, &body),
            cell_count: body.len() as u32,
        });
    }

    (continents, oceans)
}

/// World-space mean of the cells' sites — the label anchor. Summed in the given
/// (deterministic) cell order; plain f32 arithmetic, so byte-stable.
fn centroid(mesh: &MeshData, cells: &[usize]) -> [f32; 2] {
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    for &c in cells {
        let p = mesh.sites[c];
        sx += p[0];
        sy += p[1];
    }
    let k = cells.len().max(1) as f32;
    [sx / k, sy / k]
}

/// The major landmass under a world-space point, for the continent-aware drill.
#[derive(Clone, Debug, PartialEq)]
pub struct ContinentHit {
    /// World-space centroid of the landmass (the drill re-centers here).
    pub cx: f32,
    pub cy: f32,
    /// Cells in the landmass — the UI sizes the drill depth from this.
    pub cell_count: u32,
    /// The grounded name of this landmass (from `world.continents`) — for the
    /// drilled-region label. Empty if the world was never named.
    pub name: String,
}

/// The major landmass under world-space point `(x, y)`, or `None` when the point
/// is over sea or a sub-threshold speck. Re-runs the same land flood-fill + size
/// threshold the naming stage uses (`connected_bodies` + `MIN_CONTINENT_DIVISOR`),
/// so "a continent" means exactly what the planisphere labels — a click on a
/// labelled continent always resolves to it. Recomputed per click (the drill is
/// the only consumer), so nothing is persisted. `O(n)` over the mesh.
pub fn continent_at(world: &WorldData, x: f32, y: f32) -> Option<ContinentHit> {
    let mesh = &world.mesh;
    let n = mesh.cell_count();
    if n == 0 {
        return None;
    }
    let elev = &world.terrain.elevation;
    let is_land = |i: usize| elev.get(i).copied().unwrap_or(0.0) > 0.0;

    // Nearest cell-site to the point (the Voronoi cell it falls in).
    let dist2 = |c: usize| {
        let p = mesh.sites[c];
        let (dx, dy) = (p[0] - x, p[1] - y);
        dx * dx + dy * dy
    };
    let cell = (0..n).min_by(|&a, &b| {
        dist2(a)
            .partial_cmp(&dist2(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    if !is_land(cell) {
        return None; // clicked the sea
    }

    // The land body containing the cell, then the same significance threshold.
    let body = connected_bodies(mesh, is_land)
        .into_iter()
        .find(|b| b.contains(&cell))?;
    if body.len() * MIN_CONTINENT_DIVISOR < n {
        return None; // a speck island, not a continent — let the caller grid-drill
    }
    let c = centroid(mesh, &body);
    // Resolve the grounded name: `name_geographic_bodies` built `world.continents`
    // from the SAME `connected_bodies` + threshold, so this body's entry is the one
    // whose centroid is nearest `c` (exact in practice — same `centroid` fn on the
    // same cells — but nearest-match is robust to any f32 wobble). Empty if unnamed.
    let name = world
        .continents
        .iter()
        .min_by(|a, b| {
            let da = (a.centroid[0] - c[0]).powi(2) + (a.centroid[1] - c[1]).powi(2);
            let db = (b.centroid[0] - c[0]).powi(2) + (b.centroid[1] - c[1]).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|c| c.name.clone())
        .unwrap_or_default();
    Some(ContinentHit {
        cx: c[0],
        cy: c[1],
        cell_count: body.len() as u32,
        name,
    })
}

/// Land cells adjacent to a sea body — the body's coast — in first-seen order
/// (deterministic). Used to ground an ocean's name in the cultures on its shore.
fn coastal_land(
    mesh: &MeshData,
    sea_body: &[usize],
    is_land: &impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    let mut coast = Vec::new();
    for &c in sea_body {
        if let Some(ns) = mesh.neighbors.get(c) {
            for &nb in ns {
                let nb = nb as usize;
                if is_land(nb) && seen.insert(nb) {
                    coast.push(nb);
                }
            }
        }
    }
    coast
}

/// Minimum cluster size (in cells) for a mountain range to earn a name.
const MIN_NAMED_RANGE_CELLS: usize = 5;

/// Biome ids for high terrain (mirrors `mapgen-render`'s palette
/// constants): SNOW = 0, ALPINE = 11.
fn is_mountain_biome(b: u8) -> bool {
    b == 0 || b == 11
}

/// Connected components of ALPINE/SNOW cells over the mesh neighbor
/// graph — each is a candidate mountain range. Iterative DFS; component
/// order is deterministic (ascending start-cell index).
fn cluster_mountain_ranges(world: &WorldData) -> Vec<Vec<u32>> {
    let biome = &world.climate.biome;
    let n = world.mesh.cell_count();
    let is_mtn = |i: usize| {
        biome
            .get(i)
            .copied()
            .map(is_mountain_biome)
            .unwrap_or(false)
    };

    let mut visited = vec![false; n];
    let mut ranges: Vec<Vec<u32>> = Vec::new();
    for start in 0..n {
        if visited[start] || !is_mtn(start) {
            continue;
        }
        let mut stack = vec![start];
        visited[start] = true;
        let mut comp = Vec::new();
        while let Some(c) = stack.pop() {
            comp.push(c as u32);
            for &nj in &world.mesh.neighbors[c] {
                let j = nj as usize;
                if !visited[j] && is_mtn(j) {
                    visited[j] = true;
                    stack.push(j);
                }
            }
        }
        ranges.push(comp);
    }
    ranges
}

/// Minimum river length (in cells) to earn a name — keeps minor
/// watercourses unlabeled. Calibrated value in `docs/tuning_log.md`.
const MIN_NAMED_RIVER_CELLS: usize = 8;
/// Minimum lake size (in cells) to earn a name.
const MIN_NAMED_LAKE_CELLS: usize = 3;

/// Resolve the language index of the culture occupying `cell`, clamped
/// to the language roster, falling back to language 0 (the first
/// culture's) when the cell is unassigned (e.g., a water cell).
fn culture_language(world: &WorldData, cell: u32, last: usize) -> usize {
    world
        .cultures
        .culture_id
        .get(cell as usize)
        .and_then(|x| *x)
        .map(|c| c as usize)
        .unwrap_or(0)
        .min(last)
}

/// Connected components of mesh cells satisfying `keep`, by BFS over the
/// neighbor graph, returned largest-first. Equal-sized bodies keep ascending
/// start-cell order (Rust's sort is stable), so the body order — and therefore
/// the RNG draw order when naming them — is deterministic.
///
/// Migrated from the planet renderer: continents/oceans are now flood-filled in
/// the pipeline and named, so the renderer just reads the result. Used to find
/// both continents (`keep` = land) and oceans (`keep` = sea).
pub fn connected_bodies(mesh: &MeshData, keep: impl Fn(usize) -> bool) -> Vec<Vec<usize>> {
    let n = mesh.cell_count();
    let mut seen = vec![false; n];
    let mut bodies: Vec<Vec<usize>> = Vec::new();
    for start in 0..n {
        if seen[start] || !keep(start) {
            continue;
        }
        let mut body = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        seen[start] = true;
        while let Some(c) = queue.pop_front() {
            body.push(c);
            if let Some(ns) = mesh.neighbors.get(c) {
                for &nb in ns {
                    let nb = nb as usize;
                    if !seen[nb] && keep(nb) {
                        seen[nb] = true;
                        queue.push_back(nb);
                    }
                }
            }
        }
        bodies.push(body);
    }
    bodies.sort_by_key(|b| std::cmp::Reverse(b.len()));
    bodies
}

/// The culture owning the most of `cells`, breaking ties toward the *lowest*
/// `culture_id` so the choice is byte-stable across native↔wasm (an unstable
/// argmax would flake the determinism golden). `None` when no listed cell is
/// owned (e.g. an uninhabited landmass), so the caller falls back to language 0.
pub fn dominant_culture(cells: &[usize], culture_id: &[Option<u16>]) -> Option<u16> {
    // BTreeMap iterates keys ascending, so folding "replace only on a strictly
    // greater count" keeps the first (lowest-id) culture on a tie.
    let mut tally: std::collections::BTreeMap<u16, usize> = std::collections::BTreeMap::new();
    for &c in cells {
        if let Some(Some(cid)) = culture_id.get(c) {
            *tally.entry(*cid).or_insert(0) += 1;
        }
    }
    tally
        .into_iter()
        .reduce(|best, cur| if cur.1 > best.1 { cur } else { best })
        .map(|(id, _)| id)
}

/// Hardcoded per-race phonotactic profile. Each profile leans toward a
/// recognizable Tolkienesque register (soft vowel-y Elven; hard
/// consonant-y Dwarven; harsh-cluster Orcish; etc.) so a glance at the
/// rendered map identifies the culture before reading the label.
///
/// Future enhancement: load profiles from CSV per race-archetype so
/// world-by-world variation is possible without recompiling.
fn language_for_race(race: Race) -> Language {
    match race {
        Race::Human => Language {
            name: "Lalrian".into(),
            vowels: "aeiou".chars().collect(),
            consonants: "kgtdpbsnrlmh".chars().collect(),
            syllable_patterns: vec!["CV".into(), "CVC".into(), "V".into()],
            min_syllables: 2,
            max_syllables: 4,
        },
        Race::Elf => Language {
            name: "Eldarin".into(),
            vowels: "aeio".chars().collect(),
            consonants: "lrnsvfmh".chars().collect(),
            syllable_patterns: vec!["V".into(), "CV".into(), "CVl".into()],
            min_syllables: 3,
            max_syllables: 5,
        },
        Race::Dwarf => Language {
            name: "Khuzdic".into(),
            vowels: "aiu".chars().collect(),
            consonants: "kgbdrmnt".chars().collect(),
            syllable_patterns: vec!["CVC".into(), "CV".into(), "CCV".into()],
            min_syllables: 1,
            max_syllables: 3,
        },
        Race::Orc => Language {
            name: "Grimsh".into(),
            vowels: "auo".chars().collect(),
            consonants: "krgzjbdvft".chars().collect(),
            syllable_patterns: vec!["CVC".into(), "CCVC".into(), "CV".into()],
            min_syllables: 1,
            max_syllables: 3,
        },
        Race::Halfling => Language {
            name: "Greenfolk".into(),
            vowels: "aeio".chars().collect(),
            consonants: "lmnsrbdh".chars().collect(),
            syllable_patterns: vec!["CV".into(), "CVC".into(), "V".into()],
            min_syllables: 2,
            max_syllables: 3,
        },
        // Reserve races — placeholder profiles so the spec's
        // language-pool invariants pass even if a later CSV swaps in
        // one of these archetypes.
        Race::Lizardfolk => Language {
            name: "Sszaar".into(),
            vowels: "aei".chars().collect(),
            consonants: "szrkht".chars().collect(),
            syllable_patterns: vec!["CV".into(), "CVCC".into()],
            min_syllables: 1,
            max_syllables: 3,
        },
        Race::SeaFolk => Language {
            name: "Maral".into(),
            vowels: "aeiou".chars().collect(),
            consonants: "mlrnvw".chars().collect(),
            syllable_patterns: vec!["CV".into(), "VCV".into()],
            min_syllables: 2,
            max_syllables: 4,
        },
        Race::Underdark => Language {
            name: "Drath".into(),
            vowels: "auo".chars().collect(),
            consonants: "drthzkv".chars().collect(),
            syllable_patterns: vec!["CVC".into(), "CCV".into()],
            min_syllables: 1,
            max_syllables: 3,
        },
        Race::Giant => Language {
            name: "Joten".into(),
            vowels: "aou".chars().collect(),
            consonants: "jbthrmg".chars().collect(),
            syllable_patterns: vec!["CV".into(), "CVC".into()],
            min_syllables: 1,
            max_syllables: 3,
        },
    }
}

// `generate_name` now lives in `mapgen_core::naming` (imported above) so the
// history crate can name characters from the same per-culture languages
// without depending on `mapgen-world`. Behavior is unchanged.

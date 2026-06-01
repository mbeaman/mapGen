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
use mapgen_core::{generate_name, MeshData, MountainRange, WorldData};
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

    world.languages = languages;
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

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

use mapgen_core::entities::{Language, Race};
use mapgen_core::WorldData;
use rand_chacha::rand_core::RngCore;
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

    world.languages = languages;
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

/// Generate a single phonotactic name from `lang`. The RNG must be
/// advanced consistently to preserve determinism across runs of the
/// same seed.
pub fn generate_name(lang: &Language, rng: &mut ChaCha8Rng) -> String {
    let span = lang.max_syllables - lang.min_syllables + 1;
    let n_syllables = lang.min_syllables + (rng.next_u32() % span as u32) as u8;
    let mut out = String::new();
    for _ in 0..n_syllables {
        let pat = &lang.syllable_patterns[(rng.next_u32() as usize) % lang.syllable_patterns.len()];
        for ch in pat.chars() {
            let picked = match ch {
                'C' => lang.consonants[(rng.next_u32() as usize) % lang.consonants.len()],
                'V' => lang.vowels[(rng.next_u32() as usize) % lang.vowels.len()],
                literal => literal,
            };
            out.push(picked);
        }
    }
    capitalize(&out)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

//! Phase 3d naming spec — failing-red until `naming::name_world` is
//! implemented.
//!
//! ARCHITECTURE.md Phase 3d: "Phonotactic generator + Markov fallback,
//! per `Language`." Markov fallback is backlogged; MVP exercises only
//! the phonotactic path. The contract here is therefore:
//!
//! * One language per culture, with non-empty vowel + consonant pools.
//! * Every settlement / polity / religion carries a generated name,
//!   not the templated `"{culture.name} Capital"` etc. that the
//!   earlier stages emit.
//! * Names are determinist for a fixed seed.
//! * Names are non-empty ASCII letters.

use mapgen_core::entities::{
    Alignment, Architecture, Culture, DiplomaticPattern, Language, MagicStyle, PantheonPattern,
    Race, Religion, Settlement, SettlementIcon, SettlementTier, TechProfile,
};
use mapgen_core::world_data::{Nation, Road};
use mapgen_core::{MeshData, Stage, StageRng, TerrainData, WorldData, WorldMeta};
use mapgen_world::{
    generate_full,
    naming::{self, NamingParams},
    GenerateParams,
};

fn params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
        periodic: false,
    }
}

fn world_with_names(seed: u64) -> WorldData {
    let mut world = generate_full(params(seed));
    let mut rng = StageRng::new(seed).stream(Stage::Names);
    naming::name_world(&mut world, NamingParams::default(), &mut rng);
    world
}

// ──────────────────────────────────────────────────────────────────────
// Contract helpers.
// ──────────────────────────────────────────────────────────────────────

fn check_languages_roster_matches_culture_count(world: &WorldData) {
    assert_eq!(
        world.languages.len(),
        world.cultures.cultures.len(),
        "naming stage must produce one language per culture"
    );
}

fn check_language_pools_are_non_empty(world: &WorldData) {
    for (i, lang) in world.languages.iter().enumerate() {
        assert!(
            !lang.vowels.is_empty(),
            "language {i} ({:?}) has no vowels",
            lang.name
        );
        assert!(
            !lang.consonants.is_empty(),
            "language {i} ({:?}) has no consonants",
            lang.name
        );
        assert!(
            !lang.syllable_patterns.is_empty(),
            "language {i} ({:?}) has no syllable patterns",
            lang.name
        );
        assert!(
            lang.min_syllables >= 1,
            "language {i} ({:?}) has min_syllables 0 — would emit empty names",
            lang.name
        );
        assert!(
            lang.max_syllables >= lang.min_syllables,
            "language {i} ({:?}) max_syllables {} < min_syllables {}",
            lang.name,
            lang.max_syllables,
            lang.min_syllables
        );
    }
}

fn check_culture_language_ids_are_valid(world: &WorldData) {
    for (i, culture) in world.cultures.cultures.iter().enumerate() {
        assert!(
            (culture.language_id as usize) < world.languages.len(),
            "culture {i} ({:?}) references language {} but only {} exist",
            culture.name,
            culture.language_id,
            world.languages.len()
        );
    }
}

fn check_no_settlement_uses_a_template_name(world: &WorldData) {
    // The polities-stage template was "{culture.name} Capital" /
    // "{culture.name} Town N". After name_world, no settlement should
    // start with its owning culture's exonym.
    for (i, s) in world.society.settlements.iter().enumerate() {
        let culture_idx = world.society.nations[s.polity_id as usize].capital_cell as usize;
        let culture_id = world.cultures.culture_id.get(culture_idx).and_then(|x| *x);
        let Some(culture_id) = culture_id else {
            continue;
        };
        let culture_name = &world.cultures.cultures[culture_id as usize].name;
        assert!(
            !s.name.starts_with(culture_name.as_str()),
            "settlement {i} still bears the templated name {:?} — naming \
             stage didn't rewrite it (culture {:?})",
            s.name,
            culture_name
        );
    }
}

fn check_no_polity_uses_a_template_name(world: &WorldData) {
    // The polities-stage template was "{culture.name} Realm".
    for (i, n) in world.society.nations.iter().enumerate() {
        let culture_id = world
            .cultures
            .culture_id
            .get(n.capital_cell as usize)
            .and_then(|x| *x);
        let Some(culture_id) = culture_id else {
            continue;
        };
        let culture_name = &world.cultures.cultures[culture_id as usize].name;
        let template = format!("{culture_name} Realm");
        assert_ne!(
            n.name, template,
            "polity {i} still bears the templated name {:?}",
            n.name
        );
    }
}

fn check_no_religion_uses_a_template_name(world: &WorldData) {
    // The religions-stage template was "{culture.name} <pantheon-suffix>".
    let pantheon_suffix_substrings = [
        "Faith",
        "Pantheon",
        "Twin Path",
        "Spirits",
        "Ancestors",
        "Way",
    ];
    for (i, r) in world.religions.religions.iter().enumerate() {
        let culture_name = &world.cultures.cultures[r.founder_culture_id as usize].name;
        // Reject religion names that BOTH start with the founder culture's
        // name AND end in a pantheon-suffix word. That's the precise
        // template shape `religions::found` emits.
        let starts_with_culture = r.name.starts_with(culture_name.as_str());
        let ends_with_suffix = pantheon_suffix_substrings
            .iter()
            .any(|s| r.name.contains(s));
        assert!(
            !(starts_with_culture && ends_with_suffix),
            "religion {i} ({:?}) still bears the templated name — culture \
             {:?} + pantheon suffix",
            r.name,
            culture_name
        );
    }
}

fn check_names_are_non_empty(world: &WorldData) {
    for s in &world.society.settlements {
        assert!(!s.name.is_empty(), "settlement has empty name");
    }
    for n in &world.society.nations {
        assert!(!n.name.is_empty(), "polity has empty name");
    }
    for r in &world.religions.religions {
        assert!(!r.name.is_empty(), "religion has empty name");
    }
}

// ──────────────────────────────────────────────────────────────────────
// Synthetic-world test — green today.
// ──────────────────────────────────────────────────────────────────────

fn synthetic_named_world() -> WorldData {
    let mut world = WorldData {
        meta: WorldMeta::new(0),
        mesh: MeshData::default(),
        terrain: TerrainData::default(),
        ..Default::default()
    };
    world.mesh.sites = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
    world.mesh.neighbors = vec![vec![1], vec![0, 2], vec![1]];
    world.terrain.elevation = vec![0.5, 0.4, 0.6];
    world.cultures.cultures = vec![Culture {
        name: "TestFolk".into(),
        race: Race::Human,
        archetype_id: 0,
        language_id: 0,
        religion_id: None,
        alignment: Alignment::default(),
        tech: TechProfile::default(),
        magic: MagicStyle::None,
        settlement: SettlementIcon::Hall,
        architecture: Architecture::Classical,
        diplomatic: DiplomaticPattern::Mercantile,
    }];
    world.cultures.culture_id = vec![Some(0), Some(0), Some(0)];
    world.languages = vec![Language {
        name: "Testish".into(),
        vowels: vec!['a', 'e', 'i'],
        consonants: vec!['k', 'l', 'n', 's'],
        syllable_patterns: vec!["CV".into(), "CVC".into()],
        min_syllables: 2,
        max_syllables: 3,
    }];
    world.society.nations = vec![Nation {
        name: "Kalena".into(), // non-template
        capital_cell: 0,
        color: [128, 128, 128],
        ..Default::default()
    }];
    world.society.settlements = vec![Settlement {
        name: "Sileka".into(), // non-template
        cell: 0,
        tier: SettlementTier::Capital,
        polity_id: 0,
        population: 1.0,
    }];
    world.society.roads = vec![Road { cells: vec![0] }];
    world.society.control = vec![Some(0), Some(0), Some(0)];
    world.religions.religions = vec![Religion {
        name: "Aneko".into(), // non-template (no culture-name prefix, no pantheon suffix)
        pantheon: PantheonPattern::Mono,
        founder_culture_id: 0,
        alignment: Alignment::default(),
        sacred_sites: vec![0],
    }];
    world.religions.religion_id = vec![Some(0), Some(0), Some(0)];
    world
}

#[test]
fn synthetic_world_satisfies_naming_contract() {
    let world = synthetic_named_world();
    check_languages_roster_matches_culture_count(&world);
    check_language_pools_are_non_empty(&world);
    check_culture_language_ids_are_valid(&world);
    check_no_settlement_uses_a_template_name(&world);
    check_no_polity_uses_a_template_name(&world);
    check_no_religion_uses_a_template_name(&world);
    check_names_are_non_empty(&world);
}

// ──────────────────────────────────────────────────────────────────────
// name_world-based tests — RED until impl lands.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn when_name_world_runs_languages_roster_matches_culture_count() {
    check_languages_roster_matches_culture_count(&world_with_names(42));
}

#[test]
fn when_name_world_runs_language_pools_are_non_empty() {
    check_language_pools_are_non_empty(&world_with_names(42));
}

#[test]
fn when_name_world_runs_culture_language_ids_are_valid() {
    check_culture_language_ids_are_valid(&world_with_names(42));
}

#[test]
fn when_name_world_runs_no_settlement_uses_a_template_name() {
    check_no_settlement_uses_a_template_name(&world_with_names(42));
}

#[test]
fn when_name_world_runs_no_polity_uses_a_template_name() {
    check_no_polity_uses_a_template_name(&world_with_names(42));
}

#[test]
fn when_name_world_runs_no_religion_uses_a_template_name() {
    check_no_religion_uses_a_template_name(&world_with_names(42));
}

#[test]
fn when_name_world_runs_names_are_non_empty() {
    check_names_are_non_empty(&world_with_names(42));
}

#[test]
fn when_name_world_runs_output_is_deterministic_for_a_fixed_seed() {
    let a = world_with_names(42);
    let b = world_with_names(42);
    let a_settlements: Vec<&str> = a
        .society
        .settlements
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    let b_settlements: Vec<&str> = b
        .society
        .settlements
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        a_settlements, b_settlements,
        "two runs with seed 42 produced different settlement names"
    );
    let a_pol: Vec<&str> = a.society.nations.iter().map(|n| n.name.as_str()).collect();
    let b_pol: Vec<&str> = b.society.nations.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(
        a_pol, b_pol,
        "two runs with seed 42 produced different polity names"
    );
}

#[test]
fn major_rivers_are_named_and_minor_ones_are_not() {
    // Phase-3e polish: the naming stage now labels *major* natural
    // features. Contract: at least one major river earns a name on the
    // reference world; every named river clears the major-length
    // threshold; names are alphabetic and deterministic.
    let world = generate_full(params(42));

    let named: Vec<&str> = world
        .hydrology
        .rivers
        .iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| r.name.as_str())
        .collect();
    assert!(
        !named.is_empty(),
        "no major rivers were named on seed 42 — threshold too high?"
    );
    for r in &world.hydrology.rivers {
        if !r.name.is_empty() {
            assert!(
                r.cells.len() >= 8,
                "a {}-cell river was named ({:?}) — minor features should stay unnamed",
                r.cells.len(),
                r.name
            );
            assert!(
                r.name.chars().all(|c| c.is_ascii_alphabetic()),
                "river name has non-alphabetic chars: {:?}",
                r.name
            );
        }
    }

    // Determinism: a second run yields identical river + lake names.
    let world2 = generate_full(params(42));
    let rn1: Vec<&str> = world
        .hydrology
        .rivers
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    let rn2: Vec<&str> = world2
        .hydrology
        .rivers
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(rn1, rn2, "river names not deterministic across runs");
    let ln1: Vec<&str> = world
        .hydrology
        .lakes
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    let ln2: Vec<&str> = world2
        .hydrology
        .lakes
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(ln1, ln2, "lake names not deterministic across runs");
    for l in &world.hydrology.lakes {
        if !l.name.is_empty() {
            assert!(l.cells.len() >= 3, "a tiny lake was named: {:?}", l.name);
        }
    }
}

#[test]
fn major_mountain_ranges_are_named() {
    // Phase-3e polish: the naming stage clusters ALPINE/SNOW cells into
    // ranges and names the major ones. Contract: ≥1 named range on the
    // reference world; every range clears the size floor; names are
    // alphabetic + deterministic.
    let world = generate_full(params(42));
    assert!(
        !world.mountain_ranges.is_empty(),
        "no mountain ranges named on seed 42 — threshold too high?"
    );
    for r in &world.mountain_ranges {
        assert!(
            r.cells.len() >= 5,
            "a {}-cell cluster was named a range ({:?})",
            r.cells.len(),
            r.name
        );
        assert!(
            !r.name.is_empty() && r.name.chars().all(|c| c.is_ascii_alphabetic()),
            "range name not non-empty alphabetic: {:?}",
            r.name
        );
    }
    let world2 = generate_full(params(42));
    let n1: Vec<&str> = world
        .mountain_ranges
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    let n2: Vec<&str> = world2
        .mountain_ranges
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(n1, n2, "mountain-range names not deterministic across runs");
}

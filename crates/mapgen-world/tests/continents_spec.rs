//! Grounded continent / ocean naming spec.
//!
//! The planisphere used to label landmasses *positionally* (antique Latin
//! `TERRA SEPTENTRIONALIS` by where they sit). This stage instead flood-fills
//! the land/sea into major bodies and names each in the lore naming system —
//! grounded in the culture that dominates it — and stores the result on
//! `WorldData` (like `mountain_ranges`), so the renderer just reads it.
//!
//! Contract:
//! * `connected_bodies` partitions the kept cells into disjoint components that
//!   cover every kept cell (the flood-fill primitive, tested directly).
//! * `dominant_culture` picks the most-owned culture with a *stable* lowest-id
//!   tiebreak, and is `None` when nothing is owned (determinism gate).
//! * A generated world carries named continents (and at least one ocean): each
//!   with a non-empty name, an in-bounds centroid, a positive cell count, and
//!   ordered largest-first.
//! * A continent's name is grounded in its dominant culture's language.
//! * Naming is deterministic for a fixed seed.

use std::collections::HashSet;

use mapgen_core::entities::{
    Alignment, Architecture, Culture, DiplomaticPattern, MagicStyle, Race, SettlementIcon,
    TechProfile,
};
use mapgen_core::{MeshData, Stage, StageRng, TerrainData, WorldData, WorldMeta};
use mapgen_world::{
    generate_full,
    naming::{self, connected_bodies, continent_at, dominant_culture, NamingParams},
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
    }
}

// ──────────────────────────────────────────────────────────────────────
// Flood-fill + dominant-culture primitives (tested directly — see the
// advisor note: we store results, not member cells, so disjoint/coverage
// can't be inferred from `WorldData` and must be pinned on the helper).
// ──────────────────────────────────────────────────────────────────────

/// A 5-cell chain `0-1 | 2(water) | 3-4` (— = neighbor). Land = cells whose
/// elevation > 0; cell 2 is sea, splitting the land into two bodies.
fn chain_mesh() -> (MeshData, Vec<f32>) {
    let mut mesh = MeshData {
        sites: vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0], [4.0, 0.0]],
        neighbors: vec![vec![1], vec![0, 2], vec![1, 3], vec![2, 4], vec![3]],
        ..Default::default()
    };
    mesh.width = 5.0;
    mesh.height = 1.0;
    let elevation = vec![0.5, 0.4, -0.2, 0.3, 0.6];
    (mesh, elevation)
}

#[test]
fn connected_bodies_partitions_kept_cells_into_disjoint_covering_components() {
    let (mesh, elev) = chain_mesh();
    let bodies = connected_bodies(&mesh, |i| elev[i] > 0.0);

    // Two land bodies: {0,1} and {3,4}, largest-first (here both size 2, stable
    // on ascending start so {0,1} precedes {3,4}).
    assert_eq!(bodies.len(), 2, "expected two land bodies, got {bodies:?}");

    // Disjoint + covering: every land cell appears exactly once across bodies.
    let mut seen: Vec<usize> = bodies.iter().flatten().copied().collect();
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![0, 1, 3, 4],
        "bodies must cover exactly the land cells"
    );
    let unique: HashSet<usize> = seen.iter().copied().collect();
    assert_eq!(unique.len(), seen.len(), "bodies must be disjoint");
}

#[test]
fn connected_bodies_orders_largest_first() {
    // Land {0,1,2} (size 3) and {4} (size 1), split by sea cell 3.
    let mut mesh = MeshData {
        sites: vec![[0.0, 0.0]; 5],
        neighbors: vec![vec![1], vec![0, 2], vec![1, 3], vec![2, 4], vec![3]],
        ..Default::default()
    };
    mesh.width = 5.0;
    mesh.height = 1.0;
    let elev = [0.5, 0.5, 0.5, -0.1, 0.5];
    let bodies = connected_bodies(&mesh, |i| elev[i] > 0.0);
    assert_eq!(bodies[0].len(), 3, "largest body first");
    assert_eq!(bodies[1].len(), 1);
}

#[test]
fn dominant_culture_breaks_ties_toward_the_lowest_id() {
    // Cell-0..3 owned 2× by culture 2 and 2× by culture 5 — a tie. Lowest id wins.
    let culture_id = vec![Some(5u16), Some(2), Some(5), Some(2)];
    let all: Vec<usize> = (0..4).collect();
    assert_eq!(dominant_culture(&all, &culture_id), Some(2));
}

#[test]
fn dominant_culture_picks_the_strict_majority_and_handles_unowned() {
    let culture_id = vec![Some(9u16), Some(9), Some(9), None, Some(3)];
    let all: Vec<usize> = (0..5).collect();
    assert_eq!(dominant_culture(&all, &culture_id), Some(9));
    // No owned cell in the set → None (caller falls back to language 0).
    assert_eq!(dominant_culture(&[3], &culture_id), None);
    assert_eq!(dominant_culture(&[], &culture_id), None);
}

// ──────────────────────────────────────────────────────────────────────
// continent_at — the continent-aware drill's point→landmass query.
// ──────────────────────────────────────────────────────────────────────

/// A 100-cell line: cells 0..50 are one big landmass, 50..99 sea, cell 99 a
/// lone speck island. Sites at `[i, 0]`; linear neighbour chain.
fn linear_land_sea_world() -> WorldData {
    let mut world = WorldData {
        meta: WorldMeta::new(0),
        mesh: MeshData::default(),
        terrain: TerrainData::default(),
        ..Default::default()
    };
    let n = 100;
    world.mesh.sites = (0..n).map(|i| [i as f32, 0.0]).collect();
    world.mesh.neighbors = (0..n)
        .map(|i| {
            let mut ns = Vec::new();
            if i > 0 {
                ns.push((i - 1) as u32);
            }
            if i + 1 < n {
                ns.push((i + 1) as u32);
            }
            ns
        })
        .collect();
    world.mesh.width = n as f32;
    world.mesh.height = 1.0;
    let mut elev = vec![-1.0f32; n]; // sea by default
    (0..50).for_each(|i| elev[i] = 1.0); // big landmass
    elev[99] = 1.0; // a 1-cell speck
    world.terrain.elevation = elev;
    world
}

#[test]
fn continent_at_returns_the_clicked_landmass_centroid_and_size() {
    let world = linear_land_sea_world();
    // A point over the big landmass (cells 0..50) resolves to it.
    let hit = continent_at(&world, 25.0, 0.0).expect("point over land hits a continent");
    assert_eq!(hit.cell_count, 50);
    // Centroid is the mean of x = 0..49 = 24.5 (re-center target).
    assert!((hit.cx - 24.5).abs() < 1e-3, "centroid x was {}", hit.cx);
    assert!((hit.cy - 0.0).abs() < 1e-3);
}

#[test]
fn continent_at_is_none_over_sea_and_over_a_speck() {
    let world = linear_land_sea_world();
    // Over open sea (cells 50..99) → None: the caller grid-drills instead.
    assert_eq!(
        continent_at(&world, 75.0, 0.0),
        None,
        "sea is not a continent"
    );
    // Over the 1-cell speck at x=99 (1 of 100 cells, < 2.5%) → None.
    assert_eq!(
        continent_at(&world, 99.0, 0.0),
        None,
        "a speck is not a continent"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Generated-world contract — RED until `name_world` populates continents.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn generated_world_has_named_continents_and_an_ocean() {
    let world = generate_full(params(42));
    assert!(
        !world.continents.is_empty(),
        "a generated world should name at least one continent"
    );
    let (w, h) = (world.mesh.width, world.mesh.height);
    let mut prev = u32::MAX;
    for c in &world.continents {
        assert!(!c.name.is_empty(), "every continent earns a name");
        assert!(c.cell_count > 0, "a continent spans cells");
        assert!(
            c.centroid[0] >= 0.0
                && c.centroid[0] <= w
                && c.centroid[1] >= 0.0
                && c.centroid[1] <= h,
            "centroid {:?} within the world {w}×{h}",
            c.centroid
        );
        assert!(c.cell_count <= prev, "continents are ordered largest-first");
        prev = c.cell_count;
    }
    assert!(
        !world.oceans.is_empty(),
        "a world with a sea names at least one ocean"
    );
    assert!(world.oceans.iter().all(|o| !o.name.is_empty()));
}

#[test]
fn continent_name_is_grounded_in_the_owning_cultures_language() {
    // A single landmass entirely owned by one Elf culture: its name must be
    // generable from that culture's (Elf) language alphabet — i.e. grounded in
    // the owner, not a positional placeholder or a foreign tongue.
    let world = run_naming(elf_landmass());
    assert!(!world.continents.is_empty(), "the landmass should be named");
    let name = &world.continents[0].name;
    // name_world rebuilds languages from race; languages[0] is the Elf language.
    assert!(
        name_in_alphabet(name, &world.languages[0]),
        "continent name {name:?} is not in the owning culture's alphabet"
    );
}

#[test]
fn the_dominant_culture_among_competitors_determines_the_continent_name() {
    // Same two cultures (Dwarf = id 0, Elf = id 1) on the same 6-cell landmass;
    // only which one owns the MAJORITY differs. If naming ignored ownership (e.g.
    // always used language 0, or picked the wrong culture), the two names would
    // be identical — so the inequality pins that the *dominant* culture, not a
    // fixed index, drives the name. The single-culture test above can't catch
    // this: with one language, every selection collapses to index 0.
    let dwarf_major = run_naming(two_culture_landmass([0, 0, 0, 0, 1, 1]));
    let elf_major = run_naming(two_culture_landmass([1, 1, 1, 1, 0, 0]));

    let a = &dwarf_major.continents[0].name;
    let b = &elf_major.continents[0].name;
    assert_ne!(
        a, b,
        "swapping which culture dominates must change the grounded name"
    );
    // …and each is in its dominant culture's alphabet (Dwarf = languages[0],
    // Elf = languages[1]; the roster is the same in both worlds).
    assert!(
        name_in_alphabet(a, &dwarf_major.languages[0]),
        "Dwarf-dominant continent {a:?} not in the Dwarf alphabet"
    );
    assert!(
        name_in_alphabet(b, &elf_major.languages[1]),
        "Elf-dominant continent {b:?} not in the Elf alphabet"
    );
}

#[test]
fn continent_naming_is_deterministic_for_a_fixed_seed() {
    let a = generate_full(params(42));
    let b = generate_full(params(42));
    assert!(!a.continents.is_empty(), "non-trivial determinism check");
    assert_eq!(a.continents.len(), b.continents.len());
    for (x, y) in a.continents.iter().zip(&b.continents) {
        assert_eq!(x.name, y.name, "names stable across runs");
        assert_eq!(x.centroid, y.centroid, "centroids stable across runs");
        assert_eq!(x.cell_count, y.cell_count);
    }
}

/// Run the naming stage on a synthetic world with a fixed `Stage::Names` seed.
fn run_naming(mut world: WorldData) -> WorldData {
    let mut rng = StageRng::new(7).stream(Stage::Names);
    naming::name_world(&mut world, NamingParams::default(), &mut rng);
    world
}

/// True if every character of `name` is producible from `lang` (its vowels,
/// consonants, and any literal chars in its syllable patterns). `generate_name`
/// draws only from those, so a name grounded in `lang` always passes — a
/// positional placeholder or a foreign tongue (with chars outside the set) does
/// not.
fn name_in_alphabet(name: &str, lang: &mapgen_core::entities::Language) -> bool {
    let allowed: HashSet<char> = lang
        .vowels
        .iter()
        .chain(lang.consonants.iter())
        .copied()
        .chain(
            lang.syllable_patterns
                .iter()
                .flat_map(|p| p.chars())
                .filter(|c| *c != 'C' && *c != 'V'),
        )
        .collect();
    name.to_lowercase().chars().all(|c| allowed.contains(&c))
}

fn culture(race: Race) -> Culture {
    Culture {
        name: "TestFolk".into(),
        race,
        archetype_id: 0,
        language_id: 0,
        religion_id: None,
        alignment: Alignment::default(),
        tech: TechProfile::default(),
        magic: MagicStyle::None,
        settlement: SettlementIcon::Hall,
        architecture: Architecture::Classical,
        diplomatic: DiplomaticPattern::Mercantile,
    }
}

/// A 6-cell connected landmass (all elevation > 0), owned per `culture_id` by
/// the given `races` roster. Minimal state so `name_world` runs end to end (its
/// earlier passes iterate empty settlement/polity/religion vecs harmlessly).
fn landmass6(races: Vec<Race>, culture_id: [u16; 6]) -> WorldData {
    let mut world = WorldData {
        meta: WorldMeta::new(0),
        mesh: MeshData::default(),
        terrain: TerrainData::default(),
        ..Default::default()
    };
    world.mesh.sites = (0..6).map(|i| [i as f32, 0.0]).collect();
    world.mesh.neighbors = vec![
        vec![1],
        vec![0, 2],
        vec![1, 3],
        vec![2, 4],
        vec![3, 5],
        vec![4],
    ];
    world.mesh.width = 6.0;
    world.mesh.height = 1.0;
    world.terrain.elevation = vec![0.5; 6];
    world.cultures.cultures = races.into_iter().map(culture).collect();
    world.cultures.culture_id = culture_id.iter().map(|&c| Some(c)).collect();
    world
}

/// One landmass, one Elf culture (id 0) owning every cell.
fn elf_landmass() -> WorldData {
    landmass6(vec![Race::Elf], [0; 6])
}

/// One landmass shared by a Dwarf culture (id 0) and an Elf culture (id 1),
/// owned per `culture_id`. Their alphabets differ (Dwarf has k/g/b/d/t/u; Elf
/// has e/o/l/s/v/f/h), so a name reveals which culture was chosen as dominant.
fn two_culture_landmass(culture_id: [u16; 6]) -> WorldData {
    landmass6(vec![Race::Dwarf, Race::Elf], culture_id)
}

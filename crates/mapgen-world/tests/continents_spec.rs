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
    naming::{self, connected_bodies, dominant_culture, NamingParams},
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
fn continent_name_is_grounded_in_the_dominant_cultures_language() {
    // A single landmass entirely owned by one Elf culture: its name must be
    // generable from that culture's (Elf) language alphabet — i.e. grounded in
    // the owner, not a positional placeholder or a foreign tongue.
    let mut world = elf_landmass();
    let mut rng = StageRng::new(7).stream(Stage::Names);
    naming::name_world(&mut world, NamingParams::default(), &mut rng);

    assert!(!world.continents.is_empty(), "the landmass should be named");
    let lang = &world.languages[0]; // name_world rebuilds languages from race
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
    let name = &world.continents[0].name;
    assert!(
        name.to_lowercase().chars().all(|c| allowed.contains(&c)),
        "continent name {name:?} is not in the dominant culture's alphabet {allowed:?}"
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

/// A 6-cell connected landmass (all elevation > 0) entirely owned by one Elf
/// culture. Minimal state so `name_world` runs end to end (its earlier passes
/// iterate empty settlement/polity/religion vecs harmlessly).
fn elf_landmass() -> WorldData {
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
    world.cultures.cultures = vec![Culture {
        name: "Woodkin".into(),
        race: Race::Elf,
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
    world.cultures.culture_id = vec![Some(0); 6];
    world
}

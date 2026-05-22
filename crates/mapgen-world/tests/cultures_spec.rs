//! Phase 3a cultures spec — failing-red until `cultures::populate` is
//! implemented.
//!
//! Two kinds of tests live here:
//!
//! * **`when_*_populates`** — call `cultures::populate` on a real world
//!   and run the contract. Every one of these panics today (the impl is
//!   `todo!()`), and every one greens when the implementation commit
//!   lands. Same pattern as `cc2ba06` (Phase 2 spec red against todo!()).
//!
//! * **`synthetic_world_satisfies_contract`** — builds a hand-filled
//!   `WorldData` whose `cultures` field manually satisfies every
//!   invariant, then runs the same `check_*` helpers. **This test is
//!   green today.** Its job is to prove that the assertion code itself
//!   is well-formed — the populate-based tests can't tell you that
//!   while they panic upstream of the assertions.
//!
//! Source of truth: `docs/ARCHITECTURE.md` §4 Phase 3a exit criteria —
//!
//!   "Tests: every land cell has a culture, no culture's average
//!    habitat-score below 0.3, distribution is non-trivial."
//!
//! Plus structural invariants the schema implies (`culture_id` length
//! matches mesh cells; indices are valid; sea cells are `None`). The
//! habitat-fitness floor test is out of scope here because it needs an
//! oracle function the impl owns; it lands with the impl commit so the
//! test can call into the same fitness routine the assignment uses.

use mapgen_core::{
    entities::Culture, MeshData, Stage, StageRng, TerrainData, WorldData, WorldMeta,
};
use mapgen_world::{
    cultures::{self, CulturesParams},
    generate_full, GenerateParams,
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

fn world_with_cultures(seed: u64) -> WorldData {
    let mut world = generate_full(params(seed));
    let mut rng = StageRng::new(seed).stream(Stage::Cultures);
    cultures::populate(&mut world, CulturesParams::default(), &mut rng);
    world
}

// ──────────────────────────────────────────────────────────────────────
// Contract helpers — each invariant is a free function on `&WorldData`.
// Both the populate-based tests below AND `synthetic_world_satisfies_
// contract` call these, so the assertion bodies run in *some* test
// today (synthetic) and in *all* tests once impl lands (populate-based).
// ──────────────────────────────────────────────────────────────────────

fn check_culture_id_vector_length(world: &WorldData) {
    assert_eq!(
        world.cultures.culture_id.len(),
        world.mesh.cell_count(),
        "culture_id must have one slot per mesh cell"
    );
}

fn check_roster_is_non_empty(world: &WorldData) {
    assert!(
        !world.cultures.cultures.is_empty(),
        "at least one culture must be placed; an empty roster means no \
         land was habitable enough for any archetype"
    );
}

fn check_every_assigned_index_is_valid(world: &WorldData) {
    let roster_size = world.cultures.cultures.len();
    for (cell, &slot) in world.cultures.culture_id.iter().enumerate() {
        if let Some(id) = slot {
            assert!(
                (id as usize) < roster_size,
                "cell {cell} references culture index {id} but roster has \
                 only {roster_size} entries"
            );
        }
    }
}

fn check_every_land_cell_is_assigned(world: &WorldData) {
    // ARCHITECTURE.md §4 Phase 3a: "every land cell has a culture."
    let mut land_total = 0usize;
    let mut land_unassigned = Vec::new();
    for (cell, &elev) in world.terrain.elevation.iter().enumerate() {
        if elev < 0.0 {
            continue;
        }
        land_total += 1;
        if world.cultures.culture_id[cell].is_none() {
            land_unassigned.push(cell);
        }
    }
    assert!(
        land_unassigned.is_empty(),
        "{} of {land_total} land cells have no culture; first 8: {:?}",
        land_unassigned.len(),
        &land_unassigned[..land_unassigned.len().min(8)]
    );
}

fn check_every_sea_cell_is_unassigned(world: &WorldData) {
    // Implicit corollary — sea cells are explicitly excluded, not
    // "unassigned land."
    for (cell, &elev) in world.terrain.elevation.iter().enumerate() {
        if elev < 0.0 {
            assert!(
                world.cultures.culture_id[cell].is_none(),
                "sea cell {cell} (elev {elev:.3}) has culture assignment \
                 {:?} — should be None",
                world.cultures.culture_id[cell]
            );
        }
    }
}

fn check_distribution_is_non_trivial(world: &WorldData) {
    // ARCHITECTURE.md §4 Phase 3a: "distribution is non-trivial." A
    // one-culture world technically satisfies "every land cell has a
    // culture" but defeats the Mearsheimer-loop tension that needs ≥ 2
    // actors.
    let mut seen = std::collections::HashSet::new();
    for id in world.cultures.culture_id.iter().flatten() {
        seen.insert(*id);
    }
    assert!(
        seen.len() >= 2,
        "only {} distinct culture(s) assigned across the map: {seen:?}",
        seen.len()
    );
}

fn check_no_orphan_roster_entries(world: &WorldData) {
    // An entry in `cultures` with no cells in `culture_id` is dead data
    // (assignment under-filled the roster, or a discard pass forgot to
    // shrink the roster after removing a low-fitness culture).
    let roster_size = world.cultures.cultures.len();
    let mut cells_per_culture = vec![0usize; roster_size];
    for id in world.cultures.culture_id.iter().flatten() {
        cells_per_culture[*id as usize] += 1;
    }
    let orphans: Vec<usize> = cells_per_culture
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| if n == 0 { Some(i) } else { None })
        .collect();
    assert!(
        orphans.is_empty(),
        "culture(s) in roster with no cells assigned: {orphans:?} \
         (roster size {roster_size}, cells-per-culture {cells_per_culture:?})"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Synthetic-world test — green today; proves the helpers above are
// well-formed independently of `cultures::populate`.
// ──────────────────────────────────────────────────────────────────────

/// A minimal `WorldData` with 4 cells (2 land, 2 sea) and a hand-filled
/// `cultures` field whose values satisfy every contract invariant. No
/// climate / hydrology — the helpers don't touch them.
fn synthetic_contract_conforming_world() -> WorldData {
    let mut world = WorldData {
        meta: WorldMeta::new(0),
        mesh: MeshData::default(),
        terrain: TerrainData::default(),
        ..Default::default()
    };
    // 4 sites; the actual coordinates don't matter for contract checks,
    // only that `mesh.cell_count()` returns 4 and aligns with the other
    // per-cell vectors.
    world.mesh.sites = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
    world.terrain.elevation = vec![0.5, -0.5, 0.3, -0.2];
    world.cultures.cultures = vec![
        Culture {
            name: "Synth-A".into(),
            ..Default::default()
        },
        Culture {
            name: "Synth-B".into(),
            ..Default::default()
        },
    ];
    // Land cells (idx 0, 2) get cultures; sea cells (idx 1, 3) stay None.
    // Both roster entries get at least one cell → no orphans, ≥ 2 distinct.
    world.cultures.culture_id = vec![Some(0), None, Some(1), None];
    world
}

#[test]
fn synthetic_world_satisfies_contract() {
    let world = synthetic_contract_conforming_world();
    check_culture_id_vector_length(&world);
    check_roster_is_non_empty(&world);
    check_every_assigned_index_is_valid(&world);
    check_every_land_cell_is_assigned(&world);
    check_every_sea_cell_is_unassigned(&world);
    check_distribution_is_non_trivial(&world);
    check_no_orphan_roster_entries(&world);
}

// ──────────────────────────────────────────────────────────────────────
// Populate-based tests — RED today (panic from `todo!()` in populate).
// ──────────────────────────────────────────────────────────────────────

#[test]
fn when_populate_runs_culture_id_vector_matches_cell_count() {
    let world = world_with_cultures(42);
    check_culture_id_vector_length(&world);
}

#[test]
fn when_populate_runs_roster_is_non_empty() {
    let world = world_with_cultures(42);
    check_roster_is_non_empty(&world);
}

#[test]
fn when_populate_runs_every_assigned_index_is_valid() {
    let world = world_with_cultures(42);
    check_every_assigned_index_is_valid(&world);
}

#[test]
fn when_populate_runs_every_land_cell_has_a_culture() {
    let world = world_with_cultures(42);
    check_every_land_cell_is_assigned(&world);
}

#[test]
fn when_populate_runs_sea_cells_stay_unassigned() {
    let world = world_with_cultures(42);
    check_every_sea_cell_is_unassigned(&world);
}

#[test]
fn when_populate_runs_distribution_is_non_trivial() {
    let world = world_with_cultures(42);
    check_distribution_is_non_trivial(&world);
}

#[test]
fn when_populate_runs_roster_entries_all_have_cells() {
    let world = world_with_cultures(42);
    check_no_orphan_roster_entries(&world);
}

#[test]
fn when_populate_runs_output_is_deterministic_for_a_fixed_seed() {
    // Two runs with the same seed produce byte-identical output.
    // Catches an accidental `thread_rng()`; the seed-42 golden hash
    // extends this when the impl commit re-anchors it.
    let a = world_with_cultures(42);
    let b = world_with_cultures(42);
    assert_eq!(
        a.cultures.culture_id, b.cultures.culture_id,
        "two runs with seed 42 produced different per-cell assignments"
    );
    assert_eq!(
        a.cultures.cultures.len(),
        b.cultures.cultures.len(),
        "two runs with seed 42 produced different roster sizes"
    );
}

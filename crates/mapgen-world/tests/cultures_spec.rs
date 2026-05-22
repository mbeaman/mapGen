//! Phase 3a cultures spec — failing-red until `cultures::populate` is
//! implemented. Each test calls `populate` directly (not via
//! `generate_full`) so the skeleton stays decoupled from the existing
//! pipeline until implementation lands.
//!
//! Source of truth: `docs/ARCHITECTURE.md` §4 Phase 3a exit criteria —
//!
//!   "Tests: every land cell has a culture, no culture's average
//!    habitat-score below 0.3, distribution is non-trivial."
//!
//! Plus structural invariants the schema implies (`culture_id` length
//! matches mesh cells; indices are valid; sea cells are `None`). The
//! habitat-fitness floor test is out of scope for this spec because it
//! needs an oracle function the impl owns; it lands with the impl
//! commit so the test can call into the same fitness routine the
//! assignment uses.
//!
//! Discipline note: this file is in tree *after* the cultures module
//! stub (`98c7422`) and *before* its implementation. Until the impl
//! lands, every test below panics from the `todo!()` in `populate`.
//! That is the contract — first commit of a phase is the spec; last
//! commit makes it green.

use mapgen_core::{Stage, StageRng};
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

fn world_with_cultures(seed: u64) -> mapgen_core::WorldData {
    let mut world = generate_full(params(seed));
    let mut rng = StageRng::new(seed).stream(Stage::Cultures);
    cultures::populate(&mut world, CulturesParams::default(), &mut rng);
    world
}

// ──────────────────────────────────────────────────────────────────────
// Structural invariants — the schema implies these even before behavior.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn culture_id_vector_length_matches_cell_count() {
    let world = world_with_cultures(42);
    assert_eq!(
        world.cultures.culture_id.len(),
        world.mesh.cell_count(),
        "culture_id must have one slot per mesh cell"
    );
}

#[test]
fn roster_is_non_empty_after_populate() {
    let world = world_with_cultures(42);
    assert!(
        !world.cultures.cultures.is_empty(),
        "at least one culture must be placed; empty roster means no land was \
         habitable enough for any archetype — possible on degenerate worlds, \
         but seed 42 is not one of them"
    );
}

#[test]
fn every_assigned_culture_id_is_a_valid_index() {
    // ARCHITECTURE.md §4 Phase 3a implicit invariant: `culture_id` values
    // index into `cultures`. Stale references (a u16 past the end of the
    // roster) would crash the renderer + lore engine downstream.
    let world = world_with_cultures(42);
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

// ──────────────────────────────────────────────────────────────────────
// Behavioral invariants — ARCHITECTURE.md §4 Phase 3a exit criteria.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn every_land_cell_has_a_culture_assignment() {
    // ARCHITECTURE.md §4 Phase 3a: "every land cell has a culture."
    // Sea cells stay `None`; land cells (elevation >= 0) must be `Some`.
    let world = world_with_cultures(42);
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

#[test]
fn sea_cells_have_no_culture_assignment() {
    // Implicit corollary of "every land cell has a culture" — sea cells
    // are not "unassigned land," they are explicitly excluded.
    let world = world_with_cultures(42);
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

#[test]
fn culture_distribution_is_non_trivial() {
    // ARCHITECTURE.md §4 Phase 3a: "distribution is non-trivial."
    // Interpretation: at least two distinct cultures must be present in
    // the per-cell assignment. A world with one mega-culture covering
    // every land cell would technically satisfy "every land cell has a
    // culture" but defeat the point — Mearsheimer-loop tension needs at
    // least two actors.
    let world = world_with_cultures(42);
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

#[test]
fn roster_cultures_all_have_at_least_one_assigned_cell() {
    // An entry in `cultures` with no cells in `culture_id` is dead data —
    // either the assignment under-filled the roster, or a discard pass
    // forgot to shrink the roster after removing a low-fitness culture.
    let world = world_with_cultures(42);
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

#[test]
fn populate_is_deterministic_for_a_fixed_seed() {
    // Determinism is foundational; the seed-42 golden hash (extended
    // to Phase 3a in a later commit) will keep this honest, but pinning
    // it explicitly at the spec level catches a class of bug where
    // implementation reaches for `rand::thread_rng()` by accident.
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

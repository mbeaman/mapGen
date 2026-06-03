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
    generate_full,
    naming::connected_bodies,
    pipeline::{Pipeline, PipelineStage},
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

#[test]
fn when_populate_runs_mean_habitat_fitness_meets_the_floor() {
    // ARCHITECTURE.md §4 Phase 3a exit criterion: "no culture's average
    // habitat-score below 0.3." Calls into the same scoring function
    // (`cultures::habitat_fitness`) the assignment uses — so this test
    // can't drift from the algorithm. The culling pass in `populate`
    // discards sub-threshold cultures and reassigns their cells until
    // the survivors meet the bar (or until only 2 remain, the
    // distribution-non-trivial floor).
    //
    // If this test fails on seed 42, either the world is too uniform
    // for any 2 archetypes to clear 0.3, or the culling-reassignment
    // step is leaving cells in the wrong culture. The error message
    // names which culture and at what mean so the diagnosis path is
    // direct.
    let world = world_with_cultures(42);
    let floor = mapgen_world::cultures::CulturesParams::default().min_habitat_fitness;
    let n_cultures = world.cultures.cultures.len();
    for culture_idx in 0..n_cultures {
        let mean = mapgen_world::cultures::mean_habitat_fitness(&world, culture_idx as u16);
        assert!(
            mean >= floor,
            "culture {} ({:?}, archetype_id {}) has mean habitat-fitness {:.3}, \
             below the {floor:.2} floor",
            culture_idx,
            world.cultures.cultures[culture_idx].name,
            world.cultures.cultures[culture_idx].archetype_id,
            mean
        );
    }
}

// ──────────────────────────────────────────────────────────────────────
// Landmass-distinct society (the Sundered Lanes foundation rework). Cultures
// are INSTANCED per landmass — the same archetype on two continents becomes two
// distinct `Culture` ids — so every downstream stage (polities/control,
// history, naming) confines to one landmass and inter-continental reach becomes
// something the sea lanes must EARN. See docs/inter_continental_design.md.
//
// These run on PLANET seeds (multi-continent); seed42 is single-landmass, where
// the instancing is the identity of the old numbering (golden no-op).
// ──────────────────────────────────────────────────────────────────────

/// Sizable landmasses (>= the sea-lanes islet threshold), and a per-cell body
/// label (`usize::MAX` for sea / sub-threshold islets), via the same flood-fill
/// the cultures instancing and sea-lanes stages use.
fn sizable_body_labels(world: &WorldData) -> (usize, Vec<usize>) {
    let bodies = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] >= 0.0);
    let mut body_of = vec![usize::MAX; world.mesh.cell_count()];
    let mut kept = 0usize;
    for body in &bodies {
        if body.len() < 24 {
            continue;
        }
        for &c in body {
            body_of[c] = kept;
        }
        kept += 1;
    }
    (kept, body_of)
}

#[test]
fn cultures_are_instanced_per_landmass() {
    // The direct instancing signal: every culture id is confined to ONE
    // landmass. Before the rework a global "Riverfolk" spanned every continent
    // — this asserts that can no longer happen.
    let world = generate_full(GenerateParams::planet(11));
    let (n_bodies, body_of) = sizable_body_labels(&world);
    assert!(
        n_bodies >= 2,
        "planet seed 11 must have multiple landmasses"
    );

    let n_cultures = world.cultures.cultures.len();
    let mut body_of_culture = vec![usize::MAX; n_cultures];
    for (c, &bi) in body_of.iter().enumerate() {
        if let Some(cid) = world.cultures.culture_id[c] {
            if bi == usize::MAX {
                continue; // sub-threshold islet — not a sizable body
            }
            let slot = &mut body_of_culture[cid as usize];
            if *slot == usize::MAX {
                *slot = bi;
            } else {
                assert_eq!(
                    *slot, bi,
                    "culture {cid} appears on two landmasses ({slot} and {bi}) — not instanced",
                );
            }
        }
    }
    // And instancing genuinely multiplied the roster past the ~5 archetypes.
    assert!(
        n_cultures > 8,
        "expected many per-landmass culture instances, got {n_cultures}",
    );
}

#[test]
fn polities_are_confined_to_one_landmass_yet_the_planet_is_populated() {
    // THE load-bearing guard the premise failure named: AT GEN TIME no polity
    // controls cells on more than one sizable landmass (probe measured 3 such
    // polities BEFORE the rework → this asserts 0). Checked at the Polities stage,
    // BEFORE history — the cross-water carrier (Mearsheimer) legitimately creates
    // earned overseas holdings later (see history_spec), so this must read the
    // gen-time partition, not the post-history world. Paired with a population
    // floor so it can't pass on a GUTTED planet (over-aggressive instancing that
    // left most land uncontrolled would also trivially "confine").
    for seed in [11u64, 19] {
        let mut p = Pipeline::new(GenerateParams::planet(seed));
        loop {
            match p.step() {
                Some(PipelineStage::Polities) => break,
                Some(_) => continue,
                None => panic!("pipeline finished before the Polities stage"),
            }
        }
        let world = p.into_world();
        let (n_bodies, body_of) = sizable_body_labels(&world);
        assert!(
            n_bodies >= 2,
            "planet seed {seed} must have multiple landmasses"
        );

        let n_pol = world.society.nations.len();
        let mut spans: Vec<std::collections::BTreeSet<usize>> = vec![Default::default(); n_pol];
        let (mut land, mut owned) = (0usize, 0usize);
        for (c, &bi) in body_of.iter().enumerate() {
            if bi == usize::MAX {
                continue;
            }
            land += 1;
            if let Some(p) = world.society.control[c] {
                owned += 1;
                spans[p as usize].insert(bi);
            }
        }

        // Confinement: the carrier-blocker is gone.
        let multi = spans.iter().filter(|s| s.len() >= 2).count();
        assert_eq!(
            multi, 0,
            "seed {seed}: {multi} polities still span >1 landmass (must be 0 — reach must be earned)",
        );

        // Populated, not gutted: most sizable-body land is controlled, and there
        // are at least as many polities as landmasses (each continent has its
        // own society). Probe measured 92–100% controlled, 13–22 polities.
        let pct = 100 * owned / land.max(1);
        assert!(
            pct >= 80,
            "seed {seed}: only {pct}% of sizable-body land controlled — the planet is gutted",
        );
        assert!(
            n_pol >= n_bodies,
            "seed {seed}: {n_pol} polities for {n_bodies} landmasses — too sparse to be per-continent",
        );
    }
}

#[test]
fn no_culture_instance_spans_a_sea_lanes_body() {
    // The arc's premise is "the sea lanes are the ONLY inter-body link", which
    // requires cultures and sea_lanes to agree on what a landmass IS. They use
    // different land predicates: cultures instances over `elev >= 0.0`, sea_lanes
    // flood-fills land with `elev > 0.0` (cells at exactly sea level are sea
    // there). Today no cell sits at exactly 0.0 so the two agree — but a future
    // zero-elevation cell bridging two `>0.0` components would merge them into ONE
    // culture instance while sea_lanes still builds a lane between them, putting a
    // single polity on both sides of a lane (the spanning bug RELATIVE to the lane
    // graph, invisible to the >=0.0-based confinement test). This pins the
    // invariant independently of the cultures predicate, so it fails loudly if
    // that divergence ever becomes real. (Passes trivially today; that's the
    // point — it's the tripwire.)
    let world = generate_full(GenerateParams::planet(11));
    let sea_bodies = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] > 0.0);
    let mut sea_body_of = vec![usize::MAX; world.mesh.cell_count()];
    for (bi, b) in sea_bodies.iter().enumerate() {
        for &c in b {
            sea_body_of[c] = bi;
        }
    }
    let n_cultures = world.cultures.cultures.len();
    let mut body_of_culture = vec![usize::MAX; n_cultures];
    for (c, &sbi) in sea_body_of.iter().enumerate() {
        if sbi == usize::MAX {
            continue; // sea, or a cell sea_lanes counts as non-land
        }
        if let Some(cid) = world.cultures.culture_id[c] {
            let slot = &mut body_of_culture[cid as usize];
            if *slot == usize::MAX {
                *slot = sbi;
            } else {
                assert_eq!(
                    *slot, sbi,
                    "culture {cid} spans two sea_lanes bodies ({slot} and {sbi}) — \
                     breaks 'the sea lanes are the only inter-body link'",
                );
            }
        }
    }
}

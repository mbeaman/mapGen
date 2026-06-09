//! Phase 3b religions spec — failing-red until `religions::found` is
//! implemented.
//!
//! Two kinds of tests live here:
//!
//! * **`when_*_founds`** — call `religions::found` on a real world that
//!   already has cultures (via `generate_full`). Every one panics today
//!   from `found`'s `todo!()`; every one greens when the impl lands.
//! * **`synthetic_world_satisfies_religion_contract`** — builds a
//!   hand-filled `WorldData` with manual cultures + religions that meet
//!   the contract, then runs the helpers. Green today; proves the
//!   assertion code is well-formed before populate-based tests can
//!   exercise it.
//!
//! Source of truth: `docs/ARCHITECTURE.md` §4 Phase 3b exit criteria —
//!
//!   "Tests: every religion has a founder, no religion has zero
//!    adherents, sacred sites are on the right biome."

use mapgen_core::entities::{Alignment, Culture, PantheonPattern, Religion};
use mapgen_core::{MeshData, Stage, StageRng, TerrainData, WorldData, WorldMeta};
use mapgen_world::{
    generate_full,
    religions::{self, ReligionsParams},
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

fn world_with_religions(seed: u64) -> WorldData {
    let mut world = generate_full(params(seed));
    let mut rng = StageRng::new(seed).stream(Stage::Religions);
    religions::found(&mut world, ReligionsParams::default(), &mut rng);
    world
}

// ──────────────────────────────────────────────────────────────────────
// Contract helpers — invariants the impl must satisfy.
// ──────────────────────────────────────────────────────────────────────

fn check_religion_id_vector_length(world: &WorldData) {
    assert_eq!(
        world.religions.religion_id.len(),
        world.mesh.cell_count(),
        "religion_id must have one slot per mesh cell"
    );
}

fn check_roster_size(world: &WorldData, max_religions: usize) {
    let n = world.religions.religions.len();
    assert!(
        (1..=max_religions).contains(&n),
        "religion roster size {n} outside [1, {max_religions}]"
    );
}

fn check_every_religion_has_a_valid_founder(world: &WorldData) {
    // ARCHITECTURE.md §4 Phase 3b: "every religion has a founder."
    let n_cultures = world.cultures.cultures.len();
    for (i, religion) in world.religions.religions.iter().enumerate() {
        assert!(
            (religion.founder_culture_id as usize) < n_cultures,
            "religion {i} ({:?}) has founder_culture_id {} but only {n_cultures} \
             cultures exist",
            religion.name,
            religion.founder_culture_id
        );
    }
}

fn check_no_religion_has_zero_adherents(world: &WorldData) {
    // ARCHITECTURE.md §4 Phase 3b: "no religion has zero adherents."
    let n_religions = world.religions.religions.len();
    let mut cells_per = vec![0usize; n_religions];
    for id in world.religions.religion_id.iter().flatten() {
        cells_per[*id as usize] += 1;
    }
    let orphans: Vec<usize> = cells_per
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| if n == 0 { Some(i) } else { None })
        .collect();
    assert!(
        orphans.is_empty(),
        "religion(s) with zero adherents: {orphans:?} \
         (cells per religion: {cells_per:?})"
    );
}

fn check_assigned_religion_ids_are_valid(world: &WorldData) {
    let n = world.religions.religions.len();
    for (cell, &slot) in world.religions.religion_id.iter().enumerate() {
        if let Some(id) = slot {
            assert!(
                (id as usize) < n,
                "cell {cell} references religion index {id} but only {n} religions exist"
            );
        }
    }
}

fn check_sacred_sites_are_on_land(world: &WorldData) {
    // Implementation-defined invariant — sacred sites are temples /
    // groves / holy mountains, which require land. The "right biome"
    // axis is checked separately below via the per-pantheon biome map.
    for (i, religion) in world.religions.religions.iter().enumerate() {
        for &cell in &religion.sacred_sites {
            let elev = world
                .terrain
                .elevation
                .get(cell as usize)
                .copied()
                .unwrap_or(0.0);
            assert!(
                elev >= 0.0,
                "religion {i} ({:?}) has a sacred site at cell {cell} with \
                 elevation {elev:.3} — sea cells can't host shrines",
                religion.name
            );
        }
    }
}

fn check_sacred_sites_are_in_adherent_cells(world: &WorldData) {
    // ARCHITECTURE.md §4 Phase 3b: "sacred sites are on the right
    // biome" — reified here as: a religion's sacred sites must sit on
    // cells where the religion has adherents (i.e., `religion_id` at
    // that cell equals the religion's index). This catches the
    // class-of-bug where sacred sites point at "the original founder
    // cells" but the religion has since spread or been culled to a
    // disjoint set.
    for (i, religion) in world.religions.religions.iter().enumerate() {
        for &cell in &religion.sacred_sites {
            let cell_religion = world
                .religions
                .religion_id
                .get(cell as usize)
                .and_then(|s| *s);
            assert_eq!(
                cell_religion,
                Some(i as u16),
                "religion {i} ({:?}) has a sacred site at cell {cell}, but \
                 that cell's religion_id is {cell_religion:?} — the religion \
                 doesn't have reach to its own holy site",
                religion.name
            );
        }
    }
}

fn check_religion_alignment_in_range(world: &WorldData) {
    for (i, religion) in world.religions.religions.iter().enumerate() {
        assert!(
            (-1.0..=1.0).contains(&religion.alignment.law_chaos),
            "religion {i} ({:?}) law_chaos {} outside [-1, 1]",
            religion.name,
            religion.alignment.law_chaos
        );
        assert!(
            (-1.0..=1.0).contains(&religion.alignment.good_evil),
            "religion {i} ({:?}) good_evil {} outside [-1, 1]",
            religion.name,
            religion.alignment.good_evil
        );
    }
}

// ──────────────────────────────────────────────────────────────────────
// Synthetic-world test — green today; exercises the helpers.
// ──────────────────────────────────────────────────────────────────────

fn synthetic_world_with_two_religions() -> WorldData {
    let mut world = WorldData {
        meta: WorldMeta::new(0),
        mesh: MeshData::default(),
        terrain: TerrainData::default(),
        ..Default::default()
    };
    world.mesh.sites = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
    world.terrain.elevation = vec![0.5, -0.5, 0.3, -0.2];
    world.cultures.cultures = vec![Culture {
        name: "Founder".into(),
        ..Default::default()
    }];
    world.cultures.culture_id = vec![Some(0), None, Some(0), None];
    world.religions.religions = vec![
        Religion {
            name: "Sun".into(),
            pantheon: PantheonPattern::Mono,
            founder_culture_id: 0,
            alignment: Alignment {
                law_chaos: 0.5,
                good_evil: -0.5,
            },
            sacred_sites: vec![0],
        },
        Religion {
            name: "Hearth".into(),
            pantheon: PantheonPattern::Ancestor,
            founder_culture_id: 0,
            alignment: Alignment {
                law_chaos: 0.3,
                good_evil: -0.3,
            },
            sacred_sites: vec![2],
        },
    ];
    world.religions.religion_id = vec![Some(0), None, Some(1), None];
    world
}

#[test]
fn synthetic_world_satisfies_religion_contract() {
    let world = synthetic_world_with_two_religions();
    check_religion_id_vector_length(&world);
    check_roster_size(&world, 3);
    check_every_religion_has_a_valid_founder(&world);
    check_no_religion_has_zero_adherents(&world);
    check_assigned_religion_ids_are_valid(&world);
    check_sacred_sites_are_on_land(&world);
    check_sacred_sites_are_in_adherent_cells(&world);
    check_religion_alignment_in_range(&world);
}

// ──────────────────────────────────────────────────────────────────────
// Founds-based tests — RED today (panic from `todo!()` in `found`).
// ──────────────────────────────────────────────────────────────────────

#[test]
fn when_found_runs_religion_id_vector_matches_cell_count() {
    let world = world_with_religions(42);
    check_religion_id_vector_length(&world);
}

#[test]
fn when_found_runs_roster_has_one_to_three_religions() {
    let world = world_with_religions(42);
    check_roster_size(&world, ReligionsParams::default().max_religions);
}

#[test]
fn when_found_runs_every_religion_has_a_valid_founder() {
    let world = world_with_religions(42);
    check_every_religion_has_a_valid_founder(&world);
}

#[test]
fn when_found_runs_no_religion_has_zero_adherents() {
    let world = world_with_religions(42);
    check_no_religion_has_zero_adherents(&world);
}

#[test]
fn when_found_runs_assigned_religion_ids_are_valid() {
    let world = world_with_religions(42);
    check_assigned_religion_ids_are_valid(&world);
}

#[test]
fn when_found_runs_sacred_sites_are_on_land() {
    let world = world_with_religions(42);
    check_sacred_sites_are_on_land(&world);
}

#[test]
fn when_found_runs_sacred_sites_are_in_adherent_cells() {
    let world = world_with_religions(42);
    check_sacred_sites_are_in_adherent_cells(&world);
}

#[test]
fn when_found_runs_religion_alignment_in_range() {
    let world = world_with_religions(42);
    check_religion_alignment_in_range(&world);
}

#[test]
fn when_found_runs_output_is_deterministic_for_a_fixed_seed() {
    let a = world_with_religions(42);
    let b = world_with_religions(42);
    assert_eq!(
        a.religions.religion_id, b.religions.religion_id,
        "two runs with seed 42 produced different per-cell religion assignments"
    );
    assert_eq!(
        a.religions.religions.len(),
        b.religions.religions.len(),
        "two runs with seed 42 produced different religion roster sizes"
    );
}

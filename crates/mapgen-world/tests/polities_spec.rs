//! Phase 3c polities spec — failing-red until `polities::lay_out` is
//! implemented.
//!
//! Source of truth: TASKS.md Phase 3c — "every settlement reachable
//! from its capital; capital on suitable cell; Zipf rank-size."
//!
//! Plus structural invariants the schema implies (polity_id values are
//! valid indices, capitals are on land cells, etc.) and a determinism
//! pin.

use std::collections::HashSet;

use mapgen_core::entities::{Culture, Settlement, SettlementTier};
use mapgen_core::world_data::{Nation, Road};
use mapgen_core::{MeshData, Stage, StageRng, TerrainData, WorldData, WorldMeta};
use mapgen_world::{
    cultures, generate_full,
    polities::{self, PolitiesParams},
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

fn world_with_polities(seed: u64) -> WorldData {
    let mut world = generate_full(params(seed));
    let mut rng = StageRng::new(seed).stream(Stage::Capitals);
    polities::lay_out(&mut world, PolitiesParams::default(), &mut rng);
    world
}

// ──────────────────────────────────────────────────────────────────────
// Contract helpers.
// ──────────────────────────────────────────────────────────────────────

fn check_polities_non_empty(world: &WorldData) {
    assert!(
        !world.society.nations.is_empty(),
        "polities stage produced 0 polities"
    );
}

fn check_each_polity_has_a_capital_on_land(world: &WorldData) {
    for (p, polity) in world.society.nations.iter().enumerate() {
        let cell = polity.capital_cell as usize;
        let elev = world.terrain.elevation.get(cell).copied().unwrap_or(0.0);
        assert!(
            elev >= 0.0,
            "polity {p} ({:?}) capital at cell {cell} has elev {elev:.3} — sea",
            polity.name
        );
    }
}

fn check_capital_is_on_a_suitable_cell(world: &WorldData) {
    // TASKS.md: "capital on suitable cell." Reified: the culture that
    // owns the capital cell must score `habitat_fitness >= 0.4` there
    // (a touch above the 0.3 cultures-stage floor — capitals get the
    // best-of-the-best). If the cultures-stage placement was sane, this
    // is satisfied by construction; this test catches a regression
    // where polities-stage picked a capital on a marginal cell.
    let archetypes = cultures::load_archetypes();
    for (p, polity) in world.society.nations.iter().enumerate() {
        let cell = polity.capital_cell as usize;
        let culture_idx = world
            .cultures
            .culture_id
            .get(cell)
            .and_then(|s| *s)
            .unwrap_or_else(|| {
                panic!(
                    "polity {p} ({:?}) capital cell {cell} has no culture",
                    polity.name
                )
            });
        let culture = &world.cultures.cultures[culture_idx as usize];
        let archetype = &archetypes[culture.archetype_id as usize];
        let fit = cultures::habitat_fitness(world, cell, archetype);
        assert!(
            fit >= 0.4,
            "polity {p} ({:?}) capital at cell {cell} has habitat-fitness \
             {fit:.3} for its culture {:?} — below 0.4 ('suitable cell')",
            polity.name,
            culture.name
        );
    }
}

fn check_settlement_polity_ids_are_valid(world: &WorldData) {
    let n_polities = world.society.nations.len();
    for (i, s) in world.society.settlements.iter().enumerate() {
        assert!(
            (s.polity_id as usize) < n_polities,
            "settlement {i} ({:?}) references polity {} but only {n_polities} exist",
            s.name,
            s.polity_id
        );
    }
}

fn check_every_polity_has_one_capital(world: &WorldData) {
    let n = world.society.nations.len();
    let mut capital_count = vec![0usize; n];
    for s in &world.society.settlements {
        if s.tier == SettlementTier::Capital {
            capital_count[s.polity_id as usize] += 1;
        }
    }
    for (i, &c) in capital_count.iter().enumerate() {
        assert_eq!(c, 1, "polity {i} has {c} capitals (expected exactly 1)");
    }
}

fn check_every_settlement_reachable_from_its_capital(world: &WorldData) {
    // TASKS.md: "every settlement reachable from its capital."
    // Connectivity check through road cells: for each polity, BFS from
    // the capital's cell through the union of road cells, then assert
    // every town/village of that polity sits in the visited set.
    let n_polities = world.society.nations.len();
    for p in 0..n_polities {
        let capital_cell = world.society.nations[p].capital_cell;
        let polity_settlements: Vec<&Settlement> = world
            .society
            .settlements
            .iter()
            .filter(|s| s.polity_id as usize == p)
            .collect();
        if polity_settlements.len() <= 1 {
            continue; // only the capital — trivially connected
        }
        // Build a set of road-traversable cells for this polity. Any road
        // touching the capital opens the network — we include all roads
        // for simplicity, then BFS.
        let mut road_cells: HashSet<u32> = HashSet::new();
        for road in &world.society.roads {
            for &c in &road.cells {
                road_cells.insert(c);
            }
        }
        let visited = bfs_through_roads(world, capital_cell, &road_cells);
        for s in &polity_settlements {
            if s.tier == SettlementTier::Capital {
                continue;
            }
            assert!(
                visited.contains(&s.cell),
                "polity {p}: settlement {:?} (tier {:?}) at cell {} is not road-\
                 reachable from capital at cell {capital_cell}",
                s.name,
                s.tier,
                s.cell
            );
        }
    }
}

/// BFS through `start` and any neighbor that's either `start` itself or
/// in `road_cells`. Returns the set of reachable cells. Used to verify
/// "every settlement reachable from its capital."
fn bfs_through_roads(world: &WorldData, start: u32, road_cells: &HashSet<u32>) -> HashSet<u32> {
    let mut visited = HashSet::new();
    let mut frontier = vec![start];
    visited.insert(start);
    while let Some(cell) = frontier.pop() {
        for &nb in &world.mesh.neighbors[cell as usize] {
            if visited.contains(&nb) {
                continue;
            }
            if road_cells.contains(&nb) {
                visited.insert(nb);
                frontier.push(nb);
            }
        }
    }
    visited
}

fn check_zipf_rank_size_holds(world: &WorldData) {
    // TASKS.md: "Zipf rank-size." Real Zipf is population ∝ 1/rank; for
    // MVP with two tiers it's enough to assert capitals out-populate
    // towns, AND no town exceeds any capital. Catches a bug where the
    // tier→population mapping inverted.
    let capitals_max = world
        .society
        .settlements
        .iter()
        .filter(|s| s.tier == SettlementTier::Capital)
        .map(|s| s.population)
        .fold(f32::NEG_INFINITY, f32::max);
    let towns_max = world
        .society
        .settlements
        .iter()
        .filter(|s| s.tier == SettlementTier::Town)
        .map(|s| s.population)
        .fold(0.0_f32, f32::max);
    let capitals_min = world
        .society
        .settlements
        .iter()
        .filter(|s| s.tier == SettlementTier::Capital)
        .map(|s| s.population)
        .fold(f32::INFINITY, f32::min);
    assert!(
        capitals_min >= towns_max,
        "Zipf rank-size violated: smallest capital population {capitals_min:.3} \
         < largest town population {towns_max:.3} (capitals_max={capitals_max:.3})"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Synthetic-world test — green today.
// ──────────────────────────────────────────────────────────────────────

fn synthetic_world_with_one_polity() -> WorldData {
    let mut world = WorldData {
        meta: WorldMeta::new(0),
        mesh: MeshData::default(),
        terrain: TerrainData::default(),
        ..Default::default()
    };
    // 4 cells: 3 land in a line + 1 sea.
    world.mesh.sites = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
    world.mesh.neighbors = vec![vec![1], vec![0, 2], vec![1, 3], vec![2]];
    world.terrain.elevation = vec![0.5, 0.4, 0.6, -0.2];
    world.cultures.cultures = vec![Culture {
        name: "TestFolk".into(),
        ..Default::default()
    }];
    // Land cells (0, 1, 2) all assigned to the one culture.
    world.cultures.culture_id = vec![Some(0), Some(0), Some(0), None];
    world.society.nations = vec![Nation {
        name: "TestRealm".into(),
        capital_cell: 0,
        color: [200, 100, 100],
        ..Default::default()
    }];
    world.society.settlements = vec![
        Settlement {
            name: "Capital".into(),
            cell: 0,
            tier: SettlementTier::Capital,
            polity_id: 0,
            population: 1.0,
        },
        Settlement {
            name: "Town".into(),
            cell: 2,
            tier: SettlementTier::Town,
            polity_id: 0,
            population: 0.5,
        },
    ];
    // One road connecting capital (cell 0) to town (cell 2) via cell 1.
    world.society.roads = vec![Road {
        cells: vec![0, 1, 2],
    }];
    world.society.control = vec![Some(0), Some(0), Some(0), None];
    world
}

#[test]
fn synthetic_world_satisfies_polity_contract() {
    let world = synthetic_world_with_one_polity();
    check_polities_non_empty(&world);
    check_each_polity_has_a_capital_on_land(&world);
    // Skip `check_capital_is_on_a_suitable_cell` — it requires the real
    // archetypes CSV + a populated `world.climate.biome` for the
    // fitness function to be meaningful. The other helpers don't need
    // the full pipeline.
    check_settlement_polity_ids_are_valid(&world);
    check_every_polity_has_one_capital(&world);
    check_every_settlement_reachable_from_its_capital(&world);
    check_zipf_rank_size_holds(&world);
}

// ──────────────────────────────────────────────────────────────────────
// lay_out-based tests — RED until impl lands.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn when_lay_out_runs_polities_are_non_empty() {
    let world = world_with_polities(42);
    check_polities_non_empty(&world);
}

#[test]
fn when_lay_out_runs_capitals_are_on_land() {
    let world = world_with_polities(42);
    check_each_polity_has_a_capital_on_land(&world);
}

#[test]
fn when_lay_out_runs_capital_is_on_a_suitable_cell() {
    let world = world_with_polities(42);
    check_capital_is_on_a_suitable_cell(&world);
}

#[test]
fn when_lay_out_runs_settlement_polity_ids_are_valid() {
    let world = world_with_polities(42);
    check_settlement_polity_ids_are_valid(&world);
}

#[test]
fn when_lay_out_runs_every_polity_has_one_capital() {
    let world = world_with_polities(42);
    check_every_polity_has_one_capital(&world);
}

#[test]
fn when_lay_out_runs_every_settlement_reachable_from_its_capital() {
    let world = world_with_polities(42);
    check_every_settlement_reachable_from_its_capital(&world);
}

#[test]
fn when_lay_out_runs_zipf_rank_size_holds() {
    let world = world_with_polities(42);
    check_zipf_rank_size_holds(&world);
}

#[test]
fn when_lay_out_runs_output_is_deterministic_for_a_fixed_seed() {
    let a = world_with_polities(42);
    let b = world_with_polities(42);
    assert_eq!(
        a.society.nations.len(),
        b.society.nations.len(),
        "two runs with seed 42 produced different polity counts"
    );
    let a_caps: Vec<u32> = a.society.nations.iter().map(|n| n.capital_cell).collect();
    let b_caps: Vec<u32> = b.society.nations.iter().map(|n| n.capital_cell).collect();
    assert_eq!(
        a_caps, b_caps,
        "two runs with seed 42 produced different capital cells"
    );
    assert_eq!(
        a.society.settlements.len(),
        b.society.settlements.len(),
        "two runs with seed 42 produced different settlement counts"
    );
}

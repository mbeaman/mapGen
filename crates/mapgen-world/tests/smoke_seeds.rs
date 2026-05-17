//! Smoke-seed sweep — runs the full geography pipeline on 10 seeds and
//! asserts a minimum-viable world per seed. Catches future seed-specific
//! generation regressions (`generate_full` panics on a seed, or a class of
//! seeds produces a degenerate world) without requiring per-bug specs.
//!
//! Per-seed acceptance gates (any failure = a seed-specific bug to file):
//!   * Generation does not panic.
//!   * Both land and sea cells exist.
//!   * At least 3 distinct biomes appear on land.
//!   * At least 1 river is extracted.
//!   * At least 1 lake is extracted.
//!
//! Uses 4000 cells per seed — same as `phase2_spec` / `realism_spec`, the
//! smallest count at which lake formation is reliable on every seed in
//! this range. Runs in well under 10s in release; debug-build runs
//! slower but still in the seconds, not minutes.

use std::collections::BTreeSet;

use mapgen_world::{generate_full, GenerateParams};

fn smoke_params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    }
}

#[test]
fn all_ten_seeds_produce_viable_worlds() {
    for seed in 1u64..=10 {
        let world = generate_full(smoke_params(seed));
        let n = world.mesh.cell_count();

        let land = (0..n).filter(|&i| world.terrain.elevation[i] > 0.0).count();
        let sea = n - land;
        assert!(
            land > 0 && sea > 0,
            "seed {seed}: degenerate land/sea split (land={land}, sea={sea})"
        );

        let land_biomes: BTreeSet<u8> = (0..n)
            .filter(|&i| world.terrain.elevation[i] > 0.0)
            .map(|i| world.climate.biome[i])
            .collect();
        assert!(
            land_biomes.len() >= 3,
            "seed {seed}: only {} distinct land biome(s) ({:?}); expected ≥3",
            land_biomes.len(),
            land_biomes
        );

        let river_count = world.hydrology.rivers.len();
        assert!(
            river_count >= 1,
            "seed {seed}: no rivers extracted (river_count=0)"
        );

        let lake_count = world.hydrology.lakes.len();
        assert!(
            lake_count >= 1,
            "seed {seed}: no lakes extracted (lake_count=0)"
        );
    }
}

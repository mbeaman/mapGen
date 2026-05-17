//! River realism spec — addresses the calibration failures the user
//! identified in the rendered maps:
//!
//!   * Too many rivers (every cell has one).
//!   * Rivers starting anywhere instead of in highlands / springs / lakes.
//!   * No riparian ecosystem effect (no green corridor through arid biomes).
//!   * No visible width growth at confluences (single-width polylines).
//!
//! Each test below is an executable check that one of those properties
//! holds on the generated world. Tests do NOT cite a single seed where
//! avoidable; they check distribution-level properties so they survive
//! seed variation.

use mapgen_world::{generate_full, GenerateParams};

fn params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 2048.0,
        height: 1280.0,
        cell_count: 6000,
        plate_count: 14,
        nation_count: 8,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// River count is bounded
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn rivers_dont_cover_more_than_a_quarter_of_land() {
    // Earth's actual river-cell fraction is maybe 5% of land. We accept
    // 25% as the upper bound (our cells are coarser than reality);
    // anything higher means the threshold is mis-calibrated.
    let world = generate_full(params(42));
    let total_land = (0..world.mesh.cell_count())
        .filter(|&i| world.terrain.elevation[i] > 0.0)
        .count();
    let river_cells: std::collections::BTreeSet<u32> = world
        .hydrology
        .rivers
        .iter()
        .flat_map(|r| r.cells.iter().copied())
        .filter(|&c| world.terrain.elevation[c as usize] > 0.0)
        .collect();
    let frac = river_cells.len() as f32 / total_land as f32;
    assert!(
        frac < 0.25,
        "river cells cover {:.1}% of land (expected <25%)",
        frac * 100.0
    );
}

#[test]
fn no_more_than_one_river_per_50_land_cells() {
    // Real continents have on the order of one major river per 1000 km²;
    // at our cell scale that's roughly one river per 50-100 land cells.
    // We assert <= 1 per 50 as a loose upper bound.
    let world = generate_full(params(42));
    let land = (0..world.mesh.cell_count())
        .filter(|&i| world.terrain.elevation[i] > 0.0)
        .count();
    let n_rivers = world.hydrology.rivers.len();
    let cells_per_river = land as f32 / n_rivers.max(1) as f32;
    assert!(
        cells_per_river >= 50.0,
        "{} rivers on {} land cells ({:.1} cells/river — too many)",
        n_rivers,
        land,
        cells_per_river
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Headwaters are physical: mountains, springs, or lake outlets
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn river_headwaters_originate_in_highlands_or_at_lake_outlets() {
    // The first cell of every river chain should be one of:
    //   (a) high-elevation cell (≥ 0.30 normalized — mountain / hill)
    //   (b) a cell adjacent to a lake (lake outlet)
    //   (c) a cell in a high-precipitation zone (orographic spring)
    // Rivers that "start" in lowland savanna are non-physical.
    let world = generate_full(params(42));
    let elev = &world.terrain.elevation;
    let neighbors = &world.mesh.neighbors;

    let lake_cells: std::collections::BTreeSet<u32> = world
        .hydrology
        .lakes
        .iter()
        .flat_map(|l| l.cells.iter().copied())
        .collect();

    let mut non_physical = 0;
    for river in &world.hydrology.rivers {
        let h = river.cells[0] as usize;
        let is_highland = elev[h] >= 0.30;
        let is_lake_adjacent = neighbors[h].iter().any(|&j| lake_cells.contains(&j));
        if !is_highland && !is_lake_adjacent {
            non_physical += 1;
        }
    }
    let frac = non_physical as f32 / world.hydrology.rivers.len().max(1) as f32;
    assert!(
        frac < 0.10,
        "{:.1}% of rivers ({}/{}) have non-physical lowland headwaters",
        frac * 100.0,
        non_physical,
        world.hydrology.rivers.len()
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Riparian biomes — rivers create green corridors in arid regions
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn rivers_create_riparian_biome_in_arid_land() {
    // Where a river passes through DESERT or SHRUBLAND, the cells along
    // the river (and the immediate surroundings) should become RIPARIAN
    // — a green corridor analogous to the Nile through the Sahara or
    // the Colorado through the Mojave.
    use mapgen_world::biomes::*;
    let world = generate_full(params(42));
    let biome = &world.climate.biome;

    let river_cells: std::collections::BTreeSet<u32> = world
        .hydrology
        .rivers
        .iter()
        .flat_map(|r| r.cells.iter().copied())
        .collect();

    let mut river_cells_in_arid_surround = 0;
    let mut river_cells_riparian = 0;
    for &rc in &river_cells {
        let i = rc as usize;
        if world.terrain.elevation[i] <= 0.0 {
            continue;
        }
        // Look at the surrounding biome (excluding this river cell).
        let surround_arid = world.mesh.neighbors[i]
            .iter()
            .any(|&j| matches!(biome[j as usize], DESERT | SHRUBLAND | SAVANNA));
        if surround_arid {
            river_cells_in_arid_surround += 1;
            if biome[i] == RIPARIAN {
                river_cells_riparian += 1;
            }
        }
    }
    if river_cells_in_arid_surround < 10 {
        return; // not enough samples to assess
    }
    let frac = river_cells_riparian as f32 / river_cells_in_arid_surround as f32;
    assert!(
        frac > 0.5,
        "{:.1}% of river cells passing through arid regions are RIPARIAN ({}/{}) — expected >50%",
        frac * 100.0,
        river_cells_riparian,
        river_cells_in_arid_surround
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Width growth at confluences
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn river_segments_widen_as_flow_accumulates() {
    // The renderer should draw each cell of a river with a width
    // proportional to that cell's flow value. We assert that for each
    // river, the LAST cell's flow strictly exceeds the FIRST cell's
    // flow (water accumulates as we go downstream).
    let world = generate_full(params(42));
    let flow = &world.hydrology.flow;
    let mut shrinking = 0;
    let mut total = 0;
    for river in &world.hydrology.rivers {
        if river.cells.len() < 3 {
            continue;
        }
        total += 1;
        let head_flow = flow[river.cells[0] as usize];
        let mouth_flow = flow[*river.cells.last().unwrap() as usize];
        if mouth_flow <= head_flow {
            shrinking += 1;
        }
    }
    assert!(
        total > 0,
        "no rivers with ≥3 cells — rivers too short or no rivers at all"
    );
    let frac = shrinking as f32 / total as f32;
    assert!(
        frac < 0.05,
        "{:.1}% of rivers shrink from head to mouth ({}/{}) — flow accumulation broken",
        frac * 100.0,
        shrinking,
        total
    );
}

#[test]
fn longer_rivers_drain_more_water_than_short_ones() {
    // A 50-cell river should have mouth-flow far greater than a 5-cell
    // creek's mouth-flow. Rank correlation between length and mouth
    // flow should be strongly positive.
    let world = generate_full(params(42));
    let flow = &world.hydrology.flow;
    let mut samples: Vec<(usize, f32)> = world
        .hydrology
        .rivers
        .iter()
        .filter(|r| r.cells.len() >= 3)
        .map(|r| (r.cells.len(), flow[*r.cells.last().unwrap() as usize]))
        .collect();
    if samples.len() < 5 {
        return;
    }
    samples.sort_by_key(|&(l, _)| l);
    let short_third: f32 = samples
        .iter()
        .take(samples.len() / 3)
        .map(|(_, f)| f)
        .sum::<f32>()
        / (samples.len() / 3) as f32;
    let long_third: f32 = samples
        .iter()
        .rev()
        .take(samples.len() / 3)
        .map(|(_, f)| f)
        .sum::<f32>()
        / (samples.len() / 3) as f32;
    assert!(
        long_third > short_third * 1.5,
        "longest-third mouth-flow ({long_third:.2}) should be ≥1.5x shortest-third ({short_third:.2})"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Minimum length
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn no_river_shorter_than_minimum_length() {
    // Streams of 1-2 cells are noise; the extractor should drop them.
    let world = generate_full(params(42));
    for river in &world.hydrology.rivers {
        assert!(
            river.cells.len() >= 3,
            "river of length {} below minimum",
            river.cells.len()
        );
    }
}

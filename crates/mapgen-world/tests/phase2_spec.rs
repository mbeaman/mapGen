//! Phase 2 spec — the executable definition of "Phase 2 done."
//!
//! Every test in this file is the contract for one Phase 2 invariant.
//! Implementation lands when these are green. Tests start `#[ignore]`'d
//! so CI stays green during the phase; remove the `ignore` attribute one
//! at a time as features land.
//!
//! Phase 2 scope:
//!   * Hydraulic erosion (droplet) + thermal erosion
//!   * Priority-Flood depression filling
//!   * Flow direction + flow accumulation
//!   * River extraction (width ∝ √flow)
//!   * Lake formation (depressions become lakes at spill elevation)
//!   * Orographic precipitation (prevailing winds + rain shadow)
//!   * Latitude/lapse-rate temperature
//!   * Whittaker biome assignment
//!   * Coast detection (a cell is coast iff it touches both land and sea)

use mapgen_world::{generate, GenerateParams};

fn fixed_params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Erosion
// ──────────────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "Phase 2: erosion not implemented"]
fn erosion_conserves_mass_within_epsilon() {
    // Hydraulic erosion moves sediment but doesn't create or destroy it,
    // modulo the small fraction carried off the map at the coast. Total
    // elevation must be conserved within a small tolerance.
    let pre = generate(GenerateParams {
        cell_count: 2000,
        ..fixed_params(1)
    });
    let pre_sum: f64 = pre.terrain.elevation.iter().map(|&e| e as f64).sum();

    // TODO Phase 2: invoke erosion stage. For now we just assert the API
    // exists so this test compiles when erosion is added.
    let post = pre.clone();
    let post_sum: f64 = post.terrain.elevation.iter().map(|&e| e as f64).sum();

    let n = pre.mesh.cell_count() as f64;
    let tol = 1e-3 * n;
    assert!(
        (pre_sum - post_sum).abs() <= tol,
        "mass not conserved: pre={pre_sum:.3}, post={post_sum:.3}, tol={tol:.3}"
    );
}

#[test]
#[ignore = "Phase 2: erosion not implemented"]
fn erosion_sharpens_local_features() {
    // After erosion the heightmap should have *more* spatial variation in
    // the slope distribution (valleys and ridges become more distinct).
    // Crude proxy: mean absolute slope between neighbors strictly
    // increases.
    let pre = generate(fixed_params(7));
    let pre_slope = mean_abs_slope(&pre.terrain.elevation, &pre.mesh.neighbors);

    let post = pre.clone(); // TODO Phase 2: run erosion
    let post_slope = mean_abs_slope(&post.terrain.elevation, &post.mesh.neighbors);

    assert!(
        post_slope > pre_slope,
        "erosion should sharpen features, got pre={pre_slope:.4} post={post_slope:.4}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Hydrology
// ──────────────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "Phase 2: priority-flood depression fill not implemented"]
fn no_internal_depressions_after_priority_flood() {
    // After Priority-Flood, no land cell may have all of its neighbors
    // strictly higher than itself (that would be an unfilled pit).
    let world = generate(fixed_params(42));
    let elev = &world.terrain.elevation;
    for i in 0..world.mesh.cell_count() {
        if elev[i] <= 0.0 {
            continue;
        }
        let nbrs = &world.mesh.neighbors[i];
        if nbrs.is_empty() {
            continue;
        }
        let all_higher = nbrs.iter().all(|&j| elev[j as usize] > elev[i]);
        assert!(
            !all_higher,
            "cell {i} is an unfilled depression at elev={}",
            elev[i]
        );
    }
}

#[test]
#[ignore = "Phase 2: flow direction not implemented"]
fn flow_directions_strictly_descend() {
    // Every land cell's flow_to neighbor (when one exists) must have a
    // strictly lower elevation. Sea cells and lake outlets are exempt.
    let world = generate(fixed_params(42));
    let elev = &world.terrain.elevation;
    // TODO Phase 2: read flow_direction off WorldData.hydrology when added.
    let flow_direction: Vec<Option<u32>> = vec![None; world.mesh.cell_count()];

    for (i, &to) in flow_direction.iter().enumerate() {
        if let Some(j) = to {
            assert!(
                elev[j as usize] < elev[i],
                "flow at cell {i} (e={}) goes UP to cell {j} (e={})",
                elev[i],
                elev[j as usize]
            );
        }
    }
}

#[test]
#[ignore = "Phase 2: flow accumulation not implemented"]
fn flow_accumulation_is_monotonic_downstream() {
    // Downstream cells must have accumulation >= upstream cells along any
    // flow path. (Mass conservation of water under rain.)
    let world = generate(fixed_params(42));
    let flow: &[f32] = &world.hydrology.flow;
    let flow_direction: Vec<Option<u32>> = vec![None; world.mesh.cell_count()];

    for (i, &to) in flow_direction.iter().enumerate() {
        if let Some(j) = to {
            assert!(
                flow[j as usize] >= flow[i] - 1e-4,
                "flow accumulation non-monotonic: cell {i} flow={}, downstream cell {j} flow={}",
                flow[i],
                flow[j as usize]
            );
        }
    }
}

#[test]
#[ignore = "Phase 2: river extraction not implemented"]
fn rivers_reach_sea_or_lake() {
    // Every river's last cell must either be a coast (next cell is sea)
    // or a lake cell. No river may end mid-land.
    let world = generate(fixed_params(42));
    for river in &world.hydrology.rivers {
        let last = *river.cells.last().expect("empty river");
        let last_elev = world.terrain.elevation[last as usize];
        let in_lake = world
            .hydrology
            .lakes
            .iter()
            .any(|l| l.cells.contains(&last));
        let is_coast = world.mesh.coast[last as usize];
        assert!(
            last_elev <= 0.0 || is_coast || in_lake,
            "river ends mid-land at cell {last}, elev={last_elev}"
        );
    }
}

#[test]
#[ignore = "Phase 2: river extraction not implemented"]
fn river_width_scales_with_sqrt_flow() {
    // Per Amit Patel: river width should be proportional to sqrt(flow).
    // We check the relative ordering, not exact constants.
    let world = generate(fixed_params(42));
    let mut rivers: Vec<_> = world.hydrology.rivers.iter().collect();
    if rivers.len() < 2 {
        return; // nothing to compare
    }
    rivers.sort_by(|a, b| a.width.partial_cmp(&b.width).unwrap());
    let small_flow = max_flow_along(&rivers[0].cells, &world.hydrology.flow);
    let big_flow = max_flow_along(&rivers[rivers.len() - 1].cells, &world.hydrology.flow);
    assert!(
        big_flow >= small_flow,
        "wider river should drain at least as much water"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Coastline
// ──────────────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "Phase 2: coast detection not implemented"]
fn coast_cells_touch_both_land_and_sea() {
    // Definition: a cell is coast iff it is land AND has at least one sea
    // neighbor, OR it is sea AND has at least one land neighbor.
    let world = generate(fixed_params(42));
    let elev = &world.terrain.elevation;
    let coast = &world.mesh.coast;

    for i in 0..world.mesh.cell_count() {
        if !coast[i] {
            continue;
        }
        let self_is_land = elev[i] > 0.0;
        let has_opposite = world.mesh.neighbors[i]
            .iter()
            .any(|&j| (elev[j as usize] > 0.0) != self_is_land);
        assert!(
            has_opposite,
            "cell {i} marked coast but neighbors are all same side"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Climate
// ──────────────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "Phase 2: temperature model not implemented"]
fn temperature_decreases_with_elevation() {
    // Lapse rate: at the same latitude, higher cells should be cooler.
    // We bin cells by latitude band and check the trend within each band.
    let world = generate(fixed_params(42));
    let temp = &world.climate.temperature;
    let elev = &world.terrain.elevation;
    let h = world.mesh.height;

    let band_count = 8;
    for band in 0..band_count {
        let lo = h * (band as f32) / band_count as f32;
        let hi = h * (band as f32 + 1.0) / band_count as f32;
        let mut samples: Vec<(f32, f32)> = world
            .mesh
            .sites
            .iter()
            .enumerate()
            .filter(|(i, s)| s[1] >= lo && s[1] < hi && elev[*i] > 0.0)
            .map(|(i, _)| (elev[i], temp[i]))
            .collect();
        if samples.len() < 10 {
            continue;
        }
        samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let lowest_third = avg_temp(&samples[..samples.len() / 3]);
        let highest_third = avg_temp(&samples[samples.len() * 2 / 3..]);
        assert!(
            lowest_third > highest_third,
            "latitude band {band}: low-elev avg temp {lowest_third} not warmer than high-elev avg {highest_third}"
        );
    }
}

#[test]
#[ignore = "Phase 2: orographic precipitation not implemented"]
fn rain_shadow_dries_leeward_side() {
    // Walk along the prevailing wind direction across a mountain range.
    // Mean precipitation on the leeward side should be less than the
    // windward side at the same elevation band.
    let world = generate(fixed_params(42));
    // Simple proxy: split the map down the middle on the wind axis (assume
    // westerly for the test band) and compare averages on land cells of
    // similar elevation.
    let precip = &world.climate.precipitation;
    let elev = &world.terrain.elevation;
    let w = world.mesh.width;

    let (mut west, mut east) = (Vec::new(), Vec::new());
    for (i, s) in world.mesh.sites.iter().enumerate() {
        if elev[i] <= 0.0 {
            continue;
        }
        if s[0] < w * 0.4 {
            west.push(precip[i]);
        } else if s[0] > w * 0.6 {
            east.push(precip[i]);
        }
    }
    // For the seed under test, prevailing wind is configured westerly,
    // so east is leeward — strictly drier on average.
    let west_avg: f32 = west.iter().sum::<f32>() / west.len().max(1) as f32;
    let east_avg: f32 = east.iter().sum::<f32>() / east.len().max(1) as f32;
    assert!(
        west_avg > east_avg,
        "no rain-shadow effect: west avg precip {west_avg} <= east avg {east_avg}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Biomes
// ──────────────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "Phase 2: biome assignment not implemented"]
fn every_land_cell_has_a_biome() {
    let world = generate(fixed_params(42));
    assert_eq!(world.climate.biome.len(), world.mesh.cell_count());
    for i in 0..world.mesh.cell_count() {
        if world.terrain.elevation[i] > 0.0 {
            assert!(
                world.climate.biome[i] != u8::MAX,
                "cell {i} land but biome unassigned"
            );
        }
    }
}

#[test]
#[ignore = "Phase 2: biome assignment not implemented"]
fn snow_biome_only_where_cold() {
    // Whittaker invariant: snow/ice biomes only appear where mean
    // temperature is below freezing (≤ 0 in our normalized scale).
    const SNOW_BIOME: u8 = 0; // TODO Phase 2: pull from biomes::BiomeId
    let world = generate(fixed_params(42));
    for i in 0..world.mesh.cell_count() {
        if world.climate.biome[i] == SNOW_BIOME {
            assert!(
                world.climate.temperature[i] <= 0.0,
                "snow biome at warm cell {i}: temp={}",
                world.climate.temperature[i]
            );
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Cross-platform determinism (Tier A — pulled forward from Phase 5)
// ──────────────────────────────────────────────────────────────────────────────
//
// This test only proves the native side here; the wasm32 counterpart goes
// into crates/mapgen-wasm via wasm-bindgen-test and must produce the same
// hash. Done when both targets agree.

#[test]
#[ignore = "Phase 2: golden-hash anchor pending erosion + hydrology"]
fn golden_hash_native_matches_committed_value() {
    use std::io::Write;
    let world = generate(GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 4000,
        plate_count: 12,
        nation_count: 6,
    });
    let mut bytes = Vec::new();
    ciborium::into_writer(&world, &mut bytes).unwrap();
    let hash = blake3_hex(&bytes);
    // Replace with the committed hash once Phase 2 lands all the deterministic
    // stages and we anchor a real value into tests/golden/.
    let committed = include_str!("golden/seed42_phase2.blake3.txt").trim();
    let _ = std::io::stderr().write_all(format!("hash = {hash}\n").as_bytes());
    assert_eq!(
        hash, committed,
        "Phase 2 output drifted from committed golden hash"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// helpers
// ──────────────────────────────────────────────────────────────────────────────

fn mean_abs_slope(elev: &[f32], neighbors: &[Vec<u32>]) -> f32 {
    let mut sum = 0.0;
    let mut n = 0;
    for i in 0..elev.len() {
        for &j in &neighbors[i] {
            sum += (elev[i] - elev[j as usize]).abs();
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f32
    }
}

fn max_flow_along(cells: &[u32], flow: &[f32]) -> f32 {
    cells
        .iter()
        .map(|&c| flow[c as usize])
        .fold(0.0_f32, f32::max)
}

fn avg_temp(samples: &[(f32, f32)]) -> f32 {
    samples.iter().map(|(_, t)| *t).sum::<f32>() / samples.len().max(1) as f32
}

fn blake3_hex(_bytes: &[u8]) -> String {
    // Phase 2 wires blake3 in as a dev-dependency. Until then this returns a
    // sentinel that will never match the committed hash, keeping the test red.
    String::from("PENDING_PHASE_2")
}

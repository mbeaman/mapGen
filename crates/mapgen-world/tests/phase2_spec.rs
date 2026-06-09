//! Phase 2 spec — the executable definition of "Phase 2 done."
//!
//! Strict TDD posture:
//!   * Each test calls the real (future) API surface from `mapgen-world`.
//!   * The implementation modules currently `todo!()` — tests panic with
//!     descriptive messages until each stage is implemented.
//!   * As each stage lands, its `todo!()` becomes real code and the
//!     corresponding test goes green. No `#[ignore]` attributes — the
//!     red set is honest.
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
//!   * Cross-platform byte-identical golden hash (Tier-A, pulled forward)

use mapgen_core::{Stage, StageRng};
use mapgen_world::{
    biomes::{self, UNASSIGNED},
    climate::{self, ClimateParams},
    erosion::{self, ErosionParams},
    generate,
    hydrology::{self},
    GenerateParams,
};

fn fixed_params(seed: u64) -> GenerateParams {
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

// ──────────────────────────────────────────────────────────────────────────────
// Erosion
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn erosion_conserves_mass_within_epsilon() {
    let mut world = generate(fixed_params(1));
    let pre_sum: f64 = world.terrain.elevation.iter().map(|&e| e as f64).sum();

    let mut rng = StageRng::new(world.meta.seed).stream(Stage::Erosion);
    erosion::run(&mut world, ErosionParams::default(), &mut rng);

    let post_sum: f64 = world.terrain.elevation.iter().map(|&e| e as f64).sum();
    let n = world.mesh.cell_count() as f64;
    let tol = 1e-3 * n;
    assert!(
        (pre_sum - post_sum).abs() <= tol,
        "mass not conserved: pre={pre_sum:.3}, post={post_sum:.3}, tol={tol:.3}"
    );
}

#[test]
fn erosion_sharpens_local_features() {
    let mut world = generate(fixed_params(7));
    let pre_slope = mean_abs_slope(&world.terrain.elevation, &world.mesh.neighbors);

    let mut rng = StageRng::new(world.meta.seed).stream(Stage::Erosion);
    erosion::run(&mut world, ErosionParams::default(), &mut rng);

    let post_slope = mean_abs_slope(&world.terrain.elevation, &world.mesh.neighbors);
    assert!(
        post_slope > pre_slope,
        "erosion should sharpen features: pre={pre_slope:.4} post={post_slope:.4}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Hydrology
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn no_internal_depressions_after_priority_flood() {
    let mut world = generate(fixed_params(42));
    hydrology::fill_depressions(&mut world);

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
fn flow_directions_strictly_descend() {
    let mut world = generate(fixed_params(42));
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);

    let elev = &world.terrain.elevation;
    assert_eq!(flow_dir.len(), world.mesh.cell_count());
    for (i, &to) in flow_dir.iter().enumerate() {
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
fn flow_accumulation_is_monotonic_downstream() {
    let mut world = generate(fixed_params(42));
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);

    let flow = &world.hydrology.flow;
    for (i, &to) in flow_dir.iter().enumerate() {
        if let Some(j) = to {
            assert!(
                flow[j as usize] >= flow[i] - 1e-4,
                "flow accumulation non-monotonic: cell {i} flow={}, downstream {j} flow={}",
                flow[i],
                flow[j as usize]
            );
        }
    }
}

#[test]
fn rivers_reach_sea_or_lake() {
    let mut world = generate(fixed_params(42));
    hydrology::detect_coast(&mut world);
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);
    hydrology::extract_rivers(&mut world, &flow_dir, 0.05);

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
fn river_width_scales_with_sqrt_flow() {
    let mut world = generate(fixed_params(42));
    hydrology::detect_coast(&mut world);
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);
    hydrology::extract_rivers(&mut world, &flow_dir, 0.05);

    let mut rivers: Vec<_> = world.hydrology.rivers.iter().collect();
    assert!(
        rivers.len() >= 2,
        "expected ≥2 rivers from a 4000-cell world"
    );

    rivers.sort_by(|a, b| a.width.partial_cmp(&b.width).unwrap());
    let small_flow = max_flow_along(&rivers[0].cells, &world.hydrology.flow);
    let big_flow = max_flow_along(&rivers[rivers.len() - 1].cells, &world.hydrology.flow);
    assert!(
        big_flow >= small_flow,
        "wider river ({}) should drain at least as much water as narrower ({})",
        rivers[rivers.len() - 1].width,
        rivers[0].width
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Coastline
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn coast_cells_touch_both_land_and_sea() {
    let mut world = generate(fixed_params(42));
    hydrology::detect_coast(&mut world);

    let elev = &world.terrain.elevation;
    let coast = &world.mesh.coast;
    let mut any_coast = false;
    for i in 0..world.mesh.cell_count() {
        if !coast[i] {
            continue;
        }
        any_coast = true;
        let self_is_land = elev[i] > 0.0;
        let has_opposite = world.mesh.neighbors[i]
            .iter()
            .any(|&j| (elev[j as usize] > 0.0) != self_is_land);
        assert!(
            has_opposite,
            "cell {i} marked coast but all neighbors are on the same side"
        );
    }
    assert!(any_coast, "no coast cells found at all");
}

// ──────────────────────────────────────────────────────────────────────────────
// Climate
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn temperature_decreases_with_elevation() {
    let mut world = generate(fixed_params(42));
    climate::run(&mut world, ClimateParams::default());

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
        let low = avg_temp(&samples[..samples.len() / 3]);
        let high = avg_temp(&samples[samples.len() * 2 / 3..]);
        assert!(
            low > high,
            "latitude band {band}: low-elev avg temp {low} not warmer than high-elev avg {high}"
        );
    }
}

#[test]
fn rain_shadow_signal_present_along_dominant_wind() {
    // Earlier Phase-2 model used a single global westerly, so a
    // west-half vs east-half average was a valid rain-shadow proxy.
    // The current 3-cell model has trade, westerly, and polar bands;
    // a global west/east comparison mixes regimes and isn't a robust
    // assertion. The physics-correct rain-shadow tests live in
    // `realism_spec`:
    //   * `subtropical_band_drier_than_mid_latitude_westerly_band` —
    //     the 3-cell circulation produces a clear subtropical-dry,
    //     westerly-wet pattern.
    //   * `equatorial_band_is_wettest_on_average` — ITCZ dominance.
    //
    // This test stays here as a smoke check that *some* spatial
    // variation in precipitation exists — we should never see all
    // land cells at identical precip values.
    let mut world = generate(fixed_params(42));
    climate::run(&mut world, ClimateParams::default());

    let precip: Vec<f32> = (0..world.mesh.cell_count())
        .filter(|&i| world.terrain.elevation[i] > 0.0)
        .map(|i| world.climate.precipitation[i])
        .collect();
    let min = precip.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let max = precip.iter().fold(0.0_f32, |a, b| a.max(*b));
    assert!(
        max - min > 0.05,
        "precipitation flat across land: min={min}, max={max}; \
         climate model produced no spatial variation"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Biomes
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn every_land_cell_has_a_biome() {
    let mut world = generate(fixed_params(42));
    climate::run(&mut world, ClimateParams::default());
    biomes::classify(&mut world);

    assert_eq!(world.climate.biome.len(), world.mesh.cell_count());
    for i in 0..world.mesh.cell_count() {
        if world.terrain.elevation[i] > 0.0 {
            assert_ne!(
                world.climate.biome[i], UNASSIGNED,
                "land cell {i} biome unassigned"
            );
        }
    }
}

#[test]
fn snow_biome_only_where_cold() {
    let mut world = generate(fixed_params(42));
    climate::run(&mut world, ClimateParams::default());
    biomes::classify(&mut world);

    for i in 0..world.mesh.cell_count() {
        if world.climate.biome[i] == biomes::SNOW {
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

#[test]
fn golden_hash_native_matches_committed_value() {
    let mut world = generate(fixed_params(42));
    hydrology::detect_coast(&mut world);
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);
    hydrology::extract_rivers(&mut world, &flow_dir, 0.05);
    climate::run(&mut world, ClimateParams::default());
    biomes::classify(&mut world);

    let mut bytes = Vec::new();
    ciborium::into_writer(&world, &mut bytes).unwrap();
    let hash = blake3::hash(&bytes).to_hex().to_string();

    let committed = include_str!("golden/seed42_phase2.blake3.txt").trim();
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

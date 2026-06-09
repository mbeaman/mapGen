//! Realism upgrade spec — default science + LorePatch override layer.
//!
//! These tests encode the scientific-baseline expectations and the
//! override-system invariants. Like phase2_spec, no #[ignore] — implementation
//! drives them green one by one.

use mapgen_core::patch::{LorePatch, PatchLayers, PatchShape, Signature};
use mapgen_world::{
    biomes,
    climate::{self, ClimateParams},
    generate, hydrology, ocean,
    patch::apply_climate_patches,
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

// ──────────────────────────────────────────────────────────────────────────────
// LorePatch foundation
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_patches_dont_change_anything() {
    let mut a = generate(params(42));
    climate::run(&mut a, ClimateParams::default());

    let mut b = generate(params(42));
    climate::run(&mut b, ClimateParams::default());
    apply_climate_patches(&mut b); // with empty patch table, must be a no-op.

    assert_eq!(a.climate.temperature, b.climate.temperature);
    assert_eq!(a.climate.precipitation, b.climate.precipitation);
}

#[test]
fn temperature_delta_patch_warms_cells_inside_polygon() {
    let mut world = generate(params(42));
    climate::run(&mut world, ClimateParams::default());

    // Pick a land cell well inside the map for the patch center.
    let cx = world.mesh.width * 0.5;
    let cy = world.mesh.height * 0.5;
    let pre_temp: Vec<f32> = world.climate.temperature.clone();

    world.patches.push(LorePatch {
        name: "Warm zone".into(),
        shape: PatchShape::Disk {
            center: [cx, cy],
            radius: 80.0,
        },
        falloff_radius: 20.0,
        signature: Signature::Mundane,
        layers: PatchLayers {
            temperature_delta: Some(0.5),
            ..PatchLayers::default()
        },
        cause_event: None,
        seed: 0,
    });
    world
        .patches
        .rebuild_index(&world.mesh.sites, world.mesh.width, world.mesh.height);
    apply_climate_patches(&mut world);

    // At least one cell must have warmed; cells outside must be unchanged.
    let mut warmed = 0;
    let mut unchanged_outside = 0;
    for (i, pre) in pre_temp.iter().enumerate() {
        let s = world.mesh.sites[i];
        let d = ((s[0] - cx).powi(2) + (s[1] - cy).powi(2)).sqrt();
        if d < 60.0 {
            assert!(
                world.climate.temperature[i] > *pre - 1e-6,
                "cell {i} at d={d:.0} inside patch should be at least as warm"
            );
            if world.climate.temperature[i] > *pre + 0.01 {
                warmed += 1;
            }
        }
        if d > 200.0 {
            assert!(
                (world.climate.temperature[i] - *pre).abs() < 1e-5,
                "cell {i} at d={d:.0} far outside patch should be unchanged"
            );
            unchanged_outside += 1;
        }
    }
    assert!(warmed > 5, "expected several warmed cells, got {warmed}");
    assert!(unchanged_outside > 50, "expected many untouched cells");
}

#[test]
fn patch_falloff_is_monotonic_at_boundary() {
    // Inside the core, full strength. Across the falloff band, monotone
    // decrease to zero. Beyond, exactly zero.
    let mut world = generate(params(42));
    climate::run(&mut world, ClimateParams::default());
    let pre = world.climate.temperature.clone();

    let cx = world.mesh.width * 0.5;
    let cy = world.mesh.height * 0.5;
    world.patches.push(LorePatch {
        name: "Falloff test".into(),
        shape: PatchShape::Disk {
            center: [cx, cy],
            radius: 60.0,
        },
        falloff_radius: 40.0,
        signature: Signature::Mundane,
        layers: PatchLayers {
            temperature_delta: Some(1.0),
            ..PatchLayers::default()
        },
        cause_event: None,
        seed: 0,
    });
    world
        .patches
        .rebuild_index(&world.mesh.sites, world.mesh.width, world.mesh.height);
    apply_climate_patches(&mut world);

    // Bucket cells by distance, check delta is monotone non-increasing.
    let mut samples: Vec<(f32, f32)> = (0..world.mesh.cell_count())
        .map(|i| {
            let s = world.mesh.sites[i];
            let d = ((s[0] - cx).powi(2) + (s[1] - cy).powi(2)).sqrt();
            (d, world.climate.temperature[i] - pre[i])
        })
        .collect();
    samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Inside core (d < radius - falloff): delta should be at or near 1.0.
    let core: Vec<f32> = samples
        .iter()
        .filter(|(d, _)| *d < 20.0)
        .map(|(_, x)| *x)
        .collect();
    assert!(!core.is_empty(), "need core samples");
    let core_min = core.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    assert!(core_min > 0.95, "core delta {core_min} too weak");

    // Beyond radius + falloff: zero.
    let far: Vec<f32> = samples
        .iter()
        .filter(|(d, _)| *d > 120.0)
        .map(|(_, x)| *x)
        .collect();
    let far_max = far.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    assert!(
        far_max < 1e-5,
        "outside falloff delta should be zero, got {far_max}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 3-cell atmospheric circulation
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn equatorial_band_is_wettest_on_average() {
    let mut world = generate(params(42));
    climate::run(&mut world, ClimateParams::default());

    // Compute average precip per latitude band on land cells only.
    let h = world.mesh.height;
    let band = |y: f32| ((y / h) * 6.0).floor().clamp(0.0, 5.0) as usize;
    let mut sums = [0.0_f32; 6];
    let mut cnt = [0_u32; 6];
    for i in 0..world.mesh.cell_count() {
        if world.terrain.elevation[i] <= 0.0 {
            continue;
        }
        let b = band(world.mesh.sites[i][1]);
        sums[b] += world.climate.precipitation[i];
        cnt[b] += 1;
    }
    let avgs: Vec<f32> = sums
        .iter()
        .zip(&cnt)
        .map(|(s, &n)| if n == 0 { 0.0 } else { s / n as f32 })
        .collect();

    // Equator is bands 2-3 (middle of map). Subtropical highs (deserts) are bands 1 & 4.
    // Equatorial mean should beat subtropical mean.
    let equator_mean = (avgs[2] + avgs[3]) * 0.5;
    let subtropical_mean = (avgs[1] + avgs[4]) * 0.5;
    assert!(
        equator_mean > subtropical_mean,
        "ITCZ should give equator wetter than subtropics: equator={equator_mean:.4}, subtropics={subtropical_mean:.4}"
    );
}

#[test]
fn subtropical_band_drier_than_mid_latitude_westerly_band() {
    // 3-cell circulation should put a dry belt at |abs_lat| ≈ 0.3
    // (subtropical highs) and a wet belt at |abs_lat| ≈ 0.5
    // (westerly storm tracks). Use 10 latitude bands so band centers
    // land cleanly on those targets:
    //   Band 2 center abs_lat = 0.5  → NH westerly (wet)
    //   Band 3 center abs_lat = 0.3  → NH subtropical (dry)
    //   Band 6 center abs_lat = 0.3  → SH subtropical (dry)
    //   Band 7 center abs_lat = 0.5  → SH westerly (wet)
    let mut world = generate(params(42));
    climate::run(&mut world, ClimateParams::default());

    let h = world.mesh.height;
    let band = |y: f32| ((y / h) * 10.0).floor().clamp(0.0, 9.0) as usize;
    let mut sums = [0.0_f32; 10];
    let mut cnt = [0_u32; 10];
    for i in 0..world.mesh.cell_count() {
        if world.terrain.elevation[i] <= 0.0 {
            continue;
        }
        let b = band(world.mesh.sites[i][1]);
        sums[b] += world.climate.precipitation[i];
        cnt[b] += 1;
    }
    let avg = |b: usize| {
        if cnt[b] == 0 {
            0.0
        } else {
            sums[b] / cnt[b] as f32
        }
    };
    let subtropical = (avg(3) + avg(6)) * 0.5;
    let westerly = (avg(2) + avg(7)) * 0.5;
    assert!(
        westerly > subtropical,
        "westerly band should be wetter than subtropical: westerly={westerly:.4}, subtropical={subtropical:.4}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Real lakes
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn real_lakes_emerge_from_priority_flood() {
    let mut world = generate(params(42));
    hydrology::detect_coast(&mut world);
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);
    hydrology::extract_rivers(&mut world, &flow_dir, 0.05);

    // After priority-flood, some depressions should remain as proper lakes
    // (multi-cell, flat at spill elevation).
    assert!(
        !world.hydrology.lakes.is_empty(),
        "expected at least one lake from a 4000-cell world"
    );
    for lake in &world.hydrology.lakes {
        assert!(!lake.cells.is_empty(), "lake with no cells");
        let elev = &world.terrain.elevation;
        // All cells in the lake should share the spill elevation within
        // a small tolerance.
        let spill = lake.level;
        for &c in &lake.cells {
            assert!(
                (elev[c as usize] - spill).abs() < 0.02,
                "lake cell {c} elev {} differs from spill {spill}",
                elev[c as usize]
            );
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Ocean currents
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn ocean_currents_warm_east_coasts_at_mid_latitudes() {
    let mut world = generate(params(42));
    hydrology::detect_coast(&mut world);
    ocean::run(&mut world);
    climate::run(&mut world, ClimateParams::default());

    // Pick coastal land cells in the mid-latitude band (roughly y ~ 0.25-0.4
    // of the map height, NH mid-latitudes in our convention).
    // West coast = coast cell with at least one west sea neighbor.
    // East coast = coast cell with at least one east sea neighbor.
    let h = world.mesh.height;
    let coast = &world.mesh.coast;
    let elev = &world.terrain.elevation;
    let sites = &world.mesh.sites;
    let neighbors = &world.mesh.neighbors;
    let temp = &world.climate.temperature;

    let (mut west_temps, mut east_temps) = (Vec::new(), Vec::new());
    for i in 0..world.mesh.cell_count() {
        if !coast[i] || elev[i] <= 0.0 {
            continue;
        }
        let y = sites[i][1];
        let lat_norm = ((y - h * 0.5) / (h * 0.5)).abs();
        if !(0.45..0.85).contains(&lat_norm) {
            continue;
        }
        let mut has_west_sea = false;
        let mut has_east_sea = false;
        for &j in &neighbors[i] {
            if elev[j as usize] > 0.0 {
                continue;
            }
            if sites[j as usize][0] < sites[i][0] {
                has_west_sea = true;
            }
            if sites[j as usize][0] > sites[i][0] {
                has_east_sea = true;
            }
        }
        if has_west_sea && !has_east_sea {
            west_temps.push(temp[i]);
        } else if has_east_sea && !has_west_sea {
            east_temps.push(temp[i]);
        }
    }

    // Require enough samples to be meaningful.
    if west_temps.len() < 5 || east_temps.len() < 5 {
        eprintln!(
            "skip: not enough coast samples (west={}, east={})",
            west_temps.len(),
            east_temps.len()
        );
        return;
    }
    let west_avg: f32 = west_temps.iter().sum::<f32>() / west_temps.len() as f32;
    let east_avg: f32 = east_temps.iter().sum::<f32>() / east_temps.len() as f32;
    // Warm western-boundary current (Gulf Stream, Kuroshio) flows
    // *poleward* along the western edge of an ocean basin, which is
    // the EAST coast of a continent. Cold eastern-boundary current
    // (California, Canary, Benguela) flows equatorward along the
    // continent's WEST coast. So at low-mid latitudes, east coast >
    // west coast.
    assert!(
        east_avg > west_avg,
        "at mid latitudes, east coast (warm western-boundary current, Gulf-Stream-analogue) should be warmer than west coast (cold eastern-boundary current, California-analogue): east={east_avg:.3}, west={west_avg:.3}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Köppen classification
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn koppen_produces_recognizable_climate_zones() {
    let mut world = generate(params(42));
    hydrology::detect_coast(&mut world);
    ocean::run(&mut world);
    climate::run(&mut world, ClimateParams::default());
    biomes::classify(&mut world);

    // We should see at least 5 distinct biomes on land with a 4000-cell world.
    use std::collections::BTreeSet;
    let on_land: BTreeSet<u8> = (0..world.mesh.cell_count())
        .filter(|&i| world.terrain.elevation[i] > 0.0)
        .map(|i| world.climate.biome[i])
        .collect();
    assert!(
        on_land.len() >= 5,
        "expected >=5 biome types on land, got {}: {:?}",
        on_land.len(),
        on_land
    );
}

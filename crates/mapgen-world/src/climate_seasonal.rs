//! Two-pass seasonal climate.
//!
//! Köppen-Geiger classification needs temperature and precipitation
//! per season. We compute the climate model twice with the ITCZ shifted
//! by the axial tilt — perihelion (NH summer / SH winter) and aphelion
//! (NH winter / SH summer) — and store both results.
//!
//! The cheap implementation: run the existing 3-cell + wind model with
//! an `itcz_shift` parameter that offsets the effective latitude in the
//! per-cell wind/band lookup. The "annual" `temperature` and
//! `precipitation` arrays are filled with the mean of the two seasons
//! for backwards compatibility with consumers that don't read seasonal.

use mapgen_core::{fmath, WorldData};

use crate::climate::ClimateParams;

/// Compute climate for both summer (perihelion, NH June) and winter
/// (aphelion, NH December). The annual `temperature` and `precipitation`
/// vectors are the mean of the two seasons.
pub fn run(world: &mut WorldData, params: ClimateParams) {
    let n = world.mesh.cell_count();

    // ITCZ shift in lat_norm units (-1 to +1 spans pole-to-pole). Earth's
    // axial tilt of 23.5° corresponds to roughly 0.26 of a hemisphere
    // — the ITCZ moves about that far north in June and south in December.
    let shift = fmath::sin(params.axial_tilt) * 0.5;

    // NH-summer pass: ITCZ shifted toward NH (negative y in our convention).
    let (temp_summer, precip_summer) = one_pass(world, &params, -shift, true);
    // NH-winter pass: ITCZ shifted toward SH.
    let (temp_winter, precip_winter) = one_pass(world, &params, shift, false);

    // Annual mean for legacy consumers.
    let mut temperature = Vec::with_capacity(n);
    let mut precipitation = Vec::with_capacity(n);
    for i in 0..n {
        temperature.push((temp_summer[i] + temp_winter[i]) * 0.5);
        precipitation.push((precip_summer[i] + precip_winter[i]) * 0.5);
    }

    world.climate.temperature = temperature;
    world.climate.precipitation = precipitation;
    world.climate.temperature_summer = temp_summer;
    world.climate.temperature_winter = temp_winter;
    world.climate.precipitation_summer = precip_summer;
    world.climate.precipitation_winter = precip_winter;
    world.climate.biome = vec![0u8; n];

    crate::patch::apply_climate_patches(world);

    // River seasonal regime now that seasonal precipitation exists (6.1.5).
    crate::hydrology::classify_river_regimes(world);
}

fn one_pass(
    world: &WorldData,
    params: &ClimateParams,
    itcz_shift: f32,
    is_nh_summer: bool,
) -> (Vec<f32>, Vec<f32>) {
    let n = world.mesh.cell_count();
    let sites = &world.mesh.sites;
    let neighbors = &world.mesh.neighbors;
    let elev = &world.terrain.elevation;
    let h = world.mesh.height.max(1.0);
    let half_h = h * 0.5;

    let coastal = if world.climate.coastal_temp_anomaly.len() == n {
        Some(&world.climate.coastal_temp_anomaly)
    } else {
        None
    };

    // Effective latitude: actual lat_norm shifted toward the heated
    // hemisphere. NH summer = effective lat shifted south for solar
    // exposure (so NH heats up more, SH less).
    let effective_lat = |y: f32| -> f32 {
        let lat_norm = (y - half_h) / half_h;
        // Apply a hemispheric temperature bias proportional to the shift:
        // NH summer warms NH (lat_norm < 0), cools SH.
        (lat_norm - itcz_shift).clamp(-1.0, 1.0)
    };

    // Temperature: base on effective_lat (so summer-side hemisphere is
    // warmer), lapse on elevation, plus coastal anomaly.
    //
    // The seasonal bonus must key off the cell's *real* hemisphere, not
    // its effective latitude — the sun's apparent path north/south
    // doesn't change the cell's actual hemisphere. Otherwise equatorial
    // cells (real lat ≈ 0) get cooled in NH summer because the ITCZ
    // shift pushes their effective lat across the equator boundary.
    let mut temperature = Vec::with_capacity(n);
    for i in 0..n {
        let lat_norm = (sites[i][1] - half_h) / half_h; // <0 = NH, >0 = SH
        let eff = (lat_norm - itcz_shift).clamp(-1.0, 1.0);
        let lat_factor = fmath::cos(eff.abs() * std::f32::consts::FRAC_PI_2);
        let seasonal_bonus = match (is_nh_summer, lat_norm < 0.0) {
            (true, true) => 0.25,   // NH summer, NH cell: warmer
            (true, false) => -0.25, // NH summer, SH cell: cooler
            (false, true) => -0.25, // NH winter, NH cell: cooler
            (false, false) => 0.25, // NH winter, SH cell: warmer
        };
        let base = params.polar_temp
            + (params.equator_temp - params.polar_temp) * lat_factor
            + seasonal_bonus;
        let lapse = params.lapse_rate * elev[i].max(0.0);
        let anomaly = coastal.map(|c| c[i]).unwrap_or(0.0);
        temperature.push(base - lapse + anomaly);
    }

    // Precipitation: identical pipeline to `climate::run` but with ITCZ
    // shifted. We inline the wind-marching loop with a shifted band
    // lookup.
    let mut moisture = vec![0.0_f32; n];
    let mut precipitation = vec![0.0_f32; n];
    let mut order: Vec<u32> = (0..n as u32).collect();

    let wind_for = |y: f32| -> [f32; 2] {
        let lat_norm = (y - half_h) / half_h;
        crate::climate::wind_vector(lat_norm)
    };

    order.sort_by(|&a, &b| {
        let a = a as usize;
        let b = b as usize;
        let wa = wind_for(sites[a][1]);
        let wb = wind_for(sites[b][1]);
        let pa = sites[a][0] * wa[0] + sites[a][1] * wa[1];
        let pb = sites[b][0] * wb[0] + sites[b][1] * wb[1];
        pa.total_cmp(&pb)
    });

    for &c in &order {
        let c = c as usize;
        let cy = sites[c][1];
        let cx = sites[c][0];
        let w = wind_for(cy);

        let upwind = neighbors[c].iter().copied().max_by(|&a, &b| {
            let da = (cx - sites[a as usize][0]) * w[0] + (cy - sites[a as usize][1]) * w[1];
            let db = (cx - sites[b as usize][0]) * w[0] + (cy - sites[b as usize][1]) * w[1];
            da.total_cmp(&db)
        });
        let upwind = upwind.filter(|&u| {
            (cx - sites[u as usize][0]) * w[0] + (cy - sites[u as usize][1]) * w[1] > 0.0
        });

        let (uw_m, uw_e) = match upwind {
            Some(u) => (moisture[u as usize], elev[u as usize]),
            None => (1.0, 0.0),
        };

        // Band base on the *seasonal* effective latitude — ITCZ shifts
        // with axial tilt; subtropical highs shift with it; this is
        // what creates monsoon-like wet/dry seasons.
        let eff_abs = effective_lat(cy).abs();
        let band_base = crate::climate::band_precip(eff_abs);

        if elev[c] <= 0.0 {
            moisture[c] = uw_m + (1.0 - uw_m) * 0.20;
            precipitation[c] = params.base_precip * band_base;
            continue;
        }
        // Precip release — kept in lockstep with `climate::run` (the
        // non-seasonal path the realism specs exercise). Strong orographic
        // term + low post-depletion floor so rain shadows and subtropics reach
        // true aridity (6.1.1). NOTE: this formula is duplicated across the two
        // climate modules — a shared helper is a worthwhile follow-up refactor.
        let uplift = (elev[c] - uw_e).max(0.0);
        let release = (0.20 + uplift * 6.0).min(0.95);
        let rain = uw_m * release;
        moisture[c] = (uw_m - rain).max(0.0);
        precipitation[c] = params.base_precip * band_base * (0.085 + rain);
    }

    (temperature, precipitation)
}

//! 3-cell atmospheric circulation + lapse-rate temperature. Phase 2.
//!
//! Temperature: cosine latitude curve minus a lapse-rate term using cell
//! elevation. If `world.climate.coastal_temp_anomaly` was filled by an
//! earlier `ocean::run` pass, it adds to the temperature (warming west
//! coasts at mid latitudes, cooling east coasts and upwelling zones).
//!
//! Precipitation: cell elevation drives orographic uplift along a
//! prevailing wind vector that varies by latitude band:
//!
//! * 0–30° lat (subtropical → equator): trade winds, easterlies.
//! * 30–60° lat: westerlies (the band that drove our Phase-2 model).
//! * 60–90° lat: polar easterlies.
//!
//! Bands mirror between NH and SH. ITCZ at the equator gets a strong
//! base precipitation; subtropical highs (~30°) get a desert-baseline.
//!
//! Marching is generalized: for each cell we identify its upwind
//! neighbor as the neighbor most aligned with `-wind` direction. Cells
//! are processed in topological order along the wind direction (sort by
//! `wind · site`) so the upwind neighbor is always already resolved.
//!
//! `apply_climate_patches` runs at the end of every call, so any
//! lore-driven overrides land on top of the physical baseline.

use mapgen_core::{fmath, WorldData};

#[derive(Clone, Debug)]
pub struct ClimateParams {
    pub axial_tilt: f32,
    pub lapse_rate: f32,
    pub equator_temp: f32,
    pub polar_temp: f32,
    /// Base precipitation magnitude on saturated open ocean.
    pub base_precip: f32,
}

impl Default for ClimateParams {
    fn default() -> Self {
        Self {
            axial_tilt: 23.5_f32.to_radians(),
            // 0.5 was too aggressive — even mid-latitude land cells
            // crossed the freezing/tropical boundary too easily, and
            // moderately elevated tropical cells (Kenyan highlands at
            // 1500m equivalent) dropped out of A-class. Real wet
            // adiabatic lapse is 6.5°C/km; this corresponds to ~0.45 on
            // our normalized scale at typical elevations.
            lapse_rate: 0.45,
            equator_temp: 1.0,
            polar_temp: -0.4,
            // 0.5 produced p_annual ~0.10 on typical land, which forced
            // most cells through the aridity branch. 0.7 lifts the
            // overall distribution so cells more often have enough rain
            // to escape steppe classification.
            base_precip: 0.7,
        }
    }
}

pub fn run(world: &mut WorldData, params: ClimateParams) {
    let n = world.mesh.cell_count();
    let sites = &world.mesh.sites;
    let neighbors = &world.mesh.neighbors;
    let elev = &world.terrain.elevation;
    let h = world.mesh.height.max(1.0);
    let half_h = h * 0.5;

    // --- Temperature: lat band + lapse rate + ocean anomaly ----------------
    let coastal = if world.climate.coastal_temp_anomaly.len() == n {
        Some(&world.climate.coastal_temp_anomaly)
    } else {
        None
    };
    let mut temperature = Vec::with_capacity(n);
    for i in 0..n {
        let lat_norm = (sites[i][1] - half_h) / half_h; // -1 north pole, +1 south pole
        let lat_factor = fmath::cos(lat_norm.abs() * std::f32::consts::FRAC_PI_2);
        let base = params.polar_temp + (params.equator_temp - params.polar_temp) * lat_factor;
        let lapse = params.lapse_rate * elev[i].max(0.0);
        let anomaly = coastal.map(|c| c[i]).unwrap_or(0.0);
        temperature.push(base - lapse + anomaly);
    }

    // --- Precipitation: 3-cell winds + orographic uplift -------------------
    //
    // Wind vector per cell from latitude band. Then sort cells by their
    // position along the wind direction so the upwind neighbor is always
    // already processed.

    let mut moisture = vec![0.0_f32; n];
    let mut precipitation = vec![0.0_f32; n];
    let mut order: Vec<u32> = (0..n as u32).collect();

    // Per-cell wind direction lookup.
    let wind_for = |y: f32| -> [f32; 2] {
        // lat_norm: -1 N pole, 0 equator, +1 S pole.
        let lat_norm = (y - half_h) / half_h;
        wind_vector(lat_norm)
    };

    // Sort by a projection that mostly follows the wind direction. We
    // use a small bias on y (band index) plus the dot of cell position
    // with a representative-band wind: not perfect for cross-band cells
    // but good enough for "upwind already processed."
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

        // Upwind = neighbor whose displacement is most opposite to wind
        // direction. Equivalent to maximizing dot(self - nbr, w).
        let upwind = neighbors[c].iter().copied().max_by(|&a, &b| {
            let da = (cx - sites[a as usize][0]) * w[0] + (cy - sites[a as usize][1]) * w[1];
            let db = (cx - sites[b as usize][0]) * w[0] + (cy - sites[b as usize][1]) * w[1];
            da.total_cmp(&db)
        });
        // Only accept if the dot is positive (genuinely upwind).
        let upwind = upwind.filter(|&u| {
            (cx - sites[u as usize][0]) * w[0] + (cy - sites[u as usize][1]) * w[1] > 0.0
        });

        let (uw_m, uw_e) = match upwind {
            Some(u) => (moisture[u as usize], elev[u as usize]),
            None => (1.0, 0.0), // upwind boundary = saturated ocean
        };

        // Latitude band base: ITCZ wet, subtropical dry, mid-lat wet,
        // polar dry. Smooth function over |lat_norm|.
        let band_base = band_precip(((cy - half_h) / half_h).abs());

        if elev[c] <= 0.0 {
            // Sea: gradually approach saturation via evaporation.
            // 0.20 per cell ≈ Earth's typical fetch behavior; saturates
            // fully in ~10 cells of open ocean. A 5-cell inland sea
            // recovers ~67% but doesn't fully reset a depleted air mass.
            moisture[c] = uw_m + (1.0 - uw_m) * 0.20;
            precipitation[c] = params.base_precip * band_base;
            continue;
        }
        // Land: release rain proportional to upwind moisture, uplift, and band
        // base. A modest flat-land baseline (air loses water to friction /
        // convection) plus a *strong* orographic term, so windward slopes wring
        // out hard and leeward / deep-interior air arrives depleted. The lowered
        // post-depletion floor (0.085, was 0.10) is what lets rain-shadow and
        // subtropical cells reach true aridity — the old 0.10 floor kept
        // everywhere wet enough that deserts almost never formed (6.1.1). Tuned
        // against seeds 1/7/42/99 to ~20-30% desert + ~27% forest (was 1% / 46%);
        // kept in lockstep with `climate_seasonal::one_pass`.
        let uplift = (elev[c] - uw_e).max(0.0);
        let release = (0.20 + uplift * 6.0).min(0.95);
        let rain = uw_m * release;
        moisture[c] = (uw_m - rain).max(0.0);
        precipitation[c] = params.base_precip * band_base * (0.085 + rain);
    }

    world.climate.temperature = temperature;
    world.climate.precipitation = precipitation;
    // Biome buffer pre-allocated for classify() to overwrite.
    world.climate.biome = vec![0u8; n];

    // Lore overlay last.
    crate::patch::apply_climate_patches(world);

    // River seasonal regime now that precipitation exists (6.1.5).
    crate::hydrology::classify_river_regimes(world);
}

/// Latitude-band relative precipitation. Input is |lat_norm| in [0, 1]
/// where 0 = equator, 1 = pole.
///
/// Pattern:
///   0.0  : ITCZ peak  → 1.4
///   0.33 : subtropical desert trough → 0.4
///   0.55 : mid-lat westerly storm track → 1.1
///   1.0  : polar dry → 0.3
pub(crate) fn band_precip(abs_lat: f32) -> f32 {
    // Two cosine bumps + linear pole falloff.
    let itcz = fmath::exp(-((abs_lat - 0.0) * 4.5).powi(2)) * 0.9;
    let storm = fmath::exp(-((abs_lat - 0.55) * 4.5).powi(2)) * 0.7;
    let baseline = 0.4 * (1.0 - abs_lat); // soft falloff toward pole
    0.3 + itcz + storm + baseline
}

/// Wind vector for a cell at signed latitude `lat_norm` in [-1, 1]
/// (negative = NH, positive = SH per our y convention).
///
/// Convention: wind vector points in the direction air is moving.
/// Returned vector is in world coords where +x = east, +y = south.
pub(crate) fn wind_vector(lat_norm: f32) -> [f32; 2] {
    let abs_lat = lat_norm.abs();
    // Convert |abs_lat| to a "band index" in [0..3]:
    //   0.0–0.33: trades (easterlies blowing toward equator).
    //   0.33–0.67: westerlies blowing toward pole.
    //   0.67–1.0: polar easterlies blowing toward equator.
    //
    // Wind vector direction (east=+x, equator-direction depends on hem):
    //   * Trade: -x (east-to-west) and toward equator (sign depends on hem).
    //   * Westerly: +x (west-to-east) and toward pole.
    //   * Polar easterly: -x and toward equator.
    //
    // We normalize to magnitude 1.

    // Determine which hemisphere we're in: lat_norm < 0 = NH (north
    // pole near y=0), so "toward equator" = +y, "toward pole" = -y.
    let hem_to_equator = if lat_norm < 0.0 { 1.0 } else { -1.0 };
    let hem_to_pole = -hem_to_equator;

    let (vx, vy) = if abs_lat < 0.333 {
        // Trade winds: easterly + toward equator.
        (-1.0_f32, hem_to_equator * 0.5)
    } else if abs_lat < 0.667 {
        // Westerlies: westerly + toward pole.
        (1.0_f32, hem_to_pole * 0.5)
    } else {
        // Polar easterlies.
        (-1.0_f32, hem_to_equator * 0.3)
    };
    let mag = fmath::sqrt(vx * vx + vy * vy);
    [vx / mag, vy / mag]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_band_directions() {
        // NH trade band (lat_norm = -0.2): wind should be westward (-x) and
        // toward equator (+y in our convention).
        let v = wind_vector(-0.2);
        assert!(v[0] < 0.0, "NH trade x-component should be negative");
        assert!(
            v[1] > 0.0,
            "NH trade y-component should be positive (toward equator)"
        );

        // NH westerly band (lat_norm = -0.5): wind should be eastward
        // (+x) and toward pole (-y).
        let v = wind_vector(-0.5);
        assert!(v[0] > 0.0, "NH westerly x-component should be positive");
        assert!(
            v[1] < 0.0,
            "NH westerly y-component should be negative (toward pole)"
        );

        // SH westerly band (lat_norm = +0.5): wind eastward + toward pole (+y).
        let v = wind_vector(0.5);
        assert!(v[0] > 0.0);
        assert!(v[1] > 0.0);
    }

    #[test]
    fn band_precip_shape() {
        let eq = band_precip(0.0);
        let subtrop = band_precip(0.33);
        let westerly = band_precip(0.55);
        let polar = band_precip(1.0);
        assert!(
            eq > subtrop,
            "equator wetter than subtropical: {eq} vs {subtrop}"
        );
        assert!(
            westerly > subtrop,
            "westerly wetter than subtropical: {westerly} vs {subtrop}"
        );
        assert!(
            westerly > polar,
            "westerly wetter than polar: {westerly} vs {polar}"
        );
    }
}

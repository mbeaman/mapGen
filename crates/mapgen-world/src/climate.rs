//! Climate model: temperature + precipitation per cell. Phase 2.
//!
//! Temperature: cosine latitude curve modulated by axial tilt, minus a
//! 6.5 °C/km lapse-rate term using cell elevation.
//!
//! Precipitation: hard-code prevailing wind direction per latitude band
//! (approximation of Hadley/Ferrel/Polar cells). March cells upwind,
//! picking up moisture over ocean, dropping it on orographic uplift,
//! carrying rain shadow forward. ~50-line loop, milliseconds at 15k cells.
//!
//! Writes to `world.climate.temperature` and `world.climate.precipitation`.

use mapgen_core::{fmath, WorldData};

#[derive(Clone, Debug)]
pub struct ClimateParams {
    /// Global axial tilt (radians). Earth ≈ 23.5°.
    pub axial_tilt: f32,
    /// Lapse rate per unit normalized elevation. Tunable.
    pub lapse_rate: f32,
    /// Reference equatorial sea-level temperature (normalized scale).
    pub equator_temp: f32,
    /// Reference polar sea-level temperature.
    pub polar_temp: f32,
    /// Per-cell base precipitation before orographic effects.
    pub base_precip: f32,
}

impl Default for ClimateParams {
    fn default() -> Self {
        Self {
            axial_tilt: 23.5_f32.to_radians(),
            lapse_rate: 0.6,
            equator_temp: 1.0,
            polar_temp: -0.4,
            base_precip: 0.5,
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

    // --- Temperature: latitude curve + lapse rate -----------------------
    //
    // y=0 and y=h are poles; equator is mid-height. lat_norm in [-1, 1];
    // |lat_norm|=0 at equator, 1 at pole. Cosine-shaped band; lapse only
    // applies above sea level.
    let mut temperature = Vec::with_capacity(n);
    for i in 0..n {
        let lat_norm = (sites[i][1] - half_h) / half_h;
        let lat_factor = fmath::cos(lat_norm.abs() * std::f32::consts::FRAC_PI_2);
        let base = params.polar_temp + (params.equator_temp - params.polar_temp) * lat_factor;
        let lapse = params.lapse_rate * elev[i].max(0.0);
        temperature.push(base - lapse);
    }

    // --- Precipitation: orographic west→east march ----------------------
    //
    // Sea cells are moisture sources at saturation. Each land cell
    // inherits moisture from its most-westerly neighbor and drops rain
    // proportional to the elevation gained over that neighbor (uplift)
    // plus a small constant evaporation. Marching cells in ascending x
    // guarantees the upwind neighbor is already resolved.
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by(|&a, &b| sites[a as usize][0].total_cmp(&sites[b as usize][0]));

    let mut moisture = vec![0.0_f32; n];
    let mut precipitation = vec![0.0_f32; n];

    for &c in &order {
        let c = c as usize;
        let cx = sites[c][0];
        // Upwind = lowest-x neighbor strictly west of us. If none exists
        // (we're the westernmost cell of our row), the upwind is the
        // open western map boundary — saturated air at sea level.
        let upwind = neighbors[c]
            .iter()
            .copied()
            .filter(|&j| sites[j as usize][0] < cx)
            .min_by(|&a, &b| sites[a as usize][0].total_cmp(&sites[b as usize][0]));
        let (uw_m, uw_e) = match upwind {
            Some(u) => (moisture[u as usize], elev[u as usize]),
            None => (1.0, 0.0),
        };

        if elev[c] <= 0.0 {
            // Sea: gradually approach saturation via evaporation. A small
            // inland sea raises moisture only modestly; a long ocean
            // fetch fully saturates it. Geometric approach to 1 with a
            // low per-cell coefficient so 4-5 sea cells in a row don't
            // fully reset a depleted continental air mass.
            moisture[c] = uw_m + (1.0 - uw_m) * 0.02;
            precipitation[c] = params.base_precip;
            continue;
        }
        // Land: release rain proportional to upwind moisture and uplift.
        let uplift = (elev[c] - uw_e).max(0.0);
        let release = (0.10 + uplift * 4.0).min(0.9);
        let rain = uw_m * release;
        moisture[c] = (uw_m - rain).max(0.0);
        precipitation[c] = params.base_precip * (0.05 + rain);
    }

    world.climate.temperature = temperature;
    world.climate.precipitation = precipitation;
    // Biome assignment is biomes::classify's job; reset the buffer here so
    // a subsequent classify can write into the right-sized vec.
    world.climate.biome = vec![0u8; n];
}

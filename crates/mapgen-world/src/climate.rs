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

use mapgen_core::WorldData;

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

pub fn run(_world: &mut WorldData, _params: ClimateParams) {
    todo!("Phase 2: orographic precipitation + lapse-rate temperature")
}

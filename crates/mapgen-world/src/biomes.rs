//! Whittaker biome assignment. Phase 2.
//!
//! Index a 14-biome lookup table by (temperature, precipitation) per
//! land cell, with elevation overrides for snow/alpine. Sea cells get
//! shallow/deep based on depth. Writes to `world.climate.biome`.

use mapgen_core::WorldData;

pub const SNOW: u8 = 0;
pub const TUNDRA: u8 = 1;
pub const TAIGA: u8 = 2;
pub const TEMPERATE_FOREST: u8 = 3;
pub const TEMPERATE_GRASSLAND: u8 = 4;
pub const TEMPERATE_RAINFOREST: u8 = 5;
pub const DESERT: u8 = 6;
pub const SAVANNA: u8 = 7;
pub const TROPICAL_RAINFOREST: u8 = 8;
pub const TROPICAL_DRY_FOREST: u8 = 9;
pub const SHRUBLAND: u8 = 10;
pub const ALPINE: u8 = 11;
pub const SEA_SHALLOW: u8 = 12;
pub const SEA_DEEP: u8 = 13;

/// Sentinel for "not yet assigned." Tests rely on this.
pub const UNASSIGNED: u8 = u8::MAX;

/// Whittaker-style classification: index a biome by (temperature,
/// precipitation) per land cell, with elevation overrides for snow and
/// alpine. Sea cells split into shallow / deep by depth.
pub fn classify(world: &mut WorldData) {
    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;
    let temp = &world.climate.temperature;
    let precip = &world.climate.precipitation;
    let mut biome = vec![UNASSIGNED; n];

    for i in 0..n {
        if elev[i] <= 0.0 {
            biome[i] = if elev[i] < -0.3 {
                SEA_DEEP
            } else {
                SEA_SHALLOW
            };
            continue;
        }
        let t = temp[i];
        let p = precip[i];
        biome[i] = if t <= 0.0 {
            // Below freezing. Above-ridge cells go alpine, otherwise snow.
            if elev[i] >= 0.55 {
                ALPINE
            } else {
                SNOW
            }
        } else if t <= 0.2 {
            TUNDRA
        } else if t <= 0.4 {
            if p < 0.15 {
                SHRUBLAND
            } else {
                TAIGA
            }
        } else if t <= 0.7 {
            if p < 0.10 {
                DESERT
            } else if p < 0.20 {
                TEMPERATE_GRASSLAND
            } else if p < 0.35 {
                TEMPERATE_FOREST
            } else {
                TEMPERATE_RAINFOREST
            }
        } else if p < 0.10 {
            DESERT
        } else if p < 0.20 {
            SAVANNA
        } else if p < 0.35 {
            TROPICAL_DRY_FOREST
        } else {
            TROPICAL_RAINFOREST
        };
    }

    world.climate.biome = biome;
}

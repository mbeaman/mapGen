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

/// Biome assignment.
///
/// * If seasonal climate is present (filled by `climate_seasonal::run`),
///   route through Köppen-Geiger classification — the scientifically
///   standard scheme that distinguishes Mediterranean from steppe,
///   humid-subtropical from continental, etc.
/// * Otherwise fall back to a single-season Whittaker LUT.
///
/// Sea cells split shallow / deep by depth either way. Alpine override:
/// very high, very cold cells become rocky alpine regardless of class.
///
/// `apply_biome_patches` runs at the end so lore overrides take effect.
pub fn classify(world: &mut WorldData) {
    let n = world.mesh.cell_count();
    let seasonal = world.climate.temperature_summer.len() == n
        && world.climate.temperature_winter.len() == n
        && world.climate.precipitation_summer.len() == n
        && world.climate.precipitation_winter.len() == n;

    let mut biome = vec![UNASSIGNED; n];
    let elev = world.terrain.elevation.clone();

    if seasonal {
        let koppen_classes = crate::koppen::classify(world);
        for i in 0..n {
            if elev[i] <= 0.0 {
                biome[i] = if elev[i] < -0.3 {
                    SEA_DEEP
                } else {
                    SEA_SHALLOW
                };
                continue;
            }
            if elev[i] >= 0.6 && world.climate.temperature[i] < 0.0 {
                biome[i] = ALPINE;
                continue;
            }
            biome[i] = crate::koppen::to_biome(koppen_classes[i]);
        }
    } else {
        let temp = world.climate.temperature.clone();
        let precip = world.climate.precipitation.clone();
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
    }

    world.climate.biome = biome;
    crate::patch::apply_biome_patches(world);
}

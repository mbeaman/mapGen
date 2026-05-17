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

pub fn classify(_world: &mut WorldData) {
    todo!("Phase 2: Whittaker biome assignment")
}

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
/// Riparian / gallery — applied as an override to river cells (and
/// near-river cells) that pass through arid surroundings, modeling the
/// Nile-through-Sahara effect. Counts as biome 14.
pub const RIPARIAN: u8 = 14;
/// Wetland / marsh / swamp / bog (Phase 6.1.4) — a soil-driven override
/// applied to waterlogged Histosol cells (flat, low-lying, well-watered).
/// Fills a real ecological gap the pure-climate Köppen palette can't reach.
pub const WETLAND: u8 = 15;

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

    // Soil order (6.1.4) is derived from the same climate + drainage inputs and
    // feeds the WETLAND override below, so classify it here at the head of the
    // Biomes stage. Pure data layer — no RNG, native↔wasm byte-identical.
    crate::soils::classify(world);

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
            // Alpine override for any high-elevation cell with cold
            // summers (≥ 0.45 elevation AND tundra-class summer temp).
            // Without this, mid-elevation cells in the temperate band
            // get misclassified as low-lying tundra.
            let summer_t = world
                .climate
                .temperature_summer
                .get(i)
                .copied()
                .unwrap_or(world.climate.temperature[i]);
            if elev[i] >= 0.45 && summer_t < 0.30 {
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

    // Soil-driven override (6.1.4): waterlogged Histosol cells are marshes /
    // swamps / bogs — the one biome the pure-climate Köppen palette can't
    // express. Applied before riparian/patches so a lore patch still wins.
    let soil = &world.climate.soil;
    if soil.len() == n {
        for i in 0..n {
            if elev[i] > 0.0 && soil[i] == crate::soils::HISTOSOL {
                biome[i] = WETLAND;
            }
        }
    }

    world.climate.biome = biome;
    apply_riparian(world);
    crate::patch::apply_biome_patches(world);
}

/// Overwrite the biome of every river cell that passes through arid
/// surroundings (or whose own classified biome is arid) with RIPARIAN
/// — modeling the Nile-through-Sahara / Colorado-through-Mojave effect.
/// A river through a forest stays forest; only the arid-corridor case
/// becomes a green riparian stripe.
pub fn apply_riparian(world: &mut WorldData) {
    let n = world.mesh.cell_count();
    if world.climate.biome.len() != n {
        return;
    }
    // Collect river cells.
    let river_cells: std::collections::BTreeSet<u32> = world
        .hydrology
        .rivers
        .iter()
        .flat_map(|r| r.cells.iter().copied())
        .collect();
    if river_cells.is_empty() {
        return;
    }
    let arid = |b: u8| matches!(b, DESERT | SHRUBLAND | SAVANNA);
    let mut new_biomes = world.climate.biome.clone();
    for &rc in &river_cells {
        let i = rc as usize;
        if world.terrain.elevation[i] <= 0.0 {
            continue;
        }
        let self_arid = arid(world.climate.biome[i]);
        let any_arid_neighbor = world.mesh.neighbors[i]
            .iter()
            .any(|&j| arid(world.climate.biome[j as usize]));
        if self_arid || any_arid_neighbor {
            new_biomes[i] = RIPARIAN;
        }
    }
    world.climate.biome = new_biomes;
}

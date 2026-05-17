//! Köppen-Geiger climate classification.
//!
//! Indexes the seasonal `ClimateData` (temperature_summer/winter,
//! precipitation_summer/winter) into the standard Köppen letter codes.
//! Reference: Beck et al. 2018, "Present and future Köppen-Geiger
//! climate classification maps." We collapse the 30+ original codes
//! into the ones that matter for biome differentiation.

use mapgen_core::WorldData;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum KoppenClass {
    None,
    /// Tropical rainforest — no dry season, warm all year.
    Af,
    /// Tropical monsoon.
    Am,
    /// Tropical savanna / wet-dry.
    Aw,
    /// Hot desert.
    BWh,
    /// Cold desert.
    BWk,
    /// Hot steppe.
    BSh,
    /// Cold steppe.
    BSk,
    /// Mediterranean — hot dry summer.
    Csa,
    /// Mediterranean — warm dry summer.
    Csb,
    /// Humid subtropical — hot summer.
    Cfa,
    /// Oceanic / marine west coast.
    Cfb,
    /// Subpolar oceanic.
    Cfc,
    /// Humid continental — hot summer.
    Dfa,
    /// Humid continental — warm summer.
    Dfb,
    /// Subarctic.
    Dfc,
    /// Extreme subarctic (Siberian).
    Dfd,
    /// Tundra.
    ET,
    /// Polar ice cap.
    EF,
}

/// Classify every cell. Sea cells return `None`; land cells get a
/// Köppen letter code derived from their seasonal climate.
pub fn classify(world: &WorldData) -> Vec<KoppenClass> {
    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;
    let ts = &world.climate.temperature_summer;
    let tw = &world.climate.temperature_winter;
    let ps = &world.climate.precipitation_summer;
    let pw = &world.climate.precipitation_winter;
    let seasonal = ts.len() == n && tw.len() == n && ps.len() == n && pw.len() == n;
    let mut out = vec![KoppenClass::None; n];
    if !seasonal {
        return out;
    }
    for i in 0..n {
        if elev[i] <= 0.0 {
            continue;
        }
        out[i] = classify_one(ts[i], tw[i], ps[i], pw[i]);
    }
    out
}

/// One cell's classification. Inputs are normalized:
///   temperature in `[-0.4 .. ~1.3]` where 0 ≈ freezing, 1 = tropical;
///   precipitation in `[0 .. ~0.5]` where 0.5 ≈ rainforest annual.
fn classify_one(ts: f32, tw: f32, ps: f32, pw: f32) -> KoppenClass {
    let t_warm = ts.max(tw); // warmest-month proxy
    let t_cold = ts.min(tw); // coldest-month proxy
    let p_annual = ps + pw;
    let p_summer = ps;
    let p_winter = pw;
    let summer_dry = p_summer < p_winter * 0.4 && p_summer < 0.04;
    let winter_dry = p_winter < p_summer * 0.4 && p_winter < 0.04;

    // E — Polar / Tundra: warmest month below "tundra threshold" (~0.05).
    if t_warm < 0.05 {
        return if t_warm < -0.1 {
            KoppenClass::EF
        } else {
            KoppenClass::ET
        };
    }

    // B — Arid: aridity index. Köppen's classic: P < 2T + adjustments.
    // Simplified: low annual precip *relative to* temperature.
    // Aridity threshold scales with summer warmth (warmer air holds
    // more moisture before saturating).
    let arid_threshold = 0.04 + t_warm * 0.10;
    if p_annual < arid_threshold {
        let desert_cutoff = arid_threshold * 0.5;
        if p_annual < desert_cutoff {
            return if t_cold > 0.2 {
                KoppenClass::BWh
            } else {
                KoppenClass::BWk
            };
        } else {
            return if t_cold > 0.2 {
                KoppenClass::BSh
            } else {
                KoppenClass::BSk
            };
        }
    }

    // A — Tropical: coldest month above freezing AND warm.
    if t_cold > 0.6 {
        if winter_dry && t_cold > 0.5 {
            return KoppenClass::Aw;
        }
        if p_annual > 0.15 && !summer_dry {
            return KoppenClass::Af;
        }
        return KoppenClass::Am;
    }

    // D — Continental: cold month below freezing, warm summer.
    if t_cold < 0.05 {
        if t_warm > 0.7 {
            return KoppenClass::Dfa;
        }
        if t_warm > 0.4 {
            return KoppenClass::Dfb;
        }
        if t_warm > 0.2 {
            return KoppenClass::Dfc;
        }
        return KoppenClass::Dfd;
    }

    // C — Temperate (cold month between freezing and ~15°C).
    if summer_dry {
        return if t_warm > 0.7 {
            KoppenClass::Csa
        } else {
            KoppenClass::Csb
        };
    }
    if t_warm > 0.7 {
        return KoppenClass::Cfa;
    }
    if t_warm > 0.45 {
        return KoppenClass::Cfb;
    }
    KoppenClass::Cfc
}

/// Map a Köppen class to one of the 14 biome IDs in `crate::biomes`.
/// This bridges Köppen-class classification to the renderer's biome
/// palette without yet expanding the biome enum.
pub fn to_biome(c: KoppenClass) -> u8 {
    use crate::biomes::*;
    match c {
        KoppenClass::None => UNASSIGNED,
        KoppenClass::Af | KoppenClass::Am => TROPICAL_RAINFOREST,
        KoppenClass::Aw => SAVANNA,
        KoppenClass::BWh => DESERT,
        KoppenClass::BWk => DESERT,
        KoppenClass::BSh => SHRUBLAND,
        KoppenClass::BSk => SHRUBLAND,
        KoppenClass::Csa | KoppenClass::Csb => TEMPERATE_GRASSLAND,
        KoppenClass::Cfa => TROPICAL_DRY_FOREST,
        KoppenClass::Cfb => TEMPERATE_FOREST,
        KoppenClass::Cfc => TEMPERATE_RAINFOREST,
        KoppenClass::Dfa | KoppenClass::Dfb => TEMPERATE_FOREST,
        KoppenClass::Dfc | KoppenClass::Dfd => TAIGA,
        KoppenClass::ET => TUNDRA,
        KoppenClass::EF => SNOW,
    }
}

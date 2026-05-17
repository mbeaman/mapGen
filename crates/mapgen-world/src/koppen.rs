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

/// One cell's classification. Inputs are normalized as follows.
/// Temperature: 0 ≈ 0°C (freezing), 0.04 ≈ +1°C, 1.0 ≈ +25°C (tropical
/// sea-level mean). Precipitation: fraction of saturated-ocean baseline.
///
/// Thresholds are calibrated against the actual distribution our 3-cell
/// orographic model produces on a 4-6k cell continent. Earth-faithful
/// references: tree line at ~10°C warmest month, tropical floor at
/// ~18°C coldest month, Köppen aridity P/T threshold ~14 cm/°C.
fn classify_one(ts: f32, tw: f32, ps: f32, pw: f32) -> KoppenClass {
    let t_warm = ts.max(tw); // warmest-month proxy
    let t_cold = ts.min(tw); // coldest-month proxy
    let p_annual = ps + pw;
    let p_summer = ps;
    let p_winter = pw;
    let summer_dry = p_summer < p_winter * 0.4 && p_summer < 0.05;
    let winter_dry = p_winter < p_summer * 0.4 && p_winter < 0.05;

    // E — Polar / Tundra: warmest month below the tree-line. 10°C ≈ 0.40
    // on our scale. Cells where summer never warms past that are
    // tundra (ET); ice-cap (EF) where even summer is sub-freezing.
    if t_warm < 0.40 {
        return if t_warm < 0.0 {
            KoppenClass::EF
        } else {
            KoppenClass::ET
        };
    }

    // B — Arid. Aridity threshold scales with summer warmth and is
    // calibrated against the actual precipitation distribution our
    // 3-cell + orographic model produces (typical land p_annual ranges
    // from ~0.06 driest decile to ~0.20 wettest). Threshold sits near
    // the 30th percentile so ~30% of land qualifies as arid — close to
    // Earth's actual fraction.
    let arid_threshold = 0.10 + t_warm * 0.04;
    if p_annual < arid_threshold {
        // Our precipitation distribution is tighter than real Earth's
        // (no Atacama-scale outliers), so the BW desert cutoff sits
        // higher relative to BS steppe than Köppen's classical 0.5 ratio.
        let desert_cutoff = arid_threshold * 0.75;
        if p_annual < desert_cutoff {
            return if t_cold > 0.55 {
                KoppenClass::BWh
            } else {
                KoppenClass::BWk
            };
        } else {
            return if t_cold > 0.55 {
                KoppenClass::BSh
            } else {
                KoppenClass::BSk
            };
        }
    }

    // A — Tropical: coldest month warm year-round. Real Köppen uses
    // 18°C ≈ 0.72 on our scale, but with our lapse rate even moderately
    // elevated equatorial cells dip below that, mis-classifying
    // tropical highlands as temperate. Threshold relaxed to 0.55 so
    // tropical-elevation cells (Kenyan highlands analogue) remain in A.
    if t_cold > 0.55 {
        if winter_dry && t_cold > 0.50 {
            return KoppenClass::Aw;
        }
        if p_annual > 0.25 && !summer_dry {
            return KoppenClass::Af;
        }
        if p_annual > 0.18 {
            return KoppenClass::Am;
        }
        return KoppenClass::Aw;
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
        KoppenClass::Af => TROPICAL_RAINFOREST,
        KoppenClass::Am => TROPICAL_RAINFOREST,
        KoppenClass::Aw => SAVANNA,
        KoppenClass::BWh => DESERT,
        KoppenClass::BWk => DESERT,
        KoppenClass::BSh => SHRUBLAND,
        KoppenClass::BSk => SHRUBLAND,
        // Mediterranean is shrubland-grassland in real ecology (chaparral,
        // maquis, garrigue) — not a wet grassland.
        KoppenClass::Csa | KoppenClass::Csb => SHRUBLAND,
        // Humid subtropical (Cfa) = SE US, Hong Kong, southern Japan —
        // these are temperate deciduous forests in real ecology, not
        // tropical-dry forest.
        KoppenClass::Cfa => TEMPERATE_FOREST,
        // Oceanic / marine west coast = NW Europe, NW US — temperate forest.
        KoppenClass::Cfb => TEMPERATE_FOREST,
        // Subpolar oceanic = Iceland, Faroes — temperate rainforest if
        // wet, else mid-elevation temperate.
        KoppenClass::Cfc => TEMPERATE_RAINFOREST,
        KoppenClass::Dfa | KoppenClass::Dfb => TEMPERATE_FOREST,
        KoppenClass::Dfc | KoppenClass::Dfd => TAIGA,
        KoppenClass::ET => TUNDRA,
        KoppenClass::EF => SNOW,
    }
}

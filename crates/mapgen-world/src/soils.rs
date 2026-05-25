//! USDA soil-taxonomy soil orders (Phase 6.1.4).
//!
//! Classifies every land cell into one of the 12 USDA soil orders from the
//! climate (temperature, precipitation, seasonality), drainage (flow
//! accumulation), and topography (local relief) we already compute. Soil is a
//! genuinely new data layer — it drives the `biomes::WETLAND` override
//! (waterlogged Histosols) today, and is the substrate the multi-scale atlas
//! will hang agricultural suitability and soil-tinted rendering on later.
//!
//! This is a *climatic* approximation of soil taxonomy, not a pedological
//! simulation: the real orders are defined by diagnostic horizons that form
//! over millennia, but those horizons correlate strongly with the climate /
//! drainage / age signals we have. References: USDA Soil Taxonomy (12 orders);
//! the standard climate→order correspondences (e.g. Oxisols in humid tropics,
//! Aridisols in deserts, Spodosols in cool conifer forests, Mollisols under
//! grassland, Gelisols over permafrost). Reproducibility note: every
//! transcendental routes through `mapgen_core::fmath` (none needed here — the
//! classifier is pure comparisons), so this stage is native↔wasm byte-identical.
//!
//! `Andisol` (volcanic-ash soils) is intentionally never assigned: we model no
//! volcanism, so there is no signal to key it off. The constant exists for
//! completeness and so a future volcanism stage has a home for it.

use mapgen_core::WorldData;

// The 12 USDA soil orders. Ordered loosely by the classifier's diagnostic
// precedence (most-overriding condition first), not by taxonomic rank.
/// Permafrost soils (perennially frozen subsoil).
pub const GELISOL: u8 = 0;
/// Organic, waterlogged soils (peat / muck) — bogs, marshes, swamps.
pub const HISTOSOL: u8 = 1;
/// Young soils with no real horizon development — floodplain alluvium, dunes.
pub const ENTISOL: u8 = 2;
/// Weakly developed young soils — steep, recently eroded uplands.
pub const INCEPTISOL: u8 = 3;
/// Dry-climate soils — deserts and cold steppe.
pub const ARIDISOL: u8 = 4;
/// Deeply weathered, nutrient-poor humid-tropical soils.
pub const OXISOL: u8 = 5;
/// Swelling clay soils of seasonally wet/dry subtropics.
pub const VERTISOL: u8 = 6;
/// Acidic, weathered warm-humid soils (humid subtropics — SE US, S China).
pub const ULTISOL: u8 = 7;
/// Organic-rich grassland soils — prairie / steppe / pampas.
pub const MOLLISOL: u8 = 8;
/// Moderately weathered, fertile temperate-forest soils.
pub const ALFISOL: u8 = 9;
/// Acidic, sandy soils of cool coniferous forests (taiga, boreal).
pub const SPODOSOL: u8 = 10;
/// Volcanic-ash soils. Never assigned (no volcanism model) — see module docs.
pub const ANDISOL: u8 = 11;

/// Sentinel for sea cells (no soil).
pub const OCEAN: u8 = u8::MAX;

/// Classify every cell's soil order and write it to `world.climate.soil`.
/// Requires climate (temperature/precipitation, seasonal if available) and
/// hydrology (`flow`) to have run; topographic relief is derived from the mesh.
pub fn classify(world: &mut WorldData) {
    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;
    let temp = &world.climate.temperature;
    let precip = &world.climate.precipitation;
    let flow = &world.hydrology.flow;
    let neighbors = &world.mesh.neighbors;

    let seasonal = world.climate.precipitation_summer.len() == n
        && world.climate.precipitation_winter.len() == n
        && world.climate.temperature_summer.len() == n
        && world.climate.temperature_winter.len() == n;

    let mut soil = vec![OCEAN; n];
    if temp.len() != n || precip.len() != n {
        world.climate.soil = soil;
        return;
    }

    for i in 0..n {
        if elev[i] <= 0.0 {
            continue; // OCEAN
        }

        // Local relief: the largest elevation gap to any neighbor. High =
        // steep/rugged (young soils); ~0 = flat (stable old surfaces, basins).
        let mut relief = 0.0_f32;
        for &j in &neighbors[i] {
            let d = (elev[i] - elev[j as usize]).abs();
            if d > relief {
                relief = d;
            }
        }

        let t = temp[i];
        let (t_warm, t_cold, p_summer, p_winter) = if seasonal {
            let ts = world.climate.temperature_summer[i];
            let tw = world.climate.temperature_winter[i];
            (
                ts.max(tw),
                ts.min(tw),
                world.climate.precipitation_summer[i],
                world.climate.precipitation_winter[i],
            )
        } else {
            (t, t, precip[i] * 0.5, precip[i] * 0.5)
        };
        // Flow defaults to 0 if hydrology hasn't run — the drainage-keyed
        // rules (Histosol/Entisol) then simply don't fire, which is correct.
        let f = flow.get(i).copied().unwrap_or(0.0);

        // Annual precipitation as the *sum* of the two seasonal passes — the
        // same quantity `koppen::classify_one` keys aridity off. (Note
        // `climate.precipitation` is the half-year *mean*, so using it directly
        // would halve every threshold and over-classify arid soils.) Equals the
        // single-pass value on the non-seasonal `climate::run` path.
        let p_annual = p_summer + p_winter;

        // Strong dry season: the drier half-year is < 40% of the wetter one.
        let (p_lo, p_hi) = if p_summer < p_winter {
            (p_summer, p_winter)
        } else {
            (p_winter, p_summer)
        };
        let strong_dry_season = p_lo < p_hi * 0.4;

        // True desert only — the `BW` cutoff from `koppen::classify_one`
        // (0.75 × the arid threshold). The wetter `BS` steppe band is *not*
        // Aridisol: semi-arid grassland builds organic-rich Mollisols, so it
        // falls through to the climate branch below.
        let desert = p_annual < (0.10 + t_warm * 0.04) * 0.75;

        // `flow` is raw upstream-cell accumulation (not normalized): on a
        // 4k-cell continent the land median is ~2, p90 ~20, max ~500, so the
        // thresholds below are in those units. `relief` is the largest
        // elevation gap to a neighbour (p50 ~0.03, p90 ~0.32).
        soil[i] = if t_warm < 0.20 {
            // Perennially near-frozen → permafrost.
            GELISOL
        } else if relief < 0.035 && elev[i] < 0.30 && p_annual > 0.25 {
            // Flat, low-lying, wet ground drains poorly and stays saturated →
            // peat/muck accumulates. (No flow requirement: marshes are often
            // stagnant.) These become the WETLAND biome.
            HISTOSOL
        } else if relief > 0.20 {
            // Steep, actively eroding uplands keep only weakly developed soils.
            INCEPTISOL
        } else if f > 25.0 && relief < 0.08 {
            // Flat valley floors carrying heavy through-flow → fresh alluvium
            // reworked faster than horizons can form. Checked before Aridisol
            // so a high-flow valley in a dry belt (a Nile analogue) reads as
            // alluvium, not desert.
            ENTISOL
        } else if desert {
            ARIDISOL
        } else if t_cold > 0.55 {
            // Tropical / humid subtropical — gated on the *coldest* month
            // (Köppen's A criterion), so temperate regions with hot summers
            // don't leak in here.
            if p_annual > 0.35 && relief < 0.06 {
                OXISOL // old, flat, very wet → deep lateritic weathering
            } else if strong_dry_season {
                VERTISOL // pronounced wet/dry → swelling clays
            } else {
                ULTISOL // warm, humid, sloping → leached acidic soils
            }
        } else if t_warm > 0.40 {
            // Temperate — has a real growing-season summer.
            if p_annual < 0.30 {
                MOLLISOL // semi-arid grassland (steppe / prairie)
            } else {
                ALFISOL // subhumid → humid temperate forest soils
            }
        } else {
            // Short, cool summers → boreal / cool conifer belt.
            SPODOSOL
        };
    }

    world.climate.soil = soil;
}

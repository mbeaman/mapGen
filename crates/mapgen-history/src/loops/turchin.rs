//! Turchin demographic-fiscal secular cycle. Rise/collapse from population
//! pressure and elite overproduction.
//!
//! Phase 4b implements the **demographic backbone**: per-polity logistic
//! population growth toward a biome-derived carrying capacity, with Malthusian
//! crises (famine, plague) when population presses on capacity and exogenous
//! droughts that transiently cut it. This is the population/instability half of
//! Turchin's structural-demographic theory (Turchin & Nefedov, *Secular
//! Cycles*, 2009); the fiscal / elite-overproduction half lands in 4d.

use mapgen_core::EventKind;

use crate::emit::Emit;
use crate::loops::{CausalLoop, LoopId, TickCtx};
use crate::unit_f32;

/// Intrinsic annual population growth rate (logistic `r`).
const GROWTH_RATE: f32 = 0.03;
/// Per-year drought probability at full aridity (scaled by polity aridity).
const DROUGHT_BASE_PROB: f32 = 0.15;
/// Fraction by which a drought cuts that year's effective capacity.
const DROUGHT_SEVERITY: f32 = 0.35;
/// Stress (population / effective-capacity) above which famine strikes.
/// Famine is the Malthusian regulator and should be the *common* crisis of
/// the demographic backbone (Turchin), so the threshold sits below unity.
const FAMINE_STRESS: f32 = 0.85;
/// Plague probability per year, scaled by population density (P / K). Kept
/// low so plague is an occasional epidemic, not the dominant event — famine
/// regulates population; plague is a rarer density shock.
const PLAGUE_DENSITY_PROB: f32 = 0.010;
/// Fraction of population lost to a plague.
const PLAGUE_MORTALITY: f32 = 0.18;
/// Floor so a polity's population never vanishes — secular crises are
/// recurring (boom/bust/recover), not terminal; permanent dissolution comes
/// with conquest in 4e.
const MIN_POPULATION: f32 = 0.5;
const EPS: f32 = 1e-3;

// --- 4d fiscal / elite / instability knobs (normalized units) ---------------
/// Elite cohort growth per unit density, and natural attrition.
const ELITE_GAIN: f32 = 0.012;
const ELITE_DECAY: f32 = 0.02;
/// Elite count the polity can sustain; the excess is "overproduction".
const ELITE_SUSTAIN: f32 = 0.35;
/// Treasury revenue per unit density and cost per unit elite.
const TAX_YIELD: f32 = 0.10;
const ELITE_UPKEEP: f32 = 0.20;
/// Density above which the populace is immiserated (wages fall).
const IMMIS_THRESH: f32 = 0.75;
/// Instability gains per year from immiseration / elite overproduction /
/// fiscal strain / dynastic decadence (1 − asabiyyah), minus baseline venting.
const IMMIS_W: f32 = 0.020;
const ELITE_W: f32 = 0.020;
const FISCAL_W: f32 = 0.010;
const DECADENCE_W: f32 = 0.012;
const INSTAB_RELAX: f32 = 0.004;
/// Instability at which a secular crisis erupts and vents. Tuned (with the
/// gains above) for ~2–3 boom/bust cycles per polity across 500 years.
const CRISIS_THRESHOLD: f32 = 0.65;
/// Crisis losses.
const CRISIS_POP_LOSS: f32 = 0.30;
const CRISIS_ELITE_LOSS: f32 = 0.60;
/// Expansion: chance per quiet, growing year of founding a new town.
const EXPAND_PROB: f32 = 0.02;

/// Turchin structural-demographic loop.
pub struct Turchin;

impl CausalLoop for Turchin {
    fn id(&self) -> LoopId {
        LoopId::Turchin
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        let year = ctx.year;
        for pid in 0..ctx.state.population.len() {
            let k = ctx.state.capacity[pid];
            if k <= EPS {
                continue;
            }
            let aridity = ctx.state.aridity[pid];
            let mut p = ctx.state.population[pid];
            // Read polity facts up front (immutable) so the event emissions
            // below can take `&mut world` without a borrow conflict.
            let cell = ctx.world.society.nations[pid].capital_cell;
            let name = ctx.world.society.nations[pid].name.clone();

            // Drought: an exogenous capacity shock, weighted by how arid the
            // polity's territory is. Reduces *this year's* effective capacity.
            let drought = unit_f32(ctx.rng) < DROUGHT_BASE_PROB * aridity;
            let eff_k = if drought {
                k * (1.0 - DROUGHT_SEVERITY)
            } else {
                k
            };
            if drought {
                Emit::new(
                    year,
                    EventKind::Drought,
                    cell,
                    0.40,
                    format!("A drought parched the lands of {name}."),
                )
                .push(ctx.world);
            }

            // Malthusian famine when population presses on (drought-reduced)
            // capacity; otherwise a density-driven plague may strike.
            let stress = p / eff_k.max(EPS);
            if stress > FAMINE_STRESS {
                let mortality = (0.12 + 0.5 * (stress - FAMINE_STRESS)).clamp(0.05, 0.35);
                p *= 1.0 - mortality;
                let salience = (0.45 + 0.5 * mortality).clamp(0.0, 1.0);
                Emit::new(
                    year,
                    EventKind::Famine,
                    cell,
                    salience,
                    format!("Famine gripped {name}."),
                )
                .push(ctx.world);
            } else {
                let density = p / k.max(EPS);
                if unit_f32(ctx.rng) < PLAGUE_DENSITY_PROB * density {
                    p *= 1.0 - PLAGUE_MORTALITY;
                    Emit::new(
                        year,
                        EventKind::Plague,
                        cell,
                        0.55,
                        format!("A plague swept through {name}."),
                    )
                    .push(ctx.world);
                }
            }

            // Discrete logistic growth toward baseline capacity. Pure
            // arithmetic — no transcendental, so native ↔ wasm32 stays
            // bit-identical without routing through fmath.
            p += GROWTH_RATE * p * (1.0 - p / k.max(EPS));

            // --- 4d: fiscal / elite dynamics → secular-cycle instability ---
            let density = (p / k.max(EPS)).clamp(0.0, 2.0);
            let mut elites = ctx.state.elites[pid];
            let mut fiscal = ctx.state.fiscal[pid];
            let mut inst = ctx.state.instability[pid];
            let asabiyyah = ctx.state.asabiyyah[pid];

            elites += ELITE_GAIN * density - ELITE_DECAY * elites;
            let overproduction = (elites - ELITE_SUSTAIN).max(0.0);
            fiscal += TAX_YIELD * density - ELITE_UPKEEP * elites;
            let fiscal_strain = (-fiscal).max(0.0);
            let immiseration = (density - IMMIS_THRESH).max(0.0);
            let d_inst = IMMIS_W * immiseration
                + ELITE_W * overproduction
                + FISCAL_W * fiscal_strain
                + DECADENCE_W * (1.0 - asabiyyah);
            inst = (inst + d_inst - INSTAB_RELAX).max(0.0);

            if inst > CRISIS_THRESHOLD {
                // Secular crisis: the state fragments — settlements emptied,
                // people displaced, an elite faction purged. Population, elites,
                // and treasury crash; instability vents.
                Emit::new(
                    year,
                    EventKind::CityAbandoned,
                    cell,
                    0.62,
                    format!("A settlement of {name} was abandoned amid the upheaval."),
                )
                .push(ctx.world);
                Emit::new(
                    year,
                    EventKind::Migration,
                    cell,
                    0.5,
                    format!("Famine and strife drove people from {name}."),
                )
                .push(ctx.world);
                if overproduction > 0.0 {
                    Emit::new(
                        year,
                        EventKind::Exile,
                        cell,
                        0.55,
                        format!("A defeated faction was cast out of {name}."),
                    )
                    .push(ctx.world);
                }
                p *= 1.0 - CRISIS_POP_LOSS;
                elites *= 1.0 - CRISIS_ELITE_LOSS;
                fiscal *= 0.5;
                inst = 0.0;
            } else if inst < 0.2
                && (0.5..0.85).contains(&density)
                && unit_f32(ctx.rng) < EXPAND_PROB
            {
                // Expansion in quiet, growing years.
                Emit::new(
                    year,
                    EventKind::CityFounded,
                    cell,
                    0.32,
                    format!("A new town was founded in {name} during the long peace."),
                )
                .push(ctx.world);
            }

            ctx.state.elites[pid] = elites;
            ctx.state.fiscal[pid] = fiscal;
            ctx.state.instability[pid] = inst;
            ctx.state.population[pid] = p.max(MIN_POPULATION);
        }
    }
}

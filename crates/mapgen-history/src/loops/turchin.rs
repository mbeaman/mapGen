//! Turchin demographic-fiscal secular cycle. Rise/collapse from population
//! pressure and elite overproduction.
//!
//! Phase 4b implements the **demographic backbone**: per-polity logistic
//! population growth toward a biome-derived carrying capacity, with Malthusian
//! crises (famine, plague) when population presses on capacity and exogenous
//! droughts that transiently cut it. This is the population/instability half of
//! Turchin's structural-demographic theory (Turchin & Nefedov, *Secular
//! Cycles*, 2009); the fiscal / elite-overproduction half lands in 4d.

use mapgen_core::{CellId, Event, EventId, EventKind, WorldData};
use smallvec::SmallVec;

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
/// Floor so a polity's population never vanishes here — extinction /
/// dissolution is the fiscal loop's job in 4d.
const MIN_POPULATION: f32 = 0.5;
const EPS: f32 = 1e-3;

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
                emit(
                    ctx.world,
                    year,
                    EventKind::Drought,
                    cell,
                    0.40,
                    format!("A drought parched the lands of {name}."),
                );
            }

            // Malthusian famine when population presses on (drought-reduced)
            // capacity; otherwise a density-driven plague may strike.
            let stress = p / eff_k.max(EPS);
            if stress > FAMINE_STRESS {
                let mortality = (0.12 + 0.5 * (stress - FAMINE_STRESS)).clamp(0.05, 0.35);
                p *= 1.0 - mortality;
                let salience = (0.45 + 0.5 * mortality).clamp(0.0, 1.0);
                emit(
                    ctx.world,
                    year,
                    EventKind::Famine,
                    cell,
                    salience,
                    format!("Famine gripped {name}."),
                );
            } else {
                let density = p / k.max(EPS);
                if unit_f32(ctx.rng) < PLAGUE_DENSITY_PROB * density {
                    p *= 1.0 - PLAGUE_MORTALITY;
                    emit(
                        ctx.world,
                        year,
                        EventKind::Plague,
                        cell,
                        0.55,
                        format!("A plague swept through {name}."),
                    );
                }
            }

            // Discrete logistic growth toward baseline capacity. Pure
            // arithmetic — no transcendental, so native ↔ wasm32 stays
            // bit-identical without routing through fmath.
            p += GROWTH_RATE * p * (1.0 - p / k.max(EPS));
            ctx.state.population[pid] = p.max(MIN_POPULATION);
        }
    }
}

/// Append a located, salience-scored event to the log. `actors`/`patients`
/// are empty in 4b — named characters arrive in 4c, causal links in 4i.
fn emit(
    world: &mut WorldData,
    year: i32,
    kind: EventKind,
    cell: u32,
    salience: f32,
    summary: String,
) {
    world.events.push(Event {
        id: EventId(0), // overwritten by EventLog::push
        year,
        kind,
        actors: SmallVec::new(),
        patients: SmallVec::new(),
        location: Some(CellId(cell)),
        cause_ids: SmallVec::new(),
        salience,
        casus_belli: None,
        summary_canonical: summary,
    });
}

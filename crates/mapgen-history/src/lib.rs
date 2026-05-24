//! Deterministic agent-based history simulation. Produces a typed, append-only
//! event log (and named entities) that the rendering and lore layers consume.
//!
//! # Determinism
//! All randomness is derived hierarchically from one base seed via
//! [`mapgen_core::splitmix64`]:
//! `sim_seed → splitmix64(sim_seed, year) → splitmix64(year_seed, loop_id)`.
//! Each (year, loop) seed is *derived*, never drawn from a shared advancing
//! stream, so re-rolling or adding one loop cannot perturb another. The base
//! seed comes from the pipeline's `Stage::History` sub-stream. See
//! `tests` below and `mapgen-world/tests/history_spec.rs`.

pub mod loops;

use mapgen_core::{splitmix64, WorldData};
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaCha8Rng,
};

use crate::loops::{default_loops, CausalLoop, LoopId, TickCtx};

/// Tunables for a history run.
#[derive(Clone, Debug)]
pub struct HistoryParams {
    /// Number of years to simulate. MVP target is 500.
    pub years: i32,
}

impl Default for HistoryParams {
    fn default() -> Self {
        Self { years: 500 }
    }
}

/// Cross-loop scratch state for one sim run. Holds the system-dynamics
/// aggregates the loops read and write each year. **Never serialized** — only
/// the resulting events / entities persist (the persistence boundary from the
/// Phase-4 plan). Minimal in 4a; grows as the loops land.
#[derive(Clone, Debug, Default)]
pub struct SimState {
    /// Polity count snapshotted at sim start. Aggregating cells → per-polity
    /// state vectors is Phase 4b's job; for now this is the one real datum the
    /// driver threads through.
    pub polity_count: usize,
}

impl SimState {
    fn new(world: &WorldData) -> Self {
        Self {
            polity_count: world.society.nations.len(),
        }
    }
}

/// Derive the RNG seed for one loop in one year. Pure: depends only on the
/// base seed, the year, and the loop's stable [`LoopId`] — never on which other
/// loops exist or the order they run. That purity is the determinism property
/// the tests pin.
pub fn loop_seed(sim_seed: u64, year: i32, loop_id: LoopId) -> u64 {
    let year_seed = splitmix64(sim_seed, year as u64);
    splitmix64(year_seed, loop_id as u64)
}

/// Run `params.years` of deterministic history against `world` in place, using
/// the six causal loops in canonical order. `rng` is the `Stage::History`
/// sub-stream from the pipeline; its first draw fixes the sim's base seed, from
/// which every per-year / per-loop seed is derived.
pub fn run(world: &mut WorldData, params: HistoryParams, rng: &mut ChaCha8Rng) {
    run_with_loops(world, params, rng, default_loops());
}

/// [`run`] with an injectable loop set — the seam tests use to count ticks and
/// verify ordering / stream-independence without the real (no-op in 4a) loops.
pub fn run_with_loops(
    world: &mut WorldData,
    params: HistoryParams,
    rng: &mut ChaCha8Rng,
    mut loops: Vec<Box<dyn CausalLoop>>,
) {
    let sim_seed = rng.next_u64();
    let mut state = SimState::new(world);
    for year in 0..params.years {
        for lp in loops.iter_mut() {
            let mut lrng = ChaCha8Rng::seed_from_u64(loop_seed(sim_seed, year, lp.id()));
            let mut ctx = TickCtx {
                world,
                state: &mut state,
                year,
                rng: &mut lrng,
            };
            lp.tick(&mut ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A loop that records `(id, year, first-draw)` per tick into a shared log —
    /// lets the tests observe execution count, ordering, and the exact RNG seed
    /// each loop received.
    struct Recorder {
        id: LoopId,
        log: Rc<RefCell<Vec<(LoopId, i32, u64)>>>,
    }

    impl CausalLoop for Recorder {
        fn id(&self) -> LoopId {
            self.id
        }
        fn tick(&mut self, ctx: &mut TickCtx) {
            self.log
                .borrow_mut()
                .push((self.id, ctx.year, ctx.rng.next_u64()));
        }
    }

    /// Drive `ids` for `years` years from a fixed base seed and return the log.
    fn drive_and_record(ids: &[LoopId], years: i32) -> Vec<(LoopId, i32, u64)> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let loops: Vec<Box<dyn CausalLoop>> = ids
            .iter()
            .map(|&id| {
                Box::new(Recorder {
                    id,
                    log: log.clone(),
                }) as Box<dyn CausalLoop>
            })
            .collect();
        let mut world = WorldData::default();
        let mut rng = ChaCha8Rng::seed_from_u64(0xC0FF_EE00);
        run_with_loops(&mut world, HistoryParams { years }, &mut rng, loops);
        Rc::try_unwrap(log).unwrap().into_inner()
    }

    #[test]
    fn driver_ticks_each_loop_once_per_year_in_order() {
        let log = drive_and_record(&[LoopId::Turchin, LoopId::Mearsheimer], 10);
        assert_eq!(log.len(), 20, "2 loops x 10 years = 20 ticks");
        assert_eq!(
            log.iter()
                .filter(|(id, _, _)| *id == LoopId::Turchin)
                .count(),
            10
        );
        // Turchin (first in the set) ticks years 0..10 in order.
        let years: Vec<i32> = log
            .iter()
            .filter(|(id, _, _)| *id == LoopId::Turchin)
            .map(|(_, y, _)| *y)
            .collect();
        assert_eq!(years, (0..10).collect::<Vec<_>>());
        // Within a year, Turchin runs before Mearsheimer (ORDER preserved).
        assert_eq!(log[0].0, LoopId::Turchin);
        assert_eq!(log[1].0, LoopId::Mearsheimer);
    }

    #[test]
    fn loop_seed_is_pure_distinct_per_loop_and_year() {
        let sim = 0xDEAD_BEEF_u64;
        assert_eq!(
            loop_seed(sim, 5, LoopId::Turchin),
            loop_seed(sim, 5, LoopId::Turchin),
            "same inputs must give the same seed"
        );
        assert_ne!(
            loop_seed(sim, 5, LoopId::Turchin),
            loop_seed(sim, 5, LoopId::Mearsheimer),
            "distinct loops must get distinct seeds in the same year"
        );
        assert_ne!(
            loop_seed(sim, 5, LoopId::Turchin),
            loop_seed(sim, 6, LoopId::Turchin),
            "the same loop must get distinct seeds across years"
        );
    }

    #[test]
    fn a_loops_stream_is_unaffected_by_adding_other_loops() {
        // The core determinism property: Mearsheimer's per-year seed is
        // identical whether it runs alone or alongside other loops, because the
        // seed is derived from its own id, not from a shared advancing stream.
        let alone = drive_and_record(&[LoopId::Mearsheimer], 5);
        let crowded = drive_and_record(&[LoopId::Turchin, LoopId::Mearsheimer, LoopId::Hero], 5);
        let pick = |log: &[(LoopId, i32, u64)]| -> Vec<u64> {
            log.iter()
                .filter(|(id, _, _)| *id == LoopId::Mearsheimer)
                .map(|(_, _, s)| *s)
                .collect()
        };
        assert_eq!(
            pick(&alone),
            pick(&crowded),
            "adding loops around Mearsheimer perturbed its RNG stream"
        );
    }

    #[test]
    fn order_constant_matches_default_loops() {
        let ids: Vec<LoopId> = default_loops().iter().map(|l| l.id()).collect();
        assert_eq!(ids, loops::ORDER.to_vec());
    }
}

//! Causal loops driving history. Each loop implements [`CausalLoop`] and is
//! advanced once per simulated year by the driver in [`crate::run`].
//!
//! Phase 4's full scope runs all six loops (the MVP brief shipped two live and
//! stubbed four). In Phase 4a they are no-op skeletons: they establish the
//! trait, the fixed execution order, and the determinism contract before any
//! economics land in 4b+.

use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

use crate::SimState;

pub mod colonization;
pub mod diffusion;
pub mod hero;
pub mod khaldun;
pub mod mearsheimer;
pub mod schism;
pub mod succession;
pub mod trade;
pub mod turchin;

/// Stable identifier for each causal loop. Like [`mapgen_core::Stage`], the
/// discriminants are part of the determinism contract: they seed each loop's
/// per-year RNG sub-stream, so never renumber an existing loop — only append.
/// A loop that draws zero randoms still *derives* (not advances) its seed slot,
/// so adding a new loop cannot shift an existing loop's stream.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum LoopId {
    Turchin = 1,
    Khaldun = 2,
    Mearsheimer = 3,
    Succession = 4,
    Schism = 5,
    Hero = 6,
    Colonization = 7,
    Diffusion = 8,
    Trade = 9,
}

/// The fixed per-year execution order. Determinism depends on this order being
/// stable; append new loops at the end. Must match [`default_loops`].
pub const ORDER: [LoopId; 9] = [
    LoopId::Turchin,
    LoopId::Khaldun,
    LoopId::Mearsheimer,
    LoopId::Succession,
    LoopId::Schism,
    LoopId::Hero,
    LoopId::Colonization,
    LoopId::Diffusion,
    LoopId::Trade,
];

/// Everything a loop may touch during one yearly tick. The driver builds a
/// fresh `TickCtx` per (year, loop), handing the loop a per-loop RNG derived
/// from its [`LoopId`] so loops are mutually independent.
pub struct TickCtx<'a> {
    /// The world being mutated — loops read prior-stage state and write
    /// events / entities here (no-op in Phase 4a).
    pub world: &'a mut WorldData,
    /// Cross-loop scratch state, discarded after the sim (never serialized).
    pub state: &'a mut SimState,
    /// The current simulated year, in `0..years`.
    pub year: i32,
    /// This loop's per-year RNG sub-stream (seeded from its [`LoopId`]).
    pub rng: &'a mut ChaCha8Rng,
}

/// A causal driver of history. One `tick` = one simulated year.
pub trait CausalLoop {
    /// Stable identity — seeds this loop's RNG sub-stream.
    fn id(&self) -> LoopId;
    /// Advance one year, mutating the world and scratch state via `ctx`.
    fn tick(&mut self, ctx: &mut TickCtx);
}

/// The six loops in canonical [`ORDER`]. Boxed so the driver iterates them
/// uniformly; the Vec order must match `ORDER`.
pub fn default_loops() -> Vec<Box<dyn CausalLoop>> {
    let loops: Vec<Box<dyn CausalLoop>> = vec![
        Box::new(turchin::Turchin),
        Box::new(khaldun::Khaldun),
        Box::new(mearsheimer::Mearsheimer),
        Box::new(succession::Succession),
        Box::new(schism::Schism),
        Box::new(hero::Hero),
        Box::new(colonization::Colonization),
        Box::new(diffusion::Diffusion),
        Box::new(trade::Trade),
    ];
    loops
}

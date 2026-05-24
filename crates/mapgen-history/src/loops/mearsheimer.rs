//! Mearsheimer offensive realism + Allison's Thucydides trap. Power-transition
//! wars ignite when a rising power closes on a dominant neighbour.
//!
//! Phase 4a: no-op skeleton. Populated in 4e (power graph + wars + claims).

use crate::loops::{CausalLoop, LoopId, TickCtx};

/// Mearsheimer power-balance loop.
pub struct Mearsheimer;

impl CausalLoop for Mearsheimer {
    fn id(&self) -> LoopId {
        LoopId::Mearsheimer
    }
    fn tick(&mut self, _ctx: &mut TickCtx) {}
}

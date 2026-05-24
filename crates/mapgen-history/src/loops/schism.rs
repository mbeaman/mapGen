//! Religious schism. Alignment drift between adherent cultures splits a faith
//! into a heterodox sect.
//!
//! Phase 4a: no-op skeleton. Populated in 4g.

use crate::loops::{CausalLoop, LoopId, TickCtx};

/// Religious-schism loop.
pub struct Schism;

impl CausalLoop for Schism {
    fn id(&self) -> LoopId {
        LoopId::Schism
    }
    fn tick(&mut self, _ctx: &mut TickCtx) {}
}

//! Turchin demographic-fiscal secular cycle. Rise/collapse from population
//! pressure and elite overproduction.
//!
//! Phase 4a: no-op skeleton. Populated in 4b (demographic backbone →
//! famine/plague/drought) and 4d (fiscal half + instability).

use crate::loops::{CausalLoop, LoopId, TickCtx};

/// Turchin structural-demographic loop.
pub struct Turchin;

impl CausalLoop for Turchin {
    fn id(&self) -> LoopId {
        LoopId::Turchin
    }
    fn tick(&mut self, _ctx: &mut TickCtx) {}
}

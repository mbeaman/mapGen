//! Dynastic succession crises. Contested heirs on a ruler's death trigger
//! succession wars and activate dormant claims.
//!
//! Phase 4a: no-op skeleton. Populated in 4f.

use crate::loops::{CausalLoop, LoopId, TickCtx};

/// Succession-crisis loop.
pub struct Succession;

impl CausalLoop for Succession {
    fn id(&self) -> LoopId {
        LoopId::Succession
    }
    fn tick(&mut self, _ctx: &mut TickCtx) {}
}

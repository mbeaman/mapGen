//! Ibn Khaldun asabiyyah dynasty cycle. Group cohesion rises on the frontier
//! and decays in the metropole over a few generations.
//!
//! Phase 4a: no-op skeleton. Populated in 4d (asabiyyah + dynasty decline).

use crate::loops::{CausalLoop, LoopId, TickCtx};

/// Ibn Khaldun asabiyyah loop.
pub struct Khaldun;

impl CausalLoop for Khaldun {
    fn id(&self) -> LoopId {
        LoopId::Khaldun
    }
    fn tick(&mut self, _ctx: &mut TickCtx) {}
}

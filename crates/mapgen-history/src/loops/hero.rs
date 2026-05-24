//! Heroic / megabeast loop. Rare champions and monsters, artifacts, and the
//! prophecies that frame them.
//!
//! Phase 4a: no-op skeleton. Populated in 4h.

use crate::loops::{CausalLoop, LoopId, TickCtx};

/// Hero / megabeast loop.
pub struct Hero;

impl CausalLoop for Hero {
    fn id(&self) -> LoopId {
        LoopId::Hero
    }
    fn tick(&mut self, _ctx: &mut TickCtx) {}
}

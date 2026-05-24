//! Ibn Khaldun asabiyyah dynasty cycle. Group cohesion is high for a young
//! dynasty forged in hardship and decays as the dynasty grows urbane and
//! decadent over generations; a low-cohesion dynasty governs poorly, which the
//! Turchin loop reads as amplified instability.
//!
//! Phase 4d: state only — Khaldun maintains `asabiyyah`; its visible effect is
//! harsher secular crises (Turchin reads it). Frontier-vigor renewal beyond a
//! fresh dynasty is left for later.

use crate::loops::{CausalLoop, LoopId, TickCtx};

/// Asabiyyah a freshly founded dynasty starts with.
const ASAB_HIGH: f32 = 0.9;
/// Multiplicative yearly decay of cohesion within a stable dynasty (~0.5 after
/// ~140 years), modeling the rise of urbane decadence (Ibn Khaldun, *Muqaddimah*).
const ASAB_DECAY: f32 = 0.9955;

/// Ibn Khaldun asabiyyah loop.
pub struct Khaldun;

impl CausalLoop for Khaldun {
    fn id(&self) -> LoopId {
        LoopId::Khaldun
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        for pid in 0..ctx.state.asabiyyah.len() {
            let current = ctx.state.courts.get(pid).and_then(|c| c.dynasty);
            if current != ctx.state.last_dynasty[pid] {
                // A new dynasty (or the founding one) — cohesion is renewed.
                ctx.state.asabiyyah[pid] = ASAB_HIGH;
                ctx.state.last_dynasty[pid] = current;
            } else {
                // Stable dynasty: cohesion only decays (monotonic). Pure
                // multiplication — no transcendental, native ↔ wasm32 identical.
                ctx.state.asabiyyah[pid] = (ctx.state.asabiyyah[pid] * ASAB_DECAY).clamp(0.0, 1.0);
            }
        }
    }
}

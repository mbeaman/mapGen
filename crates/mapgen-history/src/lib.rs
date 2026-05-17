//! Deterministic agent-based history simulation. Produces a typed, append-only
//! event log that the rendering and lore layers consume.

pub mod loops;

use mapgen_core::WorldData;

/// Run `years` of history sim against `world` in place.
/// Phase 1: no-op until Phase 4 lands.
pub fn run(world: &mut WorldData, years: i32) {
    let _ = (world, years);
}

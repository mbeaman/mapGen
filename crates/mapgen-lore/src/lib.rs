//! Claude integration. Build with `cargo` on a native target only. On wasm32
//! the crate is intentionally empty so the WASM binary cannot leak API
//! credentials.

#![cfg(not(target_arch = "wasm32"))]

use mapgen_core::WorldData;

/// Placeholder until Phase 5. Will return a Claude-narrated `Work` entity.
pub fn narrate(_world: &WorldData, _event_id: u32) -> anyhow::Result<()> {
    Ok(())
}

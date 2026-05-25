//! Claude integration — the lore engine that narrates the history sim's event
//! log into grounded, in-world chronicles. Build with `cargo` on a native target
//! only. On wasm32 the crate is intentionally empty so the WASM binary cannot
//! leak API credentials.
//!
//! **Opt-in.** Narration is never part of `generate_full`; it is an explicit
//! step (`mapgen lore`). The deterministic [`template`] narrator runs offline
//! with no key. The real Anthropic client (Phase 5e) is gated behind the `lore`
//! Cargo feature *and* a runtime API key, so a stock build can make no paid call.

#![cfg(not(target_arch = "wasm32"))]

pub mod client;
pub mod schema;
pub mod template;
pub mod voice;

pub use client::LlmClient;
pub use schema::{ChronicleDraft, SCHEMA_HINT};
pub use voice::{Register, VoiceCard};

use mapgen_core::WorldData;

/// Placeholder — the full `narrate()` orchestration (prompt → client → NER
/// validate → retry → template fallback → persist `Work`) lands in 5c.
pub fn narrate(_world: &WorldData, _event_id: u32) -> anyhow::Result<()> {
    Ok(())
}

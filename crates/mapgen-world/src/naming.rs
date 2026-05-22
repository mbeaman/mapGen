//! Naming stage. Phase 3d.
//!
//! Builds a per-culture `Language` (one per culture; MVP doesn't yet
//! model dialect splits or sound-change-rule cognates — see
//! BACKLOG.md), then generates phonotactic names for settlements,
//! polities, and religions. The CSV-authored `Culture.name` strings
//! (e.g., "Riverfolk", "Iron Hold") are kept as the *exonym* — what
//! outsiders call this culture — while the language-generated names
//! supply the in-world place / institution names.
//!
//! Pipeline position: runs after `polities::lay_out` (settlements need
//! to exist before they can be named) and before any history /
//! lore stage.
//!
//! Spec: `crates/mapgen-world/tests/naming_spec.rs`.
//!
//! Architecture: `docs/ARCHITECTURE.md` Phase 3d — "Phonotactic
//! generator + Markov fallback, per `Language`." The Markov fallback
//! is BACKLOGGED; MVP uses pure phonotactic.

use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

/// Tunables for the naming stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct NamingParams {
    /// Maximum number of attempts at generating a unique name before
    /// giving up and accepting a collision. Settlements with collisions
    /// would otherwise be indistinguishable.
    pub uniqueness_attempts: usize,
}

impl Default for NamingParams {
    fn default() -> Self {
        Self {
            uniqueness_attempts: 8,
        }
    }
}

/// Generate languages + rename settlements, polities, and religions
/// with their respective cultures' generated phonotactic names.
///
/// Preconditions: `world.cultures.cultures` populated, `world.society
/// .settlements` and `world.society.nations` populated (Phase 3c must
/// have run), `world.religions.religions` populated (Phase 3b must
/// have run).
pub fn name_world(_world: &mut WorldData, _params: NamingParams, _rng: &mut ChaCha8Rng) {
    todo!("Phase 3d: naming::name_world not yet implemented — see naming_spec.rs")
}

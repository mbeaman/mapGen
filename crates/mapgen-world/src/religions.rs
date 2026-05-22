//! Religions stage. Phase 3b.
//!
//! Founds 1-3 religions per world. Each religion is tied to a founder
//! culture, takes that culture's alignment as its baseline, and picks a
//! `PantheonPattern` based on the founder's `MagicStyle` and `TechEra`.
//! Religions then spread to neighboring cultures whose alignment is
//! similar enough; each cell with a culture gets the religion whose
//! founder is closest in alignment. Sacred sites land on biome-matched
//! cells inside the founder's territory.
//!
//! Pipeline position: runs after `cultures::populate` (needs the culture
//! roster + per-cell assignment) and before any polities stage (which
//! the architecture treats as orthogonal to religion).
//!
//! Spec: `crates/mapgen-world/tests/religions_spec.rs`.
//!
//! Architecture: `docs/ARCHITECTURE.md` §4 Phase 3b.

use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

/// Tunables for the religions stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct ReligionsParams {
    /// Cap on how many religions can be founded in a single world.
    /// Architecture: "1-3 religions per world." The actual count is
    /// bounded above by this and below by the number of cultures.
    pub max_religions: usize,
    /// Maximum Euclidean distance in 2D alignment-space (law-chaos ×
    /// good-evil, each in `[-1, 1]`) at which a culture is considered
    /// "alignment-compatible" with a founder. Cultures beyond this
    /// distance from every existing religion's founder get the closest
    /// one anyway (no cell stays unconverted).
    pub alignment_spread_radius: f32,
}

impl Default for ReligionsParams {
    fn default() -> Self {
        Self {
            max_religions: 3,
            // Both axes are `[-1, 1]`; the diagonal of that square is
            // ~2.83. `1.5` lets a religion plausibly span half the
            // alignment plane but doesn't homogenize.
            alignment_spread_radius: 1.5,
        }
    }
}

/// Populate `world.religions` — roster + per-cell adherence assignment +
/// sacred-site placement.
///
/// Preconditions: `world.cultures` must be populated (the religions
/// stage runs after `cultures::populate` in `generate_full`); biome
/// data on `world.climate.biome` must exist (for sacred-site placement).
pub fn found(_world: &mut WorldData, _params: ReligionsParams, _rng: &mut ChaCha8Rng) {
    todo!("Phase 3b: religions::found not yet implemented — see religions_spec.rs")
}

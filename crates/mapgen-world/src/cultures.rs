//! Cultures stage. Per-cell habitat-fitness scoring across a roster of 4-5
//! MVP race archetypes (Phase 3a MVP; the 14-archetype roster lands as
//! `crates/mapgen-world/data/race_archetypes.csv` and is loaded as data, not
//! compiled in). Weighted Voronoi assignment writes `culture_id` per land
//! cell; sea cells stay `None`.
//!
//! Pipeline position: runs after `biomes::classify` (needs biome + climate +
//! hydrology) and before any polities stage (which keys off culture).
//!
//! Spec: `crates/mapgen-world/tests/cultures_spec.rs`.
//!
//! Architecture: `docs/ARCHITECTURE.md` §3 (data flow), §4 Phase 3a (exit
//! criteria), §5.5 "What races want" (archetype-to-habitat utility table).

use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

/// Tunables for the cultures stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct CulturesParams {
    /// Target number of cultures per world. Subject to habitat availability —
    /// a world with no high-mountain cells produces no Dwarf (Mountain)
    /// culture even if the target permits it.
    pub target_cultures: usize,
    /// Minimum mean habitat-fitness for a culture's assigned cells. Cultures
    /// whose mean fitness falls below this threshold are discarded and their
    /// cells reassigned to the next-best candidate. Per ARCHITECTURE.md §4
    /// Phase 3a: "no culture's average habitat-score below 0.3."
    pub min_habitat_fitness: f32,
}

impl Default for CulturesParams {
    fn default() -> Self {
        Self {
            target_cultures: 5,
            min_habitat_fitness: 0.3,
        }
    }
}

/// Populate `world.cultures` — roster + per-cell assignment.
///
/// Writes `world.cultures.cultures` (the roster) and
/// `world.cultures.culture_id` (per-cell back-reference, `Some(u16)` for land
/// cells the stage assigned, `None` otherwise).
///
/// Preconditions: `world.mesh`, `world.terrain`, `world.climate`, and
/// `world.hydrology` must be populated. Biome classification (`world.climate
/// .biome`) is required since archetype habitat scores key off biome.
pub fn populate(_world: &mut WorldData, _params: CulturesParams, _rng: &mut ChaCha8Rng) {
    todo!("Phase 3a: cultures::populate not yet implemented — see cultures_spec.rs")
}

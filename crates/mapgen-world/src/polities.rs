//! Polities stage. Phase 3c.
//!
//! Suitability-weighted capital placement (1 per culture in the MVP),
//! Christaller-style hierarchy of towns and villages around each
//! capital, A*-routed roads with reuse-discount producing a trunk-and-
//! branch network. Per ARCHITECTURE.md §4 Phase 3c: "Suitability-
//! weighted Poisson capitals filtered by `Culture.settlement_preference`;
//! Christaller k=4 hierarchy; A* roads with reuse discount."
//!
//! Pipeline position: runs after `religions::found` (settlements are
//! tagged with the religion of their cell). Writes:
//!
//! * `world.society.nations` — polity roster (one polity per culture).
//! * `world.society.settlements` — capitals + towns sorted by tier.
//! * `world.society.roads` — `Vec<Road>`, one road per
//!   non-capital-settlement-to-parent connection.
//! * `world.society.control` — per-cell polity id.
//!
//! Spec: `crates/mapgen-world/tests/polities_spec.rs`.

use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

/// Tunables for the polities stage.
#[derive(Clone, Debug)]
pub struct PolitiesParams {
    /// Towns per capital (Christaller k=4 ≈ 3-4 dependent places per
    /// central place; MVP uses 3).
    pub towns_per_capital: usize,
    /// Minimum mesh-graph distance (in BFS hops) between capitals so
    /// they don't clump on a single high-fitness ridge.
    pub min_capital_separation: usize,
    /// Minimum mesh-graph distance between towns sharing a capital.
    pub min_town_separation: usize,
    /// Maximum BFS radius (in hops) from a capital within which towns
    /// can be placed.
    pub town_search_radius: usize,
}

impl Default for PolitiesParams {
    fn default() -> Self {
        Self {
            towns_per_capital: 3,
            min_capital_separation: 12,
            min_town_separation: 5,
            town_search_radius: 14,
        }
    }
}

/// Populate `world.society` — polity roster, settlements (capitals +
/// towns), per-cell control, and trunk-and-branch road network.
///
/// Preconditions: `world.cultures` and `world.religions` must be
/// populated (cultures provides per-cell habitat suitability via
/// `cultures::habitat_fitness`; religions provides per-cell religion
/// assignment which the renderer reads on settlement glyphs).
pub fn lay_out(_world: &mut WorldData, _params: PolitiesParams, _rng: &mut ChaCha8Rng) {
    todo!("Phase 3c: polities::lay_out not yet implemented — see polities_spec.rs")
}

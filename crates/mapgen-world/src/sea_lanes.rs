//! Sea-lanes stage — the maritime substrate of "The Sundered Lanes" arc.
//!
//! Charts a sparse graph of inter-continental sea lanes over the ocean: a
//! persisted physical layer that later civilization carriers (beachhead
//! conquest, open-ocean colonization) and the diffusion / first-contact loops
//! replay across. Each [`SeaLane`](mapgen_core::SeaLane) connects two coastal
//! anchor cells with a traversal `cost` (anisotropic — cheaper down-current /
//! down-wind) and a `min_naval` gate: the lowest naval tech that can sail it.
//! Open-ocean lanes gate high; sheltered straits gate low. `min_naval` is
//! calibrated *relative* to the achievable naval distribution so that some
//! lanes are always crossable by the best seafarers and some are always walls
//! — by construction, never an empty solution space (see the design's Step-0
//! de-risk).
//!
//! Pipeline position: runs after `biomes::classify` (needs ocean currents from
//! [`ocean`](crate::ocean) and prevailing winds from
//! [`climate_seasonal`](crate::climate_seasonal)) and before
//! [`cultures::populate`](crate::cultures::populate), which will key migration
//! pressure off the lanes in a later phase.
//!
//! Determinism: draws from `Stage::SeaLanes` only; anchor selection and
//! Dijkstra tie-breaks resolve on lowest `cell_id` per the determinism
//! contract.
//!
//! Plan of record: `docs/inter_continental_design.md`.
//! Spec: `crates/mapgen-world/tests/sea_lanes_spec.rs` (lands with the impl).

use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

/// Tunables for the sea-lanes stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct SeaLanesParams {
    /// Coastal anchor spacing — minimum ocean-graph distance between two
    /// anchors on the same landmass, in cells. Larger spacing yields a
    /// sparser, faster lane graph.
    pub anchor_spacing: u32,
    /// Fraction of the achievable-naval distribution that the cheapest
    /// inter-continental lane must sit at or below. Drives the *relative*
    /// `min_naval` calibration: guarantees at least one crossable lane for the
    /// strongest seafarers while leaving open-ocean gaps as walls.
    pub crossable_naval_quantile: f32,
}

impl Default for SeaLanesParams {
    fn default() -> Self {
        Self {
            anchor_spacing: 16,
            crossable_naval_quantile: 0.9,
        }
    }
}

/// Chart the inter-continental sea-lane graph into `world.sea_lanes`.
///
/// Stub: a no-op until the Phase-1 implementation lands (the RED spec in
/// `tests/sea_lanes_spec.rs` follows next). Leaves `world.sea_lanes` empty so
/// the stage is wired and gated without yet moving any populated golden.
pub fn chart(_world: &mut WorldData, _params: SeaLanesParams, _rng: &mut ChaCha8Rng) {
    // Intentionally empty — see module docs and `docs/inter_continental_design.md`.
}

//! Hydrology pipeline. Phase 2.
//!
//! Stages in order:
//!
//! 1. `detect_coast` — mark every cell that touches the land/sea boundary.
//! 2. `fill_depressions` — Priority-Flood (Barnes-Lehman-Mulla 2014); after
//!    this no land cell is surrounded by strictly higher neighbors. O(N log N).
//! 3. `flow_directions` — every cell points to its steepest-descent neighbor,
//!    or `None` at sea / sink. Returns one entry per cell.
//! 4. `accumulate_flow` — topological sweep (by descending elevation) summing
//!    upstream flow into downstream cells. Writes `world.hydrology.flow`.
//! 5. `extract_rivers` — cells over `flow_threshold` become rivers; width ∝
//!    √flow. Depressions raised by step 2 become lakes at spill elevation.

use mapgen_core::WorldData;

/// Mark `world.mesh.coast[i] = true` iff cell i is land-with-sea-neighbor
/// or sea-with-land-neighbor.
pub fn detect_coast(_world: &mut WorldData) {
    todo!("Phase 2: coast detection")
}

/// Priority-Flood (Barnes-Lehman-Mulla 2014). Raises every internal
/// depression to its spill elevation so flow direction is well-defined
/// everywhere.
pub fn fill_depressions(_world: &mut WorldData) {
    todo!("Phase 2: Priority-Flood depression fill")
}

/// Steepest-descent flow direction. Returns one entry per cell: `Some(j)`
/// where j is the lower-elevation neighbor, `None` for sea cells or
/// sinks raised by `fill_depressions`.
pub fn flow_directions(_world: &WorldData) -> Vec<Option<u32>> {
    todo!("Phase 2: steepest-descent flow direction")
}

/// Topo-sort cells by elevation and accumulate runoff downstream.
/// Writes per-cell flow into `world.hydrology.flow`.
pub fn accumulate_flow(_world: &mut WorldData, _flow_dir: &[Option<u32>]) {
    todo!("Phase 2: flow accumulation")
}

/// Extract rivers (cells over the threshold) and lakes (depressions
/// raised by `fill_depressions`). Writes to `world.hydrology.rivers` and
/// `world.hydrology.lakes`.
pub fn extract_rivers(_world: &mut WorldData, _flow_dir: &[Option<u32>], _flow_threshold: f32) {
    todo!("Phase 2: river + lake extraction")
}

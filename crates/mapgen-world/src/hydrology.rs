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

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use mapgen_core::{
    fmath,
    world_data::{Lake, River, WorldData},
};

/// Min-heap entry: lowest elevation pops first; ties broken by cell index
/// for deterministic ordering.
#[derive(Copy, Clone, PartialEq)]
struct ElevCell {
    elev: f32,
    cell: u32,
}
impl Eq for ElevCell {}
impl Ord for ElevCell {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed: smallest elev should be "greatest" so it pops first
        // from BinaryHeap's max-heap.
        other
            .elev
            .total_cmp(&self.elev)
            .then(other.cell.cmp(&self.cell))
    }
}
impl PartialOrd for ElevCell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Mark `world.mesh.coast[i] = true` iff cell i is land-with-sea-neighbor
/// or sea-with-land-neighbor.
pub fn detect_coast(world: &mut WorldData) {
    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;
    let neighbors = &world.mesh.neighbors;
    let mut coast = vec![false; n];
    for i in 0..n {
        let self_is_land = elev[i] > 0.0;
        for &j in &neighbors[i] {
            if (elev[j as usize] > 0.0) != self_is_land {
                coast[i] = true;
                break;
            }
        }
    }
    world.mesh.coast = coast;
}

/// Priority-Flood (Barnes-Lehman-Mulla 2014). Raises every internal
/// depression to its spill elevation so flow direction is well-defined
/// everywhere. O(N log N) via a min-heap seeded from the sea cells.
///
/// A tiny ε is added to each raised cell to guarantee a strict descent
/// gradient out of formerly-flat regions, otherwise `flow_directions`
/// would return `None` over the filled plateau.
///
/// Side effect: a cell that gets raised by *more than* a small threshold
/// is recorded as an internally-drained basin cell. After the flood
/// completes, contiguous groups of these cells are grouped into Lake
/// records, with `level` = their (already-raised) elevation.
pub fn fill_depressions(world: &mut WorldData) {
    let n = world.mesh.cell_count();
    let neighbors = &world.mesh.neighbors;
    let elev = &mut world.terrain.elevation;
    const EPS: f32 = 1e-5;
    /// Raise amount above which we treat the cell as a real lake (not
    /// just a flat-region ε offset).
    const LAKE_RAISE_THRESHOLD: f32 = 0.005;

    let mut processed = vec![false; n];
    let mut heap: BinaryHeap<ElevCell> = BinaryHeap::new();
    let mut raised = vec![false; n]; // cells filled enough to count as a lake

    // Seed from sea cells — water can always drain there.
    for i in 0..n {
        if elev[i] <= 0.0 {
            processed[i] = true;
            heap.push(ElevCell {
                elev: elev[i],
                cell: i as u32,
            });
        }
    }

    while let Some(ElevCell {
        elev: spill,
        cell: c,
    }) = heap.pop()
    {
        for &nbr in &neighbors[c as usize] {
            let j = nbr as usize;
            if processed[j] {
                continue;
            }
            processed[j] = true;
            let original = elev[j];
            if elev[j] <= spill {
                elev[j] = spill + EPS;
                if elev[j] - original > LAKE_RAISE_THRESHOLD {
                    raised[j] = true;
                }
            }
            heap.push(ElevCell {
                elev: elev[j],
                cell: nbr,
            });
        }
    }

    // Group contiguous raised cells into Lake records via BFS.
    let mut visited = vec![false; n];
    let mut lakes: Vec<Lake> = Vec::new();
    for start in 0..n {
        if !raised[start] || visited[start] {
            continue;
        }
        let mut queue = vec![start as u32];
        visited[start] = true;
        let mut group: Vec<u32> = Vec::new();
        while let Some(c) = queue.pop() {
            group.push(c);
            for &nbr in &neighbors[c as usize] {
                let j = nbr as usize;
                if !visited[j] && raised[j] {
                    visited[j] = true;
                    queue.push(nbr);
                }
            }
        }
        // Lake level: max elevation of its cells (= spill point).
        let level = group
            .iter()
            .map(|&c| world.terrain.elevation[c as usize])
            .fold(f32::NEG_INFINITY, f32::max);
        lakes.push(Lake {
            cells: group,
            level,
            name: String::new(),
        });
    }
    world.hydrology.lakes = lakes;
}

/// Steepest-descent flow direction. Returns one entry per cell: `Some(j)`
/// where j is the lowest neighbor strictly below this cell, `None` for
/// sea cells or unresolved local minima.
pub fn flow_directions(world: &WorldData) -> Vec<Option<u32>> {
    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;
    let neighbors = &world.mesh.neighbors;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if elev[i] <= 0.0 {
            out.push(None);
            continue;
        }
        let mut best: Option<(f32, u32)> = None;
        for &j in &neighbors[i] {
            let e = elev[j as usize];
            if e < elev[i] {
                match best {
                    None => best = Some((e, j)),
                    Some((b, bj)) => {
                        // Strictly lower wins; tie-break by lower cell index
                        // for determinism.
                        if e < b || (e == b && j < bj) {
                            best = Some((e, j));
                        }
                    }
                }
            }
        }
        out.push(best.map(|(_, j)| j));
    }
    out
}

/// Topo-sort cells by descending elevation and accumulate unit runoff
/// downstream. Writes per-cell flow into `world.hydrology.flow`. After
/// this call, `flow[downstream] >= flow[upstream]` along every flow edge.
pub fn accumulate_flow(world: &mut WorldData, flow_dir: &[Option<u32>]) {
    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;

    // Each cell starts with 1.0 unit of precipitation. Process highest
    // elevation first so flow accumulates as we descend.
    let mut flow = vec![1.0_f32; n];
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by(|&a, &b| elev[b as usize].total_cmp(&elev[a as usize]));

    for &c in &order {
        if let Some(downstream) = flow_dir[c as usize] {
            flow[downstream as usize] += flow[c as usize];
        }
    }

    world.hydrology.flow = flow;
}

/// Extract rivers as polylines from physical headwaters down to sea or
/// lake. Realism rules (calibrated against the user's audit):
///
/// * **Adaptive threshold** — top ~12% of land cells by flow
///   accumulation become river-eligible. A fixed numeric threshold does
///   not generalize across world sizes / climates; with 6k-cell worlds
///   and unit-precip flow accumulation, 0.05 included nearly every
///   land cell.
/// * **Physical headwaters** — a river only starts at a cell whose
///   elevation is high enough to be a highland (≥ 0.30 normalized) OR
///   that is adjacent to a lake (lake-outlet origin). Lowland creeks
///   that spontaneously appear in flat country are dropped.
/// * **Minimum length** — rivers under 3 cells are dropped (creeks, not
///   rivers).
/// * **Width** — per-river `width` field stores the maximum flow along
///   the chain; the renderer additionally varies segment width per-cell
///   from `world.hydrology.flow[cell]` so the river grows visibly at
///   confluences.
///
/// The `_flow_threshold` argument is retained for API compatibility but
/// is now ignored — the adaptive percentile rule supersedes it.
pub fn extract_rivers(world: &mut WorldData, flow_dir: &[Option<u32>], _flow_threshold: f32) {
    use std::collections::BTreeSet;

    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;
    let flow = &world.hydrology.flow;
    let neighbors = &world.mesh.neighbors;

    // 1. Adaptive threshold.
    let mut land_flows: Vec<f32> = (0..n).filter(|&i| elev[i] > 0.0).map(|i| flow[i]).collect();
    if land_flows.is_empty() {
        world.hydrology.rivers = Vec::new();
        return;
    }
    land_flows.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let cutoff_idx = ((land_flows.len() as f32) * 0.12) as usize;
    let adaptive_threshold = land_flows.get(cutoff_idx).copied().unwrap_or(0.0).max(2.0);

    // 2. Mark river-eligible cells.
    let mut is_river = vec![false; n];
    for i in 0..n {
        if elev[i] > 0.0 && flow[i] >= adaptive_threshold {
            is_river[i] = true;
        }
    }

    // 3. Lake cell index for headwater-at-outlet rule.
    let lake_cells: BTreeSet<u32> = world
        .hydrology
        .lakes
        .iter()
        .flat_map(|l| l.cells.iter().copied())
        .collect();

    // 4. Headwaters = river cells with no upstream river cell.
    let mut incoming = vec![0u32; n];
    for c in 0..n {
        if !is_river[c] {
            continue;
        }
        if let Some(d) = flow_dir[c] {
            if is_river[d as usize] {
                incoming[d as usize] += 1;
            }
        }
    }

    let mut rivers = Vec::new();
    for start in 0..n {
        if !is_river[start] || incoming[start] != 0 {
            continue;
        }
        // Physical-origin rule.
        let is_highland = elev[start] >= 0.30;
        let is_lake_outlet = neighbors[start].iter().any(|&j| lake_cells.contains(&j));
        if !is_highland && !is_lake_outlet {
            continue;
        }

        let mut chain = vec![start as u32];
        let mut c = start;
        while let Some(d) = flow_dir[c] {
            chain.push(d);
            c = d as usize;
            if elev[c] <= 0.0 {
                break;
            }
        }
        if chain.len() < 3 {
            continue;
        }

        let max_flow = chain
            .iter()
            .map(|&i| flow[i as usize])
            .fold(0.0_f32, f32::max);
        let width = fmath::sqrt(max_flow) * 0.05;
        rivers.push(River {
            cells: chain,
            width,
            name: String::new(),
        });
    }

    world.hydrology.rivers = rivers;
    // `fill_depressions` already populated `lakes`; don't clobber.
}

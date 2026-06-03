//! Sea-lanes stage — the maritime substrate of "The Sundered Lanes" arc.
//!
//! Charts a sparse graph of inter-continental sea lanes over the ocean: a
//! persisted physical layer that later civilization carriers (beachhead
//! conquest, open-ocean colonization) and the diffusion / first-contact loops
//! replay across. Each [`SeaLane`](mapgen_core::SeaLane) connects two coastal
//! anchor *land* cells `a < b` (so a carrier can seize one directly as a
//! `BorderChange`) with a sea-path `cost` and a `min_naval` gate: the lowest
//! naval tech that can sail it.
//!
//! **Calibration (the load-bearing solvability fix).** `min_naval` is a *pure
//! function of the lane's own cost* — a fixed global curve, identical on every
//! seed (see `min_naval_for_cost`). It is anchored at the roster's ordinary
//! naval ceiling (`NAVAL_KNEE` = 40, the Riverfolk max) over the Step-0
//! crossable-gap boundary (`GAP_KNEE` = 100 world units): gaps ≲100 are
//! reachable by the strongest ordinary seafarers, gaps ≳400 by no one. Because
//! the *curve* spans both a crossable tier (≤40) and a wall tier (>40), a
//! non-empty crossable set and a non-empty wall set exist *by construction* —
//! but any single world's realized lanes may all fall in one tier (a sundered
//! seed is earned, not a bug). The curve **never** reads the world's own cost
//! distribution: a per-world quantile would make every seed crossable and is
//! the exact Refinery-Goodhart trap the design cut.
//!
//! Pipeline position: runs after `biomes::classify` (it needs only geography:
//! sea cells + sites) and before [`cultures::populate`](crate::cultures), so
//! anchors are pure geometry and `min_naval` keys off the *static* archetype
//! naval roster, not realized per-world placement.
//!
//! Determinism: coastal seeding (ascending cell id) and the Dijkstra watershed
//! resolve ties on the lowest `cell_id`; the priority queue keys on
//! `(cost.to_bits(), cell_id)` (costs are non-negative, so `to_bits` is
//! order-preserving; re-pushes strictly decrease, so no cell ties itself). All
//! transcendentals go through `fmath` — lane data is on the hashed, native↔wasm
//! determinism-checked path. `cost` is `#[serde(skip)]` (nothing reads it
//! post-gen), so only `(a, b, min_naval)` reach the golden.
//!
//! Plan of record: `docs/inter_continental_design.md`.
//! Spec: `crates/mapgen-world/tests/sea_lanes_spec.rs` + the in-module curve test.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use mapgen_core::{fmath, SeaLane, SeaLanesData, WorldData};
use rand_chacha::ChaCha8Rng;

use crate::naming::connected_bodies;

/// The roster's ordinary naval ceiling (Riverfolk = 40 in
/// `data/race_archetypes.csv`; the rest are 5–25). The cost→naval curve is
/// anchored here so a world's strongest *ordinary* seafarers can just cross a
/// `GAP_KNEE`-unit gap; a rare seafarer archetype (naval ~75) would extend the
/// reachable band beyond it without moving the knee.
const NAVAL_KNEE: f32 = 40.0;

/// The Step-0 crossable-gap boundary, in world units (the de-risk probe found
/// inter-body straits of ~20–70 and open-ocean walls of ~400–1500 coexisting on
/// canonical planet seeds). A gap of this size maps to exactly `NAVAL_KNEE`.
const GAP_KNEE: f32 = 100.0;

/// Tunables for the sea-lanes stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct SeaLanesParams {
    /// Smallest landmass (in cells) that earns sea lanes — excludes islets so
    /// the graph connects continents, not specks.
    pub min_body_cells: usize,
}

impl Default for SeaLanesParams {
    fn default() -> Self {
        Self { min_body_cells: 24 }
    }
}

/// Map a lane's sea-path `cost` (world units) to its `min_naval` gate — the
/// fixed global calibration curve. Pure function of `cost` alone (the
/// anti-Goodhart litmus): linear through the origin with slope
/// `NAVAL_KNEE / GAP_KNEE` (= 0.4), clamped to `u8`. Monotone non-decreasing, so
/// narrower gaps gate lower. A gap of `GAP_KNEE` maps to `NAVAL_KNEE`; a
/// 400-unit wall maps to 160 (beyond any culture's reach, seafarer included).
fn min_naval_for_cost(cost: f32) -> u8 {
    let naval = cost * (NAVAL_KNEE / GAP_KNEE);
    // fmath::round for house-style determinism on the hashed path.
    let q = fmath::round(naval.max(0.0));
    if q >= 255.0 {
        255
    } else {
        q as u8
    }
}

/// Straight-line distance between two cell sites (`fmath::sqrt` for house-style
/// determinism on the hashed path).
fn euclid(p: [f32; 2], q: [f32; 2]) -> f32 {
    let dx = p[0] - q[0];
    let dy = p[1] - q[1];
    fmath::sqrt(dx * dx + dy * dy)
}

/// Chart the inter-continental sea-lane graph into `world.sea_lanes`.
///
/// Isotropic substrate (Step 3a): a multi-source Dijkstra "watershed" over the
/// sea subgraph grows a basin from *every coastal land cell* (labelled by its
/// own cell + landmass), so a sea cell's `dist` is the true shortest sea path
/// from the nearest coast — not from a downsampled anchor, which would displace
/// the cost and silently flip real straits into walls. A crossing between two
/// landmasses is detected two ways and the cheapest per landmass pair is kept:
/// where two basins meet on a sea–sea edge (open-water crossings), and where a
/// labelled sea cell is directly coastal to a *foreign* landmass (single-cell
/// pinches, which the sea–sea scan alone misses). Each lane's endpoints `a < b`
/// are the two coastal *land* cells (seizable by a carrier as a `BorderChange`),
/// gated by [`min_naval_for_cost`]. Anisotropic (wind-aware) cost lands in Step
/// 3b. Pure geometry — `rng` is unused (the stage owns a stream for future
/// stochastic lanes; drawing it perturbs no other stage).
pub fn chart(world: &mut WorldData, params: SeaLanesParams, _rng: &mut ChaCha8Rng) {
    let n = world.mesh.cell_count();
    if n == 0 || world.terrain.elevation.len() != n {
        world.sea_lanes = SeaLanesData::default();
        return;
    }

    // 1. Sizable landmasses → a body index per kept land cell (islets excluded).
    let bodies = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] > 0.0);
    let elev = &world.terrain.elevation;
    let sites = &world.mesh.sites;
    let neighbors = &world.mesh.neighbors;
    // Navigable water is any cell at/below sea level. NOTE: this also admits
    // inland lakes / sub-sea-level basins; a lake touching two landmasses would
    // bridge them (a lake touching one body just labels its cells with that body
    // and forms no crossing). This is latent, not live — pinned by
    // `sea_lanes_spec::only_the_open_ocean_bridges_landmasses_no_inland_pool_does`,
    // which asserts that on every canonical seed the only sea component adjacent
    // to ≥2 sizable bodies is the dominant ocean. The fix (restrict to the ocean)
    // is deferred because a strait and a bridging-lake are topologically identical
    // here, so it needs real ocean-connectivity geometry; the tripwire fires if a
    // real lake ever bridges two continents.
    let is_sea = |i: usize| elev[i] <= 0.0;

    let mut body_of = vec![usize::MAX; n];
    let mut kept = 0usize;
    for comp in &bodies {
        if comp.len() < params.min_body_cells {
            continue;
        }
        for &c in comp {
            body_of[c] = kept;
        }
        kept += 1;
    }
    if kept < 2 {
        world.sea_lanes = SeaLanesData::default();
        return;
    }

    // 2. Multi-source Dijkstra over sea cells. Every coastal land cell (a kept
    // land cell touching sea) seeds its adjacent sea cells at the land→sea hop
    // distance, so a basin's `dist` is the full sea path from its coast. Seeding
    // in ascending cell id means the lower-id coast wins exact-cost ties (strict
    // `<`). The heap key `(cost_bits, cell)` is a total order (costs ≥ 0 ⇒
    // `to_bits` is order-preserving; re-pushes strictly decrease, so no cell ever
    // ties itself), so settle order — and every basin label — is deterministic
    // across native↔wasm.
    let mut dist = vec![f32::INFINITY; n];
    let mut src_coast = vec![u32::MAX; n]; // nearest coastal land cell
    let mut src_body = vec![usize::MAX; n];
    let mut settled = vec![false; n];
    let mut heap: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();

    for i in 0..n {
        let bi = body_of[i];
        if bi == usize::MAX {
            continue;
        }
        let si = sites[i];
        for &j in &neighbors[i] {
            let v = j as usize;
            if !is_sea(v) {
                continue;
            }
            let d = euclid(si, sites[v]);
            if d < dist[v] {
                dist[v] = d;
                src_coast[v] = i as u32;
                src_body[v] = bi;
                heap.push(Reverse((d.to_bits(), v as u32, i as u32)));
            }
        }
    }

    while let Some(Reverse((_cb, cell, _src))) = heap.pop() {
        let u = cell as usize;
        if settled[u] {
            continue;
        }
        settled[u] = true;
        let (du, su, cu, bu) = (dist[u], sites[u], src_coast[u], src_body[u]);
        for &j in &neighbors[u] {
            let v = j as usize;
            if !is_sea(v) || settled[v] {
                continue;
            }
            let nd = du + euclid(su, sites[v]);
            if nd < dist[v] {
                dist[v] = nd;
                src_coast[v] = cu;
                src_body[v] = bu;
                heap.push(Reverse((nd.to_bits(), v as u32, cu)));
            }
        }
    }

    // 3. Cheapest crossing per unordered landmass pair. `consider` keeps the
    // min-cost candidate, resolving exact ties to the lower endpoint pair (bit-
    // compare dodges the clippy float-equality lint).
    let mut best: BTreeMap<(usize, usize), (f32, u32, u32)> = BTreeMap::new();
    let mut consider = |bi: usize, bj: usize, ci: u32, cj: u32, cost: f32| {
        let (a, b) = if ci <= cj { (ci, cj) } else { (cj, ci) };
        let key = if bi < bj { (bi, bj) } else { (bj, bi) };
        let entry = best
            .entry(key)
            .or_insert((f32::INFINITY, u32::MAX, u32::MAX));
        if cost < entry.0 || (cost.to_bits() == entry.0.to_bits() && (a, b) < (entry.1, entry.2)) {
            *entry = (cost, a, b);
        }
    };

    for u in 0..n {
        let bu = src_body[u];
        if bu == usize::MAX {
            continue;
        }
        let su = sites[u];
        for &j in &neighbors[u] {
            let v = j as usize;
            // Open-water crossing: two basins meet on a sea–sea edge.
            if v > u && src_body[v] != usize::MAX && src_body[v] != bu {
                let cost = dist[u] + euclid(su, sites[v]) + dist[v];
                consider(bu, src_body[v], src_coast[u], src_coast[v], cost);
            }
            // Pinch crossing: this sea cell is directly coastal to a foreign
            // landmass (the sea–sea scan misses one-cell-wide straits).
            let bw = body_of[v];
            if bw != usize::MAX && bw != bu {
                let cost = dist[u] + euclid(su, sites[v]);
                consider(bu, bw, src_coast[u], v as u32, cost);
            }
        }
    }

    // 4. Materialize lanes (sorted by endpoints for a stable serialized order).
    let mut lanes: Vec<SeaLane> = best
        .into_values()
        .map(|(cost, a, b)| SeaLane {
            a,
            b,
            cost,
            min_naval: min_naval_for_cost(cost),
        })
        .collect();
    lanes.sort_by_key(|l| (l.a, l.b));
    world.sea_lanes = SeaLanesData { lanes };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cost→naval curve pins the DA-panel solvability finding at the unit
    /// level: it is a *pure function of cost* whose range spans both a crossable
    /// tier (≤ `NAVAL_KNEE`) and a wall tier (> seafarer reach), so both sets are
    /// non-empty by construction on every seed. A per-world quantile (the cut
    /// Refinery-Goodhart trap) could not satisfy these absolute anchors.
    #[test]
    fn min_naval_curve_is_monotone_with_a_crossable_and_a_wall_tier() {
        // Monotone non-decreasing in cost.
        let costs = [0.0_f32, 10.0, 25.0, 50.0, 100.0, 200.0, 400.0, 800.0];
        for w in costs.windows(2) {
            assert!(
                min_naval_for_cost(w[0]) <= min_naval_for_cost(w[1]),
                "curve must be non-decreasing: f({})={} > f({})={}",
                w[0],
                min_naval_for_cost(w[0]),
                w[1],
                min_naval_for_cost(w[1]),
            );
        }
        // Knee: a GAP_KNEE-unit gap is crossable by the ordinary naval ceiling.
        assert!(
            min_naval_for_cost(GAP_KNEE) <= NAVAL_KNEE as u8,
            "a {GAP_KNEE}-unit gap must gate at or below the naval ceiling {NAVAL_KNEE}"
        );
        // Wall: a 400-unit open-ocean gap is beyond every culture (seafarer ~75
        // included) — the non-empty wall tier.
        assert!(
            min_naval_for_cost(400.0) > 75,
            "a 400-unit wall must gate above any achievable naval (got {})",
            min_naval_for_cost(400.0)
        );
        // A clear strait is crossable by ordinary cultures well under the ceiling.
        assert!(
            min_naval_for_cost(50.0) < NAVAL_KNEE as u8,
            "a 50-unit strait must be comfortably crossable (got {})",
            min_naval_for_cost(50.0)
        );
    }
}

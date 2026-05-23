//! Stream-power + thermal erosion. Phase 2.
//!
//! Hydraulic erosion is the **stream-power law** (Howard 1994, Whipple &
//! Tucker 1999): each cell incises at a rate proportional to √(upstream
//! flow) × local slope, and the eroded material is transferred to the
//! downstream cell. This is mass-conserving by construction — every unit
//! removed from cell A lands on its steepest-descent neighbor — and
//! produces V-shaped valleys because incision concentrates where flow
//! accumulates.
//!
//! Thermal erosion redistributes material across any cell pair whose
//! elevation difference exceeds the talus angle, softening cliffs.
//! Process is order-independent: deltas accumulate into a buffer and
//! apply in a second sweep.
//!
//! Determinism contract: every transcendental routes through
//! `mapgen_core::fmath`; `rng` is reserved for future stochastic models
//! and currently unused (kept in the API for forward compatibility).

use mapgen_core::{fmath, WorldData};
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Debug)]
pub struct ErosionParams {
    /// Number of stream-power iterations.
    pub iterations: usize,
    /// Stream-power coefficient `k`. Higher = more downcutting per
    /// iteration. Cap on per-step erosion keeps elevation from inverting.
    pub erosion_rate: f32,
    /// Thermal smoothing passes after stream power.
    pub thermal_passes: usize,
    /// Maximum stable slope; anything steeper redistributes downhill.
    pub talus_angle: f32,
}

impl Default for ErosionParams {
    fn default() -> Self {
        Self {
            iterations: 8,
            erosion_rate: 0.04,
            thermal_passes: 0,
            talus_angle: 0.10,
        }
    }
}

/// Stream-power hydraulic erosion + thermal slope-redistribution.
///
/// For each iteration:
///   1. Compute steepest-descent flow direction per land cell.
///   2. Compute flow accumulation (unit precipitation, topo-sorted).
///   3. For every cell, erode `k * √flow * slope` capped at half the
///      local drop, and deposit the same amount onto the downstream
///      cell. Mass is exactly conserved.
///
/// Then run `thermal_passes` slope-redistribution sweeps to soften any
/// edge steeper than `talus_angle`. Each undirected edge is visited
/// once; deltas are buffered and applied after the sweep so the result
/// is order-independent.
pub fn run(world: &mut WorldData, params: ErosionParams, _rng: &mut ChaCha8Rng) {
    for _ in 0..params.iterations {
        run_iteration(world, &params);
    }
    for _ in 0..params.thermal_passes {
        thermal_pass(world, &params);
    }
}

/// One stream-power incision pass (flow direction → accumulation →
/// incision/deposition). Pure function of the current elevation field, so
/// calling it `params.iterations` times reproduces the [`run`] loop
/// exactly — the seam the `Pipeline` uses to animate erosion sub-steps.
pub fn run_iteration(world: &mut WorldData, params: &ErosionParams) {
    let n = world.mesh.cell_count();
    let neighbors = world.mesh.neighbors.clone();
    let elev = &mut world.terrain.elevation;

    // 1. Steepest-descent flow direction.
    let mut flow_to: Vec<Option<u32>> = vec![None; n];
    for i in 0..n {
        if elev[i] <= 0.0 {
            continue;
        }
        let mut best: Option<(f32, u32)> = None;
        for &j in &neighbors[i] {
            let drop = elev[i] - elev[j as usize];
            if drop > 0.0 {
                match best {
                    None => best = Some((drop, j)),
                    Some((d, _)) if drop > d => best = Some((drop, j)),
                    _ => {}
                }
            }
        }
        flow_to[i] = best.map(|(_, j)| j);
    }

    // 2. Flow accumulation: process in descending-elevation order.
    let mut flow_acc = vec![1.0_f32; n];
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by(|&a, &b| elev[b as usize].total_cmp(&elev[a as usize]));
    for &c in &order {
        if let Some(d) = flow_to[c as usize] {
            flow_acc[d as usize] += flow_acc[c as usize];
        }
    }

    // 3. Stream-power incision with lateral-bank deposition. Each
    // river cell loses material; the material spreads to its
    // *lateral* neighbors (land neighbors that aren't the downstream
    // cell) — the natural-levee analogue. This deepens the channel
    // cross-section while leaving the chain-along-the-river slopes
    // roughly intact, so total relief increases. Mass conserves
    // exactly per iteration.
    let mut delta = vec![0.0_f32; n];
    let mut banks: Vec<u32> = Vec::with_capacity(8);
    for i in 0..n {
        if elev[i] <= 0.0 {
            continue;
        }
        let Some(d) = flow_to[i] else {
            continue;
        };
        let drop = elev[i] - elev[d as usize];
        if drop <= 0.0 {
            continue;
        }
        let erosion = params.erosion_rate * fmath::sqrt(flow_acc[i]) * drop;
        let erosion = erosion.min(drop * 0.45);

        banks.clear();
        for &nbr in &neighbors[i] {
            if nbr != d && elev[nbr as usize] > 0.0 {
                banks.push(nbr);
            }
        }
        delta[i] -= erosion;
        if banks.is_empty() {
            // River cell touching only sea/downstream — fall back to
            // depositing on the downstream cell.
            delta[d as usize] += erosion;
        } else {
            let share = erosion / banks.len() as f32;
            for &b in &banks {
                delta[b as usize] += share;
            }
        }
    }
    for i in 0..n {
        elev[i] += delta[i];
    }
}

/// One thermal slope-redistribution sweep: smooth any edge steeper than
/// `talus_angle`. Each undirected edge is visited once; deltas buffer and
/// apply after the sweep so the result is order-independent.
pub fn thermal_pass(world: &mut WorldData, params: &ErosionParams) {
    let n = world.mesh.cell_count();
    let neighbors = world.mesh.neighbors.clone();
    let elev = &mut world.terrain.elevation;

    let mut delta = vec![0.0_f32; n];
    for i in 0..n {
        for &nj in &neighbors[i] {
            let j = nj as usize;
            if j <= i {
                continue;
            }
            let diff = elev[i] - elev[j];
            if diff.abs() > params.talus_angle {
                let excess = diff.abs() - params.talus_angle;
                let move_amt = excess * 0.25 * diff.signum();
                delta[i] -= move_amt;
                delta[j] += move_amt;
            }
        }
    }
    for i in 0..n {
        elev[i] += delta[i];
    }
}

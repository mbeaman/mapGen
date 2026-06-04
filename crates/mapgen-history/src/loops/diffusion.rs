//! Diffusion carrier (The Sundered Lanes, Phase 2). A faith spreads over history:
//! across a crossable sea lane to an unconverted far shore — the water crossing,
//! the milestone — and inland to unconverted land neighbours on the receiving
//! landmass. So a religion founded on one continent comes to span others, but
//! ONLY where a crossable lane reaches: the same lane substrate the war and
//! colonization carriers use, gated at an easier "a trader can carry a
//! missionary" threshold (lower than a naval power's reach).
//!
//! Own loop (`LoopId::Diffusion`) with an independent RNG sub-stream, and a
//! byte-identical no-op on a laneless, fully-converted world (the seed42 golden):
//! no crossable lane and no faithless land cell → no writes.
//!
//! Conversion is double-buffered: each year reads a snapshot of the year's start
//! and writes a fresh copy, so a faith advances at most one ring per year (no
//! intra-year cascade) and the result is independent of cell-visit order.

use crate::loops::{CausalLoop, LoopId, TickCtx};
use crate::unit_f32;

/// The lane capability a faith can ride — a trader/missionary's reach. Lower than
/// a naval power's (war queries the polity's own `naval`): faith takes the cheap
/// straits, not the open-ocean walls.
const DIFFUSION_NAVAL: u8 = 40;
/// Per-(unconverted far anchor, crossable lane) yearly chance a faith crosses.
const SEA_DIFFUSE_PROB: f32 = 0.06;
/// Per-(unconverted land cell touching a faithful land neighbour) yearly chance
/// it converts.
const LAND_DIFFUSE_PROB: f32 = 0.15;

pub struct Diffusion;

impl CausalLoop for Diffusion {
    fn id(&self) -> LoopId {
        LoopId::Diffusion
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        let n = ctx.world.mesh.cell_count();
        if ctx.world.religions.religion_id.len() != n {
            return; // religions stage hasn't run
        }
        let is_land: Vec<bool> = (0..n)
            .map(|i| ctx.world.terrain.elevation.get(i).copied().unwrap_or(0.0) > 0.0)
            .collect();
        let prev = ctx.world.religions.religion_id.clone();
        let mut next = prev.clone();

        // 1. Sea crossing: a faith on one lane anchor reaches the UNCONVERTED far
        //    anchor across a crossable lane (the milestone — faith over water).
        for lane in &ctx.world.sea_lanes.lanes {
            if lane.min_naval > DIFFUSION_NAVAL {
                continue;
            }
            let (a, b) = (lane.a as usize, lane.b as usize);
            for (src, dst) in [(a, b), (b, a)] {
                let (Some(faith), None) = (
                    prev.get(src).copied().flatten(),
                    prev.get(dst).copied().flatten(),
                ) else {
                    continue;
                };
                if is_land.get(dst).copied().unwrap_or(false)
                    && next[dst].is_none()
                    && unit_f32(ctx.rng) < SEA_DIFFUSE_PROB
                {
                    next[dst] = Some(faith);
                    record_conversion(
                        &mut ctx.state.faith_changes,
                        ctx.year,
                        dst,
                        prev[dst],
                        faith,
                    );
                }
            }
        }

        // 2. Land spread: an unconverted land cell adopts the faith of a faithful
        //    land neighbour (first in neighbour order; land neighbours are on the
        //    same body, so this never crosses water — only lanes do). Skip a cell
        //    that ALREADY converted this tick (`next.is_some()`), so a sea crossing
        //    that just landed here wins the pocket (a land hop must not clobber it,
        //    which would waste the draw and lock the anchor out of future
        //    crossings); `prev` alone would miss that same-tick conversion.
        for cell in 0..n {
            if !is_land[cell] || prev[cell].is_some() || next[cell].is_some() {
                continue;
            }
            let adopt = ctx.world.mesh.neighbors.get(cell).and_then(|nbrs| {
                nbrs.iter().find_map(|&nb| {
                    let v = nb as usize;
                    if is_land.get(v).copied().unwrap_or(false) {
                        prev.get(v).copied().flatten()
                    } else {
                        None
                    }
                })
            });
            if let Some(faith) = adopt {
                if unit_f32(ctx.rng) < LAND_DIFFUSE_PROB {
                    next[cell] = Some(faith);
                    record_conversion(
                        &mut ctx.state.faith_changes,
                        ctx.year,
                        cell,
                        prev[cell],
                        faith,
                    );
                }
            }
        }

        ctx.world.religions.religion_id = next;
    }
}

/// Append a conversion to the Faith timeline (`SimState::faith_changes`) so the
/// time-slider can reconstruct `religion_id` at any past year — the faith mirror
/// of the `border_changes` a won war records. Order follows the loop's lane- then
/// cell-iteration, so the timeline is deterministic (native↔wasm byte-identical).
/// Takes the timeline field directly (not `ctx`) so it can be called while the
/// lane loop holds an immutable borrow of `ctx.world`.
fn record_conversion(
    changes: &mut Vec<mapgen_core::history::FaithChange>,
    year: i32,
    cell: usize,
    from: Option<u16>,
    to: u16,
) {
    changes.push(mapgen_core::history::FaithChange {
        year,
        cell: cell as u32,
        from,
        to: Some(to),
    });
}

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

use mapgen_core::EventKind;

use crate::emit::Emit;
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
        //    `(faith, far continent)` first-crossings are collected here and the
        //    `FaithCrossed` events emitted AFTER the loop, because the loop holds an
        //    immutable borrow of `ctx.world` (the same reason `record_conversion`
        //    takes the timeline field directly).
        let mut faith_crossings: Vec<(u16, u16, u32)> = Vec::new(); // (faith, continent, dst cell)
        for lane in &ctx.world.sea_lanes.lanes {
            if lane.min_naval > DIFFUSION_NAVAL {
                continue;
            }
            let (a, b) = (lane.a as usize, lane.b as usize);
            // Each direction's destination carries its own anchor's continent tag.
            for (src, dst, dst_continent) in [(a, b, lane.continent_b), (b, a, lane.continent_a)] {
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
                    // Milestone: a faith's FIRST water-crossing to a NAMED far
                    // continent. Deduped per (faith, continent) across all years, so
                    // the inland cascade and later re-crossings stay silent. Only
                    // named continents (Some) fire — an unnamed speck has no shore to
                    // name in the chronicle.
                    if let Some(cont) = dst_continent {
                        if ctx.state.crossed_faiths.insert((faith, cont)) {
                            faith_crossings.push((faith, cont, dst as u32));
                        }
                    }
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

        // Announce each faith's first water-crossing to a named far continent — the
        // `FaithCrossed` milestone the lore engine narrates (it reads `far_shore` to
        // name the shore). A deterministic push per collected crossing, drawing no
        // RNG, so the diffusion stream is unperturbed. Empty on a laneless world
        // (seed42) — no crossing, no event, golden byte-identical.
        for (faith, continent, cell) in faith_crossings {
            let faith_name = ctx
                .world
                .religions
                .religions
                .get(faith as usize)
                .map(|r| r.name.clone())
                .unwrap_or_default();
            Emit::new(
                ctx.year,
                EventKind::FaithCrossed,
                cell,
                0.5,
                format!("The {faith_name} faith was carried over the open water to a far shore."),
            )
            .far_shore(Some(continent))
            .push(ctx.world);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimState;
    use mapgen_core::{EventKind, SeaLane, WorldData};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn a_faith_reaching_one_continent_over_two_anchors_announces_it_once() {
        // The (faith, continent) dedup, exercised DIRECTLY — the canonical planet
        // seeds can't, because land-spread converts a continent's other anchors
        // before a second sea crossing reaches them, so the guard never fires there
        // (an integration test of it is vacuous). Here two crossable lanes reach two
        // DIFFERENT unconverted anchors (cells 1, 2) on the SAME far continent (5)
        // from a faithful source (cell 0), and the mesh has NO neighbours — so the
        // ONLY path to 1 and 2 is by sea, and each is a distinct crossing to shore 5.
        // The dedup makes the milestone fire ONCE. (Mutation: drop the
        // `crossed_faiths.insert` guard → each anchor fires → two events → trips.)
        let n = 3;
        let mut w = WorldData::default();
        w.terrain.elevation = vec![1.0; n]; // all land
        w.mesh.sites = vec![[0.0, 0.0]; n]; // cell_count == 3
        w.mesh.neighbors = vec![Vec::new(); n]; // no land neighbours → no land spread
        w.religions.religion_id = vec![Some(0), None, None]; // only cell 0 is faithful
        w.sea_lanes.lanes = vec![
            SeaLane {
                a: 0,
                b: 1,
                cost: 1.0,
                min_naval: 30,
                continent_a: None,
                continent_b: Some(5),
            },
            SeaLane {
                a: 0,
                b: 2,
                cost: 1.0,
                min_naval: 30,
                continent_a: None,
                continent_b: Some(5),
            },
        ];

        let mut st = SimState::default();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut diff = Diffusion;
        // 400 years: each unconverted anchor rolls SEA_DIFFUSE_PROB (0.06) per year,
        // so both convert with overwhelming (and, under a fixed seed, deterministic)
        // certainty.
        for year in 0..400 {
            let mut ctx = TickCtx {
                world: &mut w,
                state: &mut st,
                year,
                rng: &mut rng,
            };
            diff.tick(&mut ctx);
        }

        assert_eq!(
            w.religions.religion_id,
            vec![Some(0), Some(0), Some(0)],
            "both far anchors must convert by sea (there is no land path between them)"
        );
        let crossings = w
            .events
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::FaithCrossed))
            .count();
        assert_eq!(
            crossings, 1,
            "one faith reaching one continent (over two anchors) must announce ONCE"
        );
    }
}

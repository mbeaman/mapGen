//! Colonization carrier (The Sundered Lanes, Phase 1). A polity with the naval
//! tech to sail a sea lane settles the lane's UNCLAIMED far-shore anchor — a
//! `from:None` `BorderChange` that plants an overseas exclave on a second
//! landmass. The sibling of the beachhead (`mearsheimer.rs`), which seizes an
//! *owned* far anchor by conquest (`from:Some`); colonization claims empty land.
//!
//! Runs as its own loop (`LoopId::Colonization`) with an independent per-year RNG
//! sub-stream, so it cannot perturb the other loops, and a world with no sea
//! lanes (the seed42 golden) is a byte-identical no-op: no lanes → no targets →
//! no RNG draws and no writes.

use mapgen_core::{EventKind, WorldData};

use crate::emit::Emit;
use crate::loops::{CausalLoop, LoopId, TickCtx};
use crate::unit_f32;

/// Per-(polity, reachable unclaimed far anchor) yearly chance to found a colony.
/// Low, so colonies appear gradually rather than all in year 0; over the ~500-year
/// sim it reliably settles a persistently-unclaimed far anchor.
const COLONIZE_PROB: f32 = 0.03;

/// Unclaimed far-shore anchors `a` can reach: for each sea lane `a` can sail
/// (`min_naval <= a`'s naval) where `a` owns one anchor, the OTHER anchor if it
/// is currently unclaimed. Lane anchors are coastal land cells on sizable bodies
/// by construction, so claiming a far anchor plants an overseas colony.
fn colony_targets(world: &WorldData, naval_a: u8, a: usize, out: &mut Vec<u32>) {
    let owns =
        |c: usize, pid: usize| world.society.control.get(c).copied().flatten() == Some(pid as u32);
    for lane in &world.sea_lanes.lanes {
        if lane.min_naval > naval_a {
            continue;
        }
        let (aa, ab) = (lane.a as usize, lane.b as usize);
        let far = if owns(aa, a) {
            ab
        } else if owns(ab, a) {
            aa
        } else {
            continue;
        };
        if world.society.control.get(far).copied().flatten().is_none() {
            out.push(far as u32);
        }
    }
}

/// The colonization loop.
pub struct Colonization;

impl CausalLoop for Colonization {
    fn id(&self) -> LoopId {
        LoopId::Colonization
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        let year = ctx.year;
        let n = ctx.state.polity_count;
        let mut targets: Vec<u32> = Vec::new();
        for a in 0..n {
            if ctx.state.dissolved[a] {
                continue;
            }
            let naval_a = ctx.state.naval[a];
            targets.clear();
            colony_targets(ctx.world, naval_a, a, &mut targets);
            for &far in &targets {
                let cell_i = far as usize;
                // This far cell can appear twice in `targets`: a coastal cell can
                // be the chosen endpoint of two deduped lanes to two different
                // bodies that `a` owns a near anchor of. Once the first occurrence
                // claims it, skip the rest (and skip the RNG draw).
                if ctx
                    .world
                    .society
                    .control
                    .get(cell_i)
                    .copied()
                    .flatten()
                    .is_some()
                {
                    continue;
                }
                let roll = unit_f32(ctx.rng);
                if roll >= COLONIZE_PROB {
                    continue;
                }
                ctx.state
                    .border_changes
                    .push(mapgen_core::history::BorderChange {
                        year,
                        cell: far,
                        from: None,
                        to: Some(a as u32),
                    });
                ctx.world.society.control[cell_i] = Some(a as u32);
                // The colony added a coastal cell to `a` — Turchin's carrying
                // capacity must track the new territory (the sibling beachhead
                // does the same after a conquest). Colonization is last in ORDER,
                // so this year's other loops already ran, but every SUBSEQUENT
                // year's Turchin reads this capacity, and Turchin never recomputes
                // it itself.
                ctx.state.capacity[a] = crate::polity_capacity(ctx.world, a);

                // Record the colony for the chronicle (CityFounded), attributed
                // to the founding realm's ruler if it has one.
                let name = ctx
                    .world
                    .society
                    .nations
                    .get(a)
                    .map(|nt| nt.name.clone())
                    .unwrap_or_default();
                let ruler = ctx.state.courts.get(a).and_then(|c| c.ruler);
                let mut ev = Emit::new(
                    year,
                    EventKind::CityFounded,
                    far,
                    0.45,
                    format!("{name} planted a colony on a far shore across the sea."),
                );
                if let Some(r) = ruler {
                    ev = ev.actors(&[r]);
                }
                ev.push(ctx.world);
            }
        }
    }
}

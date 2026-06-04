//! Trade carrier (The Sundered Lanes, Phase 2). When two DIFFERENT realms come to
//! hold the two ends of a crossable sea lane, an inter-continental trade route
//! opens between them: a one-time `TradeRouteOpened` milestone, and a permanent
//! capacity bonus to BOTH partners (the wealth of the sea trade), so the realms
//! grow larger through the existing Turchin loop — "trade makes realms grow".
//!
//! The economic twin of the other lane carriers (`mearsheimer` seizes the far
//! shore, `colonization` settles it, `diffusion` converts it); trade instead
//! *enriches* two realms that merely face each other across the water. It rides
//! the same trader's-reach gate as faith (`TRADE_NAVAL`, the cheap straits, not
//! the open-ocean walls).
//!
//! Own loop (`LoopId::Trade`) — appended LAST in `ORDER`, so it can't perturb any
//! other loop's stream — and a byte-identical no-op on a laneless world (seed42):
//! no lanes → no routes → no capacity writes and no events. Route opening is
//! DETERMINISTIC (the first year two distinct owners hold a crossable lane), so
//! the loop draws no RNG at all.

use mapgen_core::EventKind;

use crate::emit::Emit;
use crate::loops::{CausalLoop, LoopId, TickCtx};

/// The lane capability a trade route can ride — a trader's reach (the same cheap
/// straits faith diffuses over, not the open-ocean walls a navy needs).
const TRADE_NAVAL: u8 = 40;
/// Capacity bonus per open route, as a fraction of the partner's capacity at the
/// moment it opens — the carrying-capacity lift of sea-trade wealth.
const TRADE_GAIN: f32 = 0.15;

pub struct Trade;

impl CausalLoop for Trade {
    fn id(&self) -> LoopId {
        LoopId::Trade
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        // Phase 1 — scan (immutable borrow of `world`): collect routes that are
        // newly two-owner this year. `(polity_a < polity_b, anchor_cell)`.
        let mut opened: Vec<(u32, u32, u32)> = Vec::new();
        for lane in &ctx.world.sea_lanes.lanes {
            if lane.min_naval > TRADE_NAVAL {
                continue; // a wall, not a trader's strait
            }
            let (a, b) = (lane.a as usize, lane.b as usize);
            let (Some(pa), Some(pb)) = (
                ctx.world.society.control.get(a).copied().flatten(),
                ctx.world.society.control.get(b).copied().flatten(),
            ) else {
                continue; // an anchor is unclaimed — no two realms to trade
            };
            if pa == pb {
                continue; // one realm owns both ends (an exclave) — nobody to trade with
            }
            let key = (pa.min(pb), pa.max(pb));
            if ctx.state.trade_routes.contains(&key)
                || opened.iter().any(|&(x, y, _)| (x, y) == key)
            {
                continue; // already open (or the same pair on a second lane this scan)
            }
            opened.push((key.0, key.1, lane.a));
        }

        // Phase 2 — apply (mutable borrow of `world`): record the route, lift both
        // partners' capacity, and emit the milestone.
        let year = ctx.year;
        for (pa, pb, cell) in opened {
            ctx.state.trade_routes.insert((pa, pb));
            for &p in &[pa, pb] {
                let pi = p as usize;
                let bonus = TRADE_GAIN * ctx.state.capacity[pi];
                ctx.state.trade_bonus[pi] += bonus;
                ctx.state.capacity[pi] += bonus;
            }
            let name = |p: u32| {
                ctx.world
                    .society
                    .nations
                    .get(p as usize)
                    .map(|n| n.name.clone())
                    .unwrap_or_default()
            };
            Emit::new(
                year,
                EventKind::TradeRouteOpened,
                cell,
                0.40,
                format!(
                    "A sea-trade route opened between {} and {}.",
                    name(pa),
                    name(pb)
                ),
            )
            .push(ctx.world);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{effective_capacity, SimState};
    use mapgen_core::{EventKind, Nation, SeaLane, WorldData};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    /// `n` polities owning cells `0..n`, with one sea lane between cells `a` and
    /// `b` at `min_naval`. The Trade loop reads only control / lanes / nations, so
    /// the mesh stays empty.
    fn world(n: usize, a: u32, b: u32, min_naval: u8) -> WorldData {
        let mut w = WorldData::default();
        w.society.nations = vec![Nation::default(); n];
        w.society.control = (0..n as u32).map(Some).collect();
        w.sea_lanes.lanes = vec![SeaLane {
            a,
            b,
            cost: 1.0,
            min_naval,
        }];
        w
    }

    fn state(capacity: Vec<f32>) -> SimState {
        let n = capacity.len();
        SimState {
            polity_count: n,
            trade_bonus: vec![0.0; n],
            capacity,
            ..Default::default()
        }
    }

    fn trade_events(w: &WorldData) -> usize {
        w.events
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::TradeRouteOpened))
            .count()
    }

    fn tick(trade: &mut Trade, w: &mut WorldData, st: &mut SimState, year: i32) {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut ctx = TickCtx {
            world: w,
            state: st,
            year,
            rng: &mut rng,
        };
        trade.tick(&mut ctx);
    }

    #[test]
    fn a_route_lifts_exactly_the_two_partners_and_no_one_else() {
        // The discriminating claim is TARGETING (not magnitude): a crossable lane
        // between polities 0 and 1 lifts THEIR capacity by the trade gain — and
        // leaves the off-lane polity 2 untouched. A loop that boosted the wrong
        // polity (or everyone) would pass a "capacity rose" test but fail this.
        let mut w = world(3, 0, 1, 30); // lane 0↔1 crossable (≤ TRADE_NAVAL)
        let mut st = state(vec![100.0, 200.0, 50.0]);
        let before = st.capacity.clone();
        tick(&mut Trade, &mut w, &mut st, 5);

        assert_eq!(st.capacity[0], before[0] + TRADE_GAIN * before[0]);
        assert_eq!(st.capacity[1], before[1] + TRADE_GAIN * before[1]);
        assert_eq!(
            st.capacity[2], before[2],
            "off-lane polity must be untouched"
        );
        assert_eq!(st.trade_bonus[0], TRADE_GAIN * before[0]);
        assert_eq!(st.trade_bonus[1], TRADE_GAIN * before[1]);
        assert_eq!(st.trade_bonus[2], 0.0);
        assert_eq!(
            st.trade_routes.iter().copied().collect::<Vec<_>>(),
            vec![(0, 1)]
        );
        assert_eq!(trade_events(&w), 1);

        // One-time: ticking again does NOT re-boost or re-announce the open route.
        let held = st.capacity.clone();
        tick(&mut Trade, &mut w, &mut st, 6);
        assert_eq!(
            st.capacity, held,
            "an already-open route must not boost again"
        );
        assert_eq!(
            trade_events(&w),
            1,
            "TradeRouteOpened must fire exactly once"
        );
    }

    #[test]
    fn a_wall_lane_opens_no_route() {
        // A lane above the trader's reach (an open-ocean wall) carries no trade —
        // the naval gate is load-bearing, mirroring the war/faith gates.
        let mut w = world(2, 0, 1, 80); // min_naval 80 > TRADE_NAVAL
        let mut st = state(vec![100.0, 100.0]);
        tick(&mut Trade, &mut w, &mut st, 5);
        assert_eq!(st.capacity, vec![100.0, 100.0], "no boost over a wall");
        assert!(st.trade_routes.is_empty());
        assert_eq!(trade_events(&w), 0);
    }

    #[test]
    fn one_owner_of_both_anchors_opens_no_route() {
        // An exclave (one realm holding BOTH ends of a lane) has no partner to
        // trade with — the `pa == pb` guard. Without it, a realm would "trade with
        // itself" and self-boost.
        let mut w = world(2, 0, 1, 30);
        w.society.control[1] = Some(0); // polity 0 owns both anchors
        let mut st = state(vec![100.0, 100.0]);
        tick(&mut Trade, &mut w, &mut st, 5);
        assert!(st.trade_routes.is_empty());
        assert_eq!(trade_events(&w), 0);
        assert_eq!(st.capacity[0], 100.0, "no self-trade boost");
    }

    #[test]
    fn a_capacity_recompute_keeps_the_trade_bonus() {
        // `effective_capacity` is THE recompute every territory-change site uses
        // (conquest, colonization). It must re-add the trade bonus, not just the
        // territorial baseline — else a border change silently erases a realm's
        // trade prosperity. (Mutation: drop the bonus → eff collapses to territory.)
        let w = world(2, 0, 1, 30);
        let mut st = state(vec![100.0, 100.0]);
        st.trade_bonus[0] = 17.0;
        let territorial = crate::polity_capacity(&w, 0);
        let eff = effective_capacity(&w, &st, 0);
        assert_eq!(eff, territorial + 17.0, "recompute dropped the trade bonus");
        assert_ne!(
            eff, territorial,
            "the bonus must lift capacity above territory"
        );
    }
}

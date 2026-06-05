//! Trade carrier + embargo (The Sundered Lanes, Phase 2). This loop runs the full
//! lifecycle of an inter-continental sea-trade route:
//!
//! - **Open** — when two DIFFERENT, non-belligerent realms come to hold the two
//!   ends of a crossable lane, a route opens: a one-time `TradeRouteOpened` and a
//!   capacity bonus to BOTH partners (the wealth of sea trade), so the realms grow
//!   through the existing Turchin loop ("trade makes realms grow").
//! - **Embargo** — when a trade pair later goes to war (`SimState::belligerents`,
//!   set by `mearsheimer::resolve_war`), their route is SEVERED: the exact bonus is
//!   reversed and an `EmbargoImposed` fires. The severed route never re-opens (the
//!   belligerent gate), so war permanently poisons that trade tie.
//!
//! The economic twin of the other lane carriers (`mearsheimer` seizes the far
//! shore, `colonization` settles it, `diffusion` converts it); trade instead
//! *enriches* (or, on war, *impoverishes*) two realms that face each other across
//! the water. It rides the same trader's-reach gate as faith (`TRADE_NAVAL`).
//!
//! Severs are iterated over the OPEN routes, not re-scanned from lanes — a won
//! cross-water war is the beachhead carrier, after which one realm owns both
//! anchors, so a lane re-scan would hit the exclave guard and miss exactly the
//! canonical trade-pair war.
//!
//! Own loop (`LoopId::Trade`) — appended LAST in `ORDER`, so it can't perturb any
//! other loop's stream — and a byte-identical no-op on a laneless world (seed42):
//! no lanes → no routes → no capacity writes and no events (the belligerents set
//! is populated there but never read). Open/sever are DETERMINISTIC, so the loop
//! draws no RNG at all.

use mapgen_core::{EventKind, WorldData};

use crate::emit::Emit;
use crate::loops::{CausalLoop, LoopId, TickCtx};

/// The lane capability a trade route can ride — a trader's reach (the same cheap
/// straits faith diffuses over, not the open-ocean walls a navy needs).
const TRADE_NAVAL: u8 = 40;
/// Capacity bonus per open route, as a fraction of the partner's capacity at the
/// moment it opens — the carrying-capacity lift of sea-trade wealth.
const TRADE_GAIN: f32 = 0.15;

fn nation_name(world: &WorldData, p: u32) -> String {
    world
        .society
        .nations
        .get(p as usize)
        .map(|n| n.name.clone())
        .unwrap_or_default()
}

pub struct Trade;

impl CausalLoop for Trade {
    fn id(&self) -> LoopId {
        LoopId::Trade
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        // Phase 1 — collect (no event emission yet, to keep `world` borrows clean).
        //
        // SEVER: every OPEN route whose pair has gone to war (an embargo). Iterated
        // over the open routes — NOT re-scanned from lanes — on purpose: a WON
        // cross-water war is the beachhead carrier, after which one realm owns BOTH
        // anchors, so a lane re-scan would hit the `pa == pb` exclave guard and miss
        // exactly the canonical trade-pair war. The route key is control-independent.
        let to_sever: Vec<((u32, u32), [f32; 2], mapgen_core::EventId)> = {
            let belligerents = &ctx.state.belligerents;
            ctx.state
                .trade_routes
                .iter()
                .filter(|(key, _)| belligerents.contains(*key))
                .map(|(&k, &(bonus, open_id))| (k, bonus, open_id))
                .collect()
        };

        // OPEN: a crossable lane whose two anchors are held by two DISTINCT,
        // NON-belligerent realms not already trading. The belligerent gate is what
        // keeps a severed route from re-opening next year ("peaceful" = not at war).
        let mut to_open: Vec<(u32, u32, u32)> = Vec::new();
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
            if ctx.state.belligerents.contains(&key) {
                continue; // enemies don't trade (and a severed route never re-opens)
            }
            if ctx.state.trade_routes.contains_key(&key)
                || to_open.iter().any(|&(x, y, _)| (x, y) == key)
            {
                continue; // already open (or the same pair on a second lane this scan)
            }
            to_open.push((key.0, key.1, lane.a));
        }

        // Phase 2 — apply.
        let year = ctx.year;
        for ((pa, pb), [ba, bb], open_id) in to_sever {
            ctx.state.trade_routes.remove(&(pa, pb));
            ctx.state.trade_bonus[pa as usize] -= ba;
            ctx.state.capacity[pa as usize] -= ba;
            ctx.state.trade_bonus[pb as usize] -= bb;
            ctx.state.capacity[pb as usize] -= bb;
            let cell = ctx
                .world
                .society
                .nations
                .get(pa as usize)
                .map(|n| n.capital_cell)
                .unwrap_or(0);
            // Cite the route's OPENING as the embargo's cause, so the lore
            // engine's causal closure pulls the whole first-contact arc — the
            // lane is born (`TradeRouteOpened`) and dies (`EmbargoImposed`) in one
            // chronicle. The war that set `belligerents` is the deeper cause, but
            // threading its `WarDeclared` id here is deferred: the opening link
            // alone gives the chronicle its two-beat arc, and citing the war would
            // mean widening `belligerents` to carry an EventId (touching
            // mearsheimer's contract) for no extra narrative the embargo's own
            // "War severed …" summary doesn't already carry.
            Emit::new(
                year,
                EventKind::EmbargoImposed,
                cell,
                0.45,
                format!(
                    "War severed the sea-trade route between {} and {}.",
                    nation_name(ctx.world, pa),
                    nation_name(ctx.world, pb)
                ),
            )
            .causes(&[open_id])
            .push(ctx.world);
        }
        for (pa, pb, cell) in to_open {
            let bonus_a = TRADE_GAIN * ctx.state.capacity[pa as usize];
            let bonus_b = TRADE_GAIN * ctx.state.capacity[pb as usize];
            ctx.state.trade_bonus[pa as usize] += bonus_a;
            ctx.state.capacity[pa as usize] += bonus_a;
            ctx.state.trade_bonus[pb as usize] += bonus_b;
            ctx.state.capacity[pb as usize] += bonus_b;
            let open_id = Emit::new(
                year,
                EventKind::TradeRouteOpened,
                cell,
                0.40,
                format!(
                    "A sea-trade route opened between {} and {}.",
                    nation_name(ctx.world, pa),
                    nation_name(ctx.world, pb)
                ),
            )
            .push(ctx.world);
            // Remember which event opened this route, so a later embargo can cite
            // it (the first-contact arc's birth beat).
            ctx.state
                .trade_routes
                .insert((pa, pb), ([bonus_a, bonus_b], open_id));
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
            ..Default::default()
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

    fn embargo_events(w: &WorldData) -> usize {
        w.events
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::EmbargoImposed))
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
            st.trade_routes.keys().copied().collect::<Vec<_>>(),
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

    #[test]
    fn war_embargoes_only_the_belligerents_route() {
        // Embargo (the dual of trade): when a trade pair goes to war, THEIR route is
        // severed — its exact bonus reversed, an `EmbargoImposed` emitted — while a
        // peaceful route between other realms is left untouched. The discriminator is
        // TARGETING (a broken sever-condition would cut the wrong route or all of
        // them) — the vacuous sundered-seed guard can't catch that.
        let mut w = WorldData::default();
        w.society.nations = vec![Nation::default(); 4];
        w.society.control = (0..4u32).map(Some).collect();
        w.sea_lanes.lanes = vec![
            SeaLane {
                a: 0,
                b: 1,
                cost: 1.0,
                min_naval: 30,
                ..Default::default()
            },
            SeaLane {
                a: 2,
                b: 3,
                cost: 1.0,
                min_naval: 30,
                ..Default::default()
            },
        ];
        let mut st = state(vec![100.0, 200.0, 50.0, 80.0]);

        // Both routes open in peace.
        tick(&mut Trade, &mut w, &mut st, 5);
        assert_eq!(trade_events(&w), 2);
        let open_cap = st.capacity.clone();
        let open_bonus = st.trade_bonus.clone();

        // War breaks out between 0 and 1 → embargo severs their route only.
        st.belligerents.insert((0, 1));
        tick(&mut Trade, &mut w, &mut st, 6);
        assert_eq!(st.capacity[0], open_cap[0] - open_bonus[0]); // exact reversal
        assert_eq!(st.capacity[1], open_cap[1] - open_bonus[1]);
        assert_eq!(st.capacity[2], open_cap[2], "peaceful route untouched");
        assert_eq!(st.capacity[3], open_cap[3], "peaceful route untouched");
        assert_eq!(st.trade_bonus[0], 0.0);
        assert_eq!(st.trade_bonus[1], 0.0);
        assert_eq!(
            st.trade_routes.keys().copied().collect::<Vec<_>>(),
            vec![(2, 3)],
            "only the belligerents' route is severed"
        );
        assert_eq!(embargo_events(&w), 1);

        // A severed route does NOT re-open while the pair remains at war.
        tick(&mut Trade, &mut w, &mut st, 7);
        assert_eq!(
            st.trade_routes.keys().copied().collect::<Vec<_>>(),
            vec![(2, 3)]
        );
        assert_eq!(trade_events(&w), 2, "severed route must not re-open");
        assert_eq!(embargo_events(&w), 1, "must not re-embargo a gone route");
    }

    #[test]
    fn an_embargo_cites_the_route_opening_it_severs() {
        // The first-contact arc's causal spine: the `EmbargoImposed` that kills a
        // route names the `TradeRouteOpened` that bore it, so the lore engine's
        // causal closure pulls the lane's two beats (born of trade, killed by war)
        // into one chronicle. Without the link the embargo stands alone and the
        // arc has nothing to weave. (Mutation: drop `.causes(&[open_id])` in the
        // sever loop → the embargo carries no cause → this trips.)
        let mut w = world(2, 0, 1, 30);
        let mut st = state(vec![100.0, 100.0]);

        tick(&mut Trade, &mut w, &mut st, 5); // route opens
        let open_id = w
            .events
            .events
            .iter()
            .find(|e| matches!(e.kind, EventKind::TradeRouteOpened))
            .expect("a route opened")
            .id;

        st.belligerents.insert((0, 1));
        tick(&mut Trade, &mut w, &mut st, 6); // war severs it
        let embargo = w
            .events
            .events
            .iter()
            .find(|e| matches!(e.kind, EventKind::EmbargoImposed))
            .expect("an embargo fired");

        // Exactly one cause — the opening — so the first-contact arc's closure is
        // the clean two-beat {opening, embargo}, the invariant the lore weave's
        // "two peoples met" prose relies on (a single route per embargo).
        assert_eq!(
            embargo.cause_ids.as_slice(),
            &[open_id],
            "the embargo must cite ONLY the route opening it severs"
        );
    }
}

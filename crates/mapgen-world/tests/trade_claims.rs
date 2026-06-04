//! Data-layer claim for "The Sundered Lanes" Phase 2 — Trade diffusion. When two
//! different realms come to hold the two ends of a crossable sea lane, an
//! inter-continental trade route opens (`TradeRouteOpened`) and lifts both
//! partners' carrying capacity, so they grow through the existing Turchin loop
//! ("trade makes realms grow"). The capacity lift is SimState-internal (not
//! serialized), so the persistent, testable trace is the event; the lift itself is
//! pinned by the loop-level tests in `loops/trade.rs`.
//!
//! Pinned both directions, mirroring the war / faith carriers: routes open on
//! every crossing seed and are ABSENT on sundered seeds (no crossable lane → no
//! two-shore trade). The sundered half is the anti-false-green guard — a trade
//! loop that ignored the naval gate, or opened a route within one landmass, would
//! light it up.

use mapgen_core::EventKind;
use mapgen_testsupport::{planet_params, CROSSING_SEEDS, SUNDERED_SEEDS};
use mapgen_world::generate_full;

fn count_kind(world: &mapgen_core::WorldData, kind: EventKind) -> usize {
    world
        .events
        .events
        .iter()
        .filter(|e| std::mem::discriminant(&e.kind) == std::mem::discriminant(&kind))
        .count()
}

fn trade_routes_opened(world: &mapgen_core::WorldData) -> usize {
    count_kind(world, EventKind::TradeRouteOpened)
}

#[test]
fn trade_routes_open_across_crossable_lanes_and_only_there() {
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        assert!(
            trade_routes_opened(&world) > 0,
            "crossing seed {seed}: no TradeRouteOpened — two realms face each other across a \
             crossable lane, but no inter-continental trade ever opened"
        );
    }
    for &seed in SUNDERED_SEEDS {
        let world = generate_full(planet_params(seed));
        let n = trade_routes_opened(&world);
        assert_eq!(
            n, 0,
            "sundered seed {seed}: {n} trade routes opened, but no lane is crossable — trade \
             crossed water where it must not (naval gate or same-landmass route bug)"
        );
    }
}

#[test]
fn war_between_trade_partners_severs_their_route_with_an_embargo() {
    // The dual of trade: when two realms that traded across a lane go to war, the
    // route is embargoed (`EmbargoImposed`). A cross-water war IS a trade-pair war
    // (it rides the same lane), so the embargo fires on every crossing seed. The
    // sound discriminator is the INVARIANT, not the count: an embargo only severs a
    // route that opened, so `embargo ≤ trade` — and `embargo > 0` proves the
    // war→sever path actually fires in a full sim (the loop test pins the
    // mechanics; this pins that it triggers end-to-end). NB: the sundered direction
    // is vacuous for embargo (no lane → no route → trivially 0), so it isn't the
    // guard here — the loop-level `war_embargoes_only_the_belligerents_route` is.
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let trade = trade_routes_opened(&world);
        let embargo = count_kind(&world, EventKind::EmbargoImposed);
        assert!(
            embargo > 0,
            "crossing seed {seed}: no EmbargoImposed — a trade pair went to war (the beachhead \
             crosses the same lane) but no route was ever severed"
        );
        assert!(
            embargo <= trade,
            "seed {seed}: {embargo} embargoes but only {trade} routes opened — severed a route \
             that never traded"
        );
    }
}

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

fn trade_routes_opened(world: &mapgen_core::WorldData) -> usize {
    world
        .events
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TradeRouteOpened))
        .count()
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

//! DATA claim for the COLONY far-shore place tag (`CityFounded.far_shore`): the
//! Colonization carrier tags each overseas settlement with the NAMED continent it
//! was planted on (`colonization.rs` ~129), so the shore chronicle can name where
//! the settlement strand reached. This is the real-world mutation guard for the
//! colony strand of the far-shore weave — its sibling, the FAITH tag, is pinned by
//! `faith_crossing_claims.rs`; the SWORD tag by `mapgen-cli/tests/lore_cli.rs`.
//!
//! Why it lives here and not (only) in `mapgen-lore/tests/shore.rs`: under the
//! periodic planet no real seed reaches one shore by all three strands, so the
//! 3-strand weave is pinned there against a SYNTHETIC world — which, by
//! construction, builds its events with `far_shore` set inline and so cannot catch
//! a regression in the carrier's *tagging*. This test rides REAL generated worlds,
//! so dropping `.far_shore(..)` in `colonization.rs` reds it.

use mapgen_core::EventKind;
use mapgen_testsupport::{planet_params, COLONIZE_SEEDS, SUNDERED_SEEDS};
use mapgen_world::generate_full;

/// Overseas colonies: `CityFounded` events carrying a `far_shore` tag. Turchin's
/// peacetime `CityFounded` (the only other emitter) leaves `far_shore` `None`, so
/// a `Some` tag is the colonization carrier's signature.
fn tagged_colonies(world: &mapgen_core::WorldData) -> Vec<&mapgen_core::Event> {
    world
        .events
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::CityFounded) && e.far_shore.is_some())
        .collect()
}

#[test]
fn an_overseas_colony_is_tagged_with_the_named_far_shore_it_reached() {
    // BOTH directions, the anti-false-green pairing (mirrors faith_crossing_claims):
    //  - every COLONIZE seed plants ≥1 overseas colony, and EACH carries a valid
    //    `far_shore` into `world.continents` (the place tag the narrator names);
    //  - a SUNDERED seed plants none (no crossable lane → no overseas colony), so a
    //    carrier that tagged a same-landmass town, or ignored the naval gate, fails.
    // (Mutation: delete `.far_shore(far_continent)` in colonization.rs → the colonize
    // seeds drop to 0 tagged colonies → trips. Tag an out-of-range continent → the
    // validity assertion trips.)
    let mut total = 0usize;
    for &seed in COLONIZE_SEEDS {
        let world = generate_full(planet_params(seed));
        let colonies = tagged_colonies(&world);
        assert!(
            !colonies.is_empty(),
            "colonize seed {seed}: no CityFounded carries a far_shore — the colony place tag \
             never fired"
        );
        let ncont = world.continents.len();
        for e in &colonies {
            let shore = e.far_shore.expect("filtered to Some above");
            assert!(
                (shore as usize) < ncont,
                "colonize seed {seed}: far_shore {shore} is not a real continent (have {ncont})"
            );
        }
        total += colonies.len();
    }
    assert!(
        total > 0,
        "no overseas colony was tagged on ANY colonize seed — the place tag never fired"
    );

    for &seed in SUNDERED_SEEDS {
        let world = generate_full(planet_params(seed));
        let colonies = tagged_colonies(&world);
        assert!(
            colonies.is_empty(),
            "sundered seed {seed}: {} tagged overseas colonies, but no lane is crossable here — \
             the colony carrier crossed water where it must not",
            colonies.len()
        );
    }
}

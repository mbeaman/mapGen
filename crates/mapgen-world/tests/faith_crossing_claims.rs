//! DATA claims for the faith-crossing milestone (`EventKind::FaithCrossed`): the
//! Diffusion carrier announces a faith's FIRST water-crossing to a NAMED far
//! continent, tagged with that continent in `Event::far_shore`, exactly once per
//! (faith, continent) — and never on a laneless world. This is the structured
//! place tag the landmass-arc narrator consumes (see
//! `mapgen-lore/tests/contact.rs`).

use mapgen_core::EventKind;
use mapgen_testsupport::{planet_params, CROSSING_SEEDS, REFERENCE_SEED};
use mapgen_world::generate_full;

fn faith_crossings(world: &mapgen_core::WorldData) -> Vec<&mapgen_core::Event> {
    world
        .events
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::FaithCrossed))
        .collect()
}

#[test]
fn a_faith_crossing_fires_a_named_milestone_where_lanes_cross_and_never_otherwise() {
    // BOTH directions, the anti-false-green pairing:
    //  - a LANELESS world (seed42) emits ZERO FaithCrossed (no water to cross), so
    //    a loop that emitted unconditionally would fail here;
    //  - a crossing world emits at least one, and EVERY one carries a valid
    //    `far_shore` into `world.continents` (the place tag the narrator names).
    // (Mutation: delete the emit in diffusion.rs → the crossing seed drops to 0 →
    // trips. Emit ignoring the named-continent gate → seed42 still 0 but a crossing
    // to an unnamed speck would carry an out-of-range far_shore → the validity
    // assertion trips.)
    let laneless = generate_full(planet_params(REFERENCE_SEED));
    assert_eq!(
        faith_crossings(&laneless).len(),
        0,
        "a laneless world (seed {REFERENCE_SEED}) has no water for a faith to cross"
    );

    let mut total = 0usize;
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let crossings = faith_crossings(&world);
        let ncont = world.continents.len();
        for e in &crossings {
            let shore = e
                .far_shore
                .unwrap_or_else(|| panic!("seed {seed}: a FaithCrossed without a far_shore"));
            assert!(
                (shore as usize) < ncont,
                "seed {seed}: far_shore {shore} is not a real continent (have {ncont})"
            );
        }
        total += crossings.len();
    }
    assert!(
        total > 0,
        "no faith crossed water on ANY crossing seed — the milestone never fired"
    );
}

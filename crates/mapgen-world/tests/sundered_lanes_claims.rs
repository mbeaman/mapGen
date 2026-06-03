//! Data-layer claims for "The Sundered Lanes" arc (see `docs/CLAIMS.md`).
//!
//! The headline claim is that the cross-water carrier is **earned**: a polity
//! comes to hold land on a second landmass only by a war crossing a sea lane its
//! naval tech can sail. Pinning this in *one* direction (it fires on the
//! canonical crossing seeds) is not enough — a carrier that fired
//! indiscriminately, ignoring the naval gate, would also "fire" and pass. So we
//! pin **both** directions: present on `CROSSING_SEEDS`, absent on
//! `SUNDERED_SEEDS`. The sundered half is the anti-false-green guard — without
//! it, "earned" carries no information.
//!
//! These supersede the older single-direction
//! `history_spec::cross_water_conquest_produces_earned_overseas_holdings`.

use mapgen_testsupport::{
    an_earned_overseas_seizure, overseas_colonizations, planet_params,
    polities_spanning_multiple_landmasses, sizable_landmasses, COLONIZE_SEEDS, CROSSING_SEEDS,
    SUNDERED_SEEDS,
};
use mapgen_world::generate_full;

#[test]
fn cross_water_conquest_fires_on_every_crossing_seed() {
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        assert!(
            sizable_landmasses(&world).len() >= 2,
            "planet seed {seed} must have ≥2 landmasses for a crossing to mean anything"
        );
        // an_earned_overseas_seizure is CONQUEST-specific (it filters `from:Some`),
        // so this cannot be satisfied by the colonization carrier — it pins that a
        // WAR conquered a beachhead on a second landmass. Seed 11 is also a
        // COLONIZE_SEED, so a from-agnostic spanning check could green here on a
        // colony alone — exactly the false-green the anti-false-green rule targets.
        assert!(
            an_earned_overseas_seizure(&world).is_some(),
            "crossing seed {seed}: no polity CONQUERED land on a second landmass — the \
             cross-water beachhead never fired. Reach must be EARNED by conquest here."
        );
    }
}

#[test]
fn no_cross_water_conquest_on_any_sundered_seed() {
    // The crucial direction. Gen-time society is landmass-confined, and land
    // wars cannot cross water, so the ONLY way a polity spans two landmasses is
    // the cross-water carrier. A sundered seed has no crossable inter-continental
    // lane, so the carrier cannot fire — the count must be exactly zero. If this
    // trips, either the naval gate is broken (the carrier crosses where it
    // shouldn't) or this seed is not actually sundered and belongs in
    // CROSSING_SEEDS.
    for &seed in SUNDERED_SEEDS {
        let world = generate_full(planet_params(seed));
        let overseas = polities_spanning_multiple_landmasses(&world);
        assert!(
            overseas.is_empty(),
            "sundered seed {seed}: {} polity/polities span ≥2 landmasses, but no lane is \
             crossable here — the carrier fired where it must not (gate broken, or this \
             seed is mislabeled sundered): {overseas:?}",
            overseas.len(),
        );
    }
}

#[test]
fn colonization_settles_unclaimed_far_shores_and_nowhere_else() {
    // The second carrier (`from:None`), sibling of the beachhead. A polity SETTLES
    // an unclaimed far-shore anchor across a sailable lane — vs the beachhead's
    // conquest of an OWNED anchor. The signal ONLY colonization produces: a
    // `BorderChange{from:None, to:Some(P)}`. Pinned both directions: it fires where
    // a lane pairs an owner-who-can-sail-it with a persistently-unclaimed far
    // anchor (COLONIZE_SEEDS), and NOT on 4/7/19 or the sundered seeds — which
    // colonize nothing because no such pairing exists there (the far anchor is
    // owned, OR unclaimed but behind a naval wall its owner can't sail — seed 4's
    // cell 7719 — OR the lane is itself a sundered wall). The absent half guards
    // against a carrier that colonizes owned land or ignores the naval gate.
    for &seed in COLONIZE_SEEDS {
        let world = generate_full(planet_params(seed));
        assert!(
            !overseas_colonizations(&world).is_empty(),
            "colonize seed {seed}: expected a from:None overseas settlement, found none"
        );
    }
    for &seed in [4u64, 7, 19].iter().chain(SUNDERED_SEEDS.iter()) {
        let world = generate_full(planet_params(seed));
        let colonies = overseas_colonizations(&world);
        assert!(
            colonies.is_empty(),
            "seed {seed}: no lane pairs an unclaimed far anchor with an owner that can sail it, \
             so no from:None overseas settlement should occur — got {colonies:?} (carrier \
             colonizing where it must not)"
        );
    }
}

// ---- Replay layer ---------------------------------------------------------
// The carrier records each seizure as a `BorderChange`, and the time-slider
// reconstructs control at any past year via `WorldData::control_at_year`. The
// payoff claim is "the exclave appears at a year as you scrub" — so we assert,
// using the slider's OWN function, that the earned cell is NOT the conqueror's
// the year before the crossing and IS the year of it. (The web/DOM half of this
// claim — "scrubbing visibly reveals the exclave" — is blocked on exclave
// legibility, substage 4: an exclave you cannot see cannot be asserted visible.)

#[test]
fn the_earned_crossing_replays_faithfully_in_the_time_slider() {
    let mut proven = 0;
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));

        // The slider's final frame must equal the live control, or its present
        // would not match the rendered map.
        if let Some((_, last)) = world.border_change_year_span() {
            assert_eq!(
                world.control_at_year(last),
                world.society.control,
                "seed {seed}: control_at_year(last) != present control — replay is unfaithful"
            );
        }

        let Some((p, cell, year)) = an_earned_overseas_seizure(&world) else {
            panic!("crossing seed {seed}: no earned overseas seizure to replay");
        };
        let before = world.control_at_year(year - 1);
        let at = world.control_at_year(year);
        assert_ne!(
            before[cell as usize],
            Some(p),
            "seed {seed}: cell {cell} was already polity {p}'s the year before the crossing \
             — the slider would not show it flip in, so it is not earned-at-{year}"
        );
        assert_eq!(
            at[cell as usize],
            Some(p),
            "seed {seed}: cell {cell} is not polity {p}'s at year {year} — the recorded \
             crossing year disagrees with the slider reconstruction"
        );
        proven += 1;
    }
    assert_eq!(
        proven,
        CROSSING_SEEDS.len(),
        "every crossing seed must replay a crossing"
    );
}

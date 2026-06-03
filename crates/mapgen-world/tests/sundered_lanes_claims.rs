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
    planet_params, polities_spanning_multiple_landmasses, sizable_landmasses, CROSSING_SEEDS,
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
        let overseas = polities_spanning_multiple_landmasses(&world);
        assert!(
            !overseas.is_empty(),
            "crossing seed {seed}: no polity holds land on a second landmass — the \
             cross-water carrier never fired. Reach must be EARNED, and this canonical \
             seed earns it (the Step-0 probe proved a crossable lane exists here)."
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

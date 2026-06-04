//! Data-layer claim for the v21 Prosperity heatmap. The history sim's final
//! per-polity `SimState::population` (previously discarded) is carried into
//! `Nation::prosperity`, normalized to [0,1] by the world's max final population —
//! the data channel the prosperity overlay tints by.
//!
//! The discriminating signature is the NORMALIZATION + SPREAD, not "all in [0,1]":
//! after a divide-by-max, the top polity is `x / x == 1.0` (bit-exact in IEEE f32)
//! AND the rest sit strictly below it. "All in [0,1]" alone stays GREEN on all-zeros
//! (skip-the-write-back); even `max == 1.0` alone stays GREEN on a constant `1.0`
//! write-back (every realm = 1.0 → max is still 1.0). So we pin BOTH: an exact 1.0
//! maximum (populated + normalized) AND that some realm is below 1.0 (real spread) —
//! together a signal only the normalized relative-population write-back produces.

use mapgen_testsupport::{planet_params, CROSSING_SEEDS};
use mapgen_world::generate_full;

/// The max prosperity over all polities, and whether any polity has prosperity > 0.
fn prosperity_stats(world: &mapgen_core::WorldData) -> (f32, bool) {
    let mut max = 0.0f32;
    let mut any_positive = false;
    for n in &world.society.nations {
        if n.prosperity > max {
            max = n.prosperity;
        }
        if n.prosperity > 0.0 {
            any_positive = true;
        }
    }
    (max, any_positive)
}

#[test]
fn final_population_is_carried_into_prosperity_and_normalized() {
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        assert!(
            !world.society.nations.is_empty(),
            "seed {seed}: no polities — the fixture must produce society for this claim"
        );

        let (max, any_positive) = prosperity_stats(&world);

        // The channel is POPULATED: at least one realm grew a population the sim
        // carried in. (Mutation: skip the write-back → all 0.0 → this trips.)
        assert!(
            any_positive,
            "seed {seed}: every Nation::prosperity is 0 — the final SimState population \
             was never carried into WorldData (the prosperity write-back didn't run)"
        );

        // The NORMALIZATION signature: the world's most-populous realm divides to
        // exactly 1.0 (x/x in f32 is bit-exact). This is the assertion the
        // all-zeros mutation cannot satisfy — it pins that the values are the
        // *normalized* relative population, not raw or constant.
        assert_eq!(
            max, 1.0,
            "seed {seed}: max prosperity is {max}, not 1.0 — the population was not \
             normalized by the world's maximum (the heatmap would have no full-scale anchor)"
        );

        // SPREAD: max==1.0 alone is also satisfied by a CONSTANT 1.0 write-back, so
        // pin that prosperity actually VARIES — at least one realm sits below the
        // full-scale anchor. Real worlds have exactly one realm at the max and the
        // rest strictly below. (Mutation: a constant `1.0` write-back → nothing
        // below 1.0 → this trips, where max==1.0 alone would not.)
        assert!(
            world.society.nations.iter().any(|n| n.prosperity < 1.0),
            "seed {seed}: every realm's prosperity is 1.0 — a constant, not normalized \
             relative population (no spread for the heatmap to grade)"
        );

        // And the whole field stays within the renderable [0,1] heatmap domain.
        for (pid, n) in world.society.nations.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(&n.prosperity),
                "seed {seed}: polity {pid} prosperity {} out of [0,1]",
                n.prosperity
            );
        }
    }
}

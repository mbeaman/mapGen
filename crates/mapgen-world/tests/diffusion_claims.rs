//! Data-layer claims for "The Sundered Lanes" Phase 2 — Diffusion (a faith
//! provably crosses water). Two halves, both pinned here:
//!
//! 1. CONFINEMENT (gen-time): `religions::found` no longer lets a faith pre-cross
//!    oceans — each religion is confined to its founding landmass at the Religions
//!    stage. (Before this, the dominant faith routinely blanketed every
//!    continent.)
//! 2. DIFFUSION (history): the `Diffusion` loop carries a faith across a crossable
//!    sea lane over the sim, so a religion comes to span ≥2 landmasses — and ONLY
//!    where a crossable lane exists. Sundered seeds keep every faith confined. The
//!    sundered half is the anti-false-green guard, mirroring the carrier.

use std::collections::BTreeSet;

use mapgen_testsupport::{
    planet_params, religions_spanning_multiple_landmasses, CROSSING_SEEDS, SUNDERED_SEEDS,
};
use mapgen_world::{generate_full, Pipeline, PipelineStage};

/// Step the pipeline up to and including the Religions stage (gen-time, before
/// Polities/History), returning the partial world.
fn world_after_religions(seed: u64) -> mapgen_core::WorldData {
    let mut p = Pipeline::new(planet_params(seed));
    loop {
        match p.step() {
            Some(PipelineStage::Religions) => break,
            Some(_) => continue,
            None => panic!("pipeline finished before the Religions stage ran"),
        }
    }
    p.into_world()
}

#[test]
fn religions_are_confined_to_one_landmass_at_gen_time() {
    // At the Religions stage, no faith has pre-crossed water: every religion sits
    // on at most one sizable landmass. (Cross-water reach is earned over history
    // by Diffusion — pinned below.)
    for &seed in CROSSING_SEEDS.iter().chain(SUNDERED_SEEDS.iter()) {
        let world = world_after_religions(seed);
        let spanning = religions_spanning_multiple_landmasses(&world);
        assert!(
            spanning.is_empty(),
            "seed {seed}: religions {spanning:?} span ≥2 landmasses at gen-time — a faith \
             pre-crossed water (founding-landmass confinement broken)"
        );
    }
}

#[test]
fn diffusion_carries_a_faith_across_water_only_over_a_crossable_lane() {
    // The payoff: post-history a religion comes to span ≥2 sizable landmasses —
    // EARNED by the Diffusion loop crossing a crossable sea lane (gen-time
    // confinement + sea/land diffusion). Pinned both directions: it fires on the
    // crossing seeds, and is ABSENT on sundered seeds (no crossable lane → faiths
    // stay confined). The sundered half is the anti-false-green guard — a diffuser
    // that crossed water for free, or ignored the naval gate, would light it up.
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        assert!(
            !religions_spanning_multiple_landmasses(&world).is_empty(),
            "crossing seed {seed}: no faith spans ≥2 landmasses post-history — Diffusion \
             never carried a religion across a lane"
        );
    }
    for &seed in SUNDERED_SEEDS {
        let world = generate_full(planet_params(seed));
        let spanning = religions_spanning_multiple_landmasses(&world);
        assert!(
            spanning.is_empty(),
            "sundered seed {seed}: faiths {spanning:?} span ≥2 landmasses, but no lane is \
             crossable — diffusion crossed water where it must not"
        );
    }
}

#[test]
fn the_diffusion_timeline_records_every_conversion_faithfully() {
    // Substage-1 (data) for the Faith time-slider: the Diffusion loop doesn't only
    // mutate `religion_id`, it RECORDS each conversion in `history.faith_changes` —
    // chronologically, and faithfully (the present faith map is exactly the
    // timeline applied). That faithful timeline is what lets `religion_at_year`
    // replay the spread. (The recorded YEAR's correctness — that rewinding the
    // timeline yields the pre-diffusion founding — is pinned at the replay layer.)
    let mut distinct_years = BTreeSet::new();
    let mut total = 0usize;
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let fc = &world.history.faith_changes;
        assert!(
            !fc.is_empty(),
            "crossing seed {seed}: Diffusion converted faiths but recorded no timeline \
             (faith_changes empty) — the Faith slider would have nothing to replay"
        );
        // Chronological: appended per sim-year, so years never decrease.
        assert!(
            fc.windows(2).all(|w| w[0].year <= w[1].year),
            "seed {seed}: faith_changes out of chronological order"
        );
        // Faithful: the present faith of every recorded cell is exactly that cell's
        // LAST recorded conversion (the timeline reconstructs the present map). And
        // diffusion only fills unconverted cells, so every change is None→Some.
        let n = world.mesh.cell_count();
        let mut last: Vec<Option<u16>> = vec![None; n];
        for ch in fc {
            assert_eq!(
                ch.from, None,
                "seed {seed}: a diffusion change had a non-None `from`"
            );
            let to = ch.to.expect("a diffusion change recorded to=None");
            last[ch.cell as usize] = Some(to);
            distinct_years.insert(ch.year);
        }
        for (cell, &rec) in last.iter().enumerate() {
            if let Some(to) = rec {
                assert_eq!(
                    world.religions.religion_id[cell],
                    Some(to),
                    "seed {seed}: cell {cell}'s present faith ≠ its last recorded conversion \
                     — the timeline doesn't reconstruct the map"
                );
            }
        }
        // Every faith that ended up spanning ≥2 landmasses got onto its second body
        // through a RECORDED conversion — the crossing is in the timeline, not only
        // in the final map.
        for rid in religions_spanning_multiple_landmasses(&world) {
            assert!(
                fc.iter().any(|ch| ch.to == Some(rid)),
                "seed {seed}: faith {rid} spans water at present but no conversion to it was \
                 recorded — its crossing is missing from the timeline"
            );
        }
        total += fc.len();
    }
    // The spread is genuinely temporal (many sim-years), not one dump — a hardcoded
    // or wrong year would collapse this.
    assert!(
        distinct_years.len() > 1,
        "faith_changes share a single year across every crossing seed — the recorded year \
         isn't the conversion year"
    );
    assert!(total > 0, "no diffusion was recorded on any crossing seed");
}

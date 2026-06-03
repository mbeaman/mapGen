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

//! Pipeline-stepper equivalence + full-pipeline golden hash.
//!
//! `generate_full` is implemented as a thin driver over the resumable
//! `Pipeline` state machine. These tests pin two contracts:
//!   1. Stepping the `Pipeline` one stage at a time yields byte-identical
//!      `WorldData` to the `generate_full` convenience driver.
//!   2. The full pipeline output for a fixed seed matches a committed
//!      golden hash — captured from the pre-refactor flat implementation,
//!      so the stepper refactor (and the erosion iteration split) cannot
//!      silently perturb output.

use mapgen_world::{generate_full, GenerateParams, Pipeline, PipelineStage};

fn fixed_params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    }
}

fn hash_world(world: &mapgen_core::WorldData) -> String {
    let mut bytes = Vec::new();
    ciborium::into_writer(world, &mut bytes).unwrap();
    blake3::hash(&bytes).to_hex().to_string()
}

#[test]
fn full_pipeline_golden_hash() {
    let world = generate_full(fixed_params(42));
    let hash = hash_world(&world);
    let committed = include_str!("golden/seed42_full.blake3.txt").trim();
    assert_eq!(
        hash, committed,
        "generate_full output drifted from committed golden hash"
    );
}

#[test]
fn planet_seed9_far_shore_golden_hash() {
    // A LANED planet seed in the determinism golden. The seed-42 golden above is
    // the laneless continental default: no carrier crosses water, so every
    // `Event::far_shore` stays `None` and is byte-invisible (`skip_serializing_if`)
    // — that golden never exercises the tag. seed 9 is the canonical THREE-strand
    // far shore (reached by faith AND colony AND the sword; the same fixture
    // `mapgen-lore/tests/shore.rs` pins), so its `far_shore` tags fire from every
    // gen carrier. Hashing the whole planet world pins those tag VALUES; its
    // native↔wasm twin in `mapgen-wasm/tests/cross_platform.rs` makes the pin
    // cross-platform — the previously-only-structural far_shore byte-identity.
    let world = generate_full(GenerateParams::planet(9));
    let hash = hash_world(&world);
    let committed = include_str!("golden/seed9_planet_full.blake3.txt").trim();
    assert_eq!(
        hash, committed,
        "planet seed-9 output drifted from committed golden hash (far_shore tags included)"
    );
}

#[test]
fn stepper_matches_generate_full() {
    let reference = generate_full(fixed_params(42));

    let mut p = Pipeline::new(fixed_params(42));
    let mut seen = 0usize;
    while p.step().is_some() {
        seen += 1;
    }
    let stepped = p.into_world();

    assert_eq!(
        seen,
        PipelineStage::total(),
        "stepper ran {seen} stages, expected {}",
        PipelineStage::total()
    );
    assert_eq!(
        hash_world(&reference),
        hash_world(&stepped),
        "Pipeline stepping must reproduce generate_full byte-for-byte"
    );
}

#[test]
fn fine_stepper_matches_generate_full() {
    let reference = generate_full(fixed_params(7));

    let mut p = Pipeline::new(fixed_params(7));
    while p.step_fine().is_some() {}
    let stepped = p.into_world();

    assert_eq!(
        hash_world(&reference),
        hash_world(&stepped),
        "fine stepping (erosion sub-steps) must reproduce generate_full byte-for-byte"
    );
}

#[test]
fn stage_metadata_is_well_formed() {
    assert_eq!(PipelineStage::ORDER.len(), PipelineStage::total());
    let mut total_weight = 0u32;
    for (i, stage) in PipelineStage::ORDER.iter().enumerate() {
        assert_eq!(stage.index(), i, "{stage:?} index mismatch");
        assert!(!stage.label().is_empty(), "{stage:?} has empty label");
        assert!(stage.weight() > 0, "{stage:?} has zero weight");
        total_weight += stage.weight() as u32;
    }
    assert!(total_weight > 0);
}

#[test]
fn progress_is_monotonic_and_terminates_at_one() {
    let mut p = Pipeline::new(fixed_params(3));
    let mut last = 0.0_f64;
    while p.step().is_some() {
        let now = p.progress();
        assert!(
            now >= last - 1e-9,
            "progress went backwards: {last} -> {now}"
        );
        last = now;
    }
    assert!(p.is_done());
    assert!(
        (p.progress() - 1.0).abs() < 1e-9,
        "final progress {last} != 1.0"
    );
}

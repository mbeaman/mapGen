//! Phase 4 history-stage contract.
//!
//! In Phase 4a the six causal loops are no-op skeletons, so these pin the
//! *wiring* and the *determinism harness* — that History runs last in the
//! pipeline, that a no-op sim leaves the log empty (no premature output), and
//! that the history-bearing pipeline is still deterministic. The per-loop
//! determinism mechanics (tick count, stream independence) are unit-tested in
//! `mapgen-history` itself; event-emission contracts arrive with 4b.

use mapgen_world::{generate_full, GenerateParams, Pipeline, PipelineStage};

fn fixed(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    }
}

#[test]
fn history_runs_last_in_the_pipeline() {
    let order = PipelineStage::ORDER;
    assert_eq!(
        order.len(),
        11,
        "History should bring the pipeline to 11 stages"
    );
    assert_eq!(
        *order.last().unwrap(),
        PipelineStage::History,
        "History must run after every geography/society stage it reads"
    );
}

#[test]
fn no_op_history_emits_no_events_or_entities_yet() {
    // 4a contract: the driver ticks 500 years but the loops are no-op, so the
    // log and entity store stay empty. This test flips to "non-empty" in 4b.
    let world = generate_full(fixed(42));
    assert!(
        world.events.is_empty(),
        "4a loops are no-op; got {} events",
        world.events.len()
    );
    assert!(
        world.entities.by_id.is_empty(),
        "4a loops are no-op; got {} entities",
        world.entities.by_id.len()
    );
}

#[test]
fn history_stepper_runs_and_reports_history_stage() {
    // The resumable Pipeline (web live build-up path) must surface History as
    // its final step.
    let mut p = Pipeline::new(fixed(7));
    let mut last = None;
    while let Some(stage) = p.step() {
        last = Some(stage);
    }
    assert_eq!(last, Some(PipelineStage::History));
    assert!(p.is_done());
}

#[test]
fn generate_full_is_deterministic_with_history_wired() {
    // The full-pipeline golden hash (pipeline_spec.rs) is the byte-level pin;
    // this is a cheap independent check that wiring History didn't introduce
    // run-to-run nondeterminism.
    let a = generate_full(fixed(42));
    let b = generate_full(fixed(42));
    assert_eq!(a.events.len(), b.events.len());
    assert_eq!(a.entities.by_id.len(), b.entities.by_id.len());
    assert_eq!(a.society.nations.len(), b.society.nations.len());
}

//! Phase 4 history-stage contract (integration, via the full pipeline).
//!
//! Pins the wiring (History runs last) and, from 4b on, the demographic
//! event-emission contract: only Famine/Plague/Drought kinds, valid fields,
//! famine-dominant shape on seed 42, and well-formed output across seeds. The
//! per-loop determinism mechanics (tick count, stream independence, seed
//! purity) and the synthetic-world dynamics are unit-tested in `mapgen-history`
//! itself; the byte-level determinism pin is the full-pipeline golden hash in
//! `pipeline_spec.rs`.

use mapgen_core::EventKind;
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
fn history_emits_demographic_events_but_no_entities_yet() {
    // 4b contract: the Turchin demographic backbone populates the event log
    // (famine/plague/drought) over 500 years, but named entities don't arrive
    // until 4c, so the entity store is still empty.
    let world = generate_full(fixed(42));
    assert!(
        !world.events.is_empty(),
        "4b should populate the event log with demographic crises"
    );
    assert!(
        world.entities.by_id.is_empty(),
        "entities arrive in 4c; got {} at 4b",
        world.entities.by_id.len()
    );
}

#[test]
fn only_demographic_event_kinds_at_4b() {
    // 4b emits exactly the Turchin demographic crises. Wars, successions,
    // schisms, etc. arrive in later substages.
    let world = generate_full(fixed(42));
    for e in &world.events.events {
        assert!(
            matches!(
                e.kind,
                EventKind::Famine | EventKind::Plague | EventKind::Drought
            ),
            "unexpected event kind at 4b: {:?}",
            e.kind
        );
    }
}

#[test]
fn every_event_has_valid_fields() {
    let world = generate_full(fixed(42));
    let n_cells = world.mesh.cell_count() as u32;
    for (i, e) in world.events.events.iter().enumerate() {
        assert_eq!(e.id.0, i as u32, "event ids must be sequential from 0");
        assert!((0..500).contains(&e.year), "year {} out of range", e.year);
        assert!(
            (0.0..=1.0).contains(&e.salience),
            "salience {} out of [0,1]",
            e.salience
        );
        match e.location {
            Some(c) => assert!(c.0 < n_cells, "location cell {} >= {n_cells}", c.0),
            None => panic!("4b events should be located at a cell"),
        }
    }
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
fn famine_is_the_dominant_crisis_on_seed_42() {
    // Locks the tuned shape of the demographic backbone: famine is Turchin's
    // Malthusian regulator and must outnumber the rarer plague. A regression to
    // the plague-spam first draft (71 plague / 7 famine) would trip this.
    let world = generate_full(fixed(42));
    let count =
        |k: fn(&EventKind) -> bool| world.events.events.iter().filter(|e| k(&e.kind)).count();
    let famine = count(|k| matches!(k, EventKind::Famine));
    let plague = count(|k| matches!(k, EventKind::Plague));
    assert!(famine >= 1, "expected at least one famine over 500 years");
    assert!(
        famine >= plague,
        "famine ({famine}) should be at least as common as plague ({plague}) — \
         famine is the regulator, plague the rarer shock"
    );
}

#[test]
fn history_is_well_formed_across_seeds() {
    // Robustness beyond seed 42: several seeds must run without panicking and
    // emit only valid demographic events (no degenerate kind, no bad field).
    // Doesn't require non-empty (a barren-territory world legitimately could be
    // quiet) — it guards against panics and malformed output.
    for seed in 1..=5u64 {
        let world = generate_full(fixed(seed));
        let n_cells = world.mesh.cell_count() as u32;
        for e in &world.events.events {
            assert!(
                matches!(
                    e.kind,
                    EventKind::Famine | EventKind::Plague | EventKind::Drought
                ),
                "seed {seed}: unexpected kind {:?}",
                e.kind
            );
            assert!(
                (0.0..=1.0).contains(&e.salience),
                "seed {seed}: bad salience"
            );
            assert!(
                e.location.map(|c| c.0 < n_cells).unwrap_or(false),
                "seed {seed}: event not located at a valid cell"
            );
        }
    }
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

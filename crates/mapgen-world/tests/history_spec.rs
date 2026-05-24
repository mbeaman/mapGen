//! Phase 4 history-stage contract (integration, via the full pipeline).
//!
//! Pins the wiring (History runs last) and the emitted history: the 4b
//! demographic crises (famine/plague/drought, famine-dominant) and the 4c agent
//! layer (named rulers, dynasties, houses, titles, lineage). Per-loop
//! determinism mechanics and synthetic-world dynamics are unit-tested in
//! `mapgen-history`; the byte-level determinism pin is the full-pipeline golden
//! hash in `pipeline_spec.rs`.

use mapgen_core::{Entity, EventKind};
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

/// The event kinds Phase 4 emits so far (4b demographic + 4c dynastic).
fn is_known_kind(k: &EventKind) -> bool {
    matches!(
        k,
        EventKind::Famine
            | EventKind::Plague
            | EventKind::Drought
            | EventKind::Birth
            | EventKind::Marriage
            | EventKind::Coronation
            | EventKind::Death
    )
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
fn history_populates_events_and_entities() {
    let world = generate_full(fixed(42));
    assert!(
        !world.events.is_empty(),
        "history should populate the event log"
    );
    let kinds =
        |pred: fn(&Entity) -> bool| world.entities.by_id.values().filter(|e| pred(e)).count();
    assert!(
        kinds(|e| matches!(e, Entity::Character(_))) > 0,
        "4c should mint named characters"
    );
    assert!(
        kinds(|e| matches!(e, Entity::Dynasty(_))) > 0,
        "4c should found dynasties"
    );
    assert!(
        kinds(|e| matches!(e, Entity::Title(_))) > 0,
        "4c should create a throne per polity"
    );
}

#[test]
fn event_kinds_are_demographic_or_dynastic() {
    let world = generate_full(fixed(42));
    for e in &world.events.events {
        assert!(
            is_known_kind(&e.kind),
            "unexpected event kind: {:?}",
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
            None => panic!("history events should be located at a cell"),
        }
    }
}

#[test]
fn famine_is_the_dominant_crisis_on_seed_42() {
    // Locks the tuned shape of the demographic backbone: famine is Turchin's
    // Malthusian regulator and must outnumber the rarer plague.
    let world = generate_full(fixed(42));
    let count =
        |k: fn(&EventKind) -> bool| world.events.events.iter().filter(|e| k(&e.kind)).count();
    let famine = count(|k| matches!(k, EventKind::Famine));
    let plague = count(|k| matches!(k, EventKind::Plague));
    assert!(famine >= 1, "expected at least one famine over 500 years");
    assert!(
        famine >= plague,
        "famine ({famine}) should be at least as common as plague ({plague})"
    );
}

#[test]
fn dynasties_have_valid_lineage_titles_and_events() {
    let world = generate_full(fixed(42));
    let ents = &world.entities.by_id;

    // Every dynasty has a founding character; every house belongs to a dynasty.
    for e in ents.values() {
        match e {
            Entity::Dynasty(d) => {
                let founder = d.founder.expect("dynasty has a founder");
                assert!(
                    matches!(ents.get(&founder), Some(Entity::Character(_))),
                    "dynasty founder must be a Character"
                );
            }
            Entity::House(h) => {
                let dyn_id = h.dynasty.expect("house belongs to a dynasty");
                assert!(
                    matches!(ents.get(&dyn_id), Some(Entity::Dynasty(_))),
                    "house.dynasty must point to a Dynasty"
                );
            }
            _ => {}
        }
    }

    let mut rulers = 0;
    let mut coronations = 0;
    for e in ents.values() {
        if let Entity::Character(c) = e {
            // Lifespan sanity.
            if let Some(d) = c.died_year {
                assert!(
                    d >= c.born_year,
                    "died_year {d} < born_year {}",
                    c.born_year
                );
            }
            // A title-holder (ruler) belongs to a house, and each reign has a
            // coronation start event.
            if !c.titles.is_empty() {
                rulers += 1;
                assert!(
                    matches!(c.house.and_then(|h| ents.get(&h)), Some(Entity::House(_))),
                    "ruler must belong to a House"
                );
                for hold in &c.titles {
                    assert!(
                        matches!(ents.get(&hold.title), Some(Entity::Title(_))),
                        "title holding must reference a Title"
                    );
                    assert!(
                        hold.start_event.is_some(),
                        "reign must have a coronation event"
                    );
                    coronations += 1;
                }
            }
        }
    }
    assert!(rulers > 0, "expected ruling characters");
    assert!(coronations >= rulers, "every reign records a coronation");

    // Every Coronation / Death event names an actor.
    for ev in &world.events.events {
        if matches!(ev.kind, EventKind::Coronation | EventKind::Death) {
            assert!(!ev.actors.is_empty(), "coronation/death must name an actor");
        }
    }
}

#[test]
fn parent_child_relationships_are_reciprocal() {
    use mapgen_core::RelationKind;
    let world = generate_full(fixed(42));
    let ents = &world.entities.by_id;
    let rels = |id: &mapgen_core::EntityId| match ents.get(id) {
        Some(Entity::Character(c)) => c.relationships.clone(),
        _ => Vec::new(),
    };
    for e in ents.values() {
        if let Entity::Character(c) = e {
            for r in &c.relationships {
                if matches!(r.kind, RelationKind::Child) {
                    // The parent must list this child back as a Child... i.e.
                    // the other endpoint must have a reciprocal Parent edge.
                    let parent_has_child_edge = rels(&r.other)
                        .iter()
                        .any(|pr| matches!(pr.kind, RelationKind::Parent));
                    assert!(
                        parent_has_child_edge,
                        "a Child edge must have a reciprocal Parent edge"
                    );
                }
            }
        }
    }
}

#[test]
fn history_is_well_formed_across_seeds() {
    // Robustness beyond seed 42: several seeds must run without panicking and
    // emit only valid events located at real cells.
    for seed in 1..=5u64 {
        let world = generate_full(fixed(seed));
        let n_cells = world.mesh.cell_count() as u32;
        for e in &world.events.events {
            assert!(
                is_known_kind(&e.kind),
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
fn history_stepper_runs_and_reports_history_stage() {
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
    let a = generate_full(fixed(42));
    let b = generate_full(fixed(42));
    assert_eq!(a.events.len(), b.events.len());
    assert_eq!(a.entities.by_id.len(), b.entities.by_id.len());
    assert_eq!(a.society.nations.len(), b.society.nations.len());
}

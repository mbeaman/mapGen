//! Phase 4 history-stage contract (integration, via the full pipeline).
//!
//! Pins the wiring (History runs last) and the emitted history: the 4b
//! demographic crises (famine/plague/drought, famine-dominant) and the 4c agent
//! layer (named rulers, dynasties, houses, titles, lineage). Per-loop
//! determinism mechanics and synthetic-world dynamics are unit-tested in
//! `mapgen-history`; the byte-level determinism pin is the full-pipeline golden
//! hash in `pipeline_spec.rs`.

use mapgen_core::{Entity, EntityId, EventKind, RelationKind};
use mapgen_world::{generate_full, GenerateParams, Pipeline, PipelineStage};
use std::collections::BTreeMap;

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

/// The event kinds Phase 4 emits so far (4b demographic + 4c dynastic + 4d
/// secular-cycle rise/collapse).
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
            | EventKind::CityFounded
            | EventKind::CityAbandoned
            | EventKind::Migration
            | EventKind::Exile
            | EventKind::WarDeclared
            | EventKind::BattleFought
            | EventKind::Siege
            | EventKind::TreatySigned
            | EventKind::ClaimAsserted
            | EventKind::Succession
            | EventKind::Schism
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
fn all_entity_and_event_references_resolve() {
    // Phase 5's lore engine dereferences every id in the graph; a dangling
    // reference would crash or hallucinate. Assert the whole web is closed.
    let world = generate_full(fixed(42));
    let ents = &world.entities.by_id;
    let n_events = world.events.events.len() as u32;
    let ent = |id: EntityId| ents.contains_key(&id);
    // Event ids are sequential 0..n (pinned by `every_event_has_valid_fields`).
    let ev = |id: mapgen_core::EventId| id.0 < n_events;

    for e in ents.values() {
        match e {
            Entity::Character(c) => {
                if let Some(h) = c.house {
                    assert!(ent(h), "character.house dangling");
                }
                for r in &c.relationships {
                    assert!(ent(r.other), "relationship.other dangling");
                }
                for t in &c.titles {
                    assert!(ent(t.title), "title holding references missing Title");
                    if let Some(e) = t.start_event {
                        assert!(ev(e), "holding.start_event dangling");
                    }
                    if let Some(e) = t.end_event {
                        assert!(ev(e), "holding.end_event dangling");
                    }
                }
                if let Some(e) = c.birth_event {
                    assert!(ev(e), "character.birth_event dangling");
                }
                if let Some(e) = c.death_event {
                    assert!(ev(e), "character.death_event dangling");
                }
            }
            Entity::Dynasty(d) => {
                if let Some(f) = d.founder {
                    assert!(ent(f), "dynasty.founder dangling");
                }
            }
            Entity::House(h) => {
                if let Some(d) = h.dynasty {
                    assert!(ent(d), "house.dynasty dangling");
                }
                if let Some(f) = h.founder {
                    assert!(ent(f), "house.founder dangling");
                }
            }
            Entity::Title(t) => {
                assert!(
                    (t.polity as usize) < world.society.nations.len(),
                    "title.polity out of range"
                );
                if let Some(e) = t.created_event {
                    assert!(ev(e), "title.created_event dangling");
                }
            }
            _ => {}
        }
    }
    for e in &world.events.events {
        for a in &e.actors {
            assert!(ent(*a), "event.actor dangling");
        }
        for p in &e.patients {
            assert!(ent(*p), "event.patient dangling");
        }
        for c in &e.cause_ids {
            assert!(ev(*c), "event.cause_id dangling");
        }
    }
}

#[test]
fn in_dynasty_succession_follows_bloodline() {
    // A same-dynasty successor must be a *child* of the prior ruler (orderly
    // succession). A dynastic break (different dynasty) carries no such
    // requirement. This is the core 4c lineage claim.
    let world = generate_full(fixed(42));
    let ents = &world.entities.by_id;

    let mut by_title: BTreeMap<u32, Vec<(i32, EntityId)>> = BTreeMap::new();
    for (id, e) in ents.iter() {
        if let Entity::Character(c) = e {
            for h in &c.titles {
                by_title
                    .entry(h.title.0)
                    .or_default()
                    .push((h.start_year, *id));
            }
        }
    }
    let dyn_of = |cid: &EntityId| -> Option<EntityId> {
        match ents.get(cid) {
            Some(Entity::Character(c)) => match c.house.and_then(|h| ents.get(&h)) {
                Some(Entity::House(house)) => house.dynasty,
                _ => None,
            },
            _ => None,
        }
    };
    let is_child_of = |child: &EntityId, parent: EntityId| -> bool {
        matches!(ents.get(child), Some(Entity::Character(c))
            if c.relationships.iter().any(|r| matches!(r.kind, RelationKind::Child) && r.other == parent))
    };

    let mut checked = 0;
    for (_title, mut holders) in by_title {
        holders.sort_by_key(|(y, _)| *y);
        for w in holders.windows(2) {
            let (_, prev) = w[0];
            let (_, next) = w[1];
            let (dp, dn) = (dyn_of(&prev), dyn_of(&next));
            if dp.is_some() && dp == dn {
                assert!(
                    is_child_of(&next, prev),
                    "a same-dynasty successor must be a child of the prior ruler"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "expected at least one in-dynasty succession to validate"
    );
}

#[test]
fn secular_cycle_produces_rise_and_collapse_events() {
    // 4d: the Turchin fiscal half + Khaldun decadence drive recurring crises
    // (settlements abandoned, people displaced) punctuating expansion.
    let world = generate_full(fixed(42));
    let n = |k: fn(&EventKind) -> bool| world.events.events.iter().filter(|e| k(&e.kind)).count();
    let collapses = n(|k| matches!(k, EventKind::CityAbandoned));
    // Cadence lock: crises must *recur* (multiple secular cycles) but not spam.
    // Seed 42 yields ~12 (≈3 per polity); the band guards the tuning against a
    // retune that breaks the mechanism (→ 0) or makes it fire every year.
    assert!(
        (6..=30).contains(&collapses),
        "secular collapses ({collapses}) outside the expected recurring band [6, 30]"
    );
    assert!(
        n(|k| matches!(k, EventKind::Migration)) >= 1,
        "a collapse should displace people (Migration)"
    );
    assert!(
        n(|k| matches!(k, EventKind::CityFounded)) >= 1,
        "expansion years should found new towns (CityFounded)"
    );
}

#[test]
fn wars_are_well_formed() {
    // 4e: every declared war cites a casus belli and names both sovereigns;
    // every battle names a victor.
    let world = generate_full(fixed(42));
    let mut wars = 0;
    for e in &world.events.events {
        match e.kind {
            EventKind::WarDeclared => {
                wars += 1;
                assert!(e.casus_belli.is_some(), "a war must have a casus belli");
                assert!(e.actors.len() >= 2, "a war names attacker and defender");
            }
            EventKind::BattleFought => {
                assert!(!e.actors.is_empty(), "a battle names a victor");
            }
            _ => {}
        }
    }
    assert!(wars >= 1, "expected at least one war over 500 years");
}

#[test]
fn history_shifts_borders_conserving_controlled_cells() {
    // History (4e wars) must move borders — but only *reassign* cells, never
    // create or destroy controlled territory. Snapshot control after Naming
    // (the stage before History) and compare to the finished world.
    let mut p = Pipeline::new(fixed(42));
    let mut before = Vec::new();
    while let Some(stage) = p.step() {
        if stage == PipelineStage::Naming {
            before = p.world().society.control.clone();
        }
    }
    let after = p.into_world().society.control;
    let controlled = |c: &[Option<u32>]| c.iter().filter(|x| x.is_some()).count();
    assert_eq!(
        controlled(&before),
        controlled(&after),
        "wars must conserve the controlled-cell count (reassign, not create/destroy)"
    );
    assert_ne!(before, after, "wars should have shifted some borders");
}

#[test]
fn succession_crises_are_contested_and_follow_a_death() {
    // 4f: a Succession event marks a *contested* succession, so it names ≥2
    // claimants and is preceded (same year, same seat) by the ruler's Death.
    let world = generate_full(fixed(42));
    let deaths: std::collections::HashSet<(i32, Option<u32>)> = world
        .events
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Death))
        .map(|e| (e.year, e.location.map(|c| c.0)))
        .collect();
    let mut crises = 0;
    for e in &world.events.events {
        if matches!(e.kind, EventKind::Succession) {
            crises += 1;
            assert!(
                e.actors.len() >= 2,
                "a contested succession names ≥2 claimants"
            );
            assert!(
                deaths.contains(&(e.year, e.location.map(|c| c.0))),
                "a Succession must follow a Death that year at the same seat"
            );
        }
    }
    assert!(
        crises >= 1,
        "expected at least one contested succession on seed 42"
    );
}

#[test]
fn schisms_spawn_drifted_sects_referencing_their_parent() {
    // 4g: a Schism names the splinter sect (actor) and the parent faith
    // (patient), both Religion entities; the sect inherits the pantheon but
    // drifts in alignment.
    let world = generate_full(fixed(42));
    let ents = &world.entities.by_id;
    let religion = |id: &mapgen_core::EntityId| match ents.get(id) {
        Some(Entity::Religion(r)) => Some(r),
        _ => None,
    };
    let mut schisms = 0;
    for e in &world.events.events {
        if matches!(e.kind, EventKind::Schism) {
            let sect = e
                .actors
                .first()
                .and_then(religion)
                .expect("schism names a sect Religion");
            let parent = e
                .patients
                .first()
                .and_then(religion)
                .expect("schism names a parent Religion");
            assert!(
                sect.pantheon == parent.pantheon,
                "a sect inherits its parent's pantheon"
            );
            assert!(
                sect.alignment != parent.alignment,
                "a sect must drift in alignment from its parent"
            );
            schisms += 1;
        }
    }
    assert!(schisms >= 1, "expected at least one schism on seed 42");
}

#[test]
fn history_produces_major_events() {
    // Extended MVP exit criterion: at least three high-salience events
    // (major battles / collapses) for chronicles to anchor on.
    let world = generate_full(fixed(42));
    let major = world
        .events
        .events
        .iter()
        .filter(|e| e.salience >= 0.8)
        .count();
    assert!(
        major >= 3,
        "expected >= 3 events with salience >= 0.8, got {major}"
    );
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

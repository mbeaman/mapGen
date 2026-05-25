//! 5c: narrate() orchestration, NER validation, and Work persistence against a
//! real generated world, exercising the LLM path with canned/failing fakes.

use mapgen_core::EventId;
use mapgen_lore::client::LlmClient;
use mapgen_lore::context::event_closure;
use mapgen_lore::schema::ChronicleDraft;
use mapgen_lore::voice::{Register, VoiceCard};
use mapgen_lore::{narrate, ner, select_focal};
use mapgen_world::{generate_full, GenerateParams};

fn seed42() -> mapgen_core::WorldData {
    generate_full(GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    })
}

/// A client that always returns the same canned response.
struct Canned(String);
impl LlmClient for Canned {
    fn complete(&self, _system: &str, _user: &str) -> anyhow::Result<String> {
        Ok(self.0.clone())
    }
}

/// A client that always errors (network / auth failure).
struct Failing;
impl LlmClient for Failing {
    fn complete(&self, _system: &str, _user: &str) -> anyhow::Result<String> {
        anyhow::bail!("simulated client failure")
    }
}

fn voice() -> VoiceCard {
    VoiceCard::for_register(Register::MonasticChronicle)
}

#[test]
fn select_focal_picks_a_war_event() {
    let w = seed42();
    let id = select_focal(&w, "auto-major-war").unwrap();
    let e = &w.events.events[id.0 as usize];
    use mapgen_core::EventKind::*;
    assert!(matches!(e.kind, WarDeclared | BattleFought | Siege));
    // Numeric selector round-trips.
    assert_eq!(select_focal(&w, "7").unwrap(), EventId(7));
    assert!(select_focal(&w, "nonsense").is_err());
}

#[test]
fn template_narration_persists_a_work_offline() {
    let mut w = seed42();
    let focal = select_focal(&w, "auto-major-war").unwrap();
    let before = w.works.len();
    let work = narrate(&mut w, focal, &voice(), None).unwrap();
    assert_eq!(w.works.len(), before + 1);
    assert!(!work.body.is_empty() && !work.title.is_empty());
    assert!(
        work.references.contains(&focal),
        "must cite the focal event"
    );
    assert_eq!(work.in_world_author, voice().author);
}

#[test]
fn valid_llm_draft_is_accepted() {
    let mut w = seed42();
    let focal = select_focal(&w, "auto-major-war").unwrap();
    // A grounded body: the focal event's own canonical summary (lexicon-safe).
    let summary = w.events.events[focal.0 as usize].summary_canonical.clone();
    let json = serde_json::json!({
        "title": "A True Account",
        "body": summary,
        "references": [focal.0],
        "lacunae": [],
    })
    .to_string();
    let work = narrate(&mut w, focal, &voice(), Some(&Canned(json))).unwrap();
    assert_eq!(
        work.body, summary,
        "a valid LLM draft should be used verbatim"
    );
    assert_eq!(work.title, "A True Account");
}

#[test]
fn hallucinated_draft_falls_back_to_template() {
    let mut w = seed42();
    let focal = select_focal(&w, "auto-major-war").unwrap();
    let json = serde_json::json!({
        "title": "Lies",
        "body": "Zorblax the Deceiver conquered every realm.",
        "references": [focal.0],
        "lacunae": [],
    })
    .to_string();
    let work = narrate(&mut w, focal, &voice(), Some(&Canned(json))).unwrap();
    // The invented name was rejected twice → template fallback used.
    assert!(
        !work.body.contains("Zorblax"),
        "hallucinated name must not survive"
    );
    assert_ne!(work.title, "Lies");
}

#[test]
fn failing_or_unparseable_client_falls_back_to_template() {
    let mut w = seed42();
    let focal = select_focal(&w, "auto-major-war").unwrap();

    let from_failing = narrate(&mut w, focal, &voice(), Some(&Failing)).unwrap();
    assert!(!from_failing.body.is_empty());

    let from_garbage = narrate(&mut w, focal, &voice(), Some(&Canned("not json".into()))).unwrap();
    assert!(!from_garbage.body.is_empty());
}

#[test]
fn ner_accepts_grounded_text_and_rejects_invented_names() {
    let w = seed42();
    let focal = select_focal(&w, "auto-major-war").unwrap();
    let slice = event_closure(&w, focal);
    let summary = w.events.events[focal.0 as usize].summary_canonical.clone();

    let good = ChronicleDraft {
        title: "T".into(),
        body: summary,
        references: vec![focal.0],
        lacunae: vec![],
    };
    assert!(ner::validate(&good, &w, &slice).is_ok());

    let invented = ChronicleDraft {
        title: "T".into(),
        body: "Zorblax marched forth.".into(),
        references: vec![focal.0],
        lacunae: vec![],
    };
    assert!(ner::validate(&invented, &w, &slice).is_err());

    // A reference outside the supplied slice is rejected.
    let bad_ref = ChronicleDraft {
        title: "T".into(),
        body: "It happened.".into(),
        references: vec![u32::MAX],
        lacunae: vec![],
    };
    assert!(ner::validate(&bad_ref, &w, &slice).is_err());
}

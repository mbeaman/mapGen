//! 5b: prompt assembly against a real generated world (mapgen-world is a
//! dev-dependency; it doesn't depend on mapgen-lore, so there's no cycle).

use mapgen_lore::context::event_closure;
use mapgen_lore::prompt;
use mapgen_lore::voice::{Register, VoiceCard};
use mapgen_world::{generate_full, GenerateParams};

fn seed42() -> mapgen_core::WorldData {
    generate_full(GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
        periodic: false,
    })
}

/// An event that cites at least one cause (e.g. a battle citing its war).
fn linked_event(w: &mapgen_core::WorldData) -> mapgen_core::Event {
    w.events
        .events
        .iter()
        .find(|e| !e.cause_ids.is_empty())
        .expect("the history has causally-linked events")
        .clone()
}

#[test]
fn prompt_includes_bible_events_voice_and_schema() {
    let w = seed42();
    let focal = linked_event(&w).id;
    let voice = VoiceCard::for_register(Register::MonasticChronicle);
    let p = prompt::build(&w, focal, &voice);
    let user = p.user_text();

    for section in [
        "# WORLD BIBLE",
        "# ENTITY CONTEXT",
        "# SUPPLIED EVENTS",
        "# VOICE",
        "# SCHEMA",
    ] {
        assert!(user.contains(section), "prompt missing section {section}");
    }
    // The bible (the cacheable block) names a real realm.
    let nation = &w.society.nations[0].name;
    assert!(
        p.world_bible.contains(nation.as_str()),
        "world bible should name realm {nation}"
    );
    // The focal event itself is in the supplied slice.
    assert!(p.focal.contains(&format!("[{}] year", focal.0)));
    // The voice's register hint and the system rules are present.
    assert!(p.focal.contains("monastic annal"));
    assert!(p.system.contains("in-world chronicler"));
}

#[test]
fn event_closure_includes_focal_and_its_transitive_causes_sorted() {
    let w = seed42();
    let focal = linked_event(&w);
    let closure = event_closure(&w, focal.id);

    assert!(
        closure.contains(&focal.id),
        "closure must contain the focal"
    );
    for c in &focal.cause_ids {
        assert!(
            closure.contains(c),
            "closure must contain the focal's cause {c:?}"
        );
    }
    let mut sorted = closure.clone();
    sorted.sort_by_key(|e| e.0);
    assert_eq!(
        closure, sorted,
        "closure must be id-ascending (chronological)"
    );
}

#[test]
fn every_register_voice_appears_in_its_prompt() {
    let w = seed42();
    let focal = linked_event(&w).id;
    for &reg in Register::ALL {
        let p = prompt::build(&w, focal, &VoiceCard::for_register(reg));
        assert!(
            p.focal.contains(reg.style_hint()),
            "{reg:?} style hint missing from its prompt"
        );
    }
}

//! Phase-5 boundary: read-only accessors the lore engine (`mapgen-lore`) will
//! call to build chronicle prompts. Defined here, in the producer crate, as the
//! stable contract — *unused until Phase 5*, but specified and tested now so the
//! boundary is fixed. All pure functions of a finished `WorldData`.

use std::collections::BTreeSet;

use mapgen_core::{Entity, EntityId, EventId, NarrativeArc, WorldData};

/// Every proper noun the chronicler is *allowed* to write — the closed NER
/// validation set. Because every name is generated and stored, this set is
/// exact: a capitalized token in the LLM's output that isn't here is a
/// hallucination and the chronicle should be rejected.
pub fn ner_lexicon(world: &WorldData) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for e in world.entities.by_id.values() {
        let name = match e {
            Entity::Character(c) => &c.name,
            Entity::Dynasty(d) => &d.name,
            Entity::House(h) => &h.name,
            Entity::Religion(r) => &r.name,
            Entity::Artifact(a) => &a.name,
            Entity::Title(t) => &t.name,
            Entity::Megabeast(m) => &m.name,
            // Polity/Site/Deity/Claim/Culture/Language: the history sim mints
            // none of these as entities (polity names reach the set via
            // `world.society.nations` below). If a future phase mints a *named*
            // one and writes it into an event summary, add its arm here — the
            // `summaries_use_only_lexicon_names` spec will flag the omission.
            _ => continue,
        };
        set.insert(name.clone());
    }
    for n in &world.society.nations {
        set.insert(n.name.clone());
    }
    for a in &world.history.ages {
        set.insert(a.name.clone());
    }
    for a in &world.history.arcs {
        set.insert(a.title.clone());
    }
    set.remove("");
    set
}

/// A one-line description of an entity for prompt context, if it resolves.
pub fn entity_brief(world: &WorldData, id: EntityId) -> Option<String> {
    Some(match world.entities.by_id.get(&id)? {
        Entity::Character(c) => {
            let died = c
                .died_year
                .map_or_else(|| "—".to_string(), |d| d.to_string());
            format!("{} (b.{} d.{})", c.name, c.born_year, died)
        }
        Entity::Dynasty(d) => format!("the {} dynasty (founded {})", d.name, d.founded_year),
        Entity::House(h) => format!("House {}", h.name),
        Entity::Religion(r) => format!("the {} faith", r.name),
        Entity::Artifact(a) => format!("the artifact {}", a.name),
        Entity::Title(t) => t.name.clone(),
        Entity::Megabeast(m) => format!("the beast {}", m.name),
        _ => return None,
    })
}

/// Every event a chronicler needs to tell an arc: its members plus the
/// transitive closure of their causes (so "because of X, Y happened" is
/// answerable). Returned sorted by id (chronological, since id == order).
pub fn arc_event_closure(world: &WorldData, arc: &NarrativeArc) -> Vec<EventId> {
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut stack: Vec<u32> = arc.member_events.iter().map(|e| e.0).collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(e) = world.events.events.get(id as usize) {
            stack.extend(e.cause_ids.iter().map(|c| c.0));
        }
    }
    seen.into_iter().map(EventId).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mapgen_core::{ArcKind, Event, EventKind, NarrativeArc};

    fn ev(id: u32, causes: &[u32]) -> Event {
        Event {
            id: EventId(id),
            year: 0,
            kind: EventKind::Birth,
            actors: Default::default(),
            patients: Default::default(),
            location: None,
            cause_ids: causes.iter().map(|&c| EventId(c)).collect(),
            salience: 0.5,
            casus_belli: None,
            summary_canonical: String::new(),
        }
    }

    #[test]
    fn arc_event_closure_pulls_in_transitive_causes() {
        // 0 (root) <- 1 <- 2. An arc whose only member is event 2 must yield the
        // whole causal chain {0,1,2}, sorted ascending — this is the point of the
        // function (a buggy version returning members verbatim would fail here).
        let mut w = WorldData::default();
        w.events.events = vec![ev(0, &[]), ev(1, &[0]), ev(2, &[1])];
        let arc = NarrativeArc {
            title: "T".into(),
            kind: ArcKind::Chronicle,
            member_events: vec![EventId(2)],
            start_event: EventId(2),
            climax_event: EventId(2),
            end_event: EventId(2),
            key_characters: vec![],
        };
        let closure: Vec<u32> = arc_event_closure(&w, &arc).iter().map(|e| e.0).collect();
        assert_eq!(closure, vec![0, 1, 2]);
    }
}

//! ENTITY CONTEXT and EVENT SLICE assembly: the focal event plus the transitive
//! closure of its causes, the entities those events name, and their one-line
//! briefs. This is the grounded, per-call material — together with the bible it
//! defines exactly which proper nouns the narrator is allowed to use.

use std::collections::BTreeSet;

use mapgen_core::{EntityId, Event, EventId, WorldData};
use mapgen_history::lore_api::entity_brief;

/// The focal event plus the transitive closure of its `cause_ids`, ids ascending
/// (== chronological, since id == emission order). Mirrors
/// `lore_api::arc_event_closure` but seeded from a single event.
pub fn event_closure(world: &WorldData, focal: EventId) -> Vec<EventId> {
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut stack = vec![focal.0];
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

/// One brief per entity named (as actor or patient) across `events` — the
/// ENTITY CONTEXT block. Deterministic (ids ascending).
pub fn entity_context(world: &WorldData, events: &[EventId]) -> String {
    let mut ids: BTreeSet<u32> = BTreeSet::new();
    for &e in events {
        if let Some(ev) = world.events.events.get(e.0 as usize) {
            ids.extend(ev.actors.iter().map(|a| a.0));
            ids.extend(ev.patients.iter().map(|p| p.0));
        }
    }
    let mut s = String::new();
    for id in ids {
        if let Some(brief) = entity_brief(world, EntityId(id)) {
            s.push_str(&format!("- [{id}] {brief}\n"));
        }
    }
    s
}

/// The SUPPLIED EVENTS block: `id`, year, and canonical summary, one per line,
/// in chronological order.
pub fn event_slice_text(world: &WorldData, events: &[EventId]) -> String {
    let mut evs: Vec<&Event> = events
        .iter()
        .filter_map(|e| world.events.events.get(e.0 as usize))
        .collect();
    evs.sort_by_key(|e| (e.year, e.id.0));
    let mut s = String::new();
    for e in evs {
        s.push_str(&format!(
            "- [{}] year {}: {}\n",
            e.id.0, e.year, e.summary_canonical
        ));
    }
    s
}

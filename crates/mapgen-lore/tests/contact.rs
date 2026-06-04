//! Chronicle weaving — the `auto-contact` narrator surfaces the inter-continental
//! story (sea-trade routes and the embargoes that sever them) instead of only
//! wars. The contact events exist only at planet scale on a crossing seed, so
//! these tests build a real planet world via `generate_full(planet_params(11))`.

use mapgen_core::EventKind;
use mapgen_lore::select_focal;
use mapgen_lore::template::template_draft;
use mapgen_lore::voice::{Register, VoiceCard};
use mapgen_testsupport::planet_params;
use mapgen_world::generate_full;

/// A planet world on a crossing seed, where sea-trade routes provably open
/// (verified: seed 11 emits 5 `TradeRouteOpened` + 4 `EmbargoImposed`).
fn crossing_world() -> mapgen_core::WorldData {
    generate_full(planet_params(11))
}

#[test]
fn auto_contact_selects_an_inter_continental_contact_event() {
    let w = crossing_world();
    let id = select_focal(&w, "auto-contact").unwrap();
    let e = &w.events.events[id.0 as usize];
    assert!(
        matches!(
            e.kind,
            EventKind::TradeRouteOpened | EventKind::EmbargoImposed
        ),
        "auto-contact must pick a cross-water contact event, got {:?}",
        e.kind
    );
}

#[test]
fn template_draft_renders_contact_into_prose_mentioning_the_route() {
    let w = crossing_world();
    let id = select_focal(&w, "auto-contact").unwrap();
    let focal = &w.events.events[id.0 as usize];
    let voice = VoiceCard::for_register(Register::MonasticChronicle);

    // Empty slice → template injects the focal event itself (see
    // template::tests::includes_the_focal_event_even_if_omitted_from_the_slice).
    let draft = template_draft(focal, &[], &voice);

    assert!(!draft.body.is_empty(), "contact prose must be non-empty");
    assert!(!draft.title.is_empty(), "contact title must be non-empty");
    // Both contact summaries ("A sea-trade route opened between X and Y." /
    // "War severed the sea-trade route between X and Y.") name the route, so the
    // woven body must surface the inter-continental contact, not swallow it.
    assert!(
        draft.body.contains("route"),
        "contact prose must mention the trade route; got: {}",
        draft.body
    );
    assert!(
        draft.references.contains(&id.0),
        "the chronicle must cite the focal contact event"
    );
}

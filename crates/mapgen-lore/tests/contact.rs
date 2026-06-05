//! Chronicle weaving — the `auto-contact` narrator surfaces the inter-continental
//! story (sea-trade routes and the embargoes that sever them) instead of only
//! wars. The contact events exist only at planet scale on a crossing seed, so
//! these tests build a real planet world via `generate_full(planet_params(11))`.

use mapgen_core::EventKind;
use mapgen_lore::template::template_draft;
use mapgen_lore::voice::{Register, VoiceCard};
use mapgen_lore::{narrate, select_focal};
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

#[test]
fn the_first_contact_arc_weaves_the_routes_birth_and_its_severance() {
    // The first-contact arc, end to end. `auto-contact` picks the most-salient
    // contact event — on a crossing seed an `EmbargoImposed` (salience 0.45 >
    // an opening's 0.40) — and because the embargo now CITES the route opening it
    // severs (trade.rs), the focal's causal closure is a two-beat arc: the lane is
    // BORN (`TradeRouteOpened`) and KILLED (`EmbargoImposed`). The weave must
    // surface BOTH beats AND add the first-contact framing — turning two events
    // the old narrator listed flatly (or, before the cause link, could not even
    // assemble) into one Sundered-Lane chronicle.
    let mut w = crossing_world();
    let id = select_focal(&w, "auto-contact").unwrap();
    let focal = w.events.events[id.0 as usize].clone();
    assert!(
        matches!(focal.kind, EventKind::EmbargoImposed),
        "seed 11's most-salient contact event is an embargo; got {:?}",
        focal.kind
    );
    // The causal spine is present: the embargo cites EXACTLY the opening it
    // severed (one route per embargo — the invariant the "two peoples met" prose
    // relies on, so the arc is a clean two beats, not a tangle of routes).
    assert_eq!(
        focal.cause_ids.len(),
        1,
        "the embargo must cite exactly one cause (its route opening)"
    );
    let opening_id = focal.cause_ids[0];
    assert!(
        matches!(
            w.events.events[opening_id.0 as usize].kind,
            EventKind::TradeRouteOpened
        ),
        "the cited cause must be the route's opening"
    );

    let voice = VoiceCard::for_register(Register::MonasticChronicle);
    let work = narrate(&mut w, id, &voice, None).unwrap();

    // BOTH beats of the lane's life are in the prose.
    assert!(
        work.body.contains("opened between"),
        "the woven arc must narrate the route's birth; got: {}",
        work.body
    );
    assert!(
        work.body.contains("severed the sea-trade route"),
        "the woven arc must narrate the route's severance; got: {}",
        work.body
    );
    // The framing the WEAVE adds — neither marker phrase appears in any event
    // summary ("opened"/"severed") nor register frame, so each proves the framing
    // came from the weave, not the data. Pinned SEPARATELY so dropping EITHER beat
    // trips: "two peoples met" is unique to the birth beat (FIRST_CONTACT_BEAT),
    // "sundered once more" unique to the death beat (SEVERANCE_BEAT).
    assert!(
        work.body.contains("two peoples met"),
        "the weave must frame the first contact (birth beat); got: {}",
        work.body
    );
    assert!(
        work.body.contains("sundered once more"),
        "the weave must frame the severance (death beat); got: {}",
        work.body
    );
    // The chronicle cites BOTH events of the arc, and is titled on its theme.
    assert!(
        work.references.contains(&id) && work.references.contains(&opening_id),
        "the chronicle must cite both the opening and the embargo"
    );
    assert!(
        work.title.contains("Sundered Lane"),
        "a contact arc is titled on its theme; got: {}",
        work.title
    );
}

#[test]
fn a_non_contact_chronicle_gets_no_sundered_lane_framing() {
    // The Sundered-Lane framing fires ONLY for a first-contact arc. A war
    // chronicle (`auto-major-war`, whose closure is claim → war → battle, never a
    // contact event) must read in the plain register, with NO "sundered" beat and
    // a subject-titled head — else the framing is unconditional boilerplate, not a
    // weave that recognises the arc. This is the anti-false-green half: it fails if
    // the framing is added to every chronicle.
    let mut w = crossing_world();
    let id = select_focal(&w, "auto-major-war").unwrap();
    let focal = w.events.events[id.0 as usize].clone();
    assert!(
        matches!(
            focal.kind,
            EventKind::WarDeclared | EventKind::BattleFought | EventKind::Siege
        ),
        "auto-major-war must pick a war event; got {:?}",
        focal.kind
    );

    let voice = VoiceCard::for_register(Register::MonasticChronicle);
    let work = narrate(&mut w, id, &voice, None).unwrap();

    assert!(
        !work.body.contains("sundered"),
        "a war chronicle must not get the first-contact framing; got: {}",
        work.body
    );
    assert!(
        !work.title.contains("Sundered Lane"),
        "a war chronicle must not get the Sundered-Lane title; got: {}",
        work.title
    );
}

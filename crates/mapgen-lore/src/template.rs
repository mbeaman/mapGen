//! Deterministic offline narrator. Two roles:
//!   1. the narrator when no LLM client is configured (free, reproducible), and
//!   2. the mandatory fallback when an LLM's output fails NER validation twice —
//!      so the pipeline never breaks.
//!
//! NER-safe by construction: the body is built only from the events'
//! `summary_canonical` strings (which name only lexicon entities) plus fixed
//! connective prose containing no proper nouns. Because it is trusted, the
//! engine does not re-run NER validation on a template draft.

use mapgen_core::{Event, EventKind};

use crate::schema::ChronicleDraft;
use crate::voice::{Register, VoiceCard};

/// The first-contact beat, prepended when a chronicle's arc is a Sundered-Lane
/// arc (it contains a `TradeRouteOpened`). Fixed prose, no proper nouns, so the
/// body stays NER-clean. Uses "sundered" — a word that appears in no event
/// summary ("opened"/"severed") nor any register frame — so a test can pin that
/// the weave, not the data, added the framing.
const FIRST_CONTACT_BEAT: &str =
    "Across the deep that long held the shores sundered, a way was opened, \
     and two peoples met who never had before.";
/// The severance beat, appended when that same arc also carries the route's death
/// by war (an `EmbargoImposed`) — so the lane is narrated born AND killed.
const SEVERANCE_BEAT: &str =
    "But the way did not hold: war fell between them, and the shores were \
     sundered once more.";

/// Weave a chronicle from a focal event and its supporting slice (focal + the
/// transitive causes that explain it), in the given voice.
///
/// `far_shore` is the NAME of the continent an inter-continental event reached
/// (resolved by the caller from `Event::far_shore` → `world.continents`), or
/// `None`. A [`EventKind::FaithCrossed`] focal weaves it in — naming the shore the
/// faith first touched, which the bare summary deliberately omits.
pub fn template_draft(
    focal: &Event,
    slice: &[&Event],
    voice: &VoiceCard,
    far_shore: Option<&str>,
) -> ChronicleDraft {
    let mut evs: Vec<&Event> = slice.to_vec();
    if !evs.iter().any(|e| e.id == focal.id) {
        evs.push(focal);
    }
    evs.sort_by_key(|e| (e.year, e.id.0));
    evs.dedup_by_key(|e| e.id.0);

    ChronicleDraft {
        title: title_for(voice.register, focal, far_shore),
        body: render_body(voice.register, &evs, far_shore),
        references: evs.iter().map(|e| e.id.0).collect(),
        lacunae: Vec::new(),
    }
}

fn title_for(reg: Register, focal: &Event, far_shore: Option<&str>) -> String {
    // A faith's first crossing is titled on the SHORE it reached (named from
    // `far_shore`) — "The Faith Comes to Aethermoor". Falls back to the thematic
    // Sundered-Lane label if the shore is unnamed.
    if matches!(focal.kind, EventKind::FaithCrossed) {
        return match far_shore {
            Some(shore) => format!("The Faith Comes to {shore}"),
            None => "The Faith Across the Water".to_string(),
        };
    }
    // A contact focal (a sea-trade route's birth or death) is titled on the arc's
    // theme — the Sundered Lane — not its subject noun, which for an embargo is
    // the literal word "War". The label is framing, not a lexicon entity (the
    // template draft is trusted, never NER-validated).
    if matches!(
        focal.kind,
        EventKind::TradeRouteOpened | EventKind::EmbargoImposed
    ) {
        return match reg {
            Register::Saga => "A Lay of the Sundered Lane",
            Register::MonasticChronicle => "The Annal of the Sundered Lane",
            Register::Hymn => "A Hymn of the Sundered Lane",
            Register::CourtlyLetter => "Concerning the Sundered Lane",
            Register::PeasantRumor => "What They Say of the Sundered Lane",
        }
        .to_string();
    }
    // Title on the event's *subject* (its first proper noun) rather than the
    // whole summary sentence — "A Lay of Uedihi", not "A Lay of Uedihi crushed
    // Dav in the field". The subject is a lexicon name, so the title stays
    // grounded.
    match (reg, subject_of(&focal.summary_canonical)) {
        (Register::Saga, Some(s)) => format!("A Lay of {s}"),
        (Register::Saga, None) => "A Lay of the Age".to_string(),
        (Register::MonasticChronicle, Some(s)) => format!("The Annal of {s}"),
        (Register::MonasticChronicle, None) => "An Annal of the Age".to_string(),
        (Register::Hymn, Some(s)) => format!("A Hymn for {s}"),
        (Register::Hymn, None) => "A Hymn of the Age".to_string(),
        (Register::CourtlyLetter, Some(s)) => format!("Concerning {s}"),
        (Register::CourtlyLetter, None) => "A Dispatch of the Age".to_string(),
        (Register::PeasantRumor, Some(s)) => format!("What They Say of {s}"),
        (Register::PeasantRumor, None) => "Talk of the Age".to_string(),
    }
}

/// The first proper-noun-looking token in a summary (its subject), if any —
/// skipping sentence-opening common words.
fn subject_of(summary: &str) -> Option<String> {
    summary.split_whitespace().find_map(|raw| {
        let w = raw.trim_matches(|c: char| !c.is_alphanumeric());
        let w = w.strip_suffix("'s").unwrap_or(w);
        let first = w.chars().next()?;
        let common = matches!(w, "The" | "A" | "An" | "In" | "Famine");
        (first.is_uppercase() && w.chars().all(|c| c.is_alphabetic()) && !common)
            .then(|| w.to_string())
    })
}

fn render_body(reg: Register, evs: &[&Event], far_shore: Option<&str>) -> String {
    // A Sundered-Lane (first-contact) arc is framed: the birth beat opens the
    // body, the events weave between, and — if the lane was later severed — the
    // death beat closes it. The anchor is the BIRTH (`TradeRouteOpened`); the
    // severance beat is narrated only as the end of a story whose beginning we
    // told, so a stray embargo with no opening in the slice reads plainly.
    let first_contact = evs
        .iter()
        .any(|e| matches!(e.kind, EventKind::TradeRouteOpened));
    let severed = evs
        .iter()
        .any(|e| matches!(e.kind, EventKind::EmbargoImposed));
    let faith_crossing = evs
        .iter()
        .any(|e| matches!(e.kind, EventKind::FaithCrossed));

    let mut s = String::from(opening(reg));
    if first_contact {
        s.push(' ');
        s.push_str(FIRST_CONTACT_BEAT);
    }
    for e in evs {
        s.push(' ');
        if matches!(reg, Register::MonasticChronicle) {
            s.push_str(&format!("In the year {}, {}", e.year, e.summary_canonical));
        } else {
            s.push_str(&e.summary_canonical);
        }
    }
    if first_contact && severed {
        s.push(' ');
        s.push_str(SEVERANCE_BEAT);
    }
    // The faith milestone NAMES the shore it reached, read from the event's
    // `far_shore` (the bare summary says only "a far shore"). This is the consumer
    // that makes `far_shore` load-bearing: the continent name is in the chronicle
    // ONLY because the narrator read the tag — no summary nor frame carries it.
    if faith_crossing {
        if let Some(shore) = far_shore {
            s.push(' ');
            s.push_str(&format!(
                "So the faith first took root upon the shore of {shore}."
            ));
        }
    }
    s.push(' ');
    s.push_str(closing(reg));
    s
}

/// Fixed framing prose — no proper nouns, so the body stays NER-clean even if a
/// future caller ever validates a template draft.
fn opening(reg: Register) -> &'static str {
    match reg {
        Register::Saga => "Hear now the deeds of the age.",
        Register::MonasticChronicle => "Herein are set down the events of these years.",
        Register::Hymn => "Sing, then, of what came to pass.",
        Register::CourtlyLetter => "My lord, I relate the matter as it befell.",
        Register::PeasantRumor => "They say, though who can rightly tell,",
    }
}

fn closing(reg: Register) -> &'static str {
    match reg {
        Register::Saga => "So the songs remember it.",
        Register::MonasticChronicle => "Thus the record stands.",
        Register::Hymn => "Let it be remembered.",
        Register::CourtlyLetter => "I remain your faithful servant.",
        Register::PeasantRumor => "or so the telling goes.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mapgen_core::{EventId, EventKind};

    fn ev(id: u32, year: i32, summary: &str) -> Event {
        Event {
            id: EventId(id),
            year,
            kind: EventKind::BattleFought,
            actors: Default::default(),
            patients: Default::default(),
            location: None,
            cause_ids: Default::default(),
            salience: 0.9,
            casus_belli: None,
            summary_canonical: summary.to_string(),
            far_shore: None,
        }
    }

    #[test]
    fn weaves_slice_in_chronological_order_and_references_all() {
        let focal = ev(5, 200, "Uedihi crushed Dav in the field.");
        let cause = ev(2, 150, "Uedihi declared war upon Dav.");
        let voice = VoiceCard::for_register(Register::MonasticChronicle);
        let draft = template_draft(&focal, &[&focal, &cause], &voice, None);

        // chronological: the year-150 cause precedes the year-200 battle.
        let war_at = draft.body.find("declared war").unwrap();
        let crush_at = draft.body.find("crushed").unwrap();
        assert!(war_at < crush_at);
        // every supplied event is referenced, deduped.
        assert_eq!(draft.references, vec![2, 5]);
        assert!(draft.body.contains("In the year 150,"));
        assert!(!draft.title.is_empty());
    }

    #[test]
    fn includes_the_focal_event_even_if_omitted_from_the_slice() {
        let focal = ev(9, 300, "Dav made peace.");
        let voice = VoiceCard::for_register(Register::Saga);
        let draft = template_draft(&focal, &[], &voice, None);
        assert_eq!(draft.references, vec![9]);
    }

    fn ev_kind(id: u32, year: i32, kind: EventKind, summary: &str) -> Event {
        Event {
            kind,
            ..ev(id, year, summary)
        }
    }

    #[test]
    fn an_open_only_arc_gets_the_birth_beat_but_not_the_severance() {
        // A trade route that opened and was NEVER severed (no embargo in the
        // slice) is a first-contact arc with only its BIRTH beat. `auto-contact`
        // never reaches this on a crossing seed (an embargo, salience 0.45,
        // always outranks an opening's 0.40), so it is pinned synthetically here.
        // The AND-condition is the discriminator: the birth beat appears, the
        // severance beat does NOT — a route that still stands is not "sundered
        // once more". (Mutation: weaken `first_contact && severed` to `severed`
        // alone, or drop the `&& severed` guard → the severance beat leaks onto
        // an unsevered route → this trips.)
        let open = ev_kind(
            3,
            10,
            EventKind::TradeRouteOpened,
            "A sea-trade route opened between Avi and Bel.",
        );
        let voice = VoiceCard::for_register(Register::MonasticChronicle);
        let draft = template_draft(&open, &[&open], &voice, None);

        assert!(
            draft.title.contains("Sundered Lane"),
            "an opening focal is titled on the arc's theme; got: {}",
            draft.title
        );
        assert!(
            draft.body.contains("two peoples met"),
            "the birth beat must frame a first contact; got: {}",
            draft.body
        );
        assert!(
            !draft.body.contains("sundered once more"),
            "an unsevered route must NOT get the severance beat; got: {}",
            draft.body
        );
    }

    #[test]
    fn a_born_and_died_arc_gets_both_beats() {
        // The full lane life in one slice: birth (`TradeRouteOpened`) + death
        // (`EmbargoImposed`). BOTH framing beats must appear — the synthetic twin
        // of the seed-11 integration test, isolating the weave from the pipeline.
        let open = ev_kind(
            3,
            10,
            EventKind::TradeRouteOpened,
            "A sea-trade route opened between Avi and Bel.",
        );
        let embargo = ev_kind(
            4,
            42,
            EventKind::EmbargoImposed,
            "War severed the sea-trade route between Avi and Bel.",
        );
        let voice = VoiceCard::for_register(Register::MonasticChronicle);
        let draft = template_draft(&embargo, &[&open, &embargo], &voice, None);

        assert!(draft.body.contains("two peoples met"), "missing birth beat");
        assert!(
            draft.body.contains("sundered once more"),
            "missing severance beat; got: {}",
            draft.body
        );
        // Chronological: the birth is narrated before the death.
        let birth = draft.body.find("opened between").unwrap();
        let death = draft.body.find("severed the sea-trade").unwrap();
        assert!(birth < death, "the route must be born before it dies");
    }

    #[test]
    fn a_faith_crossing_names_the_far_shore_from_the_tag() {
        // The consumer of `far_shore`: a FaithCrossed focal whose bare summary says
        // only "a far shore" gets the SHORE NAMED in both title and body — but the
        // name is the caller-supplied `far_shore`, NEVER the summary. So the name
        // appearing proves the narrator read the tag. "Aethermoor" is in no summary
        // nor frame. (Mutation: drop the far-shore beat / title branch → the name
        // vanishes → trips.)
        let faith = ev_kind(
            7,
            120,
            EventKind::FaithCrossed,
            "The Verdance faith was carried over the open water to a far shore.",
        );
        let voice = VoiceCard::for_register(Register::MonasticChronicle);

        let named = template_draft(&faith, &[&faith], &voice, Some("Aethermoor"));
        assert!(
            named.title.contains("Aethermoor"),
            "the title must name the reached shore; got: {}",
            named.title
        );
        assert!(
            named.body.contains("Aethermoor") && named.body.contains("took root upon the shore"),
            "the body must name the reached shore (from far_shore, not the summary); got: {}",
            named.body
        );

        // No tag (an unnamed speck shore): NO far-shore beat — the narrator does not
        // invent a name. This is the negative half: the beat is gated on the tag.
        let unnamed = template_draft(&faith, &[&faith], &voice, None);
        assert!(
            !unnamed.body.contains("took root upon the shore"),
            "an unnamed shore must get no far-shore beat; got: {}",
            unnamed.body
        );

        // The tag is consumed ONLY for a FaithCrossed focal: a non-faith event
        // handed the same `far_shore` gets no faith beat (so the framing can't leak
        // onto, say, a war chronicle that happened to be tagged).
        let war = ev_kind(
            8,
            120,
            EventKind::BattleFought,
            "Avi crushed Bel in the field.",
        );
        let mislabelled = template_draft(&war, &[&war], &voice, Some("Aethermoor"));
        assert!(
            !mislabelled.body.contains("took root upon the shore"),
            "a non-faith chronicle must not get the faith beat; got: {}",
            mislabelled.body
        );
    }
}

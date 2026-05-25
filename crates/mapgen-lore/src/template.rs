//! Deterministic offline narrator. Two roles:
//!   1. the narrator when no LLM client is configured (free, reproducible), and
//!   2. the mandatory fallback when an LLM's output fails NER validation twice —
//!      so the pipeline never breaks.
//!
//! NER-safe by construction: the body is built only from the events'
//! `summary_canonical` strings (which name only lexicon entities) plus fixed
//! connective prose containing no proper nouns. Because it is trusted, the
//! engine does not re-run NER validation on a template draft.

use mapgen_core::Event;

use crate::schema::ChronicleDraft;
use crate::voice::{Register, VoiceCard};

/// Weave a chronicle from a focal event and its supporting slice (focal + the
/// transitive causes that explain it), in the given voice.
pub fn template_draft(focal: &Event, slice: &[&Event], voice: &VoiceCard) -> ChronicleDraft {
    let mut evs: Vec<&Event> = slice.to_vec();
    if !evs.iter().any(|e| e.id == focal.id) {
        evs.push(focal);
    }
    evs.sort_by_key(|e| (e.year, e.id.0));
    evs.dedup_by_key(|e| e.id.0);

    ChronicleDraft {
        title: title_for(voice.register, focal),
        body: render_body(voice.register, &evs),
        references: evs.iter().map(|e| e.id.0).collect(),
        lacunae: Vec::new(),
    }
}

fn title_for(reg: Register, focal: &Event) -> String {
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

fn render_body(reg: Register, evs: &[&Event]) -> String {
    let mut s = String::from(opening(reg));
    for e in evs {
        s.push(' ');
        if matches!(reg, Register::MonasticChronicle) {
            s.push_str(&format!("In the year {}, {}", e.year, e.summary_canonical));
        } else {
            s.push_str(&e.summary_canonical);
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
        }
    }

    #[test]
    fn weaves_slice_in_chronological_order_and_references_all() {
        let focal = ev(5, 200, "Uedihi crushed Dav in the field.");
        let cause = ev(2, 150, "Uedihi declared war upon Dav.");
        let voice = VoiceCard::for_register(Register::MonasticChronicle);
        let draft = template_draft(&focal, &[&focal, &cause], &voice);

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
        let draft = template_draft(&focal, &[], &voice);
        assert_eq!(draft.references, vec![9]);
    }
}

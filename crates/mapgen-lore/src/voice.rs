//! The in-world author and literary register a chronicle is written in. Shapes
//! the prompt (and the offline template narrator's phrasing); the chosen author
//! is recorded on the resulting [`mapgen_core::Work`].

use serde::{Deserialize, Serialize};

/// Literary register — the voice the narrator writes in. Append-only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Register {
    /// Heroic, oral — the deeds of champions and beasts.
    Saga,
    /// Dry, dated, clerical — a monastic year-by-year annal.
    MonasticChronicle,
    /// Reverent, liturgical — praise and lament.
    Hymn,
    /// Formal, first-person — a dispatch between courts.
    CourtlyLetter,
    /// Colloquial, hearsay, unreliable — what the commons say.
    PeasantRumor,
}

impl Register {
    /// Every register, in declaration order — for CLI listing / defaulting.
    pub const ALL: &'static [Register] = &[
        Register::Saga,
        Register::MonasticChronicle,
        Register::Hymn,
        Register::CourtlyLetter,
        Register::PeasantRumor,
    ];

    /// Stable kebab-case identifier (CLI parse + display).
    pub fn as_str(self) -> &'static str {
        match self {
            Register::Saga => "saga",
            Register::MonasticChronicle => "monastic-chronicle",
            Register::Hymn => "hymn",
            Register::CourtlyLetter => "courtly-letter",
            Register::PeasantRumor => "peasant-rumor",
        }
    }

    /// Parse the kebab-case identifier.
    pub fn parse(s: &str) -> Option<Register> {
        Register::ALL.iter().copied().find(|r| r.as_str() == s)
    }

    /// One-line instruction for the prompt's VOICE_CARD.
    pub fn style_hint(self) -> &'static str {
        match self {
            Register::Saga => {
                "Write as a heroic oral saga: vivid, declamatory, dwelling on deeds and fate."
            }
            Register::MonasticChronicle => {
                "Write as a terse monastic annal: dated, plain, year by year, without embellishment."
            }
            Register::Hymn => {
                "Write as a liturgical hymn: reverent, rhythmic, given to praise and lament."
            }
            Register::CourtlyLetter => {
                "Write as a formal first-person letter from one court to another: measured and diplomatic."
            }
            Register::PeasantRumor => {
                "Write as common hearsay: colloquial, hedged, openly unsure of its own facts."
            }
        }
    }
}

/// Who is writing, and in what register. `author` is recorded on the `Work`.
#[derive(Clone, Debug)]
pub struct VoiceCard {
    pub author: String,
    pub register: Register,
}

impl VoiceCard {
    /// A generic in-world author phrase for a register, used when the caller
    /// doesn't supply a named author. Deliberately *not* a proper noun (so it
    /// never trips NER validation if it surfaces in the body).
    pub fn for_register(register: Register) -> VoiceCard {
        let author = match register {
            Register::Saga => "an anonymous skald",
            Register::MonasticChronicle => "a cloistered annalist",
            Register::Hymn => "a temple cantor",
            Register::CourtlyLetter => "a court envoy",
            Register::PeasantRumor => "a traveller repeating common talk",
        }
        .to_string();
        VoiceCard { author, register }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_str_round_trips_for_all() {
        for &r in Register::ALL {
            assert_eq!(
                Register::parse(r.as_str()),
                Some(r),
                "{r:?} did not round-trip"
            );
            assert!(!r.style_hint().is_empty());
        }
        assert_eq!(Register::ALL.len(), 5);
        assert_eq!(Register::parse("not-a-register"), None);
    }
}

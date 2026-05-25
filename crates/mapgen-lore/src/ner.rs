//! Anti-hallucination validation of a narrator draft. Two checks, per the
//! architecture: every `reference` must be one of the supplied events, and every
//! proper noun in the body must be a real world name (present in the closed
//! `ner_lexicon`). Names are deterministic, so the lexicon is exact.
//!
//! Proper-noun detection without an NLP model is heuristic: a capitalized,
//! alphabetic token is treated as a name unless it's a common English word
//! (`STOPWORDS`) or a token of some lexicon name. That can false-positive on an
//! unusual capitalized common word — which only costs a retry and then the
//! (always-valid) template fallback, never a crash. The stoplist is meant to be
//! refined against real model output once the Anthropic client lands.

use std::collections::BTreeSet;

use mapgen_core::{EventId, WorldData};
use mapgen_history::lore_api::ner_lexicon;

use crate::schema::ChronicleDraft;

/// Validate a draft against the world. `slice` is the set of events supplied to
/// the narrator (its `references` must be drawn from these). Returns a
/// human-readable violation on failure (fed back into the retry prompt).
pub fn validate(
    draft: &ChronicleDraft,
    world: &WorldData,
    slice: &[EventId],
) -> Result<(), String> {
    let supplied: BTreeSet<u32> = slice.iter().map(|e| e.0).collect();
    for &r in &draft.references {
        if !supplied.contains(&r) {
            return Err(format!(
                "cited event [{r}] was not among the supplied events"
            ));
        }
    }

    let lexicon = ner_lexicon(world);
    // Decompose names into whitespace tokens, so a multi-word name validates
    // token-by-token ("House Varn" allows both "House" and "Varn").
    let allowed: BTreeSet<&str> = lexicon.iter().flat_map(|n| n.split_whitespace()).collect();

    for raw in draft.body.split_whitespace() {
        let w = raw.trim_matches(|c: char| !c.is_alphanumeric());
        let w = w.strip_suffix("'s").unwrap_or(w);
        let Some(first) = w.chars().next() else {
            continue;
        };
        if !first.is_uppercase() || !w.chars().all(|c| c.is_alphabetic()) {
            continue; // not a proper-noun candidate
        }
        if allowed.contains(w) || STOPWORDS.contains(&w.to_lowercase().as_str()) {
            continue;
        }
        return Err(format!(
            "body names \"{w}\", which is not in the supplied world context (possible hallucination)"
        ));
    }
    Ok(())
}

/// Common English words that legitimately appear capitalized (sentence-initial
/// or otherwise). Lowercased comparison. Not exhaustive by design — see the
/// module note.
const STOPWORDS: &[&str] = &[
    // articles / conjunctions / prepositions
    "the",
    "a",
    "an",
    "and",
    "but",
    "or",
    "nor",
    "for",
    "yet",
    "so",
    "of",
    "to",
    "in",
    "on",
    "at",
    "by",
    "with",
    "from",
    "into",
    "onto",
    "unto",
    "upon",
    "over",
    "under",
    "above",
    "below",
    "between",
    "among",
    "amid",
    "amidst",
    "against",
    "through",
    "throughout",
    "during",
    "before",
    "after",
    "since",
    "until",
    "till",
    "while",
    "as",
    "than",
    "about",
    "across",
    "beyond",
    "within",
    "without",
    "toward",
    "towards",
    // pronouns / determiners
    "i",
    "it",
    "its",
    "he",
    "him",
    "his",
    "she",
    "her",
    "hers",
    "they",
    "them",
    "their",
    "theirs",
    "we",
    "us",
    "our",
    "ours",
    "you",
    "your",
    "yours",
    "this",
    "that",
    "these",
    "those",
    "such",
    "all",
    "any",
    "both",
    "each",
    "every",
    "few",
    "more",
    "most",
    "other",
    "others",
    "some",
    "no",
    "none",
    "not",
    "only",
    "own",
    "same",
    "very",
    "who",
    "whom",
    "whose",
    "which",
    "what",
    "when",
    "where",
    "why",
    "how",
    // common verbs / adverbs / interjections a chronicler opens with
    "is",
    "was",
    "were",
    "be",
    "been",
    "being",
    "are",
    "am",
    "had",
    "has",
    "have",
    "did",
    "do",
    "does",
    "can",
    "will",
    "shall",
    "should",
    "would",
    "could",
    "may",
    "might",
    "must",
    "let",
    "now",
    "then",
    "thus",
    "hence",
    "therefore",
    "here",
    "there",
    "thence",
    "thereafter",
    "lo",
    "behold",
    "hear",
    "sing",
    "yea",
    "nay",
    "alas",
    "indeed",
    "verily",
    // numerals / ordinals a chronicle uses
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "last",
    "many",
    "much",
    "great",
    "long",
    "old",
    "new",
    // recurring domain words (capitalized in prose but not names)
    "year",
    "years",
    "age",
    "realm",
    "realms",
    "war",
    "wars",
    "peace",
    "faith",
    "god",
    "gods",
    "beast",
    "champion",
    "prophecy",
    "throne",
    "crown",
    "dynasty",
    "house",
    "holy",
    "battle",
    "siege",
    "king",
    "queen",
    "lord",
    "lady",
    "north",
    "south",
    "east",
    "west",
];

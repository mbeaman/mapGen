//! Phonotactic name generation. The generator is a pure function of a
//! [`Language`] (which lives in this crate) plus an RNG, so every crate that
//! holds a `Language` can name things identically: `mapgen-world`'s naming
//! stage names settlements / polities / features, and `mapgen-history` names
//! characters, houses, and dynasties from the same per-culture languages.
//!
//! Keeping the one generator here (rather than in `mapgen-world`) is what lets
//! `mapgen-history` — which must not depend on `mapgen-world` — reuse it.

use rand_chacha::{rand_core::RngCore, ChaCha8Rng};

use crate::entities::Language;

/// Generate a single phonotactic name from `lang`. Walks `min..=max`
/// syllables, each a randomly chosen syllable pattern (`C` = consonant,
/// `V` = vowel, other chars literal), and capitalizes the result. The RNG is
/// advanced deterministically, so the same `(lang, rng-state)` yields the same
/// name across runs and targets.
pub fn generate_name(lang: &Language, rng: &mut ChaCha8Rng) -> String {
    let span = lang.max_syllables - lang.min_syllables + 1;
    let n_syllables = lang.min_syllables + (rng.next_u32() % span as u32) as u8;
    let mut out = String::new();
    for _ in 0..n_syllables {
        let pat = &lang.syllable_patterns[(rng.next_u32() as usize) % lang.syllable_patterns.len()];
        for ch in pat.chars() {
            let picked = match ch {
                'C' => lang.consonants[(rng.next_u32() as usize) % lang.consonants.len()],
                'V' => lang.vowels[(rng.next_u32() as usize) % lang.vowels.len()],
                literal => literal,
            };
            out.push(picked);
        }
    }
    capitalize(&out)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

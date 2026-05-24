//! Deterministic RNG harness. One master seed feeds independent sub-streams
//! per pipeline stage so re-rolling a stage cannot perturb downstream stages.

use rand_chacha::{rand_core::SeedableRng, ChaCha8Rng};

/// Pipeline stages, each with an independent RNG sub-stream. Discriminants are
/// part of the determinism contract — never renumber an existing stage; only
/// append new ones.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum Stage {
    Mesh = 1,
    Plates = 2,
    Noise = 3,
    Erosion = 4,
    Hydro = 5,
    Climate = 6,
    Capitals = 7,
    Hierarchy = 8,
    Roads = 9,
    Names = 10,
    History = 11,
    Render = 12,
    /// Phase 3a — cultures stage. Conceptually runs between Climate and
    /// Capitals; appended here to keep discriminants stable.
    Cultures = 13,
    /// Phase 3b — religions stage. Runs after Cultures, before Capitals.
    Religions = 14,
}

/// Lightweight RNG factory. Hold the master seed; mint a fresh
/// `ChaCha8Rng` for each stage on demand.
#[derive(Copy, Clone, Debug)]
pub struct StageRng {
    master: u64,
}

impl StageRng {
    pub fn new(master: u64) -> Self {
        Self { master }
    }

    pub fn master(&self) -> u64 {
        self.master
    }

    /// Build a fresh sub-stream RNG for the given stage.
    /// Uses SplitMix64 to mix master + stage discriminant into a fresh seed —
    /// avoids any reliance on ChaCha's `set_stream` API surface (stable, simple,
    /// portable).
    pub fn stream(&self, stage: Stage) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(splitmix64(self.master, stage as u64))
    }
}

/// SplitMix64 finalizer over a `(seed, key)` pair. Pure, portable, and the
/// single mixing primitive the whole project derives sub-streams with: the
/// per-stage RNG above uses it as `splitmix64(master, stage)`, and the history
/// sim uses it one level deeper (`splitmix64(sim_seed, year)` →
/// `splitmix64(year_seed, loop_id)`) so re-rolling one loop in one year cannot
/// perturb any other loop or year. Exposed so every crate mixes identically —
/// determinism depends on there being exactly one such function.
#[inline]
pub fn splitmix64(seed: u64, key: u64) -> u64 {
    let mut x = seed.wrapping_add(key.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::rand_core::RngCore;

    #[test]
    fn stages_are_independent() {
        let h = StageRng::new(42);
        let mut a = h.stream(Stage::Mesh);
        let mut b = h.stream(Stage::Plates);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn cultures_stage_has_an_independent_stream() {
        // Phase 3a added `Stage::Cultures = 13` (discriminant appended per
        // the determinism contract). Pin that it actually produces
        // an independent stream from every other stage at the same master
        // seed — a copy-paste discriminant collision (e.g., Cultures = 12)
        // would break determinism silently. Comparing the first u64 is
        // enough: SplitMix64 of distinct discriminants returns distinct
        // seeds with overwhelming probability.
        let h = StageRng::new(42);
        let cultures_first = h.stream(Stage::Cultures).next_u64();
        for other in [
            Stage::Mesh,
            Stage::Plates,
            Stage::Noise,
            Stage::Erosion,
            Stage::Hydro,
            Stage::Climate,
            Stage::Capitals,
            Stage::Hierarchy,
            Stage::Roads,
            Stage::Names,
            Stage::History,
            Stage::Render,
            Stage::Religions,
        ] {
            let other_first = h.stream(other).next_u64();
            assert_ne!(
                cultures_first, other_first,
                "Stage::Cultures stream collides with Stage::{other:?} at seed 42"
            );
        }
    }

    #[test]
    fn history_stage_has_an_independent_stream() {
        // Phase 4 builds the history sim on `Stage::History = 11` and derives
        // every per-year / per-loop sub-seed from it via `splitmix64`. Pin that
        // the stage stream itself is distinct from every other stage at a fixed
        // master seed — a discriminant collision would make history co-vary
        // with another stage and silently break determinism.
        let h = StageRng::new(42);
        let history_first = h.stream(Stage::History).next_u64();
        for other in [
            Stage::Mesh,
            Stage::Plates,
            Stage::Noise,
            Stage::Erosion,
            Stage::Hydro,
            Stage::Climate,
            Stage::Capitals,
            Stage::Hierarchy,
            Stage::Roads,
            Stage::Names,
            Stage::Render,
            Stage::Cultures,
            Stage::Religions,
        ] {
            assert_ne!(
                history_first,
                h.stream(other).next_u64(),
                "Stage::History stream collides with Stage::{other:?} at seed 42"
            );
        }
    }

    #[test]
    fn splitmix64_is_pure_and_mixes_both_inputs() {
        // The single mixing primitive the whole project derives sub-streams
        // with (per-stage, and the history sim one level deeper). Pin purity
        // and that both arguments actually affect the output.
        assert_eq!(splitmix64(42, 7), splitmix64(42, 7));
        assert_ne!(splitmix64(42, 7), splitmix64(43, 7));
        assert_ne!(splitmix64(42, 7), splitmix64(42, 8));
    }

    #[test]
    fn same_stage_same_master_same_stream() {
        let h = StageRng::new(42);
        let mut a = h.stream(Stage::Mesh);
        let mut b = h.stream(Stage::Mesh);
        for _ in 0..8 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}

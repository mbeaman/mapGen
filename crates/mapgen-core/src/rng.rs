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

#[inline]
fn splitmix64(master: u64, stage: u64) -> u64 {
    let mut x = master.wrapping_add(stage.wrapping_mul(0x9E37_79B9_7F4A_7C15));
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
    fn same_stage_same_master_same_stream() {
        let h = StageRng::new(42);
        let mut a = h.stream(Stage::Mesh);
        let mut b = h.stream(Stage::Mesh);
        for _ in 0..8 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}

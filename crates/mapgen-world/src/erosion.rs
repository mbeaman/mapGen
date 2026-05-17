//! Hydraulic + thermal erosion. Phase 2.
//!
//! Plan:
//!   1. Hydraulic: Job Talle / Sebastian Lague droplet model. Spawn ~50k
//!      droplets at random points; each carries water + sediment, follows
//!      steepest descent for N steps, picks up sediment proportional to
//!      capacity gap, drops it when capacity exceeded, dies on coast or
//!      after step budget. Deterministic given a seeded RNG.
//!   2. Thermal: redistribute material along slopes steeper than the talus
//!      angle so cliffs soften. Single or two passes over the cell graph.
//!
//! Determinism contract: every transcendental routes through
//! `mapgen_core::fmath`; RNG comes from the caller (typically Stage::Erosion).

use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Debug)]
pub struct ErosionParams {
    pub droplets: usize,
    pub max_steps: usize,
    pub inertia: f32,
    pub capacity_factor: f32,
    pub thermal_passes: usize,
    pub talus_angle: f32,
}

impl Default for ErosionParams {
    fn default() -> Self {
        Self {
            droplets: 50_000,
            max_steps: 64,
            inertia: 0.05,
            capacity_factor: 4.0,
            thermal_passes: 2,
            talus_angle: 0.05,
        }
    }
}

pub fn run(_world: &mut WorldData, _params: ErosionParams, _rng: &mut ChaCha8Rng) {
    todo!("Phase 2: hydraulic droplet + thermal erosion")
}

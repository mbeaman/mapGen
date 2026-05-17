//! Multi-octave noise overlay with domain warping (Inigo Quilez style).
//!
//! Adds high-frequency detail on top of the plate-driven base elevation
//! so erosion has texture to incise into. Without this, the Phase-1
//! heightmap is too smooth — stream-power has nothing to concentrate
//! into and would just dampen the gentle plate gradients.
//!
//! Determinism contract: noise seed comes from `Stage::Noise`'s
//! sub-stream; outputs are pure functions of cell position.

use mapgen_core::{fmath, WorldData};
use noise::{Fbm, MultiFractal, NoiseFn, OpenSimplex};
use rand::Rng;
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Debug)]
pub struct NoiseParams {
    /// Peak amplitude added to existing elevation.
    pub amplitude: f32,
    /// Spatial frequency of the heightmap noise (per unit map width).
    pub frequency: f64,
    /// Number of fBM octaves.
    pub octaves: usize,
    /// Domain-warp magnitude in map-units.
    pub warp_amount: f64,
    /// Domain-warp noise frequency.
    pub warp_frequency: f64,
}

impl Default for NoiseParams {
    fn default() -> Self {
        Self {
            amplitude: 0.35,
            frequency: 0.004,
            octaves: 5,
            warp_amount: 80.0,
            warp_frequency: 0.002,
        }
    }
}

pub fn overlay(world: &mut WorldData, params: NoiseParams, rng: &mut ChaCha8Rng) {
    let n = world.mesh.cell_count();
    let seed = rng.gen::<u32>();
    let height = Fbm::<OpenSimplex>::new(seed)
        .set_octaves(params.octaves)
        .set_frequency(params.frequency)
        .set_persistence(0.5);
    let warp_x = Fbm::<OpenSimplex>::new(seed.wrapping_add(0x9E37_79B9))
        .set_octaves(3)
        .set_frequency(params.warp_frequency);
    let warp_y = Fbm::<OpenSimplex>::new(seed.wrapping_add(0xBF58_476D))
        .set_octaves(3)
        .set_frequency(params.warp_frequency);

    for i in 0..n {
        let s = world.mesh.sites[i];
        let (sx, sy) = (s[0] as f64, s[1] as f64);
        let wx = warp_x.get([sx, sy]) * params.warp_amount;
        let wy = warp_y.get([sx, sy]) * params.warp_amount;
        let raw = height.get([sx + wx, sy + wy]) as f32;
        let new_elev = world.terrain.elevation[i] + raw * params.amplitude;
        world.terrain.elevation[i] = fmath::clamp(new_elev, -1.0, 1.0);
    }
}

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

    // Periodic world → sample noise on a CYLINDER so x=0 and x=width are the same
    // meridian (no elevation seam). `None` → the legacy flat path (byte-identical).
    let period = world.mesh.periodic.then_some(world.mesh.width as f64);
    for i in 0..n {
        let s = world.mesh.sites[i];
        let raw = warped_height(
            &height,
            &warp_x,
            &warp_y,
            s[0] as f64,
            s[1] as f64,
            params.warp_amount,
            period,
        ) as f32;
        let new_elev = world.terrain.elevation[i] + raw * params.amplitude;
        world.terrain.elevation[i] = fmath::clamp(new_elev, -1.0, 1.0);
    }
}

/// Domain-warped height sample at a world point.
///
/// FLAT (`period == None`): the legacy `height([x+wx, y+wy])` with warp from `(x,y)` —
/// byte-identical to before this helper existed.
///
/// PERIODIC (`period == Some(width)`): longitude maps to a circle of radius `R = width/TAU`
/// (so arc-length per radian ≈ the flat x-scale → local feature size unchanged), sampled as
/// 3D noise `[R·cosθ, R·sinθ, y]`. Because `θ(0)=0` and `θ(width)=TAU` land on the SAME point,
/// x=0 and x=width sample identically — geography is continuous across the antimeridian. The
/// domain warp shifts the longitude (`θ(x+wx)`), so the warp wraps too. Trig routes through
/// `fmath` (libm) → cross-platform deterministic. NOTE: a 2D `cos`-only map would be a bug
/// (cos is even → θ and −θ collide → a mirror-image planet); the 3D `[cos, sin, y]` is correct.
fn warped_height(
    height: &Fbm<OpenSimplex>,
    warp_x: &Fbm<OpenSimplex>,
    warp_y: &Fbm<OpenSimplex>,
    sx: f64,
    sy: f64,
    warp_amount: f64,
    period: Option<f64>,
) -> f64 {
    match period {
        None => {
            let wx = warp_x.get([sx, sy]) * warp_amount;
            let wy = warp_y.get([sx, sy]) * warp_amount;
            height.get([sx + wx, sy + wy])
        }
        Some(w) => {
            let r = w / std::f64::consts::TAU;
            let cyl = |x: f64| {
                let th = (x / w) as f32 * std::f32::consts::TAU;
                (r * fmath::cos(th) as f64, r * fmath::sin(th) as f64)
            };
            let (cx, cz) = cyl(sx);
            let wx = warp_x.get([cx, cz, sy]) * warp_amount;
            let wy = warp_y.get([cx, cz, sy]) * warp_amount;
            let (hx, hz) = cyl(sx + wx); // the warp shifts longitude → wraps with it
            height.get([hx, hz, sy + wy])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_noise_is_continuous_across_the_seam() {
        let p = NoiseParams::default();
        let seed = 42u32;
        let height = Fbm::<OpenSimplex>::new(seed)
            .set_octaves(p.octaves)
            .set_frequency(p.frequency)
            .set_persistence(0.5);
        let warp_x = Fbm::<OpenSimplex>::new(seed.wrapping_add(0x9E37_79B9))
            .set_octaves(3)
            .set_frequency(p.warp_frequency);
        let warp_y = Fbm::<OpenSimplex>::new(seed.wrapping_add(0xBF58_476D))
            .set_octaves(3)
            .set_frequency(p.warp_frequency);
        let w = 2048.0_f64;
        let y = 357.0_f64;
        let cyl = |sx| warped_height(&height, &warp_x, &warp_y, sx, y, p.warp_amount, Some(w));

        // PERIODICITY: x=0 and x=width are the SAME meridian → the same sample (to the
        // float precision of cos(TAU) vs cos(0)). The flat path samples uncorrelated noise
        // 2048 apart here (a large diff), so this is the assertion that reverting the
        // cylinder mapping turns red.
        assert!(
            (cyl(0.0) - cyl(w)).abs() < 1e-4,
            "periodic noise discontinuous at the seam: {} vs {}",
            cyl(0.0),
            cyl(w)
        );
        // NON-DEGENERATE: longitude actually varies the field (not a constant that would
        // satisfy the continuity check vacuously).
        assert!(
            (cyl(0.0) - cyl(w * 0.5)).abs() > 1e-3,
            "noise is constant in longitude"
        );
    }
}

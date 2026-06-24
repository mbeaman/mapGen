//! Baked hillshade (the Relief addendum §3) — the relief VISIBILITY floor.
//!
//! The globe stays unlit (`MeshBasicMaterial`, the paper-globe doctrine), so
//! the 3D reading comes from Lambert shading baked into the tile raster here,
//! off-thread and deterministic. The spike measured this as ~90% of the relief
//! effect: displacement alone is invisible at nadir; shading alone reads as
//! terrain at every camera.
//!
//! Two halves share one shade source: [`lambert_grid`] (per-node shade factors
//! — also the cross-platform golden surface) and [`shade_rgba`] (bilinear
//! per-texel multiply into the raster, RGB only).
//!
//! fmath purity: only `+ - * /` and IEEE `sqrt`.

/// Shading knobs (tuning_log.md: `ShadeParams`).
pub struct ShadeParams {
    /// Slope scale per grid step — how strongly height gradients tilt the
    /// shading normal. TUNING.
    pub strength: f32,
    /// Shade floor: away-facing slopes fall toward this, never black. TUNING.
    /// Must stay in `[0.5, 1.0]` so the flat-field identity is exact in f32
    /// (`ambient + (1 - ambient) == 1.0` relies on Sterbenz at `a ≥ 0.5`).
    pub ambient: f32,
    /// Light direction in raster space (x right, y DOWN): NW-ish overhead.
    pub light: [f32; 3],
}

impl Default for ShadeParams {
    fn default() -> Self {
        // Spike-read values: NW light, gentle floor — terrain reads without
        // crushing the parchment palette.
        Self {
            strength: 14.0,
            ambient: 0.6,
            light: [-1.0, -1.0, 1.25],
        }
    }
}

/// Per-node Lambert shade-factor grid over a `gw × gh` heightfield (row-major,
/// row 0 = north). Central differences inside, one-sided at the edges.
///
/// Per node: `n = (-s·gx, -s·gy, 1)`, `lambert = max(0, n·L̂) / |n|`,
/// `f = ambient + (1 − ambient) · lambert / L̂z`.
///
/// Normalized so a FLAT field gives `f ≡ 1.0` EXACTLY: flat ⇒ `n = (0,0,1)` ⇒
/// `lambert = L̂z` ⇒ `lambert / L̂z = 1.0` (IEEE x/x) ⇒
/// `f = ambient + (1 − ambient) = 1.0` (exact for `ambient ≥ 0.5`). Shading a
/// flat/sea tile is therefore a byte-no-op on the raster — light-facing slopes
/// brighten past 1, away-facing fall toward `ambient`.
pub fn lambert_grid(heights: &[f32], gw: u32, gh: u32, p: &ShadeParams) -> Vec<f32> {
    assert_eq!(heights.len(), (gw * gh) as usize, "heights len != gw*gh");
    let llen = (p.light[0] * p.light[0] + p.light[1] * p.light[1] + p.light[2] * p.light[2]).sqrt();
    let lx = p.light[0] / llen;
    let ly = p.light[1] / llen;
    let lz = p.light[2] / llen;
    let one_minus_a = 1.0 - p.ambient;
    let s = p.strength;
    let at = |i: i64, j: i64| -> f32 {
        let i = i.clamp(0, gw as i64 - 1);
        let j = j.clamp(0, gh as i64 - 1);
        heights[(j * gw as i64 + i) as usize]
    };
    let mut out = Vec::with_capacity((gw * gh) as usize);
    for j in 0..gh as i64 {
        for i in 0..gw as i64 {
            // SEAM-2 (R4): central difference INSIDE; the CROSS-edge gradient is
            // ZEROED at a tile boundary. A one-sided cross-edge difference would
            // measure the slope on THIS tile's side only — the neighbour measures
            // the opposite side, so the two disagree by up to ~0.18 lambert and
            // draw a bright/dark line at every sector seam (measured 0.177 ≫ the
            // 0.005 tolerance, the un-fixed one-sided form). Zeroing makes edge
            // shading depend only on the ALONG-edge slope, which is bit-identical
            // across tiles (the edge heights are — SEAM-1) ⇒ bit-identical edge
            // lambert, no seam line. Cost: a one-node band loses its cross-edge
            // relief shading (named, far better than the discontinuity); the
            // along-edge slope and ALL interior shading are untouched.
            let gx = if i == 0 || i == gw as i64 - 1 {
                0.0
            } else {
                (at(i + 1, j) - at(i - 1, j)) * 0.5
            };
            let gy = if j == 0 || j == gh as i64 - 1 {
                0.0
            } else {
                (at(i, j + 1) - at(i, j - 1)) * 0.5
            };
            let nx = -s * gx;
            let ny = -s * gy;
            let ndotl = nx * lx + ny * ly + lz; // nz = 1
            let nlen = (nx * nx + ny * ny + 1.0).sqrt();
            let lambert = (ndotl / nlen).max(0.0);
            out.push(p.ambient + one_minus_a * (lambert / lz));
        }
    }
    out
}

/// Multiply the shade grid into an RGBA raster: per texel, bilinear-sample the
/// lambert grid (texel centres mapped onto the node lattice) and scale RGB.
/// ALPHA IS NEVER TOUCHED — the per-texel `min_alpha == 255` invariant
/// (CLAIMS: putImageData premultiply safety) holds by construction.
pub fn shade_rgba(rgba: &mut [u8], w: u32, h: u32, lambert: &[f32], gw: u32, gh: u32) {
    assert_eq!(rgba.len(), (w * h * 4) as usize, "rgba len != w*h*4");
    assert_eq!(lambert.len(), (gw * gh) as usize, "lambert len != gw*gh");
    for y in 0..h {
        // Map texel centres onto the node lattice [0, g-1].
        let gy = (y as f32 + 0.5) / h as f32 * (gh - 1) as f32;
        let j0 = (gy as u32).min(gh - 2);
        let fy = gy - j0 as f32;
        for x in 0..w {
            let gx = (x as f32 + 0.5) / w as f32 * (gw - 1) as f32;
            let i0 = (gx as u32).min(gw - 2);
            let fx = gx - i0 as f32;
            let idx = |i: u32, j: u32| lambert[(j * gw + i) as usize];
            let top = idx(i0, j0) + (idx(i0 + 1, j0) - idx(i0, j0)) * fx;
            let bot = idx(i0, j0 + 1) + (idx(i0 + 1, j0 + 1) - idx(i0, j0 + 1)) * fx;
            let f = top + (bot - top) * fy;
            let o = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                let v = rgba[o + c] as f32 * f;
                rgba[o + c] = if v >= 255.0 { 255 } else { (v + 0.5) as u8 };
            }
            // rgba[o + 3] untouched.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_field_shading_is_a_byte_noop() {
        // The exactness contract: a flat (all-sea) tile must shade to EXACTLY
        // 1.0 per node so the raster multiply is a byte-no-op — otherwise every
        // ocean tile's colors drift and the lens-differ/min-alpha contracts get
        // noisy. Checks the f32 identity, not an epsilon.
        let heights = vec![0.0f32; 33 * 17];
        let lam = lambert_grid(&heights, 33, 17, &ShadeParams::default());
        assert!(
            lam.iter().all(|f| *f == 1.0),
            "flat field did not shade to exactly 1.0"
        );

        let mut rgba: Vec<u8> = (0..33u32 * 17 * 4).map(|i| (i % 251) as u8).collect();
        let before = rgba.clone();
        shade_rgba(&mut rgba, 33, 17, &lam, 33, 17);
        assert_eq!(rgba, before, "flat shade was not a byte-no-op");
    }

    #[test]
    fn slopes_shade_asymmetrically_toward_the_light() {
        // A west-rising ridge: the west face looks toward the NW light
        // (brightens, f > 1), the east face looks away (falls toward ambient).
        // Only correct gradient → normal → Lambert wiring produces the
        // asymmetry; a sign slip flips it.
        let (gw, gh) = (33u32, 17u32);
        let mut heights = vec![0.0f32; (gw * gh) as usize];
        for j in 0..gh {
            for i in 0..gw {
                // Peak at the centre column, linear flanks.
                let d = (i as f32 - 16.0).abs();
                heights[(j * gw + i) as usize] = (8.0 - d).max(0.0) * 0.02;
            }
        }
        let p = ShadeParams::default();
        let lam = lambert_grid(&heights, gw, gh, &p);
        let mid = (gh / 2 * gw) as usize;
        let west_face = lam[mid + 12]; // rising toward the peak, facing NW-ish
        let east_face = lam[mid + 20]; // falling away from the peak, facing SE
        assert!(
            west_face > 1.0 && east_face < 1.0,
            "ridge shading not asymmetric (west {west_face}, east {east_face})"
        );
        assert!(east_face >= p.ambient, "shade fell below the ambient floor");
    }

    #[test]
    fn shade_rgba_never_touches_alpha() {
        // Steep random terrain, all alphas 255: shading may move RGB anywhere,
        // alpha must stay EXACTLY 255 per texel (the putImageData premultiply
        // invariant rides on it).
        let (gw, gh) = (17u32, 9u32);
        let heights: Vec<f32> = (0..gw * gh)
            .map(|i| ((i * 37 % 11) as f32) * 0.05)
            .collect();
        let lam = lambert_grid(&heights, gw, gh, &ShadeParams::default());
        let (w, h) = (64u32, 32u32);
        let mut rgba: Vec<u8> = (0..w * h * 4)
            .map(|i| if i % 4 == 3 { 255 } else { (i % 199) as u8 })
            .collect();
        let before = rgba.clone();
        shade_rgba(&mut rgba, w, h, &lam, gw, gh);
        assert!(
            rgba.chunks_exact(4).all(|px| px[3] == 255),
            "shade_rgba touched the alpha channel"
        );
        // And it genuinely changed RGB somewhere — shading steep terrain must
        // not be a silent no-op (that would false-green the alpha assert too).
        assert_ne!(rgba, before, "shading steep terrain changed nothing");
    }
}

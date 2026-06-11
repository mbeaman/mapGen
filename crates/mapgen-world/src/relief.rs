//! Relief heightfield sampling (the Relief addendum, superseding Increment G).
//!
//! A streamed globe tile's displaced geometry + baked hillshade both consume a
//! regular `gw × gh` grid sampled here. The load-bearing property is SEAM
//! BIT-IDENTITY: two adjacent tiles must produce bit-identical heights along
//! their shared edge, or displaced patches crack. The tile's OWN refined field
//! cannot provide that — `pin_edges_to_shared` pins per-SITE values on
//! *different* meshes (measured: 0/65 edge nodes bit-identical, ~6% deltas),
//! and `fill_depressions` mutates elevation after the pin anyway. So edge nodes
//! sample the ROOT world's field (one object, shared by all tiles) at
//! bit-identical world coordinates, smoothstep-blended inland to the tile's own
//! refined field — mirroring `pin_edges_to_shared`'s band math at SAMPLE level.
//!
//! fmath purity: only `+ - * /` and IEEE `sqrt` (exempted by `fmath_purity.rs`).

use mapgen_core::WorldData;

use crate::plates::wrap_dx;
use crate::scale::Sector;

/// Bucket count per axis. 64×64 measured ~26× faster than the brute scan on a
/// refined sector (~8k sites) while staying bit-identical to it.
const BUCKETS: usize = 64;
/// IDW neighbour count — mirrors `parent_anchor_elevation` (scale.rs).
const K: usize = 4;
/// IDW weight epsilon — mirrors `parent_anchor_elevation`.
const IDW_EPS: f32 = 1e-6;

/// Spatially-bucketed EXACT k-nearest IDW over a site set.
///
/// Exactness (bit-identity with the O(n) brute scan) needs two things beyond
/// the naive sketch, both spike-measured as load-bearing:
/// - candidates rank by lexicographic `(d2, site_index)` so near-ties resolve
///   scan-order-independently;
/// - rings expand until `ring_lower_bound² > kth-best d2` — "stop once k found"
///   is WRONG (a closer site can sit in the next ring).
///
/// Buckets always span the WHOLE site set — never a caller-supplied rect
/// pre-filter: two tiles would filter differently at a shared edge and silently
/// break edge bit-identity. `period = Some(width)` wraps x by minimum image
/// (the periodic planet root); tile meshes pass `None`.
pub struct SiteBuckets<'a> {
    sites: &'a [[f32; 2]],
    period: Option<f32>,
    grid: Vec<Vec<u32>>, // BUCKETS × BUCKETS, row-major [by * BUCKETS + bx]
    x0: f32,
    y0: f32,
    cw: f32, // bucket cell width
    ch: f32, // bucket cell height
}

impl<'a> SiteBuckets<'a> {
    pub fn new(sites: &'a [[f32; 2]], period: Option<f32>) -> Self {
        // Domain: full period in x when periodic (so bucket modulo is clean);
        // otherwise the site bounding box. Deterministic in the site set.
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for s in sites {
            x0 = x0.min(s[0]);
            x1 = x1.max(s[0]);
            y0 = y0.min(s[1]);
            y1 = y1.max(s[1]);
        }
        if let Some(p) = period {
            x0 = 0.0;
            x1 = p;
        }
        // Guard degenerate spans (single site / collinear): a 1-wide domain.
        let w = (x1 - x0).max(1.0);
        let h = (y1 - y0).max(1.0);
        let cw = w / BUCKETS as f32;
        let ch = h / BUCKETS as f32;
        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); BUCKETS * BUCKETS];
        for (i, s) in sites.iter().enumerate() {
            let bx = bucket_index(s[0], x0, cw);
            let by = bucket_index(s[1], y0, ch);
            grid[by * BUCKETS + bx].push(i as u32);
        }
        Self {
            sites,
            period,
            grid,
            x0,
            y0,
            cw,
            ch,
        }
    }

    /// Exact k=4 inverse-distance-weighted sample of `field` at `p`.
    /// `w = 1 / (d2 + 1e-6)` — the same kernel as `parent_anchor_elevation`,
    /// accumulated in `(d2, idx)`-sorted order so f32 summation is canonical.
    pub fn idw4(&self, field: &[f32], p: [f32; 2]) -> f32 {
        // Fixed-size best-k slots, ascending lexicographic (d2, idx).
        let mut best: [(f32, u32); K] = [(f32::MAX, u32::MAX); K];
        let mut found = 0usize;
        let pbx = bucket_index(p[0], self.x0, self.cw) as i32;
        let pby = bucket_index(p[1], self.y0, self.ch) as i32;
        let cell_min = if self.cw < self.ch { self.cw } else { self.ch };
        let max_ring = BUCKETS as i32; // covers the whole domain (+wrap)
        for ring in 0..=max_ring {
            // Stop once the ring cannot contain anything closer than the kth best.
            if found >= K {
                let lb = (ring - 1).max(0) as f32 * cell_min;
                if lb * lb > best[K - 1].0 {
                    break;
                }
            }
            self.scan_ring(pbx, pby, ring, p, &mut best, &mut found);
        }
        let mut num = 0.0f32;
        let mut den = 0.0f32;
        for &(d2, idx) in best.iter().take(found.min(K)) {
            let w = 1.0 / (d2 + IDW_EPS);
            num += w * field[idx as usize];
            den += w;
        }
        if den == 0.0 {
            0.0
        } else {
            num / den
        }
    }

    /// Visit every bucket at Chebyshev ring distance `ring`, inserting each
    /// site lexicographically by `(d2, idx)` into the best-k slots.
    fn scan_ring(
        &self,
        pbx: i32,
        pby: i32,
        ring: i32,
        p: [f32; 2],
        best: &mut [(f32, u32); K],
        found: &mut usize,
    ) {
        let lo_y = pby - ring;
        let hi_y = pby + ring;
        for by in lo_y..=hi_y {
            if by < 0 || by >= BUCKETS as i32 {
                continue;
            }
            let on_y_edge = by == lo_y || by == hi_y;
            let mut bx = pbx - ring;
            while bx <= pbx + ring {
                // Interior rows visit only the two x-edges of the ring.
                if !on_y_edge && bx != pbx - ring && bx != pbx + ring {
                    bx = pbx + ring;
                    continue;
                }
                let bxw = match self.period {
                    Some(_) => bx.rem_euclid(BUCKETS as i32),
                    None => {
                        if bx < 0 || bx >= BUCKETS as i32 {
                            bx += 1;
                            continue;
                        }
                        bx
                    }
                };
                for &idx in &self.grid[by as usize * BUCKETS + bxw as usize] {
                    let s = self.sites[idx as usize];
                    let dx = match self.period {
                        Some(period) => wrap_dx(p[0] - s[0], period),
                        None => p[0] - s[0],
                    };
                    let dy = p[1] - s[1];
                    let d2 = dx * dx + dy * dy;
                    insert_best(best, found, d2, idx);
                }
                bx += 1;
            }
        }
    }
}

/// Clamp-to-domain bucket index (sites exactly on the far edge land in the last
/// bucket; halo sites outside the domain clamp inward, which only ever makes the
/// ring search MORE conservative).
fn bucket_index(v: f32, v0: f32, cell: f32) -> usize {
    let i = ((v - v0) / cell) as i32;
    i.clamp(0, BUCKETS as i32 - 1) as usize
}

/// Insert `(d2, idx)` into the ascending best-k slots, lexicographic tie-break
/// on idx — the scan-order-independence that makes bucket == brute bitwise.
/// Idempotent per site: a periodic ring that wraps far enough can visit the
/// same bucket twice, so a site already in the slots is skipped rather than
/// occupying two of the k places.
fn insert_best(best: &mut [(f32, u32); K], found: &mut usize, d2: f32, idx: u32) {
    for slot in best.iter() {
        if slot.1 == idx {
            return; // already ranked (wrap-around re-visit)
        }
    }
    let cand = (d2, idx);
    let mut pos = K;
    for (i, slot) in best.iter().enumerate() {
        if cand.0 < slot.0 || (cand.0 == slot.0 && cand.1 < slot.1) {
            pos = i;
            break;
        }
    }
    if pos < K {
        for j in (pos + 1..K).rev() {
            best[j] = best[j - 1];
        }
        best[pos] = cand;
        *found = (*found + 1).min(K);
    }
}

/// Grid coordinate `i` of `gw` across `[x0, x1]`, ENDPOINT-PINNED: the naive
/// `x0 + (x1-x0) * t` does NOT round back to `x1` at `t == 1.0` in f32 (the ulp
/// trap), and shared-edge bit-identity rides on both neighbours evaluating the
/// edge at the SAME bits — `Sector::rect` computes a shared edge as
/// `(sx+1) as f32 * cw` on both sides, so pinning to the rect endpoint is exact.
fn grid_coord(x0: f32, x1: f32, i: u32, gw: u32) -> f32 {
    if i == 0 {
        x0
    } else if i == gw - 1 {
        x1
    } else {
        x0 + (x1 - x0) * (i as f32 / (gw - 1) as f32)
    }
}

/// Seam-banded relief grid for a refined tile: row-major, `gh` rows × `gw`
/// cols, ROW 0 = NORTH (`rect.y0`), COL 0 = WEST (`rect.x0`). Heights are RAW
/// sea-clamped elevation `max(0, blended)` — never per-tile-normalized (per-tile
/// maxima differ → normalization would re-break edge bit-identity) and not
/// root-max-normalized (`VERT_EXAG` is the single JS knob; elevation is
/// per-world relative, see tuning_log).
pub fn relief_grid(
    parent: &WorldData,
    tile: &WorldData,
    sector: Sector,
    gw: u32,
    gh: u32,
) -> Result<Vec<f32>, String> {
    if !(2..=257).contains(&gw) || !(2..=257).contains(&gh) {
        return Err(format!("relief grid dims out of range: {gw}x{gh}"));
    }
    if !sector.is_valid() {
        return Err(format!("invalid sector {sector:?}"));
    }
    let w = parent.mesh.width;
    let h = parent.mesh.height;
    let rect = sector.rect(w, h);
    // The tile must BE this sector's refinement — its region rect must match
    // bitwise (refine_sector stores the true sector rect; `Sector::rect` is the
    // same computation, so equality is exact, not approximate).
    if tile.mesh.view_rect() != rect {
        return Err("tile view_rect does not match sector rect".to_string());
    }
    let [x0, y0, x1, y1] = rect;
    let tile_buckets = SiteBuckets::new(&tile.mesh.sites, None);
    let root_period = parent.mesh.periodic.then_some(w);
    let root_buckets = SiteBuckets::new(&parent.mesh.sites, root_period);
    // Band width mirrors pin_edges_to_shared (scale.rs): 12% of the short rect
    // side, floored at 1 world unit.
    let band = {
        let short = if x1 - x0 < y1 - y0 { x1 - x0 } else { y1 - y0 };
        (short * 0.12).max(1.0)
    };
    let mut out = Vec::with_capacity(gw as usize * gh as usize);
    for j in 0..gh {
        let y = grid_coord(y0, y1, j, gh);
        for i in 0..gw {
            let x = grid_coord(x0, x1, i, gw);
            // ANTIMERIDIAN CANONICALIZATION: the wrap pair evaluates the seam
            // at x == W on one side and x == 0 on the other, and minimum-image
            // wrap_dx is NOT bitwise-neutral for sites with x < W/2 (Sterbenz
            // fails below W/2), so near-tie k-NN ranking could flip. Sampling
            // the root at the canonical x = 0 makes the wrap edge bit-identical
            // like any interior edge.
            let x_canon = if parent.mesh.periodic && x == w {
                0.0
            } else {
                x
            };
            let h_root = root_buckets.idw4(&parent.terrain.elevation, [x_canon, y]);
            // Distance inside the rect to the nearest edge: exactly 0 at the
            // pinned endpoints, so edge nodes are PURE root samples.
            let d_in = (x - x0).min(x1 - x).min(y - y0).min(y1 - y);
            let t = (d_in / band).clamp(0.0, 1.0);
            let wgt = t * t * (3.0 - 2.0 * t);
            let h_val = if wgt == 0.0 {
                h_root // skip the tile sample entirely: edge nodes are root-pure
            } else {
                let h_tile = tile_buckets.idw4(&tile.terrain.elevation, [x, y]);
                h_root + (h_tile - h_root) * wgt
            };
            // Sea clamp AFTER the blend: both neighbours clamp the identical
            // edge value, preserving bit-identity. Sea level = 0 (TerrainData).
            out.push(h_val.max(0.0));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brute-force k=4 IDW — the oracle the buckets must match bitwise.
    fn brute_idw4(sites: &[[f32; 2]], period: Option<f32>, field: &[f32], p: [f32; 2]) -> f32 {
        let mut best: [(f32, u32); K] = [(f32::MAX, u32::MAX); K];
        let mut found = 0usize;
        for (i, s) in sites.iter().enumerate() {
            let dx = match period {
                Some(per) => wrap_dx(p[0] - s[0], per),
                None => p[0] - s[0],
            };
            let dy = p[1] - s[1];
            insert_best(&mut best, &mut found, dx * dx + dy * dy, i as u32);
        }
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for &(d2, idx) in best.iter().take(found.min(K)) {
            let w = 1.0 / (d2 + IDW_EPS);
            num += w * field[idx as usize];
            den += w;
        }
        num / den
    }

    /// Deterministic scatter without a rand dep: a tiny LCG.
    fn scatter(n: usize, w: f32, h: f32, seed: u64) -> Vec<[f32; 2]> {
        let mut state = seed;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as f32) / (u32::MAX as f32 / 2.0)
        };
        (0..n).map(|_| [next() * w, next() * h]).collect()
    }

    #[test]
    fn idw4_is_directional() {
        // Two sites: left carries +1, right carries -1. A correct nearest-site
        // weighting must read ~+1 near the left site and ~-1 near the right —
        // a signal only correct distance ranking produces.
        let sites = vec![[10.0, 50.0], [90.0, 50.0]];
        let field = vec![1.0, -1.0];
        let b = SiteBuckets::new(&sites, None);
        assert!(b.idw4(&field, [12.0, 50.0]) > 0.9);
        assert!(b.idw4(&field, [88.0, 50.0]) < -0.9);
    }

    #[test]
    fn bucket_idw_is_bit_identical_to_brute_force() {
        let sites = scatter(4_000, 512.0, 512.0, 42);
        let field: Vec<f32> = (0..sites.len()).map(|i| ((i % 17) as f32) - 8.0).collect();
        let b = SiteBuckets::new(&sites, None);
        let mut mism = 0;
        for j in 0..33 {
            for i in 0..33 {
                let p = [i as f32 * 16.0, j as f32 * 16.0];
                if b.idw4(&field, p).to_bits() != brute_idw4(&sites, None, &field, p).to_bits() {
                    mism += 1;
                }
            }
        }
        assert_eq!(mism, 0, "bucketed IDW diverged from the brute oracle");
    }

    #[test]
    fn periodic_bucket_idw_matches_brute_and_wraps() {
        let w = 512.0;
        let sites = scatter(2_000, w, 256.0, 7);
        let field: Vec<f32> = (0..sites.len()).map(|i| ((i % 13) as f32) * 0.25).collect();
        let b = SiteBuckets::new(&sites, Some(w));
        for j in 0..9 {
            for i in 0..17 {
                let p = [i as f32 * 32.0, j as f32 * 32.0];
                assert_eq!(
                    b.idw4(&field, p).to_bits(),
                    brute_idw4(&sites, Some(w), &field, p).to_bits(),
                    "periodic bucket/brute divergence at {p:?}"
                );
            }
        }
        // Wrap really engages: a probe at x=1 must see a site at x=W-1 as near.
        let sites2 = vec![[w - 1.0, 10.0], [w * 0.5, 10.0]];
        let field2 = vec![5.0, -5.0];
        let b2 = SiteBuckets::new(&sites2, Some(w));
        assert!(
            b2.idw4(&field2, [1.0, 10.0]) > 4.0,
            "minimum-image wrap not applied"
        );
    }

    #[test]
    fn ties_break_by_site_index() {
        // p exactly equidistant from two sites with different values: the
        // lexicographic (d2, idx) rule weights them identically here (same d2)
        // — but k=1-style dominance shows through field asymmetry when three
        // sites tie pairwise. Assert the result is EXACTLY the symmetric mean,
        // scan-order independent (reverse the site order → same bits).
        let sites = vec![[10.0, 10.0], [30.0, 10.0]];
        let rev: Vec<[f32; 2]> = sites.iter().rev().cloned().collect();
        let field = vec![1.0, 3.0];
        let frev = vec![3.0, 1.0];
        let a = SiteBuckets::new(&sites, None).idw4(&field, [20.0, 10.0]);
        let b = SiteBuckets::new(&rev, None).idw4(&frev, [20.0, 10.0]);
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "site scan order leaked into the result"
        );
    }

    #[test]
    fn grid_coord_pins_endpoints_bitwise() {
        // The ulp trap: naive `x0 + (x1-x0)*t` at t=1 misses x1 by an ulp for
        // SOME spans. Prove both halves over a deterministic span population:
        // the trap is real (naive misses at least once) and the pin never does.
        let mut state = 99u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as f32) / (u32::MAX as f32 / 2.0)
        };
        let (mut naive_misses, mut pin_misses) = (0u32, 0u32);
        for _ in 0..10_000 {
            let x0 = next() * 100.0 + 0.001;
            let x1 = x0 + next() * 37.0 + 0.001;
            let naive = x0 + (x1 - x0) * 1.0;
            if naive.to_bits() != x1.to_bits() {
                naive_misses += 1;
            }
            if grid_coord(x0, x1, 128, 129).to_bits() != x1.to_bits()
                || grid_coord(x0, x1, 0, 129).to_bits() != x0.to_bits()
            {
                pin_misses += 1;
            }
        }
        assert!(
            naive_misses > 0,
            "the ulp trap never fired — naive lerp would suffice"
        );
        assert_eq!(pin_misses, 0, "endpoint pin failed on {pin_misses} spans");
    }
}

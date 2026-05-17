//! Bridson 2007 Poisson-disk sampling in 2D.
//! Determinism-critical: every transcendental routes through `mapgen_core::fmath`.

use mapgen_core::fmath;
use rand::Rng;
use rand_chacha::rand_core::RngCore;

const TAU: f32 = std::f32::consts::TAU;
const SQRT_2_INV: f32 = 0.707_106_77;

/// Bridson Poisson-disk sample. Returns points in `[0, width) x [0, height)`
/// no closer than `min_dist`. `k` is the candidates-per-attempt parameter
/// (Bridson recommends 30).
pub fn poisson_disk_2d<R: RngCore>(
    width: f32,
    height: f32,
    min_dist: f32,
    k: usize,
    rng: &mut R,
) -> Vec<[f32; 2]> {
    assert!(width > 0.0 && height > 0.0 && min_dist > 0.0);
    let cell_size = min_dist * SQRT_2_INV;
    let grid_w = (width / cell_size).ceil() as usize + 1;
    let grid_h = (height / cell_size).ceil() as usize + 1;
    let mut grid: Vec<Option<u32>> = vec![None; grid_w * grid_h];

    let mut points: Vec<[f32; 2]> = Vec::new();
    let mut active: Vec<u32> = Vec::new();

    let seed = [rng.gen_range(0.0..width), rng.gen_range(0.0..height)];
    push(&mut points, &mut active, &mut grid, grid_w, cell_size, seed);

    while !active.is_empty() {
        let idx = rng.gen_range(0..active.len());
        let parent = points[active[idx] as usize];
        let mut accepted = false;

        for _ in 0..k {
            let angle = rng.gen_range(0.0..TAU);
            let radius = rng.gen_range(min_dist..2.0 * min_dist);
            let cand = [
                parent[0] + radius * fmath::cos(angle),
                parent[1] + radius * fmath::sin(angle),
            ];
            if cand[0] < 0.0 || cand[0] >= width || cand[1] < 0.0 || cand[1] >= height {
                continue;
            }
            if too_close(&points, &grid, grid_w, grid_h, cell_size, cand, min_dist) {
                continue;
            }
            push(&mut points, &mut active, &mut grid, grid_w, cell_size, cand);
            accepted = true;
            break;
        }

        if !accepted {
            active.swap_remove(idx);
        }
    }

    points
}

#[inline]
fn push(
    points: &mut Vec<[f32; 2]>,
    active: &mut Vec<u32>,
    grid: &mut [Option<u32>],
    grid_w: usize,
    cell_size: f32,
    p: [f32; 2],
) {
    let id = points.len() as u32;
    points.push(p);
    active.push(id);
    let gx = (p[0] / cell_size) as usize;
    let gy = (p[1] / cell_size) as usize;
    grid[gy * grid_w + gx] = Some(id);
}

#[inline]
fn too_close(
    points: &[[f32; 2]],
    grid: &[Option<u32>],
    grid_w: usize,
    grid_h: usize,
    cell_size: f32,
    p: [f32; 2],
    min_dist: f32,
) -> bool {
    let cx = (p[0] / cell_size) as isize;
    let cy = (p[1] / cell_size) as isize;
    let min2 = min_dist * min_dist;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 || nx >= grid_w as isize || ny >= grid_h as isize {
                continue;
            }
            if let Some(pi) = grid[ny as usize * grid_w + nx as usize] {
                let q = points[pi as usize];
                let ddx = q[0] - p[0];
                let ddy = q[1] - p[1];
                if ddx * ddx + ddy * ddy < min2 {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use mapgen_core::{Stage, StageRng};

    #[test]
    fn produces_separated_points() {
        let mut rng = StageRng::new(7).stream(Stage::Mesh);
        let pts = poisson_disk_2d(100.0, 100.0, 5.0, 30, &mut rng);
        // Enough fit in a 100x100 region with min_dist=5 (Bridson density ≈ 0.7).
        assert!(pts.len() > 100, "got {} points", pts.len());
        for i in 0..pts.len() {
            for j in (i + 1)..pts.len() {
                let dx = pts[i][0] - pts[j][0];
                let dy = pts[i][1] - pts[j][1];
                assert!(dx * dx + dy * dy >= 5.0 * 5.0 - 1e-3);
            }
        }
    }

    #[test]
    fn deterministic_across_runs() {
        let pts_a = {
            let mut rng = StageRng::new(123).stream(Stage::Mesh);
            poisson_disk_2d(64.0, 64.0, 3.0, 30, &mut rng)
        };
        let pts_b = {
            let mut rng = StageRng::new(123).stream(Stage::Mesh);
            poisson_disk_2d(64.0, 64.0, 3.0, 30, &mut rng)
        };
        assert_eq!(pts_a, pts_b);
    }
}

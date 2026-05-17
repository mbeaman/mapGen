//! Plate tectonics, Phase 1 cut:
//!   1. Pick N plate centers uniformly within bounds.
//!   2. Assign each cell to its nearest plate (Voronoi-of-plates).
//!   3. Each plate gets a random kind (oceanic / continental) + drift + base
//!      elevation.
//!   4. Boundary uplift: where two plates meet, push cells up (convergent) or
//!      down (divergent) by stress = dot(Δdrift, normal).
//!   5. Smooth twice over the cell graph to soften plate-edge blockiness.
//!
//! Phase 2 will overlay OpenSimplex2 + domain warping + erosion. Until then,
//! this gets us a recognizable continent silhouette.

use mapgen_core::{
    fmath,
    ids::PlateId,
    world_data::{MeshData, PlateKind, PlateRecord, TerrainData},
};
use rand::Rng;
use rand_chacha::ChaCha8Rng;

const TAU: f32 = std::f32::consts::TAU;

pub fn generate(mesh: &MeshData, plate_count: usize, rng: &mut ChaCha8Rng) -> TerrainData {
    let n = mesh.cell_count();

    // 1. Plate centers — uniform across the map.
    let mut centers: Vec<[f32; 2]> = Vec::with_capacity(plate_count);
    for _ in 0..plate_count {
        centers.push([
            rng.gen_range(0.0..mesh.width),
            rng.gen_range(0.0..mesh.height),
        ]);
    }

    // 2. Per-plate kind / drift / base elevation.
    let mut plates: Vec<PlateRecord> = Vec::with_capacity(plate_count);
    for &c in &centers {
        let kind = if rng.gen::<f32>() < 0.55 { PlateKind::Oceanic } else { PlateKind::Continental };
        let theta = rng.gen_range(0.0..TAU);
        let speed = rng.gen_range(0.4..1.0);
        let drift = [speed * fmath::cos(theta), speed * fmath::sin(theta)];
        let base_elevation = match kind {
            PlateKind::Oceanic => rng.gen_range(-0.55..-0.30),
            PlateKind::Continental => rng.gen_range(0.05..0.30),
        };
        plates.push(PlateRecord { kind, center: c, drift, base_elevation });
    }

    // 3. Assign each cell to nearest plate center.
    let plate_id: Vec<PlateId> = mesh
        .sites
        .iter()
        .map(|&s| {
            let (idx, _) = centers
                .iter()
                .enumerate()
                .map(|(i, &c)| (i, sq_dist(s, c)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap();
            PlateId(idx as u32)
        })
        .collect();

    // 4. Base elevation from plate.
    let mut elevation: Vec<f32> = (0..n).map(|i| plates[plate_id[i].0 as usize].base_elevation).collect();

    // 5. Boundary uplift. Walk every cell; for each neighbor on a different
    // plate, compute stress and accumulate an uplift contribution.
    let mut uplift = vec![0f32; n];
    for i in 0..n {
        let pi = plate_id[i].0 as usize;
        for &j in &mesh.neighbors[i] {
            let j = j as usize;
            let pj = plate_id[j].0 as usize;
            if pi == pj {
                continue;
            }
            // Normal points from j → i. Approximate with site delta.
            let si = mesh.sites[i];
            let sj = mesh.sites[j];
            let nx = si[0] - sj[0];
            let ny = si[1] - sj[1];
            let nlen = fmath::hypot(nx, ny).max(1e-6);
            let nx = nx / nlen;
            let ny = ny / nlen;
            // Relative drift of i's plate relative to j's plate.
            let dvx = plates[pi].drift[0] - plates[pj].drift[0];
            let dvy = plates[pi].drift[1] - plates[pj].drift[1];
            // Convergent if i is drifting *toward* the boundary (negative
            // along normal), i.e. dot(drift_i - drift_j, normal_i_away_from_j)
            // is negative.
            let stress = -(dvx * nx + dvy * ny);

            // Magnitude scaled by plate-kind interaction.
            let scale = match (plates[pi].kind, plates[pj].kind) {
                (PlateKind::Continental, PlateKind::Continental) => 0.60, // fold belt + plateau
                (PlateKind::Continental, PlateKind::Oceanic) => 0.50,     // coastal mountain on continent
                (PlateKind::Oceanic, PlateKind::Continental) => -0.25,    // oceanic side subsides (trench)
                (PlateKind::Oceanic, PlateKind::Oceanic) => 0.20,         // island arc
            };
            uplift[i] += stress * scale;
        }
    }
    for i in 0..n {
        elevation[i] = fmath::clamp(elevation[i] + uplift[i], -1.0, 1.0);
    }

    // 6. Two passes of neighbor-averaging to soften plate seams.
    elevation = smooth(&mesh.neighbors, &elevation, 2, 0.5);

    TerrainData { elevation, plate_id, plates }
}

#[inline]
fn sq_dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

fn smooth(neighbors: &[Vec<u32>], values: &[f32], passes: usize, weight: f32) -> Vec<f32> {
    let mut current = values.to_vec();
    let mut next = vec![0f32; values.len()];
    for _ in 0..passes {
        for i in 0..current.len() {
            let mut sum = 0.0;
            let ns = &neighbors[i];
            for &j in ns {
                sum += current[j as usize];
            }
            let avg = if ns.is_empty() { current[i] } else { sum / ns.len() as f32 };
            next[i] = current[i] * (1.0 - weight) + avg * weight;
        }
        std::mem::swap(&mut current, &mut next);
    }
    current
}

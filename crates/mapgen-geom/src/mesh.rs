//! Voronoi cell graph over a Poisson-disk point set, with Lloyd relaxation
//! built in. Exposes both an in-memory `Mesh` (with helpful accessors) and a
//! pure-data `MeshData` for serialization.

use mapgen_core::{fmath, world_data::MeshData};
use rand_chacha::rand_core::RngCore;
use voronoice::{BoundingBox, Point, Voronoi, VoronoiBuilder};

use crate::poisson::poisson_disk_2d;

#[derive(Clone, Debug)]
pub struct MeshBuildParams {
    pub width: f32,
    pub height: f32,
    pub target_cells: usize,
    pub lloyd_iterations: usize,
    /// Wrap cell adjacency in x (a cylinder: the antimeridian at x=0 and x=width is
    /// the SAME meridian; latitude still clamps at the poles). For the planet/globe
    /// world so the sphere has no seam. `false` ⇒ the legacy flat, hard-edged mesh —
    /// BYTE-IDENTICAL to before this field existed (no ghost rebuild, no seam edges).
    pub periodic: bool,
}

/// Parameters for a *sub-region* mesh (Phase 7 multi-scale refinement). The
/// Voronoi tiles only `region` (a world-space rectangle), but `full_width` /
/// `full_height` are the whole-world extent — preserved so plate scatter and
/// climate latitude stay global when the stages run over the sub-mesh.
#[derive(Clone, Debug)]
pub struct RegionMeshParams {
    pub full_width: f32,
    pub full_height: f32,
    /// `[x0, y0, x1, y1]` in world coordinates.
    pub region: [f32; 4],
    pub target_cells: usize,
    pub lloyd_iterations: usize,
}

/// Constructed mesh. Wraps a `voronoice::Voronoi` plus a few projected
/// data tables for fast access.
pub struct Mesh {
    pub width: f32,
    pub height: f32,
    /// `Some(rect)` for a sub-region mesh; `None` for a whole-world mesh.
    pub region: Option<[f32; 4]>,
    pub voronoi: Voronoi,
    /// Cross-seam (x=0 ↔ x=width) neighbour pairs for a periodic mesh — empty unless
    /// built with `periodic: true`. Augments the voronoi-derived adjacency; symmetric
    /// (each pair is added to BOTH endpoints in `neighbors`/`into_mesh_data`).
    pub seam_edges: Vec<(u32, u32)>,
}

impl Mesh {
    pub fn build<R: RngCore>(params: MeshBuildParams, rng: &mut R) -> Self {
        // Bridson density ~0.7 in a unit square at min_dist=1 → roughly
        //   N ≈ 0.7 * (W*H) / min_dist^2.
        // Invert for min_dist, then add slack so we slightly overshoot the
        // target rather than undershoot (clipping later is cheap; not enough
        // cells is annoying).
        let area = params.width * params.height;
        let min_dist = fmath::sqrt(0.7 * area / params.target_cells as f32);

        let seeds = poisson_disk_2d(params.width, params.height, min_dist, 30, rng);
        let sites: Vec<Point> = seeds
            .iter()
            .map(|p| Point {
                x: p[0] as f64,
                y: p[1] as f64,
            })
            .collect();

        let bbox = BoundingBox::new(
            Point {
                x: (params.width as f64) / 2.0,
                y: (params.height as f64) / 2.0,
            },
            params.width as f64,
            params.height as f64,
        );

        let voronoi = VoronoiBuilder::default()
            .set_sites(sites)
            .set_bounding_box(bbox)
            .set_lloyd_relaxation_iterations(params.lloyd_iterations)
            .build()
            .expect("voronoi build");

        // Periodic mesh: re-triangulate the RELAXED sites + seam ghosts to read off
        // cross-seam adjacency (the legacy non-periodic path is untouched → byte-identical).
        let seam_edges = if params.periodic {
            periodic_seam_edges(
                voronoi.sites(),
                params.width as f64,
                params.height as f64,
                min_dist as f64,
            )
        } else {
            Vec::new()
        };

        Self {
            width: params.width,
            height: params.height,
            region: None,
            voronoi,
            seam_edges,
        }
    }

    /// Build a finer mesh over a world-space sub-rectangle (Phase 7). Poisson
    /// sampling is translation-invariant, so we sample in local `[0,w)×[0,h)`
    /// and offset by the region origin — reusing `poisson_disk_2d` unchanged.
    /// The mesh keeps the *full*-world `width`/`height`; only its cells (and the
    /// Voronoi bounding box) are confined to `region`.
    pub fn build_region<R: RngCore>(params: RegionMeshParams, rng: &mut R) -> Self {
        let [x0, y0, x1, y1] = params.region;
        let rw = (x1 - x0).max(1.0);
        let rh = (y1 - y0).max(1.0);
        let area = rw * rh;
        let min_dist = fmath::sqrt(0.7 * area / params.target_cells as f32);

        let local = poisson_disk_2d(rw, rh, min_dist, 30, rng);
        let sites: Vec<Point> = local
            .iter()
            .map(|p| Point {
                x: (p[0] + x0) as f64,
                y: (p[1] + y0) as f64,
            })
            .collect();

        let bbox = BoundingBox::new(
            Point {
                x: ((x0 + x1) * 0.5) as f64,
                y: ((y0 + y1) * 0.5) as f64,
            },
            rw as f64,
            rh as f64,
        );

        let voronoi = VoronoiBuilder::default()
            .set_sites(sites)
            .set_bounding_box(bbox)
            .set_lloyd_relaxation_iterations(params.lloyd_iterations)
            .build()
            .expect("voronoi build");

        Self {
            width: params.full_width,
            height: params.full_height,
            region: Some(params.region),
            voronoi,
            // A drilled sub-region is always a continental (non-wrapping) view, even from
            // a planet parent — periodicity is root-only (see the design doc §3).
            seam_edges: Vec::new(),
        }
    }

    pub fn cell_count(&self) -> usize {
        self.voronoi.sites().len()
    }

    pub fn site(&self, i: usize) -> [f32; 2] {
        let s = &self.voronoi.sites()[i];
        [s.x as f32, s.y as f32]
    }

    pub fn neighbors(&self, i: usize) -> Vec<u32> {
        let mut nbrs: Vec<u32> = self
            .voronoi
            .cell(i)
            .iter_neighbors()
            .map(|n| n as u32)
            .collect();
        for &(a, b) in &self.seam_edges {
            if a as usize == i && !nbrs.contains(&b) {
                nbrs.push(b);
            }
            if b as usize == i && !nbrs.contains(&a) {
                nbrs.push(a);
            }
        }
        nbrs
    }

    /// Project into the serializable `MeshData` form.
    pub fn into_mesh_data(self) -> MeshData {
        let n = self.cell_count();
        let mut sites = Vec::with_capacity(n);
        for i in 0..n {
            sites.push(self.site(i));
        }

        let vertices: Vec<[f32; 2]> = self
            .voronoi
            .vertices()
            .iter()
            .map(|v| [v.x as f32, v.y as f32])
            .collect();

        let mut cell_vertices = Vec::with_capacity(n);
        let mut neighbors = Vec::with_capacity(n);
        for i in 0..n {
            let verts: Vec<u32> = self.voronoi.cells()[i].iter().map(|&v| v as u32).collect();
            cell_vertices.push(verts);
            let nbrs: Vec<u32> = self
                .voronoi
                .cell(i)
                .iter_neighbors()
                .map(|v| v as u32)
                .collect();
            neighbors.push(nbrs);
        }
        // Inject the cross-seam edges symmetrically (no-op when non-periodic → the
        // serialized neighbour arrays are byte-identical to before this field existed).
        for &(a, b) in &self.seam_edges {
            let (a, b) = (a as usize, b as usize);
            if !neighbors[a].contains(&(b as u32)) {
                neighbors[a].push(b as u32);
            }
            if !neighbors[b].contains(&(a as u32)) {
                neighbors[b].push(a as u32);
            }
        }

        MeshData {
            width: self.width,
            height: self.height,
            sites,
            vertices,
            cell_vertices,
            neighbors,
            coast: vec![false; n],
            region: self.region,
        }
    }
}

/// Cross-seam (x=0 ↔ x=width) neighbour pairs for a PERIODIC (cylinder) mesh.
/// voronoice has no toroidal mode, so we RE-TRIANGULATE the already-relaxed sites plus
/// GHOST copies of the seam-margin sites at x ± width (y unchanged → the poles stay hard
/// edges) at ZERO Lloyd iterations — ghosts must NOT relax, or each drifts independently
/// of its original and the seam stops mirroring itself. A real cell adjacent to a ghost
/// is adjacent to that ghost's real owner ACROSS the seam. The two sides' local
/// triangulations are NOT mirror-symmetric, so we keep only opposite-edge pairs within a
/// few cell widths (the true wrapped neighbours; the proven `ghost_seam_adjacency` spike)
/// and the caller adds each edge to BOTH endpoints. Draws no RNG → deterministic; pure
/// f64 add/sub/abs/min (no transcendentals) → cross-platform stable, like the base build.
fn periodic_seam_edges(
    relaxed: &[Point],
    width: f64,
    height: f64,
    min_dist: f64,
) -> Vec<(u32, u32)> {
    let margin = 3.0 * min_dist;
    let n_real = relaxed.len();
    let mut sites: Vec<Point> = relaxed.to_vec();
    let mut owner: Vec<usize> = Vec::new();
    for (i, p) in relaxed.iter().enumerate() {
        if p.x < margin {
            sites.push(Point {
                x: p.x + width,
                y: p.y,
            });
            owner.push(i);
        }
        if p.x > width - margin {
            sites.push(Point {
                x: p.x - width,
                y: p.y,
            });
            owner.push(i);
        }
    }
    if owner.is_empty() {
        return Vec::new();
    }
    let bbox = BoundingBox::new(
        Point {
            x: width / 2.0,
            y: height / 2.0,
        },
        width + 2.0 * margin,
        height,
    );
    let ghosted = VoronoiBuilder::default()
        .set_sites(sites)
        .set_bounding_box(bbox)
        .set_lloyd_relaxation_iterations(0)
        .build()
        .expect("ghost voronoi build");
    let real_of = |n: usize| -> usize {
        if n < n_real {
            n
        } else {
            owner[n - n_real]
        }
    };

    let mut edges: std::collections::BTreeSet<(u32, u32)> = Default::default();
    for i in 0..n_real {
        for n in ghosted.cell(i).iter_neighbors() {
            let j = real_of(n);
            if j == i {
                continue;
            }
            let (xi, xj) = (relaxed[i].x, relaxed[j].x);
            let opposite =
                (xi < margin && xj > width - margin) || (xj < margin && xi > width - margin);
            let dx = (xi - xj).abs();
            let wdx = dx.min(width - dx); // wrapped longitude gap
            if opposite && wdx < 4.0 * min_dist {
                edges.insert((i.min(j) as u32, i.max(j) as u32));
            }
        }
    }
    edges.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mapgen_core::{Stage, StageRng};

    const W: f32 = 256.0;
    fn build(periodic: bool) -> MeshData {
        let mut rng = StageRng::new(3).stream(Stage::Mesh);
        Mesh::build(
            MeshBuildParams {
                width: W,
                height: W,
                target_cells: 400,
                lloyd_iterations: 2,
                periodic,
            },
            &mut rng,
        )
        .into_mesh_data()
    }
    /// Count neighbour pairs that straddle the seam (one cell near x=0, the other near
    /// x=width) — the signal that ONLY a periodic (wrapped) mesh produces.
    fn cross_seam_links(d: &MeshData) -> usize {
        let margin = 3.0 * fmath::sqrt(0.7 * (W * W) / 400.0);
        let mut c = 0;
        for i in 0..d.cell_count() {
            for &j in &d.neighbors[i] {
                let (xi, xj) = (d.sites[i][0], d.sites[j as usize][0]);
                if (xi < margin && xj > W - margin) || (xj < margin && xi > W - margin) {
                    c += 1;
                }
            }
        }
        c
    }

    #[test]
    fn neighbor_symmetry_holds_for_flat_and_periodic() {
        for periodic in [false, true] {
            let data = build(periodic);
            for i in 0..data.cell_count() {
                for &j in &data.neighbors[i] {
                    assert!(
                        data.neighbors[j as usize].contains(&(i as u32)),
                        "periodic={periodic}: cell {i}'s neighbor {j} doesn't list {i} back"
                    );
                }
            }
        }
    }

    // The periodic-gen FOUNDATION (Phase 0): a periodic mesh wraps adjacency at the
    // antimeridian (x=0 ↔ x=width); a flat mesh does NOT. The differential is the signal
    // only the ghost-topology can produce — `cross_seam_links` is 0 on the legacy path
    // (proving byte-identity is preserved there) and >0 only when periodic.
    #[test]
    fn periodic_mesh_wraps_the_seam_and_flat_does_not() {
        assert_eq!(
            cross_seam_links(&build(false)),
            0,
            "flat mesh must NOT cross the seam"
        );
        assert!(
            cross_seam_links(&build(true)) > 0,
            "periodic mesh has no seam-crossing neighbours"
        );
    }
}

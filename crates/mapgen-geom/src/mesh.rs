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

        Self {
            width: params.width,
            height: params.height,
            region: None,
            voronoi,
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
        self.voronoi
            .cell(i)
            .iter_neighbors()
            .map(|n| n as u32)
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use mapgen_core::{Stage, StageRng};

    #[test]
    fn neighbor_symmetry() {
        let mut rng = StageRng::new(3).stream(Stage::Mesh);
        let mesh = Mesh::build(
            MeshBuildParams {
                width: 256.0,
                height: 256.0,
                target_cells: 400,
                lloyd_iterations: 2,
            },
            &mut rng,
        );
        let data = mesh.into_mesh_data();
        for i in 0..data.cell_count() {
            for &j in &data.neighbors[i] {
                let j = j as usize;
                assert!(
                    data.neighbors[j].contains(&(i as u32)),
                    "cell {i}'s neighbor {j} doesn't list {i} as its neighbor"
                );
            }
        }
    }
}

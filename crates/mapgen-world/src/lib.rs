//! Geography pipeline. Composes mesh, terrain, erosion, hydrology, climate,
//! biomes, and society into a `WorldData`.

pub mod plates;

use mapgen_core::{Stage, StageRng, WorldData, WorldMeta};
use mapgen_geom::{Mesh, MeshBuildParams};

/// Top-level world generation parameters.
#[derive(Clone, Debug)]
pub struct GenerateParams {
    pub seed: u64,
    pub width: f32,
    pub height: f32,
    pub cell_count: usize,
    pub plate_count: usize,
    pub nation_count: usize,
}

impl Default for GenerateParams {
    fn default() -> Self {
        Self {
            seed: 0,
            width: 2048.0,
            height: 1280.0,
            cell_count: 15_000,
            plate_count: 14,
            nation_count: 8,
        }
    }
}

/// Phase 1: mesh + plate-driven heightmap, nothing else.
pub fn generate(params: GenerateParams) -> WorldData {
    let rng = StageRng::new(params.seed);

    let mesh = Mesh::build(
        MeshBuildParams {
            width: params.width,
            height: params.height,
            target_cells: params.cell_count,
            lloyd_iterations: 2,
        },
        &mut rng.stream(Stage::Mesh),
    );

    let mesh_data = mesh.into_mesh_data();
    let terrain = plates::generate(
        &mesh_data,
        params.plate_count,
        &mut rng.stream(Stage::Plates),
    );

    WorldData {
        meta: WorldMeta::new(params.seed),
        mesh: mesh_data,
        terrain,
        ..Default::default()
    }
}

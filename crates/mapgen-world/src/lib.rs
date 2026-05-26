//! Geography pipeline. Composes mesh, terrain, erosion, hydrology, climate,
//! biomes, and society into a `WorldData`.

pub mod biomes;
pub mod climate;
pub mod climate_seasonal;
pub mod cultures;
pub mod erosion;
pub mod hydrology;
pub mod koppen;
pub mod naming;
pub mod noise;
pub mod ocean;
pub mod patch;
pub mod pipeline;
pub mod plates;
pub mod polities;
pub mod religions;
pub mod scale;
pub mod soils;

pub use pipeline::{FineStep, Pipeline, PipelineStage};

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

impl GenerateParams {
    /// Planet-scale generation: a wider 2:1 aspect and a higher plate count, so
    /// the world reads as several continents in an encircling sea rather than a
    /// single dominant landmass. Render it with `Style::Planet`; drill into any
    /// region with `scale::refine_sector` for the continental ornate view (the
    /// planet is the root, continents are its sectors).
    pub fn planet(seed: u64) -> Self {
        Self {
            seed,
            width: 2048.0,
            height: 1024.0,
            cell_count: 18_000,
            plate_count: 32,
            nation_count: 12,
        }
    }
}

/// Convenience: run every scientific stage in canonical order and return
/// a world ready for rendering. CLI consumers should call this; tests
/// that exercise individual stages should call `generate()` + the stages
/// they need.
pub fn generate_full(params: GenerateParams) -> WorldData {
    generate_full_with(
        params,
        erosion::ErosionParams::default(),
        climate::ClimateParams::default(),
    )
}

/// Same as [`generate_full`] but with caller-supplied `ErosionParams` and
/// `ClimateParams`. Used by the `mapgen sweep` CLI to vary a single
/// parameter while keeping the rest of the pipeline at defaults.
/// Defaults-only callers should prefer [`generate_full`].
pub fn generate_full_with(
    params: GenerateParams,
    erosion_params: erosion::ErosionParams,
    climate_params: climate::ClimateParams,
) -> WorldData {
    let mut pipeline = Pipeline::with_params(params, erosion_params, climate_params);
    while pipeline.step().is_some() {}
    pipeline.into_world()
}

/// Build a world up through the geography pipeline: mesh, plate uplift,
/// noise overlay with domain warping. Later stages (erosion, hydrology,
/// climate, biomes) are called by the consumer explicitly so each can be
/// tested and re-run in isolation.
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

    let mut world = WorldData {
        meta: WorldMeta::new(params.seed),
        mesh: mesh_data,
        terrain,
        ..Default::default()
    };
    noise::overlay(
        &mut world,
        noise::NoiseParams::default(),
        &mut rng.stream(Stage::Noise),
    );
    world
}

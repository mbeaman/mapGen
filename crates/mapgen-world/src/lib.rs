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
pub mod plates;
pub mod polities;
pub mod religions;

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
    let rng = mapgen_core::StageRng::new(params.seed);
    let mut world = generate(params);

    erosion::run(
        &mut world,
        erosion_params,
        &mut rng.stream(mapgen_core::Stage::Erosion),
    );
    hydrology::detect_coast(&mut world);
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);
    hydrology::extract_rivers(&mut world, &flow_dir, 0.05);
    ocean::run(&mut world);
    climate_seasonal::run(&mut world, climate_params);
    biomes::classify(&mut world);
    cultures::populate(
        &mut world,
        cultures::CulturesParams::default(),
        &mut rng.stream(mapgen_core::Stage::Cultures),
    );
    religions::found(
        &mut world,
        religions::ReligionsParams::default(),
        &mut rng.stream(mapgen_core::Stage::Religions),
    );
    polities::lay_out(
        &mut world,
        polities::PolitiesParams::default(),
        &mut rng.stream(mapgen_core::Stage::Capitals),
    );
    naming::name_world(
        &mut world,
        naming::NamingParams::default(),
        &mut rng.stream(mapgen_core::Stage::Names),
    );
    world
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

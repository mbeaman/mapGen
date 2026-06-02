//! Resumable generation pipeline — a driver-controlled state machine over
//! the same stages [`crate::generate_full`] runs in sequence.
//!
//! The caller drives one stage at a time via [`Pipeline::step`] (or
//! [`Pipeline::step_fine`], which additionally breaks erosion into its
//! per-iteration sub-steps), inspecting the partial [`WorldData`] between
//! stages. This is the seam the web worker uses to render the map as it
//! builds up, and the CLI uses for progress / timings / stage dumps.
//!
//! **Determinism:** the stepper calls the identical stage functions in the
//! identical order, drawing from the identical per-stage RNG sub-streams
//! ([`StageRng`] is stateless and `Copy`), so a full run is byte-identical
//! to [`crate::generate_full`]. See `tests/pipeline_spec.rs`.

use mapgen_core::{Stage, StageRng, WorldData};

use crate::{
    biomes, climate::ClimateParams, climate_seasonal, cultures, erosion, hydrology, naming, ocean,
    polities, religions, sea_lanes, GenerateParams,
};

/// The visible pipeline stages, in execution order. Discriminant order is
/// the execution order; keep `ORDER`, [`PipelineStage::weight`], and
/// [`PipelineStage::label`] in sync (guarded by `tests/pipeline_spec.rs`).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PipelineStage {
    Terrain,
    Erosion,
    Hydrology,
    Ocean,
    Climate,
    Biomes,
    SeaLanes,
    Cultures,
    Religions,
    Polities,
    Naming,
    History,
}

impl PipelineStage {
    /// Canonical execution order.
    pub const ORDER: [PipelineStage; 12] = [
        PipelineStage::Terrain,
        PipelineStage::Erosion,
        PipelineStage::Hydrology,
        PipelineStage::Ocean,
        PipelineStage::Climate,
        PipelineStage::Biomes,
        PipelineStage::SeaLanes,
        PipelineStage::Cultures,
        PipelineStage::Religions,
        PipelineStage::Polities,
        PipelineStage::Naming,
        PipelineStage::History,
    ];

    /// Number of stages in a full run.
    pub fn total() -> usize {
        Self::ORDER.len()
    }

    /// Position of this stage in [`Self::ORDER`].
    pub fn index(self) -> usize {
        Self::ORDER.iter().position(|&s| s == self).unwrap()
    }

    /// Stable lowercase identifier (used by the frontend to pick a render
    /// style per stage). Never rename — it is part of the JS contract.
    pub fn id(self) -> &'static str {
        match self {
            PipelineStage::Terrain => "terrain",
            PipelineStage::Erosion => "erosion",
            PipelineStage::Hydrology => "hydrology",
            PipelineStage::Ocean => "ocean",
            PipelineStage::Climate => "climate",
            PipelineStage::Biomes => "biomes",
            PipelineStage::SeaLanes => "sea_lanes",
            PipelineStage::Cultures => "cultures",
            PipelineStage::Religions => "religions",
            PipelineStage::Polities => "polities",
            PipelineStage::Naming => "naming",
            PipelineStage::History => "history",
        }
    }

    /// Human-readable progress label.
    pub fn label(self) -> &'static str {
        match self {
            PipelineStage::Terrain => "Raising continents",
            PipelineStage::Erosion => "Eroding mountains",
            PipelineStage::Hydrology => "Carving rivers",
            PipelineStage::Ocean => "Filling the seas",
            PipelineStage::Climate => "Turning the seasons",
            PipelineStage::Biomes => "Painting biomes",
            PipelineStage::SeaLanes => "Charting sea lanes",
            PipelineStage::Cultures => "Settling cultures",
            PipelineStage::Religions => "Founding religions",
            PipelineStage::Polities => "Drawing borders",
            PipelineStage::Naming => "Naming the world",
            PipelineStage::History => "Simulating history",
        }
    }

    /// Static relative cost weight (calibrated from a 15k-cell run; no
    /// runtime measurement). Erosion dominates. Progress normalizes by the
    /// sum, so the absolute values need not total 100. History is a forward
    /// estimate (no-op until Phase 4b); recalibrate in 4j once the sim lands.
    pub fn weight(self) -> u16 {
        match self {
            PipelineStage::Terrain => 12,
            PipelineStage::Erosion => 40,
            PipelineStage::Hydrology => 14,
            PipelineStage::Ocean => 3,
            PipelineStage::Climate => 12,
            PipelineStage::Biomes => 3,
            PipelineStage::SeaLanes => 4,
            PipelineStage::Cultures => 8,
            PipelineStage::Religions => 3,
            PipelineStage::Polities => 4,
            PipelineStage::Naming => 1,
            PipelineStage::History => 8,
        }
    }
}

/// A single fine-grained step. For every stage except erosion this is one
/// whole stage (`sub == sub_total == 1`); erosion reports one
/// stream-power / thermal sweep at a time so its long run animates.
#[derive(Copy, Clone, Debug)]
pub struct FineStep {
    pub stage: PipelineStage,
    /// 1-based sub-step index within the stage.
    pub sub: u32,
    /// Total sub-steps in this stage.
    pub sub_total: u32,
}

/// Resumable world generation. Construct with [`Pipeline::new`], then call
/// [`Pipeline::step`] until it returns `None`.
pub struct Pipeline {
    world: WorldData,
    rng: StageRng,
    params: GenerateParams,
    erosion_params: erosion::ErosionParams,
    climate_params: ClimateParams,
    /// Cursor into [`PipelineStage::ORDER`]; `== total()` means done.
    next: usize,
    /// Sub-step cursor within the Erosion stage (fine stepping only).
    erosion_iter: u32,
}

impl Pipeline {
    /// New pipeline with default erosion/climate params. Cheap — builds
    /// nothing until the first [`step`](Self::step).
    pub fn new(params: GenerateParams) -> Self {
        Self::with_params(
            params,
            erosion::ErosionParams::default(),
            ClimateParams::default(),
        )
    }

    /// New pipeline with caller-supplied erosion/climate params.
    pub fn with_params(
        params: GenerateParams,
        erosion_params: erosion::ErosionParams,
        climate_params: ClimateParams,
    ) -> Self {
        let rng = StageRng::new(params.seed);
        Self {
            world: WorldData::default(),
            rng,
            params,
            erosion_params,
            climate_params,
            next: 0,
            erosion_iter: 0,
        }
    }

    /// Run the next whole stage. Returns the stage just executed, or
    /// `None` when the pipeline is finished.
    pub fn step(&mut self) -> Option<PipelineStage> {
        if self.next >= PipelineStage::ORDER.len() {
            return None;
        }
        let stage = PipelineStage::ORDER[self.next];
        self.run_stage(stage);
        self.next += 1;
        Some(stage)
    }

    /// Like [`step`](Self::step) but breaks the Erosion stage into its
    /// individual stream-power / thermal sweeps, so a caller can render a
    /// frame per erosion iteration. Every other stage is one fine step.
    /// A full fine run is byte-identical to [`step`](Self::step).
    pub fn step_fine(&mut self) -> Option<FineStep> {
        if self.next >= PipelineStage::ORDER.len() {
            return None;
        }
        let stage = PipelineStage::ORDER[self.next];

        if stage == PipelineStage::Erosion {
            let iters = self.erosion_params.iterations as u32;
            let thermal = self.erosion_params.thermal_passes as u32;
            let total_sub = iters + thermal;

            if total_sub == 0 {
                self.next += 1;
                return Some(FineStep {
                    stage,
                    sub: 1,
                    sub_total: 1,
                });
            }

            if self.erosion_iter < iters {
                erosion::run_iteration(&mut self.world, &self.erosion_params);
            } else {
                erosion::thermal_pass(&mut self.world, &self.erosion_params);
            }
            self.erosion_iter += 1;
            let sub = self.erosion_iter;
            if self.erosion_iter >= total_sub {
                self.next += 1;
            }
            return Some(FineStep {
                stage,
                sub,
                sub_total: total_sub,
            });
        }

        self.run_stage(stage);
        self.next += 1;
        Some(FineStep {
            stage,
            sub: 1,
            sub_total: 1,
        })
    }

    /// The world built so far. Safe to render at any point — every render
    /// style tolerates partially-populated `WorldData`.
    pub fn world(&self) -> &WorldData {
        &self.world
    }

    /// Cumulative weighted completion fraction in `0.0..=1.0`. During a
    /// fine erosion run, the Erosion weight band is sub-divided by the
    /// iteration cursor for smooth progress.
    pub fn progress(&self) -> f64 {
        let total: u32 = PipelineStage::ORDER.iter().map(|s| s.weight() as u32).sum();
        let done: u32 = PipelineStage::ORDER[..self.next]
            .iter()
            .map(|s| s.weight() as u32)
            .sum();
        let mut frac = done as f64;

        // Mid-erosion (fine stepping): add the partial erosion band.
        if self.erosion_iter > 0 && self.next == PipelineStage::Erosion.index() {
            let ep = &self.erosion_params;
            let total_sub = (ep.iterations + ep.thermal_passes) as f64;
            if total_sub > 0.0 {
                let w = PipelineStage::Erosion.weight() as f64;
                frac += w * (self.erosion_iter as f64 / total_sub);
            }
        }

        if total == 0 {
            return 1.0;
        }
        (frac / total as f64).min(1.0)
    }

    /// Whether every stage has run.
    pub fn is_done(&self) -> bool {
        self.next >= PipelineStage::ORDER.len()
    }

    /// Consume the pipeline and return the finished world.
    pub fn into_world(self) -> WorldData {
        self.world
    }

    fn run_stage(&mut self, stage: PipelineStage) {
        match stage {
            PipelineStage::Terrain => {
                self.world = crate::generate(self.params.clone());
            }
            PipelineStage::Erosion => {
                let mut r = self.rng.stream(Stage::Erosion);
                erosion::run(&mut self.world, self.erosion_params.clone(), &mut r);
            }
            PipelineStage::Hydrology => {
                hydrology::detect_coast(&mut self.world);
                hydrology::fill_depressions(&mut self.world);
                let flow_dir = hydrology::flow_directions(&self.world);
                hydrology::accumulate_flow(&mut self.world, &flow_dir);
                hydrology::extract_rivers(&mut self.world, &flow_dir, 0.05);
            }
            PipelineStage::Ocean => {
                ocean::run(&mut self.world);
            }
            PipelineStage::Climate => {
                climate_seasonal::run(&mut self.world, self.climate_params.clone());
            }
            PipelineStage::Biomes => {
                biomes::classify(&mut self.world);
            }
            PipelineStage::SeaLanes => {
                let mut r = self.rng.stream(Stage::SeaLanes);
                sea_lanes::chart(
                    &mut self.world,
                    sea_lanes::SeaLanesParams::default(),
                    &mut r,
                );
            }
            PipelineStage::Cultures => {
                let mut r = self.rng.stream(Stage::Cultures);
                cultures::populate(&mut self.world, cultures::CulturesParams::default(), &mut r);
            }
            PipelineStage::Religions => {
                let mut r = self.rng.stream(Stage::Religions);
                religions::found(
                    &mut self.world,
                    religions::ReligionsParams::default(),
                    &mut r,
                );
            }
            PipelineStage::Polities => {
                let mut r = self.rng.stream(Stage::Capitals);
                polities::lay_out(&mut self.world, polities::PolitiesParams::default(), &mut r);
            }
            PipelineStage::Naming => {
                let mut r = self.rng.stream(Stage::Names);
                naming::name_world(&mut self.world, naming::NamingParams::default(), &mut r);
            }
            PipelineStage::History => {
                let mut r = self.rng.stream(Stage::History);
                mapgen_history::run(
                    &mut self.world,
                    mapgen_history::HistoryParams::default(),
                    &mut r,
                );
            }
        }
    }
}

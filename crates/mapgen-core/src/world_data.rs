//! The trunk type. Everything the simulation builds and everything the
//! renderer / lore engine reads flows through `WorldData`.

use serde::{Deserialize, Serialize};

use crate::{
    entities::EntityStore,
    event::{EventLog, Work},
    ids::PlateId,
};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorldData {
    pub meta: WorldMeta,
    pub mesh: MeshData,
    pub terrain: TerrainData,
    #[serde(default)]
    pub hydrology: HydrologyData,
    #[serde(default)]
    pub climate: ClimateData,
    #[serde(default)]
    pub society: SocietyData,
    #[serde(default)]
    pub entities: EntityStore,
    #[serde(default)]
    pub events: EventLog,
    #[serde(default)]
    pub works: Vec<Work>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldMeta {
    pub seed: u64,
    pub schema_version: u32,
}

impl Default for WorldMeta {
    fn default() -> Self {
        Self {
            seed: 0,
            schema_version: SCHEMA_VERSION,
        }
    }
}

impl WorldMeta {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            schema_version: SCHEMA_VERSION,
        }
    }
}

/// Cell graph + geometry. Persisted in full so worlds can be re-rendered
/// without re-running the Voronoi.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MeshData {
    pub width: f32,
    pub height: f32,
    /// One entry per cell: cell center.
    pub sites: Vec<[f32; 2]>,
    /// Pool of polygon vertices; cells index into this.
    pub vertices: Vec<[f32; 2]>,
    /// Per cell: ordered indices into `vertices` describing the polygon.
    pub cell_vertices: Vec<Vec<u32>>,
    /// Per cell: indices of neighboring cells.
    pub neighbors: Vec<Vec<u32>>,
    /// Per cell: true if cell touches the sea (coast). Filled in Phase 2.
    #[serde(default)]
    pub coast: Vec<bool>,
}

impl MeshData {
    pub fn cell_count(&self) -> usize {
        self.sites.len()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TerrainData {
    /// Per cell, normalized elevation. <0 is sea, >0 is land. Sea level = 0.
    pub elevation: Vec<f32>,
    /// Per cell, the plate this cell belongs to.
    pub plate_id: Vec<PlateId>,
    /// Per plate: type + drift + base elevation. Kept around so renderers /
    /// later passes can introspect tectonic origin.
    pub plates: Vec<PlateRecord>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PlateRecord {
    pub kind: PlateKind,
    pub center: [f32; 2],
    pub drift: [f32; 2],
    pub base_elevation: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlateKind {
    Oceanic,
    Continental,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HydrologyData {
    pub flow: Vec<f32>,
    pub rivers: Vec<River>,
    pub lakes: Vec<Lake>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct River {
    pub cells: Vec<u32>,
    pub width: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lake {
    pub cells: Vec<u32>,
    pub level: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClimateData {
    pub precipitation: Vec<f32>,
    pub temperature: Vec<f32>,
    pub biome: Vec<u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SocietyData {
    pub nations: Vec<Nation>,
    pub roads: Vec<Road>,
    /// Per cell, the controlling nation (`None` for unclaimed land/sea).
    pub control: Vec<Option<u32>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Nation {
    pub name: String,
    pub capital_cell: u32,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Road {
    pub cells: Vec<u32>,
}

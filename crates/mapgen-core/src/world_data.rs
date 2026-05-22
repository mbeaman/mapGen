//! The trunk type. Everything the simulation builds and everything the
//! renderer / lore engine reads flows through `WorldData`.

use serde::{Deserialize, Serialize};

use crate::{
    entities::{Culture, EntityStore, Religion, Settlement},
    event::{EventLog, Work},
    ids::PlateId,
};

/// On-disk schema version. Bump on any breaking change to `WorldData` shape
/// that older readers can't recover from via `#[serde(default)]`.
///
/// History:
/// * v1 — Phase 1 baseline (mesh + terrain + entity store + events).
/// * v2 — Phase 2 climate/hydrology/biome fields landed via `#[serde(default)]`.
/// * v3 — Phase 3a cultures field added. Pre-v3 worlds deserialize with an
///   empty `CulturesData` (compatible — `#[serde(default)]`).
/// * v4 — Phase 3b religions field added. Pre-v4 worlds deserialize with an
///   empty `ReligionsData` (compatible — `#[serde(default)]`).
/// * v5 — Phase 3c polities field — `SocietyData::settlements` populated.
///   Pre-v5 worlds load with an empty settlements vec (compatible —
///   `#[serde(default)]` on the field).
pub const SCHEMA_VERSION: u32 = 5;

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
    /// Phase 3a — cultures stage output. Empty on pre-Phase-3a worlds and
    /// on worlds where the cultures stage hasn't run yet.
    #[serde(default)]
    pub cultures: CulturesData,
    /// Phase 3b — religions stage output. Empty on pre-Phase-3b worlds and
    /// on worlds where the religions stage hasn't run yet.
    #[serde(default)]
    pub religions: ReligionsData,
    #[serde(default)]
    pub entities: EntityStore,
    #[serde(default)]
    pub events: EventLog,
    #[serde(default)]
    pub works: Vec<Work>,
    /// Lore-driven overrides on top of scientific defaults. Empty by
    /// default. History sim and authored content populate this.
    #[serde(default)]
    pub patches: crate::patch::PatchData,
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
    /// Per-cell temperature delta from ocean currents (gyre limbs +
    /// upwelling). Computed by `ocean::run` before `climate::run`. Empty
    /// if ocean stage hasn't run.
    #[serde(default)]
    pub coastal_temp_anomaly: Vec<f32>,
    /// Seasonal climate (filled by `climate_seasonal::run`). Annual
    /// values above are means of these two passes. Empty if seasonal
    /// stage hasn't run.
    #[serde(default)]
    pub temperature_summer: Vec<f32>,
    #[serde(default)]
    pub temperature_winter: Vec<f32>,
    #[serde(default)]
    pub precipitation_summer: Vec<f32>,
    #[serde(default)]
    pub precipitation_winter: Vec<f32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SocietyData {
    /// Polity roster (the architecture's "polities"; type kept as
    /// `Nation` for schema continuity). Each polity has a capital cell
    /// and a display color.
    pub nations: Vec<Nation>,
    /// Settlements (capitals + towns + villages) in Christaller-hierarchy
    /// order. Filled by the Phase 3c polities stage. Empty on pre-v5
    /// worlds via `#[serde(default)]`.
    #[serde(default)]
    pub settlements: Vec<Settlement>,
    pub roads: Vec<Road>,
    /// Per cell, the controlling polity (`None` for unclaimed land/sea).
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

/// Output of the Phase 3a cultures stage.
///
/// `cultures` holds the per-world roster; `culture_id` is the per-cell
/// assignment. A cell's index into `culture_id` matches its mesh cell index.
/// `None` means "unassigned" — sea cells, or land cells the cultures stage
/// couldn't place under habitat-fitness rules.
///
/// Indices into `cultures` are `u16` so a culture id fits alongside a cell id
/// pair in the Voronoi-assignment scratch space. 65 535 cultures per world is
/// far above the realistic ceiling (architecture targets 4-8 in MVP).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CulturesData {
    pub cultures: Vec<Culture>,
    /// Per cell, `Some(index)` into `cultures` or `None` for unassigned.
    /// Empty when the cultures stage hasn't run.
    pub culture_id: Vec<Option<u16>>,
}

/// Output of the Phase 3b religions stage.
///
/// `religions` is the per-world roster of founded religions (1-3 per
/// world); `religion_id` is the per-cell back-reference, `Some(index)`
/// for cells with a religion or `None` for sea / unconverted land.
///
/// Religions attach to cultures (not to polities) and can spread across
/// cultural borders by alignment compatibility — per ARCHITECTURE.md §4
/// Phase 3b. Indices into `religions` are `u16` for symmetry with
/// `CulturesData::culture_id`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReligionsData {
    pub religions: Vec<Religion>,
    /// Per cell, `Some(index)` into `religions` or `None` for sea /
    /// unconverted. Empty when the religions stage hasn't run.
    pub religion_id: Vec<Option<u16>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_v3_worlds_deserialize_with_empty_cultures() {
        // A worst-case "v2-shaped" payload: every documented field except
        // the new `cultures` slot. `#[serde(default)]` on the field is the
        // forward-compat contract that lets v2 archives load through v3
        // code without a migration; this test fails the moment that
        // contract breaks (typo in `default`, accidental
        // `deny_unknown_fields`, etc.).
        let v2_json = r#"{
            "meta": { "seed": 0, "schema_version": 2 },
            "mesh": {
                "width": 0.0, "height": 0.0,
                "sites": [], "vertices": [],
                "cell_vertices": [], "neighbors": []
            },
            "terrain": { "elevation": [], "plate_id": [], "plates": [] }
        }"#;

        let world: WorldData =
            serde_json::from_str(v2_json).expect("v2-shaped JSON must deserialize under v3 schema");

        assert!(
            world.cultures.cultures.is_empty(),
            "pre-v3 world loaded with non-empty cultures roster"
        );
        assert!(
            world.cultures.culture_id.is_empty(),
            "pre-v3 world loaded with non-empty culture_id vec"
        );
        // Sanity: every other `#[serde(default)]` field also takes its default.
        assert_eq!(world.meta.schema_version, 2, "schema_version preserved");
        assert!(world.hydrology.rivers.is_empty());
        assert!(world.climate.temperature.is_empty());
        // v4 forward-compat: religions field also defaults empty.
        assert!(world.religions.religions.is_empty());
        assert!(world.religions.religion_id.is_empty());
        // v5 forward-compat: settlements vec also defaults empty.
        assert!(world.society.settlements.is_empty());
    }
}

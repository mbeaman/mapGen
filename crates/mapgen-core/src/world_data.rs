//! The trunk type. Everything the simulation builds and everything the
//! renderer / lore engine reads flows through `WorldData`.

use serde::{Deserialize, Serialize};

use crate::{
    entities::{Culture, EntityStore, Language, Religion, Settlement},
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
/// * v6 — Phase 3d languages field — `WorldData::languages` populated.
///   Pre-v6 worlds load with an empty languages vec.
/// * v7 — Phase 3e polish: `River::name` / `Lake::name` for major
///   features. Empty names are `skip_serializing_if`-elided, so the
///   on-disk shape of pre-v7 / unnamed worlds is unchanged (the Phase-2
///   golden hash is unaffected; only the full golden re-anchors).
/// * v8 — Phase 3e polish: `WorldData::mountain_ranges` (named clusters
///   of ALPINE/SNOW cells). `skip_serializing_if`-elided when empty;
///   both goldens re-anchor for the `schema_version` byte itself.
/// * v9 — Phase 4b: the history sim populates `WorldData::events` (Turchin
///   demographic backbone → Famine / Plague / Drought). No struct change
///   (the `events` field already existed); both goldens re-anchor for the
///   `schema_version` byte and `seed42_full` additionally for the events.
/// * v10 — Phase 4c: agent layer. New `Entity::Title` variant + additive
///   `#[serde(default)]` fields on `Character` / `Dynasty` / `House`
///   (lineage, titles, sex, culture). History populates `WorldData::entities`
///   with rulers / houses / dynasties / titles. Both goldens re-anchor for
///   the `schema_version` byte; `seed42_full` additionally for the entities.
/// * v11 — Phase 4i: `WorldData::history` (`HistoryData` — mythic ages +
///   narrative arcs woven from the causal event graph). `skip`-elided when
///   empty; both goldens re-anchor for the `schema_version` byte, `seed42_full`
///   additionally for the arcs/ages.
/// * v12 — Phase 4k: new `Entity::Megabeast` variant. The hero loop now mints
///   the beast as a named entity (so it enters the NER lexicon) and references
///   it as actor/patient on its `MegabeastRise` / `MegabeastSlain` events. Both
///   goldens re-anchor for the `schema_version` byte; `seed42_full` additionally
///   for the new entities + event actor refs.
/// * v13 — Phase 4k: new `ArcKind::Conquest` variant (a plain war of expansion,
///   distinct from HolyWar / DynasticConflict / Chronicle). Both goldens
///   re-anchor for the `schema_version` byte; `seed42_full` additionally for the
///   re-woven arcs (HolyWar now requires a real schism; wars cite schisms).
/// * v14 — Phase 6.1.4: per-cell USDA soil order (`ClimateData::soil`, filled by
///   `soils::classify`) plus a soil-driven `biomes::WETLAND` (id 15) override for
///   waterlogged Histosol cells. Both goldens re-anchor for the `schema_version`
///   byte and the new `soil` array; `seed42_full`/`seed42_phase2` additionally
///   for any cells that flip to WETLAND.
/// * v15 — Phase 6.1.5: per-cell Strahler stream order (`HydrologyData::strahler`)
///   plus per-river `River::strahler` (mouth order) and `River::regime` (seasonal
///   flow class). Both goldens re-anchor for the `schema_version` byte and the new
///   hydrology arrays; `seed42_full` additionally for the per-river fields.
/// * v16 — time-slider: `HistoryData::border_changes` records each cell that
///   changed hands in a won war, so `control_at_year` can reconstruct the map at
///   any past year. Both goldens re-anchor for the `schema_version` byte;
///   `seed42_full` additionally for the recorded changes (phase2 has no history).
/// * v17 — planet view: named geographic bodies `WorldData::continents`
///   (flood-filled landmasses) and `WorldData::oceans` (major seas), each labelled
///   in its dominant culture's tongue so the planisphere can ground continent /
///   ocean names. Populated by the naming stage; `skip`-elided when empty. All
///   three goldens re-anchor for the `schema_version` byte; `seed42_full`
///   additionally for the named bodies (phase2/sector snapshot pre-naming state).
/// * v18 — "The Sundered Lanes" maritime substrate: `WorldData::sea_lanes`
///   (`SeaLanesData` — inter-continental sea lanes, each a coastal-anchor pair
///   `a<b` with a `min_naval` crossing gate; the f32 `cost` is `serde(skip)`, off
///   the hashed path). Populated by the new `sea_lanes` stage, `skip`-elided when
///   empty. The graph is inter-*continental*, so it stays empty on a single-
///   landmass world: the three goldens (continental seed 42 + its phase2/sector
///   snapshots) grow no lanes and re-anchored for the `schema_version` byte alone
///   — verified by reverting the constant to 17 with all sea_lanes code in place
///   and confirming the goldens hold. Multi-continent (planet-scale) worlds, which
///   no golden covers, carry the realized lane graph.
/// * v19 — landmass-distinct society (the Sundered Lanes foundation): the cultures
///   stage now INSTANCES each global culture per landmass (`cultures::populate`
///   remap — the same archetype on two continents becomes two distinct
///   `culture_id`s), so polities/control/history/naming all confine to a single
///   landmass and inter-continental reach must be earned over the sea lanes. No
///   `WorldData` field SHAPE changes (`cultures: Vec<Culture>` / `culture_id:
///   Vec<Option<u16>>` keep their types; only the Vec length grows on multi-
///   landmass worlds), so this is a pure version-byte bump like v18. seed42 is a
///   single landmass, where the instancing is the IDENTITY of the old survivor
///   numbering — verified byte-identical content with the constant held at 18
///   before bumping; the three goldens re-anchor for the version byte alone.
///   (Religion still spreads globally at gen time — that crossing is deferred to
///   the Phase-2 Diffusion loop, not this change.)
/// * v20 — Faith time-slider: `HistoryData::faith_changes` records each cell the
///   Diffusion carrier converts, so `religion_at_year` can reconstruct the faith
///   map at any past year (the mirror of v16's `border_changes`). `skip`-elided
///   when empty, and seed42's diffusion is a no-op (no crossable lane, no
///   faithless land), so the field is byte-invisible there — verified by holding
///   the constant at 19 with all faith_changes code in place and confirming the
///   three goldens hold (a non-perturbation proof), THEN bumping. The three
///   goldens re-anchor for the `schema_version` byte alone; the diffusion timeline
///   itself is pinned on crossing seeds (which no golden covers).
pub const SCHEMA_VERSION: u32 = 20;

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
    /// Phase 3d — generated per-culture languages. Indexed by
    /// `Culture::language_id`. Empty until the naming stage runs.
    #[serde(default)]
    pub languages: Vec<Language>,
    /// Phase 3e — named major mountain ranges (connected clusters of
    /// ALPINE/SNOW cells). Populated by the naming stage; `skip`-elided
    /// when empty so unnamed worlds keep their on-disk shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mountain_ranges: Vec<MountainRange>,
    /// Named major continents (flood-filled landmasses). Populated by the naming
    /// stage; `skip`-elided when empty so unnamed worlds keep their on-disk shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continents: Vec<Continent>,
    /// Named major oceans / seas. Populated by the naming stage; `skip`-elided
    /// when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oceans: Vec<Ocean>,
    /// Inter-continental sea lanes — the maritime-connectivity substrate
    /// ("The Sundered Lanes", `docs/inter_continental_design.md`). Populated by
    /// the `sea_lanes` stage from geography (currents + winds + distance); the
    /// history sim queries them, naval-gated, to let society reach across oceans.
    /// `skip`-elided when empty so pre-v18 / laneless worlds keep their shape.
    #[serde(default, skip_serializing_if = "SeaLanesData::is_empty")]
    pub sea_lanes: SeaLanesData,
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
    /// Phase 4i — narrative scaffolding (mythic ages + arcs) woven from the
    /// event graph after the sim. `skip`-elided when empty.
    #[serde(default, skip_serializing_if = "crate::history::HistoryData::is_empty")]
    pub history: crate::history::HistoryData,
}

impl WorldData {
    /// Reconstruct the per-cell controlling polity as it was at the end of
    /// `year` (the time-slider). `society.control` holds the *present* state;
    /// this walks it backward by undoing every recorded `border_change` with a
    /// later year — in reverse insertion order, so a cell that changed hands
    /// several times is restored correctly. With no recorded history it returns
    /// the present control unchanged.
    pub fn control_at_year(&self, year: i32) -> Vec<Option<u32>> {
        let mut control = self.society.control.clone();
        for ch in self.history.border_changes.iter().rev() {
            if ch.year > year {
                if let Some(slot) = control.get_mut(ch.cell as usize) {
                    *slot = ch.from;
                }
            }
        }
        control
    }

    /// Inclusive `(first, last)` year span of recorded territorial change, or
    /// `None` if no borders ever moved. Drives the time-slider's range.
    pub fn border_change_year_span(&self) -> Option<(i32, i32)> {
        let years = self.history.border_changes.iter().map(|c| c.year);
        let first = years.clone().min()?;
        let last = self.history.border_changes.iter().map(|c| c.year).max()?;
        Some((first, last))
    }
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
    /// World-space rectangle `[x0, y0, x1, y1]` the cells actually occupy
    /// (Phase 7 multi-scale). `None` on a whole-world (level-0) mesh, where the
    /// cells fill `[0,0]..[width,height]`; `Some` on a refined sub-sector, whose
    /// `width`/`height` stay the *full* world extent (so plate scatter and
    /// climate latitude remain global) while its cells occupy only this
    /// sub-rectangle. Render uses it as the viewport. Elided when `None`, so
    /// level-0 worlds serialize byte-identically (no golden change).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<[f32; 4]>,
}

impl MeshData {
    pub fn cell_count(&self) -> usize {
        self.sites.len()
    }

    /// The world-space rectangle `[x0, y0, x1, y1]` the cells occupy — the
    /// explicit `region` for a refined sector, else the full `[0,0,width,height]`.
    pub fn view_rect(&self) -> [f32; 4] {
        self.region.unwrap_or([0.0, 0.0, self.width, self.height])
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
    /// Per-cell Strahler stream order (Phase 6.1.5): 0 off the river network,
    /// 1 at headwaters, +1 where two equal-order streams meet. Filled by
    /// `extract_rivers`. Empty on pre-v15 worlds.
    #[serde(default)]
    pub strahler: Vec<u8>,
}

/// Seasonal flow-regime classes for [`River::regime`] (Phase 6.1.5). Stored as
/// `u8` for serde stability; `PERENNIAL` is the 0 default.
pub mod river_regime {
    /// Flow roughly balanced across seasons (humid temperate / equatorial).
    pub const PERENNIAL: u8 = 0;
    /// Summer-dominant supply — monsoonal / wet-summer catchments.
    pub const SUMMER_MONSOON: u8 = 1;
    /// Winter-dominant supply — Mediterranean / wet-winter catchments.
    pub const WINTER_RAIN: u8 = 2;
    /// Snowmelt-fed — cold headwaters, spring freshet (nival).
    pub const NIVAL: u8 = 3;
    /// Intermittent / ephemeral — arid catchment, flows only seasonally.
    pub const EPHEMERAL: u8 = 4;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct River {
    pub cells: Vec<u32>,
    pub width: f32,
    /// Generated name — set by the Phase 3d naming stage for *major*
    /// rivers only; empty for minor watercourses. `skip_serializing_if`
    /// keeps pre-naming worlds (and minor rivers) byte-identical so the
    /// Phase-2 golden hash is unaffected.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Strahler order at the river's mouth — its highest order along the chain
    /// (Phase 6.1.5). 0 on pre-v15 worlds; ~1–2 = creek/brook, 3–4 = stream,
    /// 5+ = major river. Used for naming gating and ornate render width.
    #[serde(default)]
    pub strahler: u8,
    /// Seasonal flow regime (Phase 6.1.5) — one of the `river_regime::*`
    /// classes, derived from the catchment's seasonal precipitation balance
    /// and headwater temperature. 0 (`PERENNIAL`) on pre-v15 worlds.
    #[serde(default)]
    pub regime: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lake {
    pub cells: Vec<u32>,
    pub level: f32,
    /// Generated name — set by the naming stage for sizeable lakes only;
    /// empty otherwise. See `River::name` for the serialization note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// A named mountain range — a connected cluster of ALPINE/SNOW cells.
/// Produced by the Phase 3e naming stage for *major* clusters only.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MountainRange {
    pub cells: Vec<u32>,
    pub name: String,
}

/// A named continent — a *major* connected landmass (flood-filled over the mesh
/// graph). Produced by the naming stage for bodies above a size threshold, named
/// in the language of the culture that dominates it (most owned cells), falling
/// back to the first culture's language when uninhabited. Only the label anchor
/// and size are persisted — unlike [`MountainRange`], the planisphere doesn't
/// draw the cells, it places one engraved label; the per-cell membership a future
/// continent-aware drill-snap needs is a separate `continent_id` map, not this.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Continent {
    pub name: String,
    /// World-space centroid (mean of member cell sites), the label anchor.
    pub centroid: [f32; 2],
    /// Number of mesh cells in the landmass — drives label size + speck-skip.
    pub cell_count: u32,
}

/// One inter-continental sea lane: a navigable crossing between two coastal
/// *anchor* cells on different landmasses. `cost` is the sea-path cost that
/// produced it; `min_naval` is the calibrated crossing gate — a polity crosses
/// iff its naval skill `>= min_naval`.
///
/// `cost` is `#[serde(skip)]`: it is an f32 used only at build time (to derive
/// `min_naval` and order candidate crossings) and consumed by no one post-gen,
/// so it is kept *off* the hashed, native↔wasm-checked path — only the integer
/// `(a, b, min_naval)` are serialized. (A cross-platform f32 wobble that does
/// not cross a `min_naval` quantization boundary is then invisible to the
/// golden; one that does still flips `min_naval` and is caught.) Deserializes to
/// `cost = 0.0`; lanes are recomputed fresh each run, never round-tripped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeaLane {
    /// Coastal anchor cell ids, canonical `a < b`.
    pub a: u32,
    pub b: u32,
    /// Sea-path cost of the crossing (world units). Build-time only — see the
    /// struct note on why this is `#[serde(skip)]`.
    #[serde(skip)]
    pub cost: f32,
    /// Naval skill required to use the lane (the dual-filter gate).
    pub min_naval: u8,
}

/// The maritime-connectivity substrate: the set of inter-continental sea lanes.
/// Empty until the `sea_lanes` stage runs (and on continental-scale / pre-v18
/// worlds). Queried by the history sim at a polity's naval threshold.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeaLanesData {
    pub lanes: Vec<SeaLane>,
}

impl SeaLanesData {
    pub fn is_empty(&self) -> bool {
        self.lanes.is_empty()
    }
}

/// A named ocean / sea — a *major* connected body of water. Named in the
/// language of the culture dominating its coastal-adjacent land, falling back to
/// the first culture's language. Same persisted shape as [`Continent`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ocean {
    pub name: String,
    pub centroid: [f32; 2],
    pub cell_count: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClimateData {
    pub precipitation: Vec<f32>,
    pub temperature: Vec<f32>,
    pub biome: Vec<u8>,
    /// Per-cell USDA soil order (Phase 6.1.4). One of the 12 `soils::*` order
    /// ids on land; `soils::OCEAN` on sea cells. Filled by `soils::classify`
    /// (run at the head of the Biomes stage, after climate + hydrology).
    /// Empty on pre-v14 worlds and before the Biomes stage runs.
    #[serde(default)]
    pub soil: Vec<u8>,
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
    fn control_at_year_reconstructs_past_borders() {
        use crate::history::BorderChange;
        let mut w = WorldData::default();
        // Present: cell 0 owned by polity 2, having passed 0 → 1 → 2.
        w.society.control = vec![Some(2)];
        w.history.border_changes = vec![
            BorderChange {
                year: 10,
                cell: 0,
                from: Some(0),
                to: Some(1),
            },
            BorderChange {
                year: 20,
                cell: 0,
                from: Some(1),
                to: Some(2),
            },
        ];
        assert_eq!(w.control_at_year(100), vec![Some(2)], "future = present");
        assert_eq!(w.control_at_year(25), vec![Some(2)], "after both changes");
        assert_eq!(w.control_at_year(15), vec![Some(1)], "between the two");
        assert_eq!(w.control_at_year(5), vec![Some(0)], "before any change");
        assert_eq!(w.border_change_year_span(), Some((10, 20)));
    }

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
        // v6 forward-compat: languages vec also defaults empty.
        assert!(world.languages.is_empty());
    }
}

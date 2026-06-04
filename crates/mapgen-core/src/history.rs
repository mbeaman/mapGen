//! Post-simulation narrative scaffolding the history sim writes and the lore
//! engine (Phase 5) reads: dramatic **arcs** woven from the causal event graph,
//! and **mythic ages** that frame the 500 years. Derived from `events` +
//! `cause_ids`; persisted so a chronicle can pick threads without re-deriving.

use serde::{Deserialize, Serialize};

use crate::ids::{EntityId, EventId};

/// The narrative side-tables on `WorldData`. Empty until the history sim runs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HistoryData {
    /// The 500 years partitioned into named mythic ages (chronological).
    pub ages: Vec<MythicAge>,
    /// Dramatic threads woven from causally-linked, salient events.
    pub arcs: Vec<NarrativeArc>,
    /// Chronological territorial changes — each cell that changed hands in a won
    /// war (the time-slider). Lets `society.control` be reconstructed at any past
    /// year via [`crate::WorldData::control_at_year`]. Empty if no conquests
    /// occurred or on pre-time-slider worlds.
    #[serde(default)]
    pub border_changes: Vec<BorderChange>,
    /// Chronological faith conversions — each cell a religion diffused into (the
    /// Faith time-slider). Lets `religions.religion_id` be reconstructed at any
    /// past year via [`crate::WorldData::religion_at_year`]. Empty on a world with
    /// no diffusion (e.g. a laneless, fully-converted seed); `skip_serializing_if`
    /// then omits it, so adding this field left the no-op seed42 golden
    /// byte-identical (a non-perturbation *proof*, not a re-anchor) — diffusion is
    /// pinned on crossing seeds instead. Deliberately unlike [`border_changes`]
    /// (which re-anchored); see the schema-history comment on `WorldData`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faith_changes: Vec<FaithChange>,
}

/// One territorial change recorded during the history sim: in `year`, `cell`
/// passed from polity `from` to polity `to`. Appended chronologically.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BorderChange {
    pub year: i32,
    pub cell: u32,
    pub from: Option<u32>,
    pub to: Option<u32>,
}

/// One faith conversion recorded during the history sim: in `year`, `cell`'s
/// religion passed from `from` to `to`. Appended chronologically by the Diffusion
/// carrier. Diffusion only fills *unconverted* cells, so `from` is `None` today;
/// it's kept (mirroring [`BorderChange`]) for a future faith that displaces
/// another.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FaithChange {
    pub year: i32,
    pub cell: u32,
    pub from: Option<u16>,
    pub to: Option<u16>,
}

impl HistoryData {
    pub fn is_empty(&self) -> bool {
        self.ages.is_empty() && self.arcs.is_empty()
    }
}

/// A named epoch. The chronicler anchors relative chronology to these
/// ("late in the Age of Heroes…").
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MythicAge {
    pub name: String,
    pub start_year: i32,
    pub end_year: i32,
    pub motif: AgeMotif,
}

/// The dominant character of an age, classified from its event mix. Append-only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "motif")]
pub enum AgeMotif {
    /// Realms and faiths take shape.
    Founding,
    /// Champions, megabeasts, prophecy.
    Heroic,
    /// Prosperity and few crises.
    Golden,
    /// War, collapse, plague.
    Dark,
}

/// A dramatic thread: a connected cluster of causally-linked salient events
/// sharing a cast. The unit a chronicler turns into a story.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NarrativeArc {
    pub title: String,
    pub kind: ArcKind,
    /// Member events in chronological order (the thread).
    pub member_events: Vec<EventId>,
    /// The earliest, the most salient, and the latest member.
    pub start_event: EventId,
    pub climax_event: EventId,
    pub end_event: EventId,
    /// The recurring cast, most-involved first.
    pub key_characters: Vec<EntityId>,
}

/// Arc classification by dominant event kinds. Append-only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ArcKind {
    /// A war of religion.
    HolyWar,
    /// Megabeast / champion / prophecy.
    HeroSaga,
    /// Succession crisis or dynastic claim driving conflict.
    DynasticConflict,
    /// A war of expansion between realms — no schism, no dynastic claim, just
    /// territory. Appended in Phase 4k (append-only). Keeps plain border wars
    /// out of the catch-all `Chronicle`.
    Conquest,
    /// Anything else linked and salient.
    Chronicle,
}

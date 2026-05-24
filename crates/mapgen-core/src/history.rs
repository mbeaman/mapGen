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
    /// Anything else linked and salient.
    Chronicle,
}

//! Typed, immutable, append-only event log. The canonical history of a world.
//!
//! Schema is frozen: never remove a variant, never reorder discriminants, only
//! append. The lore engine reads from this; it never writes.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::{CellId, EntityId, EventId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub year: i32,
    pub kind: EventKind,
    #[serde(default)]
    pub actors: SmallVec<[EntityId; 4]>,
    #[serde(default)]
    pub patients: SmallVec<[EntityId; 4]>,
    pub location: Option<CellId>,
    #[serde(default)]
    pub cause_ids: SmallVec<[EventId; 4]>,
    pub salience: f32,
    pub casus_belli: Option<CasusBelli>,
    pub summary_canonical: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventKind {
    Birth,
    Death,
    Marriage,
    Coronation,
    Succession,
    ClaimAsserted,
    ClaimRenounced,
    WarDeclared,
    BattleFought,
    Siege,
    TreatySigned,
    TreatyBroken,
    AllianceFormed,
    TradeRouteOpened,
    EmbargoImposed,
    ReligionFounded,
    Schism,
    ProphecyUttered,
    ProphecyFulfilled,
    ArtifactForged,
    ArtifactStolen,
    ArtifactDestroyed,
    Migration,
    CityFounded,
    CityAbandoned,
    Plague,
    Famine,
    Drought,
    MegabeastRise,
    MegabeastSlain,
    Ascension,
    Exile,
    Return,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CasusBelli {
    BrokenTreaty,
    DynasticClaim,
    ReligiousSchism,
    Embargo,
    Insult,
    Prophecy,
    FrontierIncident,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EventLog {
    pub events: Vec<Event>,
    pub next_id: u32,
}

impl EventLog {
    pub fn push(&mut self, mut event: Event) -> EventId {
        event.id = EventId(self.next_id);
        self.next_id += 1;
        let id = event.id;
        self.events.push(event);
        id
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn major(&self, salience_floor: f32) -> impl Iterator<Item = &Event> {
        self.events
            .iter()
            .filter(move |e| e.salience >= salience_floor)
    }
}

/// A Claude-narrated chronicle. Persisted into the world so future chronicles
/// can cite it — the canon grows.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Work {
    pub title: String,
    pub body: String,
    pub in_world_author: String,
    pub references: Vec<EventId>,
    pub lacunae: Vec<String>,
    pub written_year: i32,
}

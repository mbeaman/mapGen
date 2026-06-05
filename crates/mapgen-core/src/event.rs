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
    /// For an inter-continental "far shore" event — today a faith crossing
    /// ([`EventKind::FaithCrossed`]); in the full landmass arc also a colony or a
    /// conquest — the index into `world.continents` of the shore that was reached.
    /// Lets the narrator NAME that shore and group everything that happened to it,
    /// structurally (not by string-matching the summary). `None` for every other
    /// event. Byte-invisible on a laneless world (seed42): no crossing fires, so it
    /// stays `None` and `skip_serializing_if` omits it from the hashed bytes. (v22)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub far_shore: Option<u16>,
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
    /// (v22) A faith first crossed a sea lane to a far shore — the water-crossing
    /// milestone of religious diffusion. Emitted once per (faith, far continent)
    /// pair, on the first conversion of that continent's anchor; the inland spread
    /// that follows is silent. Carries `far_shore` (the reached continent).
    FaithCrossed,
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Work {
    pub title: String,
    pub body: String,
    pub in_world_author: String,
    pub references: Vec<EventId>,
    pub lacunae: Vec<String>,
    pub written_year: i32,
}

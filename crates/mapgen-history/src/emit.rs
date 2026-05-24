//! The single event builder for the whole history sim — replaces the per-loop
//! `emit` helpers that had accumulated (turchin / agent / mearsheimer / schism /
//! hero each grew their own near-identical copy).
//!
//! It holds owned data, so a call site can read `world` while constructing the
//! event, then `push` once. Carrying `cause_ids` as a first-class field is what
//! lets every loop record causality cheaply — the backbone of 4i's causal chains.

use mapgen_core::ids::CellId;
use mapgen_core::{CasusBelli, EntityId, Event, EventId, EventKind, WorldData};
use smallvec::SmallVec;

/// A builder for one [`Event`]. Construct with [`Emit::new`], chain the optional
/// parts, and [`Emit::push`] it onto the world's log (which assigns the id).
#[must_use]
pub(crate) struct Emit {
    year: i32,
    kind: EventKind,
    cell: u32,
    salience: f32,
    actors: SmallVec<[EntityId; 4]>,
    patients: SmallVec<[EntityId; 4]>,
    causes: SmallVec<[EventId; 4]>,
    casus_belli: Option<CasusBelli>,
    summary: String,
}

impl Emit {
    pub fn new(year: i32, kind: EventKind, cell: u32, salience: f32, summary: String) -> Self {
        Self {
            year,
            kind,
            cell,
            salience,
            actors: SmallVec::new(),
            patients: SmallVec::new(),
            causes: SmallVec::new(),
            casus_belli: None,
            summary,
        }
    }

    pub fn actors(mut self, actors: &[EntityId]) -> Self {
        self.actors.extend(actors.iter().copied());
        self
    }

    pub fn patients(mut self, patients: &[EntityId]) -> Self {
        self.patients.extend(patients.iter().copied());
        self
    }

    /// The events this one was caused by (4i causal grammar; fan-in ≤ 2).
    pub fn causes(mut self, causes: &[EventId]) -> Self {
        self.causes.extend(causes.iter().copied());
        self
    }

    pub fn casus(mut self, casus_belli: CasusBelli) -> Self {
        self.casus_belli = Some(casus_belli);
        self
    }

    pub fn push(self, world: &mut WorldData) -> EventId {
        world.events.push(Event {
            id: EventId(0), // assigned by EventLog::push
            year: self.year,
            kind: self.kind,
            actors: self.actors,
            patients: self.patients,
            location: Some(CellId(self.cell)),
            cause_ids: self.causes,
            salience: self.salience,
            casus_belli: self.casus_belli,
            summary_canonical: self.summary,
        })
    }
}

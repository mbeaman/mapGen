//! Entity records. Minimal in Phase 1; expanded as later phases need them.
//! Schema is JSON-stable: add new variants only at the end of enums.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::ids::EntityId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Entity {
    Polity(Polity),
    Dynasty(Dynasty),
    House(House),
    Character(Character),
    Site(Site),
    Religion(Religion),
    Deity(Deity),
    Artifact(Artifact),
    Claim(Claim),
    Culture(Culture),
    Language(Language),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Polity {
    pub name: String,
    pub capital: Option<EntityId>,
    pub founded_year: i32,
    pub dissolved_year: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Dynasty {
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct House {
    pub name: String,
    pub dynasty: Option<EntityId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub born_year: i32,
    pub died_year: Option<i32>,
    pub house: Option<EntityId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Site {
    pub name: String,
    pub cell: u32,
    pub tier: u8, // 0=capital, 1=town, 2=village
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Religion {
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Deity {
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Claim {
    pub claimant: EntityId,
    pub target: EntityId,
    pub asserted_year: i32,
    pub dormant: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Culture {
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Language {
    pub name: String,
}

/// Append-only registry. `IndexMap` guarantees deterministic iteration order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EntityStore {
    pub by_id: IndexMap<EntityId, Entity>,
    pub next_id: u32,
}

impl EntityStore {
    pub fn insert(&mut self, entity: Entity) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id += 1;
        self.by_id.insert(id, entity);
        id
    }
}

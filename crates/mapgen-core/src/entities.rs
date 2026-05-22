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

/// How a religion organizes its supernatural object — driven by the
/// founder culture's `MagicStyle` and `TechEra` per ARCHITECTURE.md §4
/// Phase 3b. Append only — discriminants are part of the on-disk schema.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum PantheonPattern {
    /// Single deity, monotheistic (Christian / Muslim / Sikh).
    #[default]
    Mono = 0,
    /// Multiple deities, polytheistic (Greek / Hindu / Norse).
    Poly = 1,
    /// Two opposed principles (Zoroastrian / Manichaean).
    Dual = 2,
    /// Spirits in nature, no deity-roster (Shinto / animistic folk).
    Animism = 3,
    /// Veneration of forebears (Chinese ancestral / Confucian).
    Ancestor = 4,
    /// Abstract universal principle (Daoist / Buddhist).
    CosmicOrder = 5,
}

/// A religion: founded by one culture, spreads to others by alignment
/// compatibility, anchored to specific cells via sacred sites. Created
/// during Phase 3b's religions stage; persists for the lifetime of the
/// world. Indexed by position in `WorldData.religions.religions`.
///
/// Per ARCHITECTURE.md §4 Phase 3b: "1-3 religions per world; each tied
/// to a culture; spread by alignment compatibility. Pantheon pattern
/// picked by founder culture's tech tier and magic style."
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Religion {
    pub name: String,
    pub pantheon: PantheonPattern,
    /// Index into `WorldData.cultures.cultures` — the culture that
    /// founded this religion.
    pub founder_culture_id: u16,
    /// Alignment inherited from the founder culture. Spread tolerance
    /// to neighboring cultures keys off similarity to this value.
    pub alignment: Alignment,
    /// Mesh cell indices where this religion has its physical anchors
    /// (temples, holy mountains, sacred groves). 0..3 sites per religion
    /// in the MVP. Empty until the religions stage runs.
    pub sacred_sites: Vec<u32>,
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

/// High-level race kind. Closed enum — sub-archetypes ("Human (Norse)" vs
/// "Human (River-valley)", "Elf (High)" vs "Elf (Wood)", etc.) live in the
/// race-archetype data table referenced by `Culture::archetype_id`, so seeds
/// can drop sub-archetypes (no-Norse world, giants-only world) without
/// recompiling. Discriminants are part of the on-disk schema — never
/// renumber existing variants; only append.
///
/// Source: `docs/ARCHITECTURE.md` §5.5 "What races want" (14-archetype table).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Race {
    #[default]
    Human = 0,
    Elf = 1,
    Dwarf = 2,
    Orc = 3,
    Halfling = 4,
    Lizardfolk = 5,
    SeaFolk = 6,
    Underdark = 7,
    Giant = 8,
}

/// 2D continuous alignment per the Glorantha / D&D convention.
/// Both axes range `[-1.0, 1.0]`: `law_chaos` from law (-1) to chaos (+1);
/// `good_evil` from good (-1) to evil (+1). Continuous (not discrete LE/NE/CE
/// buckets) so cultures can sit anywhere in the plane and history-sim
/// alignment shifts produce smooth drift, not categorical jumps.
#[derive(Copy, Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Alignment {
    pub law_chaos: f32,
    pub good_evil: f32,
}

/// Technological era — the floor under per-axis skill scores. A Medieval
/// culture might still have low `naval` if landlocked. Discriminants are
/// part of the on-disk schema; append only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum TechEra {
    #[default]
    Stone = 0,
    Bronze = 1,
    Iron = 2,
    Classical = 3,
    Medieval = 4,
    Renaissance = 5,
}

/// Per-culture tech bundle. `era` is the floor; the five skill axes are 0..100
/// scores that can deviate from era (a Renaissance culture with weak naval).
/// Axes per `docs/ARCHITECTURE.md` §Context — "metallurgy / agriculture /
/// naval / military / arcane."
#[derive(Copy, Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TechProfile {
    pub era: TechEra,
    pub metallurgy: u8,
    pub agriculture: u8,
    pub naval: u8,
    pub military: u8,
    pub arcane: u8,
}

/// How a culture treats magic. Drives renderer accents (glow on arcane
/// settlements) and history-sim event weights (Highmagic cultures spawn more
/// arcane events). Append only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum MagicStyle {
    /// No institutional magic — folklore at most.
    #[default]
    None = 0,
    /// Explicit arcane-casting tradition (academies, wizards).
    Highmagic = 1,
    /// Ritual / superstition / hedge-witchery; minor effects.
    LowMagic = 2,
    /// Nature / shamanic / druidic.
    Wild = 3,
    /// God-channeled (clerics).
    Divine = 4,
    /// Ancestor-channeled (necromantic or veneration-based).
    Ancestral = 5,
}

/// Settlement icon — the visual primitive the ornate renderer uses for
/// settlements of this culture. Combined with `Architecture` to derive the
/// final glyph (per ARCHITECTURE.md §Context: "Settlement glyphs derive from
/// `Culture.settlement × Culture.architecture`"). Append only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum SettlementIcon {
    #[default]
    Castle = 0,
    Tower = 1,
    Hall = 2,
    Spire = 3,
    Longhouse = 4,
    Treehouse = 5,
    Gate = 6,
    Yurt = 7,
}

/// Architectural vocabulary — the stylistic axis paired with
/// `SettlementIcon`. Per ARCHITECTURE.md §Context: "gothic vs classical vs
/// organic vs megalithic." Append only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Architecture {
    #[default]
    Classical = 0,
    Gothic = 1,
    Organic = 2,
    Megalithic = 3,
}

/// Default diplomatic posture — input to the Mearsheimer history loop.
/// Drives initial alliance/rivalry priors; per-actor history evolves it.
/// Append only.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum DiplomaticPattern {
    #[default]
    Isolationist = 0,
    Mercantile = 1,
    Expansionist = 2,
    Tributary = 3,
    HonorBound = 4,
    Egalitarian = 5,
}

/// A culture: the unit that occupies cells, founds religions, drives polity
/// formation, and feeds the renderer's settlement glyphs. Created during the
/// cultures stage from the race-archetype roster + per-world variation;
/// persists for the lifetime of the world. Indexed by position in
/// `WorldData.cultures.cultures` (see `CulturesData::culture_id` for the
/// per-cell back-reference).
///
/// Fields per ARCHITECTURE.md §Context: "race (with sub-variant), language,
/// religion, alignment, tech profile, magic style, settlement icon,
/// architecture vocabulary, diplomatic pattern, habitat preference function."
/// The habitat-preference function is implicit (derived from race +
/// archetype_id via the data table) rather than stored.
///
/// `language_id` and `religion_id` are forward-declared; they're written when
/// Phase 3b (religions) and Phase 3d (naming/language) land. Until then they
/// stay at their default (0 / None).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Culture {
    pub name: String,
    pub race: Race,
    /// Index into `crates/mapgen-world/data/race_archetypes.csv` — the
    /// sub-variant within the high-level `race` (Mediterranean vs Norse vs
    /// River-valley for Race::Human, etc.).
    pub archetype_id: u16,
    /// Forward declaration for Phase 3d. 0 until naming/language stage lands.
    pub language_id: u16,
    /// Forward declaration for Phase 3b. `None` until religions stage lands.
    pub religion_id: Option<u16>,
    pub alignment: Alignment,
    pub tech: TechProfile,
    pub magic: MagicStyle,
    pub settlement: SettlementIcon,
    pub architecture: Architecture,
    pub diplomatic: DiplomaticPattern,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Culture` whose every field is *non-default* — that way a
    /// derive bug that silently drops or misnames a field shows up as a
    /// round-trip mismatch instead of being masked by `T::default()`
    /// values on both sides.
    fn fully_populated_culture() -> Culture {
        Culture {
            name: "Riverfolk".to_string(),
            race: Race::Halfling,
            archetype_id: 3,
            language_id: 7,
            religion_id: Some(2),
            alignment: Alignment {
                law_chaos: 0.4,
                good_evil: -0.2,
            },
            tech: TechProfile {
                era: TechEra::Iron,
                metallurgy: 45,
                agriculture: 80,
                naval: 30,
                military: 35,
                arcane: 10,
            },
            magic: MagicStyle::LowMagic,
            settlement: SettlementIcon::Hall,
            architecture: Architecture::Organic,
            diplomatic: DiplomaticPattern::Mercantile,
        }
    }

    #[test]
    fn culture_round_trips_through_serde_json() {
        let original = fully_populated_culture();
        let json = serde_json::to_string(&original).expect("serialize Culture");
        let decoded: Culture = serde_json::from_str(&json).expect("deserialize Culture");
        assert_eq!(
            original, decoded,
            "Culture lost data through JSON round-trip; JSON shape was: {json}"
        );
    }

    #[test]
    fn culture_inside_entity_round_trips() {
        // Validate the `Entity::Culture(Culture)` wrapper variant still
        // works after the Culture expansion — serde's enum-tag handling
        // for `#[serde(tag = "kind")]` must agree with the new struct.
        let original = Entity::Culture(fully_populated_culture());
        let json = serde_json::to_string(&original).expect("serialize Entity::Culture");
        let decoded: Entity = serde_json::from_str(&json).expect("deserialize Entity::Culture");
        match (&original, &decoded) {
            (Entity::Culture(a), Entity::Culture(b)) => assert_eq!(a, b),
            _ => panic!("Entity variant changed across round-trip: {decoded:?}"),
        }
    }
}

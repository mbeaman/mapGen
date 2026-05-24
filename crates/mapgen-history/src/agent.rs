//! Agent layer — the named cast the causal loops act on.
//!
//! Each simulated year (before the six causal loops run), this advances every
//! polity's ruling court: a founder establishes a dynasty + house + throne, then
//! rulers marry, bear heirs, die, and are succeeded by their eldest living
//! child — or, when a line fails, by a new dynasty. It emits Birth / Marriage /
//! Coronation / Death events and records lineage via `Relationship`s and reigns
//! via `TitleHolding`s. Names come from the founding culture's `Language` (the
//! same phonotactic generator the naming stage uses).
//!
//! Phase 4c models *orderly* succession (eldest living child). Contested
//! successions and succession wars layer on top in 4f, reading this lineage.

use mapgen_core::ids::CellId;
use mapgen_core::{
    generate_name, Character, Dynasty, Entity, EntityId, Event, EventId, EventKind, House,
    RelationKind, Relationship, Sex, Title, TitleHolding, WorldData,
};
use rand_chacha::{rand_core::RngCore, ChaCha8Rng};
use smallvec::SmallVec;

use crate::SimState;

/// Age a founder is assumed to have reached at accession (so their `born_year`
/// predates the sim; founders get no Birth event).
const FOUNDER_AGE: i32 = 30;
/// Reign length is drawn uniformly from `[REIGN_MIN, REIGN_MAX]` years.
const REIGN_MIN: i32 = 16;
const REIGN_MAX: i32 = 44;
/// Years into a reign before the ruler marries.
const MARRY_AFTER: i32 = 3;
/// Spacing between heirs (a child is born on marriage-year + 1, +5, +9, …).
const CHILD_INTERVAL: i32 = 4;
/// Upper bound on planned heirs per reign.
const MAX_CHILDREN: u32 = 4;

/// Per-polity ruling court — scratch state, never serialized.
#[derive(Clone, Debug, Default)]
pub struct Court {
    pub ruler: Option<EntityId>,
    pub dynasty: Option<EntityId>,
    pub house: Option<EntityId>,
    pub title: Option<EntityId>,
    pub accession_year: i32,
    pub reign_len: i32,
    pub married: bool,
    pub consort: Option<EntityId>,
    pub marriage_year: i32,
    pub n_children: u8,
    /// The current ruler's children, in birth order (heir candidates).
    pub children: Vec<EntityId>,
}

/// Advance every court by one year.
pub fn advance(world: &mut WorldData, state: &mut SimState, year: i32, rng: &mut ChaCha8Rng) {
    for pid in 0..state.courts.len() {
        if state.courts[pid].ruler.is_none() {
            found_dynasty(world, state, pid, year, rng);
            continue;
        }
        let reign_end = state.courts[pid].accession_year + state.courts[pid].reign_len;
        if year >= reign_end {
            succeed(world, state, pid, year, rng);
        } else if !state.courts[pid].married
            && year >= state.courts[pid].accession_year + MARRY_AFTER
        {
            marry(world, state, pid, year, rng);
        } else if state.courts[pid].married {
            maybe_birth(world, state, pid, year, rng);
        }
    }
}

fn found_dynasty(
    world: &mut WorldData,
    state: &mut SimState,
    pid: usize,
    year: i32,
    rng: &mut ChaCha8Rng,
) {
    let cell = world.society.nations[pid].capital_cell;
    let culture_id = culture_of(world, pid);
    let dyn_name = polity_name(world, pid, rng);
    let house_name = polity_name(world, pid, rng);
    let ruler_name = polity_name(world, pid, rng);
    let nation = world.society.nations[pid].name.clone();
    let sex = rand_sex(rng);
    let reign_len = REIGN_MIN + (rng.next_u32() as i32).rem_euclid(REIGN_MAX - REIGN_MIN + 1);

    let dyn_id = world.entities.insert(Entity::Dynasty(Dynasty {
        name: dyn_name.clone(),
        founder: None,
        founded_year: year,
    }));
    let house_id = world.entities.insert(Entity::House(House {
        name: house_name,
        dynasty: Some(dyn_id),
        founder: None,
        founded_year: year,
    }));
    let ruler_id = world.entities.insert(Entity::Character(Character {
        name: ruler_name.clone(),
        born_year: year - FOUNDER_AGE,
        died_year: None,
        house: Some(house_id),
        culture_id,
        sex,
        relationships: Vec::new(),
        titles: Vec::new(),
        birth_event: None,
        death_event: None,
    }));
    let title_id = world.entities.insert(Entity::Title(Title {
        name: format!("Throne of {nation}"),
        polity: pid as u16,
        created_event: None,
    }));

    if let Some(Entity::Dynasty(d)) = world.entities.by_id.get_mut(&dyn_id) {
        d.founder = Some(ruler_id);
    }
    if let Some(Entity::House(h)) = world.entities.by_id.get_mut(&house_id) {
        h.founder = Some(ruler_id);
    }

    let cor = emit(
        world,
        year,
        EventKind::Coronation,
        cell,
        0.55,
        &[ruler_id],
        format!("{ruler_name} was crowned, founding the {dyn_name} dynasty of {nation}."),
    );
    if let Some(Entity::Title(t)) = world.entities.by_id.get_mut(&title_id) {
        t.created_event = Some(cor);
    }
    if let Some(Entity::Character(c)) = world.entities.by_id.get_mut(&ruler_id) {
        c.titles.push(TitleHolding {
            title: title_id,
            start_year: year,
            end_year: None,
            start_event: Some(cor),
            end_event: None,
        });
    }

    state.courts[pid] = Court {
        ruler: Some(ruler_id),
        dynasty: Some(dyn_id),
        house: Some(house_id),
        title: Some(title_id),
        accession_year: year,
        reign_len,
        married: false,
        consort: None,
        marriage_year: 0,
        n_children: 0,
        children: Vec::new(),
    };
}

fn marry(world: &mut WorldData, state: &mut SimState, pid: usize, year: i32, rng: &mut ChaCha8Rng) {
    let Some(ruler_id) = state.courts[pid].ruler else {
        return;
    };
    let cell = world.society.nations[pid].capital_cell;
    let culture_id = culture_of(world, pid);
    let consort_name = polity_name(world, pid, rng);
    let sex = rand_sex(rng);
    let n_children = (1 + rng.next_u32() % MAX_CHILDREN) as u8;

    let consort_id = world.entities.insert(Entity::Character(Character {
        name: consort_name,
        born_year: year - 20,
        died_year: None,
        house: None,
        culture_id,
        sex,
        relationships: Vec::new(),
        titles: Vec::new(),
        birth_event: None,
        death_event: None,
    }));
    add_rel(world, ruler_id, consort_id, RelationKind::Spouse, year);
    add_rel(world, consort_id, ruler_id, RelationKind::Spouse, year);

    let rname = char_name(world, ruler_id);
    let cname = char_name(world, consort_id);
    emit(
        world,
        year,
        EventKind::Marriage,
        cell,
        0.35,
        &[ruler_id, consort_id],
        format!("{rname} wed {cname}."),
    );

    let court = &mut state.courts[pid];
    court.married = true;
    court.consort = Some(consort_id);
    court.marriage_year = year;
    court.n_children = n_children;
}

fn maybe_birth(
    world: &mut WorldData,
    state: &mut SimState,
    pid: usize,
    year: i32,
    rng: &mut ChaCha8Rng,
) {
    let (ruler, consort, marriage_year, n_children, n_existing) = {
        let c = &state.courts[pid];
        (
            c.ruler,
            c.consort,
            c.marriage_year,
            c.n_children as usize,
            c.children.len(),
        )
    };
    let Some(ruler_id) = ruler else { return };
    if n_existing >= n_children {
        return;
    }
    let since = year - marriage_year;
    if since <= 0 || since % CHILD_INTERVAL != 1 {
        return;
    }

    let cell = world.society.nations[pid].capital_cell;
    let culture_id = culture_of(world, pid);
    let house = match world.entities.by_id.get(&ruler_id) {
        Some(Entity::Character(c)) => c.house,
        _ => None,
    };
    let name = polity_name(world, pid, rng);
    let sex = rand_sex(rng);

    let child_id = world.entities.insert(Entity::Character(Character {
        name: name.clone(),
        born_year: year,
        died_year: None,
        house,
        culture_id,
        sex,
        relationships: Vec::new(),
        titles: Vec::new(),
        birth_event: None,
        death_event: None,
    }));
    let rname = char_name(world, ruler_id);
    let ev = emit(
        world,
        year,
        EventKind::Birth,
        cell,
        0.15,
        &[child_id],
        format!("{name} was born to {rname}."),
    );
    if let Some(Entity::Character(c)) = world.entities.by_id.get_mut(&child_id) {
        c.birth_event = Some(ev);
    }
    add_rel(world, ruler_id, child_id, RelationKind::Parent, year);
    add_rel(world, child_id, ruler_id, RelationKind::Child, year);
    if let Some(cons) = consort {
        add_rel(world, cons, child_id, RelationKind::Parent, year);
        add_rel(world, child_id, cons, RelationKind::Child, year);
    }
    state.courts[pid].children.push(child_id);
}

fn succeed(
    world: &mut WorldData,
    state: &mut SimState,
    pid: usize,
    year: i32,
    rng: &mut ChaCha8Rng,
) {
    let Some(ruler_id) = state.courts[pid].ruler else {
        return;
    };
    let cell = world.society.nations[pid].capital_cell;
    let reign = state.courts[pid].reign_len;
    let rname = char_name(world, ruler_id);
    let death_ev = emit(
        world,
        year,
        EventKind::Death,
        cell,
        0.45,
        &[ruler_id],
        format!("{rname} died after a reign of {reign} years."),
    );
    if let Some(Entity::Character(c)) = world.entities.by_id.get_mut(&ruler_id) {
        c.died_year = Some(year);
        c.death_event = Some(death_ev);
        if let Some(h) = c.titles.last_mut() {
            h.end_year = Some(year);
            h.end_event = Some(death_ev);
        }
    }

    let heir = state.courts[pid]
        .children
        .iter()
        .copied()
        .find(|&cid| is_alive(world, cid));
    match heir {
        Some(heir_id) => crown_heir(world, state, pid, heir_id, year, rng),
        None => found_dynasty(world, state, pid, year, rng),
    }
}

fn crown_heir(
    world: &mut WorldData,
    state: &mut SimState,
    pid: usize,
    heir_id: EntityId,
    year: i32,
    rng: &mut ChaCha8Rng,
) {
    let cell = world.society.nations[pid].capital_cell;
    let title_id = state.courts[pid].title;
    let reign_len = REIGN_MIN + (rng.next_u32() as i32).rem_euclid(REIGN_MAX - REIGN_MIN + 1);
    let hname = char_name(world, heir_id);
    let cor = emit(
        world,
        year,
        EventKind::Coronation,
        cell,
        0.5,
        &[heir_id],
        format!("{hname} ascended the throne."),
    );
    if let Some(tid) = title_id {
        if let Some(Entity::Character(c)) = world.entities.by_id.get_mut(&heir_id) {
            c.titles.push(TitleHolding {
                title: tid,
                start_year: year,
                end_year: None,
                start_event: Some(cor),
                end_event: None,
            });
        }
    }
    let court = &mut state.courts[pid];
    court.ruler = Some(heir_id);
    court.accession_year = year;
    court.reign_len = reign_len;
    court.married = false;
    court.consort = None;
    court.marriage_year = 0;
    court.n_children = 0;
    court.children = Vec::new();
}

// --- helpers ----------------------------------------------------------------

fn culture_of(world: &WorldData, pid: usize) -> Option<u16> {
    let cap = world.society.nations[pid].capital_cell as usize;
    world.cultures.culture_id.get(cap).copied().flatten()
}

/// A phonotactic name in the polity's founding-culture language (fallback to
/// the first language, or a placeholder if none exist).
fn polity_name(world: &WorldData, pid: usize, rng: &mut ChaCha8Rng) -> String {
    let cid = culture_of(world, pid).map(|c| c as usize).unwrap_or(0);
    match world.languages.get(cid).or_else(|| world.languages.first()) {
        Some(lang) => generate_name(lang, rng),
        None => "Anon".to_string(),
    }
}

fn char_name(world: &WorldData, id: EntityId) -> String {
    match world.entities.by_id.get(&id) {
        Some(Entity::Character(c)) => c.name.clone(),
        _ => "someone".to_string(),
    }
}

fn is_alive(world: &WorldData, id: EntityId) -> bool {
    matches!(world.entities.by_id.get(&id), Some(Entity::Character(c)) if c.died_year.is_none())
}

fn rand_sex(rng: &mut ChaCha8Rng) -> Sex {
    if rng.next_u32() & 1 == 0 {
        Sex::Female
    } else {
        Sex::Male
    }
}

fn add_rel(world: &mut WorldData, from: EntityId, to: EntityId, kind: RelationKind, year: i32) {
    if let Some(Entity::Character(c)) = world.entities.by_id.get_mut(&from) {
        c.relationships.push(Relationship {
            other: to,
            kind,
            since_year: year,
        });
    }
}

fn emit(
    world: &mut WorldData,
    year: i32,
    kind: EventKind,
    cell: u32,
    salience: f32,
    actors: &[EntityId],
    summary: String,
) -> EventId {
    let mut a: SmallVec<[EntityId; 4]> = SmallVec::new();
    a.extend(actors.iter().copied());
    world.events.push(Event {
        id: EventId(0),
        year,
        kind,
        actors: a,
        patients: SmallVec::new(),
        location: Some(CellId(cell)),
        cause_ids: SmallVec::new(),
        salience,
        casus_belli: None,
        summary_canonical: summary,
    })
}

//! Post-simulation weaving: turn the causal event graph into narrative arcs,
//! and partition the years into mythic ages. Pure functions of the finished
//! `WorldData` (events + `cause_ids` + entities) — deterministic, no RNG.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use mapgen_core::{
    AgeMotif, ArcKind, Entity, EntityId, EventId, EventKind, HistoryData, MythicAge, NarrativeArc,
    WorldData,
};

/// A member event must be at least this salient to anchor an arc.
const ARC_FLOOR: f32 = 0.45;
/// A causal cluster needs this many salient members to count as an arc.
const MIN_ARC_EVENTS: usize = 3;
/// Number of mythic ages the timeline is split into.
const N_AGES: i32 = 4;

pub fn build(world: &WorldData) -> HistoryData {
    HistoryData {
        ages: frame_ages(world),
        arcs: extract_arcs(world),
    }
}

// --- narrative arcs ---------------------------------------------------------

/// Union-find over event indices.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut cur = x;
        while self.parent[cur] != cur {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

fn extract_arcs(world: &WorldData) -> Vec<NarrativeArc> {
    let evs = &world.events.events;
    let n = evs.len();
    if n == 0 {
        return Vec::new();
    }
    // Event id == index (pinned by `every_event_has_valid_fields`). Connect each
    // event to its causes; connected components are candidate threads.
    let mut uf = UnionFind::new(n);
    for e in evs {
        for c in &e.cause_ids {
            let (ei, ci) = (e.id.0 as usize, c.0 as usize);
            if ci < n {
                uf.union(ei, ci);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let root = uf.find(i);
        groups.entry(root).or_default().push(i);
    }

    let mut arcs = Vec::new();
    for members_all in groups.into_values() {
        // Keep only salient members — the dramatic spine.
        let mut members: Vec<usize> = members_all
            .into_iter()
            .filter(|&i| evs[i].salience >= ARC_FLOOR)
            .collect();
        if members.len() < MIN_ARC_EVENTS {
            continue;
        }
        members.sort_by_key(|&i| (evs[i].year, i));

        let kind = classify(&members, evs);
        let climax = *members
            .iter()
            .max_by(|&&a, &&b| {
                evs[a]
                    .salience
                    .partial_cmp(&evs[b].salience)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("non-empty");

        // Key characters: the most-involved *people* across members. Filtered to
        // Characters — actors can also be beasts / religions / artifacts (e.g. a
        // megabeast on its `MegabeastRise`), which are not part of the human cast.
        let mut freq: BTreeMap<u32, usize> = BTreeMap::new();
        for &i in &members {
            for a in &evs[i].actors {
                if matches!(world.entities.by_id.get(a), Some(Entity::Character(_))) {
                    *freq.entry(a.0).or_default() += 1;
                }
            }
        }
        let mut ranked: Vec<(u32, usize)> = freq.into_iter().collect();
        ranked.sort_by_key(|&(id, count)| (Reverse(count), id));
        let key_characters: Vec<EntityId> =
            ranked.iter().take(5).map(|&(id, _)| EntityId(id)).collect();

        let title = title_for(kind, &key_characters, world);
        arcs.push(NarrativeArc {
            title,
            kind,
            start_event: EventId(members[0] as u32),
            climax_event: EventId(climax as u32),
            end_event: EventId(*members.last().unwrap() as u32),
            member_events: members.iter().map(|&i| EventId(i as u32)).collect(),
            key_characters,
        });
    }
    arcs
}

fn classify(members: &[usize], evs: &[mapgen_core::Event]) -> ArcKind {
    let has = |pred: &dyn Fn(&EventKind) -> bool| members.iter().any(|&i| pred(&evs[i].kind));
    if members.iter().any(|&i| {
        matches!(evs[i].kind, EventKind::WarDeclared)
            && matches!(
                evs[i].casus_belli,
                Some(mapgen_core::CasusBelli::ReligiousSchism)
            )
    }) {
        ArcKind::HolyWar
    } else if has(&|k| {
        matches!(
            k,
            EventKind::MegabeastSlain | EventKind::MegabeastRise | EventKind::ProphecyFulfilled
        )
    }) {
        ArcKind::HeroSaga
    } else if has(&|k| matches!(k, EventKind::Succession | EventKind::ClaimAsserted)) {
        ArcKind::DynasticConflict
    } else {
        ArcKind::Chronicle
    }
}

fn title_for(kind: ArcKind, key: &[EntityId], world: &WorldData) -> String {
    let lead = key.first().map(|&id| char_name(world, id));
    match (kind, lead) {
        (ArcKind::HeroSaga, Some(n)) => format!("The Saga of {n}"),
        (ArcKind::HeroSaga, None) => "A Saga of the Age".to_string(),
        (ArcKind::DynasticConflict, Some(n)) => format!("The Wars of {n}"),
        (ArcKind::DynasticConflict, None) => "A Disputed Succession".to_string(),
        (ArcKind::HolyWar, Some(n)) => format!("The War of Faith of {n}"),
        (ArcKind::HolyWar, None) => "A War of Faith".to_string(),
        (ArcKind::Chronicle, Some(n)) => format!("The Chronicle of {n}"),
        (ArcKind::Chronicle, None) => "A Chronicle of the Age".to_string(),
    }
}

fn char_name(world: &WorldData, id: EntityId) -> String {
    match world.entities.by_id.get(&id) {
        Some(Entity::Character(c)) => c.name.clone(),
        _ => "the unknown".to_string(),
    }
}

// --- mythic ages ------------------------------------------------------------

fn frame_ages(world: &WorldData) -> Vec<MythicAge> {
    let evs = &world.events.events;
    if evs.is_empty() {
        return Vec::new();
    }
    let span = evs.iter().map(|e| e.year).max().unwrap_or(0) + 1;
    if span <= 0 {
        return Vec::new();
    }
    let mut ages = Vec::new();
    for k in 0..N_AGES {
        let start = k * span / N_AGES;
        let end = (k + 1) * span / N_AGES;
        // Tally the window's character.
        let (mut heroic, mut dark) = (0i32, 0i32);
        for e in evs.iter().filter(|e| e.year >= start && e.year < end) {
            match e.kind {
                EventKind::MegabeastSlain | EventKind::Ascension | EventKind::ProphecyFulfilled => {
                    heroic += 1
                }
                EventKind::CityAbandoned
                | EventKind::Famine
                | EventKind::Plague
                | EventKind::Migration
                | EventKind::WarDeclared => dark += 1,
                _ => {}
            }
        }
        let motif = if k == 0 {
            AgeMotif::Founding
        } else if heroic * 3 >= dark && heroic > 0 {
            AgeMotif::Heroic
        } else if dark > 0 {
            AgeMotif::Dark
        } else {
            AgeMotif::Golden
        };
        ages.push(MythicAge {
            name: age_name(motif, k),
            start_year: start,
            end_year: end,
            motif,
        });
    }
    ages
}

fn age_name(motif: AgeMotif, index: i32) -> String {
    let ordinal = ["First", "Second", "Third", "Fourth", "Fifth"]
        .get(index as usize)
        .copied()
        .unwrap_or("Latter");
    let epithet = match motif {
        AgeMotif::Founding => "Founding",
        AgeMotif::Heroic => "Heroes",
        AgeMotif::Golden => "Plenty",
        AgeMotif::Dark => "Strife",
    };
    format!("The {ordinal} Age, an Age of {epithet}")
}

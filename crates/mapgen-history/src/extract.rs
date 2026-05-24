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
/// A thread that doesn't span at least this many years must instead be a rich
/// cluster (see `RICH_ARC_EVENTS`) to count — keeps single-year border
/// skirmishes from each masquerading as a saga.
const MIN_ARC_SPAN: i32 = 1;
/// A single-year cluster this large still counts as an arc (a war that toppled a
/// realm, or a full hero saga that played out in one year).
const RICH_ARC_EVENTS: usize = 4;
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
        // Tighten: a thread must span years or be a rich cluster — not a lone
        // one-year skirmish.
        let span_years = evs[*members.last().unwrap()].year - evs[members[0]].year;
        if span_years < MIN_ARC_SPAN && members.len() < RICH_ARC_EVENTS {
            continue;
        }

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
    disambiguate_titles(&mut arcs);
    arcs
}

/// Append regnal-style numerals to any title shared by more than one arc, in
/// chronological order ("The Wars of Vae I", "… II"), so a dynast who led
/// several conflicts doesn't yield indistinguishable threads.
fn disambiguate_titles(arcs: &mut [NarrativeArc]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for a in arcs.iter() {
        *counts.entry(a.title.clone()).or_default() += 1;
    }
    let dups: Vec<String> = counts
        .into_iter()
        .filter(|&(_, c)| c > 1)
        .map(|(t, _)| t)
        .collect();
    for title in dups {
        let mut idxs: Vec<usize> = (0..arcs.len())
            .filter(|&i| arcs[i].title == title)
            .collect();
        idxs.sort_by_key(|&i| arcs[i].start_event.0);
        for (n, &i) in idxs.iter().enumerate() {
            arcs[i].title = format!("{title} {}", roman(n + 1));
        }
    }
}

/// Minimal Roman numeral (1..=39 covers any realistic arc-title collision).
fn roman(mut n: usize) -> String {
    const TABLE: &[(usize, &str)] = &[(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut out = String::new();
    for &(v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

fn classify(members: &[usize], evs: &[mapgen_core::Event]) -> ArcKind {
    let has = |pred: &dyn Fn(&EventKind) -> bool| members.iter().any(|&i| pred(&evs[i].kind));
    // HolyWar only when an actual schism is in the thread — not merely a war
    // tagged `ReligiousSchism` (a plain inter-faith border war), which used to
    // brand most wars holy. The schism→war cause edge puts the schism in the arc.
    if has(&|k| matches!(k, EventKind::Schism)) {
        ArcKind::HolyWar
    } else if has(&|k| {
        matches!(
            k,
            EventKind::MegabeastSlain
                | EventKind::MegabeastRise
                | EventKind::ProphecyFulfilled
                | EventKind::Ascension
        )
    }) {
        ArcKind::HeroSaga
    } else if has(&|k| matches!(k, EventKind::Succession | EventKind::ClaimAsserted)) {
        ArcKind::DynasticConflict
    } else if has(&|k| {
        matches!(
            k,
            EventKind::WarDeclared | EventKind::BattleFought | EventKind::Siege
        )
    }) {
        ArcKind::Conquest
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
        (ArcKind::Conquest, Some(n)) => format!("The Conquests of {n}"),
        (ArcKind::Conquest, None) => "A War of Conquest".to_string(),
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

    // Pass 1: per-age tally of heroic deeds and *acute* catastrophes (realms
    // falling, plague, migration). War and famine are baseline — every age has
    // them — so they don't discriminate one age from another.
    let mut tally: Vec<(i32, i32, i32, i32)> = Vec::new(); // (start, end, heroic, dark)
    for k in 0..N_AGES {
        let start = k * span / N_AGES;
        let end = (k + 1) * span / N_AGES;
        let (mut heroic, mut dark) = (0i32, 0i32);
        for e in evs.iter().filter(|e| e.year >= start && e.year < end) {
            match e.kind {
                EventKind::MegabeastSlain | EventKind::Ascension | EventKind::ProphecyFulfilled => {
                    heroic += 1
                }
                EventKind::CityAbandoned | EventKind::Plague | EventKind::Migration => dark += 1,
                _ => {}
            }
        }
        tally.push((start, end, heroic, dark));
    }

    // Pass 2: classify each age *relative to the timeline's own average*, so the
    // motifs actually vary (absolute thresholds made every age the same). An age
    // above the mean in catastrophe reads Dark; above the mean in heroism reads
    // Heroic; quiet on both reads Golden. (`x*n > sum` ⇔ `x > mean`, no rounding.)
    let n = tally.len() as i32;
    let sum_h: i32 = tally.iter().map(|t| t.2).sum();
    let sum_d: i32 = tally.iter().map(|t| t.3).sum();
    let mut ages = Vec::new();
    for (k, &(start, end, heroic, dark)) in tally.iter().enumerate() {
        let motif = if k == 0 {
            AgeMotif::Founding
        } else if dark * n > sum_d && dark >= heroic {
            AgeMotif::Dark
        } else if heroic * n > sum_h {
            AgeMotif::Heroic
        } else {
            AgeMotif::Golden
        };
        ages.push(MythicAge {
            name: age_name(motif, k as i32),
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

#[cfg(test)]
mod tests {
    use super::*;
    use mapgen_core::Event;

    fn ev(id: u32, year: i32, kind: EventKind) -> Event {
        Event {
            id: EventId(id),
            year,
            kind,
            actors: Default::default(),
            patients: Default::default(),
            location: None,
            cause_ids: Default::default(),
            salience: 0.9,
            casus_belli: None,
            summary_canonical: String::new(),
        }
    }

    #[test]
    fn classify_picks_the_right_arc_kind() {
        use EventKind::*;
        let mk = |kinds: &[EventKind]| {
            let evs: Vec<Event> = kinds
                .iter()
                .enumerate()
                .map(|(i, k)| ev(i as u32, 0, k.clone()))
                .collect();
            let members: Vec<usize> = (0..evs.len()).collect();
            classify(&members, &evs)
        };
        // HolyWar requires an actual schism in the thread, not merely a war.
        assert_eq!(mk(&[WarDeclared, BattleFought, Schism]), ArcKind::HolyWar);
        assert_eq!(
            mk(&[MegabeastRise, Ascension, MegabeastSlain]),
            ArcKind::HeroSaga
        );
        assert_eq!(
            mk(&[ClaimAsserted, WarDeclared, BattleFought]),
            ArcKind::DynasticConflict
        );
        // A plain war (no schism, no claim) is a Conquest — not dumped in Chronicle.
        assert_eq!(mk(&[WarDeclared, BattleFought, Siege]), ArcKind::Conquest);
        assert_eq!(mk(&[Birth, Death, Coronation]), ArcKind::Chronicle);
    }

    fn world_with(evs: Vec<Event>) -> WorldData {
        let mut w = WorldData::default();
        w.events.events = evs;
        w
    }

    #[test]
    fn frame_ages_classifies_relative_to_the_timeline() {
        use EventKind::*;
        // span ⇒ 400, four 100-year ages.
        let mut evs = vec![ev(0, 10, Birth), ev(1, 399, Birth)];
        for y in [110, 120, 130, 140, 150] {
            let id = evs.len() as u32;
            evs.push(ev(id, y, CityAbandoned)); // age 1: above-average catastrophe
        }
        for y in [210, 220, 230, 240, 250] {
            let id = evs.len() as u32;
            evs.push(ev(id, y, MegabeastSlain)); // age 2: above-average heroism
        }
        // age 3 (300..400): quiet — only the year-399 Birth.
        let motifs: Vec<AgeMotif> = frame_ages(&world_with(evs))
            .iter()
            .map(|a| a.motif)
            .collect();
        assert_eq!(
            motifs,
            vec![
                AgeMotif::Founding,
                AgeMotif::Dark,
                AgeMotif::Heroic,
                AgeMotif::Golden,
            ],
        );
    }

    #[test]
    fn disambiguate_titles_numbers_collisions_chronologically() {
        let arc = |title: &str, start: u32| NarrativeArc {
            title: title.to_string(),
            kind: ArcKind::Conquest,
            member_events: vec![EventId(start)],
            start_event: EventId(start),
            climax_event: EventId(start),
            end_event: EventId(start),
            key_characters: vec![],
        };
        let mut arcs = vec![
            arc("The Wars of Vae", 30),
            arc("The Wars of Vae", 10),
            arc("Unique", 5),
        ];
        disambiguate_titles(&mut arcs);
        // Earliest collision gets I, later gets II; the unique title is untouched.
        assert_eq!(arcs[0].title, "The Wars of Vae II"); // started later (id 30)
        assert_eq!(arcs[1].title, "The Wars of Vae I"); // started earlier (id 10)
        assert_eq!(arcs[2].title, "Unique");
    }
}

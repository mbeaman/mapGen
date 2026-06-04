//! Polities stage. Phase 3c.
//!
//! Suitability-weighted capital placement (1 per culture in the MVP),
//! Christaller-style hierarchy of towns and villages around each
//! capital, A*-routed roads with reuse-discount producing a trunk-and-
//! branch network. Per ARCHITECTURE.md §4 Phase 3c: "Suitability-
//! weighted Poisson capitals filtered by `Culture.settlement_preference`;
//! Christaller k=4 hierarchy; A* roads with reuse discount."
//!
//! Pipeline position: runs after `religions::found` (settlements are
//! tagged with the religion of their cell). Writes:
//!
//! * `world.society.nations` — polity roster (one polity per culture).
//! * `world.society.settlements` — capitals + towns sorted by tier.
//! * `world.society.roads` — `Vec<Road>`, one road per
//!   non-capital-settlement-to-parent connection.
//! * `world.society.control` — per-cell polity id.
//!
//! Spec: `crates/mapgen-world/tests/polities_spec.rs`.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashSet};

use mapgen_core::entities::{Settlement, SettlementTier};
use mapgen_core::world_data::{Nation, Road};
use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

use crate::cultures::{self, RaceArchetype};

/// Tunables for the polities stage.
#[derive(Clone, Debug)]
pub struct PolitiesParams {
    /// Towns per capital (Christaller k=4 ≈ 3-4 dependent places per
    /// central place; MVP uses 3).
    pub towns_per_capital: usize,
    /// Minimum mesh-graph distance (in BFS hops) between capitals so
    /// they don't clump on a single high-fitness ridge.
    pub min_capital_separation: usize,
    /// Minimum mesh-graph distance between towns sharing a capital.
    pub min_town_separation: usize,
    /// Maximum BFS radius (in hops) from a capital within which towns
    /// can be placed.
    pub town_search_radius: usize,
}

impl Default for PolitiesParams {
    fn default() -> Self {
        Self {
            towns_per_capital: 3,
            min_capital_separation: 12,
            min_town_separation: 5,
            town_search_radius: 14,
        }
    }
}

/// Population value assigned per settlement tier. Hard-coded MVP curve;
/// capital = 1.0 (rank 1), town = 0.5 (rank 2), village = 1/3 if/when
/// the tier lands. Zipf-ish without computing per-cell carrying
/// capacity.
const POPULATION_CAPITAL: f32 = 1.0;
const POPULATION_TOWN: f32 = 0.5;

/// Minimum habitat-fitness for a cell to be considered as a capital
/// site. A culture whose best cell is below this floor isn't a viable
/// polity founder; the polity is dropped silently. 0.4 is "noticeably
/// above the cultures-stage cull threshold of 0.3."
const CAPITAL_FITNESS_FLOOR: f32 = 0.4;

/// Populate `world.society` — polity roster, settlements (capitals +
/// towns), per-cell control, and trunk-and-branch road network.
///
/// Preconditions: `world.cultures` and `world.religions` must be
/// populated (cultures provides per-cell habitat suitability via
/// `cultures::habitat_fitness`; religions provides per-cell religion
/// assignment which the renderer reads on settlement glyphs).
pub fn lay_out(world: &mut WorldData, params: PolitiesParams, _rng: &mut ChaCha8Rng) {
    let n_cells = world.mesh.cell_count();
    if n_cells == 0 || world.cultures.cultures.is_empty() {
        return;
    }
    let archetypes = cultures::load_archetypes();

    // 1. Place one capital per culture (where viable). Capitals are
    //    each culture's highest-fitness cell that's also far enough
    //    from every already-placed capital.
    let capitals = pick_capitals(world, &archetypes, &params);
    if capitals.is_empty() {
        return;
    }

    // 2. Build the polity roster + settlements + roads.
    let mut nations: Vec<Nation> = Vec::with_capacity(capitals.len());
    let mut settlements: Vec<Settlement> = Vec::new();
    let mut roads: Vec<Road> = Vec::new();
    let mut control: Vec<Option<u32>> = vec![None; n_cells];
    let mut road_cells: HashSet<u32> = HashSet::new();

    for (polity_id, &(culture_idx, capital_cell)) in capitals.iter().enumerate() {
        let culture = &world.cultures.cultures[culture_idx as usize];
        let archetype = &archetypes[culture.archetype_id as usize];

        nations.push(Nation {
            name: format!("{} Realm", culture.name),
            capital_cell: capital_cell as u32,
            color: polity_color(polity_id),
            // Populated by the history sim after its year loop (v21); 0 at gen time.
            prosperity: 0.0,
        });
        settlements.push(Settlement {
            name: format!("{} Capital", culture.name),
            cell: capital_cell as u32,
            tier: SettlementTier::Capital,
            polity_id: polity_id as u16,
            population: POPULATION_CAPITAL,
        });

        // 3. Place towns within the capital's reach.
        let towns = pick_towns(world, archetype, capital_cell, &params);
        for (i, &town_cell) in towns.iter().enumerate() {
            settlements.push(Settlement {
                name: format!("{} Town {}", culture.name, i + 1),
                cell: town_cell as u32,
                tier: SettlementTier::Town,
                polity_id: polity_id as u16,
                population: POPULATION_TOWN,
            });

            // 4. A* / Dijkstra road from town to capital, with reuse
            //    discount on existing road cells (trunk-and-branch).
            if let Some(path) =
                shortest_road_path(world, town_cell as u32, capital_cell as u32, &road_cells)
            {
                for &c in &path {
                    road_cells.insert(c);
                }
                roads.push(Road { cells: path });
            }
        }

        // 5. Per-cell control: every cell whose culture is this
        //    polity's founding culture gets stamped with this polity_id.
        //    A culture-to-polity mapping isn't strictly 1:1 in the
        //    architecture (one polity can span multiple cultures), but
        //    the MVP keeps it 1:1.
        for (cell, &slot) in world.cultures.culture_id.iter().enumerate() {
            if slot == Some(culture_idx) {
                control[cell] = Some(polity_id as u32);
            }
        }
    }

    world.society.nations = nations;
    world.society.settlements = settlements;
    world.society.roads = roads;
    world.society.control = control;
}

/// Provisional color per polity at gen-time (before borders move). A pure
/// function of id; the final legible colors are assigned post-history by
/// [`recolor_political`], so this only ever shows in the pre-history build-up
/// (the "Drawing borders" stage frame), where adjacency isn't settled yet.
fn polity_color(polity_id: usize) -> [u8; 3] {
    POLITICAL_PALETTE[polity_id % POLITICAL_PALETTE.len()]
}

/// Render palette for political control: enough distinct, muted parchment tones
/// that a greedy graph-coloring of the (low-degree) polity adjacency graph need
/// not repeat a color between bordering realms. Greedy needs at most
/// `max_degree + 1` colors; the observed max polity degree across planet seeds
/// is ~5, so 12 is comfortable headroom (the first 5 entries are the original
/// pre-recolor palette, kept byte-identical so the gen-time UI frame is stable).
const POLITICAL_PALETTE: &[[u8; 3]] = &[
    [180, 90, 70],   // brick
    [90, 130, 60],   // moss
    [70, 100, 150],  // dusty blue
    [170, 130, 60],  // bronze
    [130, 80, 150],  // plum
    [70, 140, 135],  // teal
    [150, 150, 75],  // olive
    [190, 115, 150], // rose
    [110, 95, 130],  // mauve
    [200, 140, 95],  // sand
    [95, 120, 95],   // sage
    [120, 160, 195], // sky
];

/// Recolor polities so no two that share a border carry the same color — making
/// the political wash, and overseas exclaves in particular, legible. Runs
/// post-history (so exclaves minted by the cross-water carrier are part of the
/// adjacency), overwriting the provisional gen-time colors.
///
/// Greedy graph-coloring over the polity adjacency graph. Two realms are joined
/// (must differ in color) if they border on the present map OR if any recorded
/// `border_change` passed a cell between them — so the present map is legible AND
/// every conquest visibly flips a cell's color as the time-slider scrubs across
/// its year (the slider re-renders past control with these same stored colors).
/// Deterministic: polities are colored in id order, each taking the lowest palette
/// index no already-colored neighbor uses. Integer-only, so native and wasm agree
/// byte-for-byte.
pub fn recolor_political(world: &mut WorldData) {
    let n_pol = world.society.nations.len();
    if n_pol == 0 {
        return;
    }
    let n_cells = world.mesh.cell_count();

    // Adjacency from final control: polities whose cells neighbor across a border.
    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_pol];
    for cell in 0..n_cells {
        let Some(a) = world.society.control.get(cell).copied().flatten() else {
            continue;
        };
        let a = a as usize;
        if a >= n_pol {
            continue;
        }
        let Some(nbrs) = world.mesh.neighbors.get(cell) else {
            continue;
        };
        for &nb in nbrs {
            if let Some(b) = world.society.control.get(nb as usize).copied().flatten() {
                let b = b as usize;
                if b < n_pol && b != a {
                    adj[a].insert(b);
                    adj[b].insert(a);
                }
            }
        }
    }

    // Also separate the two sides of every recorded territorial change. A cell
    // that passes from realm X to realm Y over history must visibly flip color
    // as the time-slider scrubs across that year — but X and Y need not still
    // border each other on the present map (X may have retreated), so the
    // present-adjacency pass above can leave them the same color, freezing the
    // slider. Force `color[X] != color[Y]` for every change.
    for ch in &world.history.border_changes {
        if let (Some(x), Some(y)) = (ch.from, ch.to) {
            let (x, y) = (x as usize, y as usize);
            if x < n_pol && y < n_pol && x != y {
                adj[x].insert(y);
                adj[y].insert(x);
            }
        }
    }

    let mut chosen = vec![usize::MAX; n_pol];
    for p in 0..n_pol {
        let mut used = vec![false; POLITICAL_PALETTE.len()];
        for &q in &adj[p] {
            if chosen[q] != usize::MAX {
                used[chosen[q]] = true;
            }
        }
        // Lowest free palette slot. If a realm somehow borders more realms than
        // the palette has colors (degree ≥ 12 — never observed; max is ~5), wrap
        // to a localized collision; the legibility tests would fail loudly if it
        // ever fired.
        let idx = used
            .iter()
            .position(|&free| !free)
            .unwrap_or(p % POLITICAL_PALETTE.len());
        chosen[p] = idx;
        world.society.nations[p].color = POLITICAL_PALETTE[idx];
    }
}

fn pick_capitals(
    world: &WorldData,
    archetypes: &[RaceArchetype],
    params: &PolitiesParams,
) -> Vec<(u16, usize)> {
    let n_cells = world.mesh.cell_count();
    let n_cultures = world.cultures.cultures.len();
    let mut placed: Vec<(u16, usize)> = Vec::new();
    let mut blocked = vec![false; n_cells];

    for culture_idx in 0..n_cultures {
        let culture = &world.cultures.cultures[culture_idx];
        let archetype = match archetypes.get(culture.archetype_id as usize) {
            Some(a) => a,
            None => continue,
        };
        let mut best_cell = 0usize;
        let mut best_fit = f32::NEG_INFINITY;
        // `cell` is used as an index into several parallel arrays —
        // `elevation`, `blocked`, `culture_id`. Iterator form would need
        // separate `.iter().enumerate()` on each, which is uglier than
        // the range loop. Allow the lint locally.
        #[allow(clippy::needless_range_loop)]
        for cell in 0..n_cells {
            if world.terrain.elevation[cell] < 0.0 || blocked[cell] {
                continue;
            }
            // Capital must be in this culture's territory; otherwise
            // the "owning culture" relationship is ambiguous.
            if world.cultures.culture_id[cell] != Some(culture_idx as u16) {
                continue;
            }
            let fit = cultures::habitat_fitness(world, cell, archetype);
            if fit > best_fit {
                best_fit = fit;
                best_cell = cell;
            }
        }
        if best_fit < CAPITAL_FITNESS_FLOOR {
            // No suitable cell — culture doesn't get a polity.
            continue;
        }
        placed.push((culture_idx as u16, best_cell));
        block_within_radius(
            world,
            best_cell,
            params.min_capital_separation,
            &mut blocked,
        );
    }
    placed
}

fn pick_towns(
    world: &WorldData,
    archetype: &RaceArchetype,
    capital_cell: usize,
    params: &PolitiesParams,
) -> Vec<usize> {
    let n_cells = world.mesh.cell_count();

    // Cells within `town_search_radius` of capital (BFS up to N hops).
    let mut candidates: Vec<usize> = Vec::new();
    let mut visited = vec![false; n_cells];
    visited[capital_cell] = true;
    let mut current = vec![capital_cell];
    for _ in 0..params.town_search_radius {
        let mut next = Vec::new();
        for &c in &current {
            for &nb in &world.mesh.neighbors[c] {
                let nb = nb as usize;
                if visited[nb] {
                    continue;
                }
                visited[nb] = true;
                if world.terrain.elevation[nb] >= 0.0 {
                    candidates.push(nb);
                }
                next.push(nb);
            }
        }
        if next.is_empty() {
            break;
        }
        current = next;
    }

    // Score candidates by archetype fitness; greedy place top-N with
    // separation constraint.
    let mut scored: Vec<(usize, f32)> = candidates
        .into_iter()
        .map(|c| (c, cultures::habitat_fitness(world, c, archetype)))
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    let mut chosen: Vec<usize> = Vec::new();
    let mut town_blocked = vec![false; n_cells];
    block_within_radius(
        world,
        capital_cell,
        params.min_town_separation,
        &mut town_blocked,
    );
    for (cell, _) in scored {
        if chosen.len() >= params.towns_per_capital {
            break;
        }
        if town_blocked[cell] {
            continue;
        }
        chosen.push(cell);
        block_within_radius(world, cell, params.min_town_separation, &mut town_blocked);
    }
    chosen
}

fn block_within_radius(world: &WorldData, start: usize, max_hops: usize, blocked: &mut [bool]) {
    let n_cells = world.mesh.cell_count();
    let mut visited = vec![false; n_cells];
    visited[start] = true;
    blocked[start] = true;
    let mut current = vec![start];
    for _ in 0..max_hops {
        let mut next = Vec::new();
        for &c in &current {
            for &nb in &world.mesh.neighbors[c] {
                let nb = nb as usize;
                if visited[nb] {
                    continue;
                }
                visited[nb] = true;
                blocked[nb] = true;
                next.push(nb);
            }
        }
        if next.is_empty() {
            break;
        }
        current = next;
    }
}

/// Dijkstra shortest path from `start` to `goal` through land cells,
/// with edge cost 1 for cells already in `road_cells` (reuse discount)
/// and 2 for fresh cells. The 1:2 ratio gives roughly the "twice as
/// cheap" reuse curve the architecture mentions for trunk-and-branch
/// networks. Ties broken by lower cell index for determinism.
fn shortest_road_path(
    world: &WorldData,
    start: u32,
    goal: u32,
    road_cells: &HashSet<u32>,
) -> Option<Vec<u32>> {
    let n_cells = world.mesh.cell_count();
    let mut dist = vec![u32::MAX; n_cells];
    let mut prev: Vec<u32> = vec![u32::MAX; n_cells];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();

    dist[start as usize] = 0;
    heap.push(Reverse((0u32, start)));

    while let Some(Reverse((d, cell))) = heap.pop() {
        if cell == goal {
            // Reconstruct.
            let mut path = vec![cell];
            let mut c = cell;
            while prev[c as usize] != u32::MAX {
                let p = prev[c as usize];
                path.push(p);
                c = p;
            }
            path.reverse();
            return Some(path);
        }
        if d > dist[cell as usize] {
            continue;
        }
        for &nb in &world.mesh.neighbors[cell as usize] {
            if world.terrain.elevation[nb as usize] < 0.0 {
                continue;
            }
            let edge_cost = if road_cells.contains(&nb) { 1u32 } else { 2u32 };
            let new_dist = d + edge_cost;
            if new_dist < dist[nb as usize] {
                dist[nb as usize] = new_dist;
                prev[nb as usize] = cell;
                heap.push(Reverse((new_dist, nb)));
            }
        }
    }
    None
}

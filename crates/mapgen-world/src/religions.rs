//! Religions stage. Phase 3b.
//!
//! Founds 1-3 religions per world. Each religion is tied to a founder
//! culture, takes that culture's alignment as its baseline, and picks a
//! `PantheonPattern` based on the founder's `MagicStyle` and `TechEra`.
//! Religions then spread to neighboring cultures whose alignment is
//! similar enough; each cell with a culture gets the religion whose
//! founder is closest in alignment. Sacred sites land on biome-matched
//! cells inside the founder's territory.
//!
//! Pipeline position: runs after `cultures::populate` (needs the culture
//! roster + per-cell assignment) and before any polities stage (which
//! the architecture treats as orthogonal to religion).
//!
//! Spec: `crates/mapgen-world/tests/religions_spec.rs`.
//!
//! Architecture: `docs/ARCHITECTURE.md` §4 Phase 3b.

use mapgen_core::entities::{Alignment, Culture, MagicStyle, PantheonPattern, Religion};
use mapgen_core::WorldData;
use rand_chacha::ChaCha8Rng;

use crate::naming::connected_bodies;

/// Tunables for the religions stage. Calibrated values land in
/// `docs/tuning_log.md` once the implementation greens up.
#[derive(Clone, Debug)]
pub struct ReligionsParams {
    /// Cap on how many religions can be founded in a single world.
    /// Architecture: "1-3 religions per world." The actual count is
    /// bounded above by this and below by the number of cultures.
    pub max_religions: usize,
    /// Maximum Euclidean distance in 2D alignment-space (law-chaos ×
    /// good-evil, each in `[-1, 1]`) at which a culture is considered
    /// "alignment-compatible" with a founder. Cultures beyond this
    /// distance from every existing religion's founder get the closest
    /// one anyway (no cell stays unconverted).
    pub alignment_spread_radius: f32,
}

impl Default for ReligionsParams {
    fn default() -> Self {
        Self {
            max_religions: 3,
            // Both axes are `[-1, 1]`; the diagonal of that square is
            // ~2.83. `1.5` lets a religion plausibly span half the
            // alignment plane but doesn't homogenize.
            alignment_spread_radius: 1.5,
        }
    }
}

/// Maximum number of sacred sites per religion. Picked from the
/// highest-elevation cells where the religion has adherents.
const SACRED_SITES_PER_RELIGION: usize = 3;

/// Populate `world.religions` — roster + per-cell adherence assignment +
/// sacred-site placement.
///
/// Preconditions: `world.cultures` must be populated (the religions
/// stage runs after `cultures::populate` in `generate_full`); biome
/// data on `world.climate.biome` must exist (for sacred-site placement).
///
/// `_rng` is reserved for future per-religion variation; today's
/// algorithm is fully deterministic from world state alone (cultures'
/// populations, alignments, and the per-cell mesh).
pub fn found(world: &mut WorldData, params: ReligionsParams, _rng: &mut ChaCha8Rng) {
    let n_cells = world.mesh.cell_count();
    let n_cultures = world.cultures.cultures.len();
    if n_cells == 0 || n_cultures == 0 {
        return;
    }

    // 1. Choose number of religions: capped by the user-supplied max and
    //    by the actual culture count (each religion needs a founder).
    let n_religions = params.max_religions.min(n_cultures).max(1);

    // 2. Pick founder cultures: top-N by adherent-cell population. Ties
    //    broken by lower culture_id for determinism. Largest cultures
    //    sit closer to where religions historically emerged (more bodies
    //    → more priests → more institutional infrastructure).
    let mut pops: Vec<(u16, usize)> = (0..n_cultures as u16)
        .map(|id| (id, count_cells_for_culture(world, id)))
        .collect();
    pops.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let founders: Vec<u16> = pops.iter().take(n_religions).map(|(id, _)| *id).collect();

    // 3. Build the religion roster. Sacred sites are filled in step 5.
    let mut religions: Vec<Religion> = founders
        .iter()
        .map(|&founder_id| {
            let founder = &world.cultures.cultures[founder_id as usize];
            let pantheon = pantheon_from_founder(founder);
            Religion {
                name: religion_name(founder, pantheon),
                pantheon,
                founder_culture_id: founder_id,
                alignment: founder.alignment,
                sacred_sites: Vec::new(),
            }
        })
        .collect();

    // 3b. Confine each religion to its FOUNDING landmass. A founder culture is
    //     landmass-instanced (v19), so a religion has one home body; cross-body
    //     reach is EARNED over history by the Diffusion loop, not granted free at
    //     gen-time (the residue Phase 2 fixes — faith no longer pre-crosses
    //     oceans). Single-landmass worlds (the seed42 golden) have one body, so
    //     every religion's home body IS every cell's body — a byte-identical
    //     no-op there.
    let bodies = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] > 0.0);
    let mut body_of = vec![usize::MAX; n_cells];
    for (bi, body) in bodies.iter().enumerate() {
        for &c in body {
            body_of[c] = bi;
        }
    }
    let home_body: Vec<usize> = religions
        .iter()
        .map(|r| {
            world
                .cultures
                .culture_id
                .iter()
                .position(|&id| id == Some(r.founder_culture_id))
                .map(|c| body_of[c])
                .unwrap_or(usize::MAX)
        })
        .collect();

    // 4. Spread by alignment compatibility, WITHIN the founding landmass. Each
    //    cell adopts the alignment-closest religion FOUNDED ON ITS OWN BODY (the
    //    `alignment_spread_radius` is a soft cap — within the body the closest
    //    religion wins anyway). A cell on a body with no native religion stays
    //    `None` (faithless) until the Diffusion loop carries a faith across a sea
    //    lane to it.
    let mut religion_id: Vec<Option<u16>> = vec![None; n_cells];
    for (cell, slot) in religion_id.iter_mut().enumerate() {
        let Some(culture_idx) = world.cultures.culture_id.get(cell).copied().flatten() else {
            continue;
        };
        let cell_body = body_of[cell];
        let culture = &world.cultures.cultures[culture_idx as usize];
        let mut best: (u16, f32) = (0, f32::MAX);
        let mut best_within_radius: Option<(u16, f32)> = None;
        for (r_idx, religion) in religions.iter().enumerate() {
            if home_body[r_idx] != cell_body {
                continue; // religion not founded on this landmass
            }
            let dist = alignment_distance(culture.alignment, religion.alignment);
            if dist < best.1 {
                best = (r_idx as u16, dist);
            }
            if dist <= params.alignment_spread_radius
                && best_within_radius.map(|(_, d)| dist < d).unwrap_or(true)
            {
                best_within_radius = Some((r_idx as u16, dist));
            }
        }
        if best.1 < f32::MAX {
            *slot = Some(best_within_radius.unwrap_or(best).0);
        }
    }

    // 5. Salvage: if any religion ended up with zero adherents (could
    //    happen if its founder culture's cells were all swallowed by a
    //    closer-aligned competitor), force-convert one of the founder
    //    culture's cells back. The architecture invariant "no religion
    //    has zero adherents" gets the floor it needs. Pick the
    //    lowest-index founder cell for determinism.
    for (r_idx, religion) in religions.iter().enumerate() {
        let has_adherent = religion_id.contains(&Some(r_idx as u16));
        if !has_adherent {
            if let Some(c) = world
                .cultures
                .culture_id
                .iter()
                .position(|&id| id == Some(religion.founder_culture_id))
            {
                religion_id[c] = Some(r_idx as u16);
            }
        }
    }

    // 6. Place sacred sites — for each religion, the top-N highest-
    //    elevation cells where the religion has adherents. High places
    //    are sacred across most pantheon patterns (mountain monasteries,
    //    holy peaks, ancestral burial mounds); MVP doesn't yet
    //    discriminate by pantheon's preferred biome. Once the ornate
    //    render exists and a magic-field LorePatch concept is in play,
    //    swap elevation for magic-field intensity.
    for (r_idx, religion) in religions.iter_mut().enumerate() {
        let mut adherents: Vec<(u32, f32)> = religion_id
            .iter()
            .enumerate()
            .filter_map(|(c, &id)| {
                if id == Some(r_idx as u16) {
                    let elev = world.terrain.elevation.get(c).copied().unwrap_or(0.0);
                    Some((c as u32, elev))
                } else {
                    None
                }
            })
            .collect();
        adherents.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let n_sites = SACRED_SITES_PER_RELIGION.min(adherents.len());
        religion.sacred_sites = adherents.iter().take(n_sites).map(|(c, _)| *c).collect();
    }

    world.religions.religions = religions;
    world.religions.religion_id = religion_id;
}

fn count_cells_for_culture(world: &WorldData, culture_id: u16) -> usize {
    world
        .cultures
        .culture_id
        .iter()
        .filter(|&&id| id == Some(culture_id))
        .count()
}

/// Map the founder culture's `MagicStyle` to a `PantheonPattern`. Per
/// ARCHITECTURE.md §4 Phase 3b: "Pantheon pattern picked by founder
/// culture's tech tier and magic style." MVP uses magic style alone;
/// tech tier modulation lands as a refinement when we have more
/// archetypes.
fn pantheon_from_founder(founder: &Culture) -> PantheonPattern {
    match founder.magic {
        MagicStyle::Highmagic => PantheonPattern::Poly,
        MagicStyle::LowMagic => PantheonPattern::Mono,
        MagicStyle::Wild => PantheonPattern::Animism,
        MagicStyle::Divine => PantheonPattern::Mono,
        MagicStyle::Ancestral => PantheonPattern::Ancestor,
        MagicStyle::None => PantheonPattern::CosmicOrder,
    }
}

/// Generate a religion name from its founder culture's name and the
/// pantheon pattern. Naming will get richer in Phase 3d (phonotactic
/// generator); for MVP a templated `"<Culture> <Suffix>"` is enough to
/// distinguish religions in test output and renders.
fn religion_name(founder: &Culture, pantheon: PantheonPattern) -> String {
    let suffix = match pantheon {
        PantheonPattern::Mono => "Faith",
        PantheonPattern::Poly => "Pantheon",
        PantheonPattern::Dual => "Twin Path",
        PantheonPattern::Animism => "Spirits",
        PantheonPattern::Ancestor => "Ancestors",
        PantheonPattern::CosmicOrder => "Way",
    };
    format!("{} {}", founder.name, suffix)
}

fn alignment_distance(a: Alignment, b: Alignment) -> f32 {
    let dlc = a.law_chaos - b.law_chaos;
    let dge = a.good_evil - b.good_evil;
    (dlc * dlc + dge * dge).sqrt()
}

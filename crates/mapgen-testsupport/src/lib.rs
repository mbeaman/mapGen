//! Shared test fixtures for the mapgen workspace: the canonical seeds and the
//! parameter builders for the claim-tests and the claims registry
//! (`docs/CLAIMS.md`). New claim-tests reference these by name; the existing
//! suite still carries inline `fixed_params` / `params` copies that should
//! migrate here over time.
//!
//! Why a crate and not yet-another `fn fixed_params` copied into every
//! `tests/*.rs`: a claim like "seed 19 grows a both-tier sea-lane substrate"
//! should reference *intent* (`CROSSING_SEEDS`), and the world size a claim is
//! asserted at must be the *same* size the goldens are anchored on — one
//! definition, so a claim test and a golden test can never silently diverge on
//! `cell_count`.
//!
//! This is a dev-dependency only; nothing ships it.

use mapgen_core::WorldData;
use mapgen_world::naming::connected_bodies;
use mapgen_world::GenerateParams;
use std::collections::BTreeSet;

/// The golden-anchor seed. At the continental reference size it is a *single
/// dominant landmass*, which is why the per-landmass society rework (schema v19)
/// is a byte-identical no-op here — every golden (`seed42_phase2/full/sector`
/// and the native↔wasm `cross_platform.rs` proof) is anchored on this seed.
///
/// Note: seed 42 appears in both [`REFERENCE_SEED`] (continental, via
/// [`reference_params`]) and [`SUNDERED_SEEDS`] (planet-scale, via
/// [`planet_params`]). Same seed, different scale → different world; that is
/// intentional, not a clash.
pub const REFERENCE_SEED: u64 = 42;

/// Planet seeds on which the beachhead carrier produces *earned* overseas
/// holdings — a polity ends up controlling cells on ≥2 sizable landmasses,
/// reached by a war crossing a sea lane its naval tech can sail. Earned-crossing
/// counts observed when the carrier shipped: 11→4, 19→3, 7→2, 4→1.
///
/// Asserted by the data-layer claim tests. If a seed here stops producing an
/// earned crossing, that test goes red — which is the point.
pub const CROSSING_SEEDS: &[u64] = &[11, 19, 7, 4];

/// Planet seeds the Step-0 probe proved are *sundered*: no inter-continental
/// lane is crossable at the world's naval tech, so no earned crossing can form.
/// The deliberate complement of [`CROSSING_SEEDS`] — together they pin the lane
/// gate in *both* directions. (A carrier that only ever fires is
/// indistinguishable from one that always fires; the sundered set is what makes
/// "earned" mean something.)
pub const SUNDERED_SEEDS: &[u64] = &[23, 42];

/// Planet seeds on which the colonization carrier settles an UNCLAIMED far-shore
/// anchor across a sea lane — a `from:None` overseas `BorderChange` (vs the
/// beachhead's `from:Some` conquest). These are the seeds with a lane that pairs
/// an owner-who-can-sail-it with a *persistently* unclaimed far anchor (gen-time
/// floor-drops never filled), so over the sim colonization reliably claims it.
///
/// The absent direction — crossing seeds 4/7/19 and the [`SUNDERED_SEEDS`] —
/// colonizes nothing, but NOT (only) because their far anchors are owned: a seed
/// can have an unclaimed far anchor that simply sits behind a naval *wall* its
/// owner can't sail (seed 4's cell 7719 is exactly this). What they lack is a
/// lane that pairs an unclaimed far anchor with an owner whose naval clears the
/// gate. So this set also guards the `min_naval` gate, not just "all owned".
pub const COLONIZE_SEEDS: &[u64] = &[2, 5, 9, 11, 18];

/// A connected land body must hold at least this many cells to count as a
/// "sizable landmass"; smaller bodies are islets. Matches the threshold used
/// inline across `cultures_spec` / `history_spec`.
pub const SIZABLE_BODY_MIN: usize = 24;

/// The canonical continental reference world: 1024×640, 4000 cells, 12 plates,
/// 6 nations. These are exactly the params the seed-42 goldens are anchored on
/// (`pipeline_spec`'s `fixed_params` / `cultures_spec`'s `params`). New claim
/// tests reuse this so they exercise the same world the goldens pin.
pub fn reference_params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    }
}

/// A planet-scale world (2:1, several continents in an encircling sea) — the
/// scale at which inter-continental claims (sea lanes, carriers, sundering) are
/// meaningful. Thin wrapper over [`GenerateParams::planet`] so claim tests name
/// the intent rather than the constructor.
pub fn planet_params(seed: u64) -> GenerateParams {
    GenerateParams::planet(seed)
}

/// The connected land bodies (`elevation > 0.0`) of at least [`SIZABLE_BODY_MIN`]
/// cells — the "real continents", with islets dropped. Deterministic order
/// (`connected_bodies` walks cells in id order). This is the body primitive the
/// data-layer claim tests partition the world by; it uses the one canonical land
/// predicate `> 0.0` (sea is `<= 0.0`) shared across cultures / naming /
/// hydrology / sea_lanes — see `docs/CLAIMS.md`.
pub fn sizable_landmasses(world: &WorldData) -> Vec<Vec<usize>> {
    connected_bodies(&world.mesh, |i| world.terrain.elevation[i] > 0.0)
        .into_iter()
        .filter(|b| b.len() >= SIZABLE_BODY_MIN)
        .collect()
}

/// Polity ids that control cells on **≥2 sizable landmasses**. At gen-time this
/// is empty — society is landmass-confined (schema v19). Post-history a
/// non-empty result is an *earned* overseas holding: the only path to it is the
/// cross-water carrier (land wars never cross water), so this is the signal that
/// the Sundered Lanes payoff actually fired. Used by the data-layer claim tests
/// in *both* directions: non-empty on a crossing seed, empty on a sundered one.
pub fn polities_spanning_multiple_landmasses(world: &WorldData) -> Vec<u32> {
    let bodies = sizable_landmasses(world);
    let mut body_of = vec![usize::MAX; world.mesh.cell_count()];
    for (bi, body) in bodies.iter().enumerate() {
        for &c in body {
            body_of[c] = bi;
        }
    }
    let n_pol = world.society.nations.len();
    let mut spans: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_pol];
    for (cell, &bi) in body_of.iter().enumerate() {
        if bi == usize::MAX {
            continue;
        }
        if let Some(p) = world.society.control[cell] {
            spans[p as usize].insert(bi);
        }
    }
    (0..n_pol as u32)
        .filter(|&p| spans[p as usize].len() >= 2)
        .collect()
}

/// For a world where the carrier fired, find one *earned overseas seizure*:
/// `(polity, cell, year)` where `polity` now controls `cell`, `cell` sits on a
/// landmass where `polity` had **no** presence at gen-time (so it was reached
/// across water), and `year` is when a recorded `BorderChange` set `cell` to
/// `polity`. Returns `None` on a sundered world (no spanning polity).
///
/// "Gen-time" is reconstructed with the slider's own machinery:
/// [`WorldData::control_at_year`]`(first_change_year - 1)` undoes every recorded
/// border change, yielding the pre-history control. So this is a replay-layer
/// probe — the slider replaying `border_changes` must show `cell` flip into
/// `polity` at exactly `year`.
pub fn an_earned_overseas_seizure(world: &WorldData) -> Option<(u32, u32, i32)> {
    let (first_year, _) = world.border_change_year_span()?;
    let baseline = world.control_at_year(first_year - 1);

    let bodies = sizable_landmasses(world);
    let mut body_of = vec![usize::MAX; world.mesh.cell_count()];
    for (bi, body) in bodies.iter().enumerate() {
        for &c in body {
            body_of[c] = bi;
        }
    }

    for p in polities_spanning_multiple_landmasses(world) {
        // The polity's gen-time landmass(es): the bodies holding its baseline
        // cells. Gen-time society is landmass-confined, so normally one body.
        let home: BTreeSet<usize> = (0..world.mesh.cell_count())
            .filter(|&c| body_of[c] != usize::MAX && baseline[c] == Some(p))
            .map(|c| body_of[c])
            .collect();
        // An earned overseas cell that was CONQUERED — `p` controls it now, it is
        // on a body that was not `p`'s at gen-time, AND a recorded change SEIZED
        // it from a prior owner (`from: Some`). The `from: Some` filter keeps this
        // a *conquest* probe: it can never return a cell the colonization carrier
        // settled (`from: None`), so the conquest replay test stays conquest-
        // specific even though both carriers now produce overseas holdings. Iterate
        // all of `p`'s overseas cells (lowest id first) so a colonized lower-id
        // cell can't hide a conquered one.
        for (cell, &bi) in body_of.iter().enumerate() {
            if bi == usize::MAX
                || home.contains(&bi)
                || world.society.control.get(cell).copied().flatten() != Some(p)
            {
                continue;
            }
            let year = world
                .history
                .border_changes
                .iter()
                .filter(|ch| ch.cell as usize == cell && ch.to == Some(p) && ch.from.is_some())
                .map(|ch| ch.year)
                .max();
            if let Some(year) = year {
                return Some((p, cell as u32, year));
            }
        }
    }
    None
}

/// Colonies settled across the sea: `(polity, cell, year)` for each recorded
/// `BorderChange { from: None, to: Some(p) }` whose cell lies on a sizable body.
/// This is the signal ONLY the colonization carrier produces — the beachhead and
/// border-transfers always record `from: Some`. Colonization targets a sea
/// lane's far anchor, which sits on a *different* body from the launch coast, so
/// these are overseas colonies by construction.
///
/// (No capital-body filter: keying "overseas" off the polity's capital silently
/// dropped every colony of a polity whose capital sits on a sub-sizable islet —
/// a false-negative in a both-directions guard. The `from: None` + sizable-body
/// pair is the precise signal.)
pub fn overseas_colonizations(world: &WorldData) -> Vec<(u32, u32, i32)> {
    let bodies = sizable_landmasses(world);
    let mut body_of = vec![usize::MAX; world.mesh.cell_count()];
    for (bi, body) in bodies.iter().enumerate() {
        for &c in body {
            body_of[c] = bi;
        }
    }
    let mut out = Vec::new();
    for ch in &world.history.border_changes {
        if ch.from.is_some() {
            continue; // colonization is from:None; the beachhead is from:Some
        }
        let Some(p) = ch.to else { continue };
        let on_sizable = body_of
            .get(ch.cell as usize)
            .is_some_and(|&b| b != usize::MAX);
        if on_sizable {
            out.push((p, ch.cell, ch.year));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_params_match_the_golden_anchor_size() {
        // If this ever drifts from `pipeline_spec`'s `fixed_params`, a claim
        // test built on `reference_params` would exercise a different world
        // than the golden pins. Both are independently anchored to these exact
        // values (the golden by its hash, this by this assertion), so they
        // cannot silently diverge.
        let p = reference_params(REFERENCE_SEED);
        assert_eq!(p.seed, 42);
        assert_eq!(p.width, 1024.0);
        assert_eq!(p.height, 640.0);
        assert_eq!(p.cell_count, 4_000);
        assert_eq!(p.plate_count, 12);
        assert_eq!(p.nation_count, 6);
    }

    #[test]
    fn planet_params_differ_from_continental() {
        let planet = planet_params(7);
        let cont = reference_params(7);
        // The planet must be wider (2:1) and carry more cells/plates than the
        // continental reference, or "planet-scale" claims would be testing the
        // wrong scale.
        assert!(planet.cell_count > cont.cell_count);
        assert!(planet.plate_count > cont.plate_count);
        assert!((planet.width / planet.height - 2.0).abs() < 0.01);
    }

    #[test]
    fn crossing_and_sundered_seed_sets_are_disjoint_and_non_empty() {
        assert!(
            !CROSSING_SEEDS.is_empty(),
            "need at least one crossing seed"
        );
        assert!(
            !SUNDERED_SEEDS.is_empty(),
            "need at least one sundered seed"
        );
        for s in CROSSING_SEEDS {
            assert!(
                !SUNDERED_SEEDS.contains(s),
                "seed {s} cannot be both a crossing and a sundered seed"
            );
        }
    }
}

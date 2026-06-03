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

/// The connected land bodies (`elevation >= 0.0`) of at least [`SIZABLE_BODY_MIN`]
/// cells — the "real continents", with islets dropped. Deterministic order
/// (`connected_bodies` walks cells in id order). This is the body primitive the
/// data-layer claim tests partition the world by; consolidating it here keeps
/// the `>= 0.0` predicate in exactly one place (see the `no_culture_instance_…`
/// tripwire in `docs/CLAIMS.md` for why the predicate matters).
pub fn sizable_landmasses(world: &WorldData) -> Vec<Vec<usize>> {
    connected_bodies(&world.mesh, |i| world.terrain.elevation[i] >= 0.0)
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

//! Fixture sanity, not a feature claim: proves the shared `mapgen-testsupport`
//! fixtures import across the dev-dependency cycle (mapgen-world's tests →
//! mapgen-testsupport → mapgen-world) and are usable here. The data-layer claim
//! tests build on these. See `docs/CLAIMS.md`.

use mapgen_testsupport::{
    planet_params, reference_params, CROSSING_SEEDS, REFERENCE_SEED, SUNDERED_SEEDS,
};

#[test]
fn shared_fixtures_are_importable_from_world_tests() {
    // The only way this fails is the dev-dependency cycle failing to resolve or
    // the fixture surface changing out from under its consumers — which is
    // exactly what it guards.
    assert_eq!(reference_params(REFERENCE_SEED).cell_count, 4_000);
    assert!(planet_params(CROSSING_SEEDS[0]).cell_count > 4_000);
    assert!(!CROSSING_SEEDS.is_empty() && !SUNDERED_SEEDS.is_empty());
}

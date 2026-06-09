//! 6.2 — Cross-platform determinism golden.
//!
//! Runs the full generate pipeline compiled to **wasm32** (executed in Node by
//! `wasm-pack test --node`) and asserts the seed-42 output hashes byte-identical
//! to the **native** golden pinned by `mapgen-world`'s `full_pipeline_golden_hash`.
//!
//! This is the load-bearing guarantee for the multi-scale atlas: it regenerates
//! sectors *in the browser*, so a sector viewed natively (CLI export) and the
//! same sector viewed in the web app must be bit-for-bit identical. Every
//! transcendental in the pipeline routes through `mapgen_core::fmath` (libm)
//! precisely so native and wasm floating-point agree exactly — this test is what
//! proves that routing actually holds end-to-end rather than in principle.
//!
//! Run: `wasm-pack test --node crates/mapgen-wasm`
//! (the golden file below is the single source of truth, shared with the native
//! test; re-anchoring the native golden re-anchors this one automatically.)

use mapgen_world::{
    generate_full,
    scale::{refine_sector, RefineParams, Sector},
    GenerateParams,
};
use wasm_bindgen_test::*;

/// Must stay identical to `fixed_params(42)` in
/// `crates/mapgen-world/tests/pipeline_spec.rs`.
fn fixed_params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
        periodic: false,
    }
}

/// Identical hashing to `mapgen-world`'s `hash_world`: blake3 over the
/// ciborium-serialized `WorldData`.
fn hash_world(world: &mapgen_core::WorldData) -> String {
    let mut bytes = Vec::new();
    ciborium::into_writer(world, &mut bytes).unwrap();
    blake3::hash(&bytes).to_hex().to_string()
}

/// Phase 7 cross-platform refine golden: a fixed refined sector hashes identical
/// under wasm32 to the native golden (`crates/mapgen-world/tests/golden/
/// seed42_sector.blake3.txt`). Extends the 6.2 guarantee (which covered
/// `generate_full`) to the refine path — sub-region mesh, projection,
/// seam-pinning — which the multi-scale atlas regenerates in the browser.
#[wasm_bindgen_test]
fn refined_sector_golden_matches_native_under_wasm() {
    let parent = generate_full(fixed_params(42));
    let child = refine_sector(
        &parent,
        Sector {
            level: 2,
            sx: 1,
            sy: 1,
        },
        RefineParams::default(),
    );
    let hash = hash_world(&child);
    let committed = include_str!("../../mapgen-world/tests/golden/seed42_sector.blake3.txt").trim();
    assert_eq!(
        hash, committed,
        "wasm32 refined sector diverged from the native golden — refine-path \
         native↔wasm byte-identity broken"
    );
}

/// Phase 7: the `refineSector` binding produces a renderable sector under wasm,
/// carrying its own sub-rectangle viewBox (level-2 (1,1) of a 2048×1280 world →
/// `512 320 512 320`).
#[wasm_bindgen_test]
fn refine_sector_binding_renders_a_sector() {
    // Refine off a whole-world handle (its society is projected onto the sector).
    let root = mapgen_wasm::generate(42, 4000, 6);
    let handle = root.refine_sector(2, 1, 1, 2000).expect("refine ok");
    let svg = handle.render("ornate").expect("render ok");
    assert!(
        svg.contains(r#"viewBox="512 320 512 320""#),
        "sector SVG must carry its own viewBox"
    );
    assert!(svg.len() > 10_000, "sector SVG should have real content");
}

/// `refineSector` rejects out-of-range coordinates rather than panicking across
/// the wasm boundary (which would surface as an opaque unreachable trap).
#[wasm_bindgen_test]
fn refine_sector_rejects_out_of_range() {
    let root = mapgen_wasm::generate(42, 2000, 6);
    // At level 1 only sx,sy in 0..2 are valid; sx = 5 must error.
    assert!(root.refine_sector(1, 5, 0, 1000).is_err());
    // A valid one still succeeds.
    assert!(root.refine_sector(1, 1, 0, 1000).is_ok());
}

#[wasm_bindgen_test]
fn full_pipeline_golden_hash_matches_native_under_wasm() {
    let world = generate_full(fixed_params(42));
    let hash = hash_world(&world);
    let committed = include_str!("../../mapgen-world/tests/golden/seed42_full.blake3.txt").trim();
    assert_eq!(
        hash, committed,
        "wasm32 generate_full output diverged from the native golden — \
         native↔wasm byte-identity is broken (suspect a transcendental that \
         bypasses mapgen_core::fmath)"
    );
}

/// A LANED planet seed pins the `Event::far_shore` tag byte-identical native↔wasm.
/// seed 42 above is the laneless continental default — no carrier crosses water, so
/// every `far_shore` stays `None` and is byte-invisible (`skip_serializing_if`), and
/// that golden never actually exercises the tag. seed 9 is the canonical THREE-strand
/// far shore (faith + colony + sword; the `mapgen-lore/tests/shore.rs` fixture), so
/// its `far_shore` values fire from every gen carrier. Hashing the whole 18k-cell,
/// divide-heavy planet world pins those tag values identical to the native golden
/// (`seed9_planet_full.blake3.txt`) — closing the far_shore native↔wasm pin that was
/// previously only structural.
#[wasm_bindgen_test]
fn planet_seed9_far_shore_golden_matches_native_under_wasm() {
    let world = generate_full(GenerateParams::planet(9));
    let hash = hash_world(&world);
    let committed =
        include_str!("../../mapgen-world/tests/golden/seed9_planet_full.blake3.txt").trim();
    assert_eq!(
        hash, committed,
        "wasm32 planet seed-9 output diverged from the native golden — far_shore tag \
         native↔wasm byte-identity broken (suspect a transcendental bypassing \
         mapgen_core::fmath)"
    );
}

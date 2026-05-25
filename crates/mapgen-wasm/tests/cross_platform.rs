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

use mapgen_world::{generate_full, GenerateParams};
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
    }
}

/// Identical hashing to `mapgen-world`'s `hash_world`: blake3 over the
/// ciborium-serialized `WorldData`.
fn hash_world(world: &mapgen_core::WorldData) -> String {
    let mut bytes = Vec::new();
    ciborium::into_writer(world, &mut bytes).unwrap();
    blake3::hash(&bytes).to_hex().to_string()
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

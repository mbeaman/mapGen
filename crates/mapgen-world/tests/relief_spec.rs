//! CONTRACT (Relief addendum §2): adjacent tiles' relief grids are BIT-IDENTICAL
//! along their shared edge — including the antimeridian wrap pair — or displaced
//! patches crack at every seam. The tile's own refined field measurably CANNOT
//! provide this (spike: 0/65 edge nodes bit-identical, ~6% deltas — different
//! meshes IDW different sites, and `fill_depressions` mutates after the edge
//! pin), so edge nodes are pure ROOT-field samples at endpoint-pinned, bitwise
//! shared coordinates (`Sector::rect` computes a shared edge identically on
//! both sides), smoothstep-blended inland to the tile's refined field.
//!
//! Red mutation (hand-verified): force the blend weight to 1.0 (tile-only
//! sampling) → the east|west assertion fails with ~0/GH identical columns —
//! the exact defect the spike measured.

use mapgen_world::scale::{refine_sector, RefineParams, Sector};
use mapgen_world::{generate_full, GenerateParams};

const GW: u32 = 33; // contract is dimension-independent; small = fast CI
const GH: u32 = 17;

fn grid(parent: &mapgen_core::WorldData, sector: Sector) -> Vec<f32> {
    let tile = refine_sector(parent, sector, RefineParams::default());
    mapgen_world::relief::relief_grid(parent, &tile, sector, GW, GH).expect("relief grid")
}

fn sector(level: u32, sx: u32, sy: u32) -> Sector {
    Sector { level, sx, sy }
}

/// Column `i` of a row-major gh×gw grid.
fn column(g: &[f32], i: u32) -> Vec<u32> {
    (0..GH)
        .map(|j| g[(j * GW + i) as usize].to_bits())
        .collect()
}

/// Row `j` of a row-major gh×gw grid.
fn row(g: &[f32], j: u32) -> Vec<u32> {
    (0..GW)
        .map(|i| g[(j * GW + i) as usize].to_bits())
        .collect()
}

#[test]
fn adjacent_tiles_share_a_bit_identical_relief_edge() {
    let world = generate_full(GenerateParams::planet(42));

    // East|west neighbours: (1,3)'s east edge column == (2,3)'s west edge column.
    let a = grid(&world, sector(3, 1, 3));
    let b = grid(&world, sector(3, 2, 3));
    assert_eq!(
        column(&a, GW - 1),
        column(&b, 0),
        "east|west relief seam is not bit-identical — displaced patches crack"
    );

    // North|south neighbours: (1,3)'s south edge row == (1,4)'s north edge row.
    let c = grid(&world, sector(3, 1, 4));
    assert_eq!(
        row(&a, GH - 1),
        row(&c, 0),
        "north|south relief seam is not bit-identical"
    );

    // ANTIMERIDIAN pair on the periodic planet: (7,3)'s east edge is the wrap
    // seam at x == W; (0,3)'s west edge is x == 0. Canonicalization must make
    // them the same root samples (wrap_dx is not bitwise-neutral at x == W for
    // sites west of W/2, so without it near-tie k-NN ranking can flip).
    let e = grid(&world, sector(3, 7, 3));
    let w = grid(&world, sector(3, 0, 3));
    assert_eq!(
        column(&e, GW - 1),
        column(&w, 0),
        "antimeridian relief seam is not bit-identical — the wrap pair cracks"
    );

    // Sanity on the field itself: sea-clamped (≥ 0, finite) and non-trivial
    // (some land relief exists on this continental sector).
    assert!(a.iter().all(|h| h.is_finite() && *h >= 0.0));
    assert!(a.iter().any(|h| *h > 0.0), "relief grid is all-sea/flat");
}

/// SEAM-2 (Relief addendum §9.3, R4 quality hunt): adjacent tiles' SHADE agrees
/// within a MEASURED tolerance. Heights are bit-identical at the shared edge
/// (SEAM-1), but the lambert gradient at an edge node pulls one stencil arm from
/// each tile's OWN first-inland node (different meshes), so edge lambert agrees
/// only approximately. Contract: max |Δlambert| along the canonical shared edge
/// < 0.005 (under the u8 quantization step 1/255 ≈ 0.0039) — the named seam
/// SCREENSHOT is the severity judge, and the pure-root edge-stencil contingency
/// (§9.3) is gated on THIS measurement, not built speculatively.
///
/// The un-fixed one-sided cross-edge gradient measured **0.177** here (≫ both
/// 0.005 AND the u8 step — a visible bright/dark line at every seam), so the
/// contingency was BUILT: `lambert_grid` zeroes the cross-edge gradient at tile
/// boundaries (mapgen-render relief.rs), making edge lambert depend only on the
/// bit-identical along-edge slope. AFTER: max |Δlambert| == 0.0 (bit-identical).
#[test]
fn seam_lambert_agrees_within_tolerance() {
    let world = generate_full(GenerateParams::planet(42));
    let lambert = |sec: Sector| -> Vec<f32> {
        let tile = refine_sector(&world, sec, RefineParams::default());
        let h = mapgen_world::relief::relief_grid(&world, &tile, sec, 129, 65).expect("relief grid");
        mapgen_render::relief::lambert_grid(&h, 129, 65, &Default::default())
    };
    // (1,3)'s east edge (column 128) vs (2,3)'s west edge (column 0) — the SAME
    // canonical shared edge SEAM-1 proves bit-identical in HEIGHT.
    let a = lambert(sector(3, 1, 3));
    let b = lambert(sector(3, 2, 3));
    let mut max_d = 0.0f32;
    for j in 0..65u32 {
        let la = a[(j * 129 + 128) as usize];
        let lb = b[(j * 129) as usize];
        max_d = max_d.max((la - lb).abs());
    }
    eprintln!("SEAM-2 measured max |Δlambert| = {max_d}");
    assert!(
        max_d < 0.005,
        "seam shade residual {max_d} exceeds tolerance 0.005 — build the pure-root edge stencil (§9.3)"
    );
}

/// for the canonical call — seed-42 continental parent → L2 (1,1) (the SAME
/// sector the `seed42_sector` cross-platform golden pins), default refine,
/// gw=129 gh=65, `ShadeParams::default()`. The relief grid + shade factors are
/// off every existing golden surface (they touch no `WorldData`), so without
/// THIS pin a native↔wasm divergence in the new samplers would ship silently.
/// A wasm twin asserts the same file (`mapgen-wasm/tests/cross_platform.rs`).
#[test]
fn relief_grid_golden_hash() {
    let parent = generate_full(GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
        periodic: false,
    });
    let sec = sector(2, 1, 1);
    let tile = refine_sector(&parent, sec, RefineParams::default());
    let heights =
        mapgen_world::relief::relief_grid(&parent, &tile, sec, 129, 65).expect("relief grid");
    let lambert = mapgen_render::relief::lambert_grid(&heights, 129, 65, &Default::default());
    let mut bytes = Vec::with_capacity((heights.len() + lambert.len()) * 4);
    for v in heights.iter().chain(lambert.iter()) {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let committed = include_str!("golden/seed42_relief_grid.blake3.txt").trim();
    assert_eq!(
        hash, committed,
        "relief grid + lambert drifted from the committed golden — the seam-band \
         sampler or the shade math changed (or diverged across platforms)"
    );
}

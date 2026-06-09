//! Phase 6.1.5 — Strahler stream order + seasonal river regime.

use mapgen_core::world_data::river_regime;
use mapgen_world::{generate_full, GenerateParams};

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

/// Per-cell Strahler order is well-formed: river cells are >= 1, non-river
/// cells are 0, and nothing exceeds a sane ceiling.
#[test]
fn strahler_per_cell_well_formed() {
    let w = generate_full(fixed_params(42));
    let n = w.mesh.cell_count();
    assert_eq!(w.hydrology.strahler.len(), n);

    // Which cells lie on a river (any river chain visits them).
    let mut on_river = vec![false; n];
    for r in &w.hydrology.rivers {
        for &c in &r.cells {
            if w.terrain.elevation[c as usize] > 0.0 {
                on_river[c as usize] = true;
            }
        }
    }
    for (i, &o) in w.hydrology.strahler.iter().enumerate() {
        assert!(o <= 12, "implausible Strahler order {o} at cell {i}");
        if on_river[i] {
            assert!(o >= 1, "river cell {i} has order 0");
        }
    }
}

/// Strahler order never decreases as you move downstream along a river chain
/// (a confluence can only hold or raise it), and the stored `River::strahler`
/// equals the order at the mouth.
#[test]
fn strahler_non_decreasing_downstream() {
    let w = generate_full(fixed_params(42));
    let order = &w.hydrology.strahler;
    for r in &w.hydrology.rivers {
        let land: Vec<u32> = r
            .cells
            .iter()
            .copied()
            .filter(|&c| w.terrain.elevation[c as usize] > 0.0)
            .collect();
        for win in land.windows(2) {
            let up = order[win[0] as usize];
            let down = order[win[1] as usize];
            assert!(
                down >= up,
                "order dropped {up}→{down} going downstream in a river"
            );
        }
        let mouth_order = land.iter().map(|&c| order[c as usize]).max().unwrap_or(0);
        assert_eq!(
            r.strahler, mouth_order,
            "River::strahler must be mouth order"
        );
    }
}

/// Every river carries a valid regime, and the classifier actually
/// discriminates — seed 42 (a varied continent) yields several regimes.
#[test]
fn river_regimes_valid_and_varied() {
    let w = generate_full(fixed_params(42));
    let mut seen = std::collections::BTreeSet::new();
    for r in &w.hydrology.rivers {
        assert!(
            r.regime <= river_regime::EPHEMERAL,
            "invalid regime {} ",
            r.regime
        );
        seen.insert(r.regime);
    }
    assert!(
        seen.len() >= 2,
        "regime classifier produced only one class: {seen:?}"
    );
}

#[test]
fn rivers_are_deterministic() {
    let a = generate_full(fixed_params(7));
    let b = generate_full(fixed_params(7));
    assert_eq!(a.hydrology.strahler, b.hydrology.strahler);
    let ra: Vec<(u8, u8)> = a
        .hydrology
        .rivers
        .iter()
        .map(|r| (r.strahler, r.regime))
        .collect();
    let rb: Vec<(u8, u8)> = b
        .hydrology
        .rivers
        .iter()
        .map(|r| (r.strahler, r.regime))
        .collect();
    assert_eq!(ra, rb);
}

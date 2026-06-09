//! Phase 6.1.4 — USDA soil orders + the soil-driven WETLAND biome.

use mapgen_world::{biomes, generate_full, soils, GenerateParams};

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

/// Every land cell gets a real soil order; every sea cell is the OCEAN
/// sentinel. (Andisol is never assigned — no volcanism model — so the valid
/// land range is the other 11 orders, all ids < 12.)
#[test]
fn every_cell_has_a_soil_order() {
    let w = generate_full(fixed_params(42));
    let n = w.mesh.cell_count();
    assert_eq!(w.climate.soil.len(), n, "soil layer must cover every cell");
    for i in 0..n {
        let s = w.climate.soil[i];
        if w.terrain.elevation[i] > 0.0 {
            assert!(
                s < 12 && s != soils::ANDISOL,
                "land cell {i} has invalid soil order {s}"
            );
        } else {
            assert_eq!(s, soils::OCEAN, "sea cell {i} must be OCEAN soil");
        }
    }
}

/// The WETLAND biome is exactly the set of Histosol land cells (the override
/// `biomes::classify` applies), and every such cell is genuinely low-lying —
/// the soil classifier's waterlogging precondition.
#[test]
fn wetland_iff_histosol_and_lowland() {
    let w = generate_full(fixed_params(42));
    let n = w.mesh.cell_count();
    for i in 0..n {
        let is_wetland = w.climate.biome[i] == biomes::WETLAND;
        let is_histosol = w.terrain.elevation[i] > 0.0 && w.climate.soil[i] == soils::HISTOSOL;
        assert_eq!(
            is_wetland, is_histosol,
            "cell {i}: WETLAND biome and Histosol soil must coincide"
        );
        if is_wetland {
            assert!(
                w.terrain.elevation[i] > 0.0 && w.terrain.elevation[i] < 0.30,
                "wetland cell {i} should be low-lying land, elev={}",
                w.terrain.elevation[i]
            );
        }
    }
}

/// Soil is a pure function of the world — two runs of the same seed agree.
/// (The golden hash covers this too; this localizes a regression to soils.)
#[test]
fn soil_is_deterministic() {
    let a = generate_full(fixed_params(7));
    let b = generate_full(fixed_params(7));
    assert_eq!(a.climate.soil, b.climate.soil);
}

/// No single soil order may swallow the whole map — a guard against a future
/// threshold change collapsing the classifier. Checked on a few seeds.
#[test]
fn no_soil_order_dominates_pathologically() {
    for seed in [1u64, 42, 99] {
        let w = generate_full(fixed_params(seed));
        let n = w.mesh.cell_count();
        let land: usize = (0..n).filter(|&i| w.terrain.elevation[i] > 0.0).count();
        let mut counts = [0usize; 12];
        for i in 0..n {
            let s = w.climate.soil[i];
            if (s as usize) < 12 {
                counts[s as usize] += 1;
            }
        }
        let max = counts.iter().copied().max().unwrap();
        assert!(
            (max as f32) < 0.75 * land as f32,
            "seed {seed}: a single soil order covers {max}/{land} land cells (>75%)"
        );
    }
}

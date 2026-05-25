//! Phase 7 — nested multi-scale refinement: geometry, the coarsening contract,
//! and determinism.

use mapgen_world::{
    generate_full,
    scale::{refine_sector, RefineParams, Sector},
    GenerateParams,
};

fn params() -> GenerateParams {
    GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    }
}

fn in_rect(p: [f32; 2], r: [f32; 4]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}

fn nearest(sites: &[[f32; 2]], p: [f32; 2]) -> usize {
    let mut best = 0;
    let mut bd = f32::MAX;
    for (i, s) in sites.iter().enumerate() {
        let d = (s[0] - p[0]).powi(2) + (s[1] - p[1]).powi(2);
        if d < bd {
            bd = d;
            best = i;
        }
    }
    best
}

/// The four child quadrants exactly tile their parent's rectangle.
#[test]
fn sector_quadrants_tile_the_parent() {
    let (w, h) = (1024.0, 640.0);
    let parent = Sector {
        level: 1,
        sx: 1,
        sy: 0,
    };
    let pr = parent.rect(w, h);
    let mut area = 0.0;
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let c = parent.child(dx, dy);
        assert_eq!(c.level, 2);
        let cr = c.rect(w, h);
        // Child lies within the parent.
        assert!(cr[0] >= pr[0] - 1e-3 && cr[2] <= pr[2] + 1e-3);
        assert!(cr[1] >= pr[1] - 1e-3 && cr[3] <= pr[3] + 1e-3);
        area += (cr[2] - cr[0]) * (cr[3] - cr[1]);
    }
    let parent_area = (pr[2] - pr[0]) * (pr[3] - pr[1]);
    assert!(
        (area - parent_area).abs() < 1e-2,
        "quadrants must tile the parent"
    );
}

/// The coarsening contract: a refined sector reproduces the parent's
/// large-scale structure. Erosion adds genuine sub-parent detail, so the match
/// is "within tolerance", not bit-exact — but the coastline and elevation field
/// track the parent closely. Tolerances are empirical (observed: 94–98% sign
/// agreement, MAD 0.04–0.11) with margin.
#[test]
fn refined_sector_reproduces_parent_within_tolerance() {
    let p = params();
    let parent = generate_full(p.clone());

    for sec in [
        Sector {
            level: 1,
            sx: 0,
            sy: 0,
        },
        Sector {
            level: 2,
            sx: 1,
            sy: 1,
        },
    ] {
        let child = refine_sector(&parent, sec, RefineParams::default());
        let rect = sec.rect(p.width, p.height);

        let mut n = 0;
        let mut mad = 0.0f32;
        let mut sign_agree = 0;
        let mut p_land = 0;
        for i in 0..parent.mesh.cell_count() {
            let pp = parent.mesh.sites[i];
            if !in_rect(pp, rect) {
                continue;
            }
            n += 1;
            let pe = parent.terrain.elevation[i];
            if pe > 0.0 {
                p_land += 1;
            }
            let ce = child.terrain.elevation[nearest(&child.mesh.sites, pp)];
            mad += (pe - ce).abs();
            if (pe > 0.0) == (ce > 0.0) {
                sign_agree += 1;
            }
        }
        assert!(
            n > 20,
            "sector {sec:?}: too few parent cells to judge ({n})"
        );

        let mad = mad / n as f32;
        let agree = sign_agree as f32 / n as f32;
        assert!(
            mad < 0.15,
            "sector {sec:?}: elevation MAD {mad:.3} too high"
        );
        assert!(
            agree > 0.90,
            "sector {sec:?}: land/sea agreement {:.1}% too low",
            agree * 100.0
        );

        // Land fraction tracks the parent's for this rectangle.
        let rect_cells: Vec<usize> = (0..child.mesh.cell_count())
            .filter(|&i| in_rect(child.mesh.sites[i], rect))
            .collect();
        let c_land = rect_cells
            .iter()
            .filter(|&&i| child.terrain.elevation[i] > 0.0)
            .count();
        let p_frac = p_land as f32 / n as f32;
        let c_frac = c_land as f32 / rect_cells.len().max(1) as f32;
        assert!(
            (p_frac - c_frac).abs() < 0.10,
            "sector {sec:?}: land fraction {c_frac:.2} drifts from parent {p_frac:.2}"
        );
    }
}

/// A sector is finer than the parent (the whole point) and carries its own
/// viewport while keeping the full-world extent for global stages.
#[test]
fn refined_sector_is_finer_and_self_describing() {
    let p = params();
    let sec = Sector {
        level: 2,
        sx: 1,
        sy: 1,
    };
    let parent = generate_full(p.clone());
    let child = refine_sector(&parent, sec, RefineParams::default());
    let rect = sec.rect(p.width, p.height);

    // Full-world extent preserved (plate scatter / climate latitude stay global).
    assert_eq!(child.mesh.width, p.width);
    assert_eq!(child.mesh.height, p.height);
    // Viewport is the true sector.
    let region = child.mesh.region.expect("sector carries a region");
    for (a, b) in region.iter().zip(rect.iter()) {
        assert!((a - b).abs() < 1e-3);
    }
    // Denser than the parent within the same rectangle (~16× the area ratio).
    let interior = (0..child.mesh.cell_count())
        .filter(|&i| in_rect(child.mesh.sites[i], rect))
        .count();
    assert!(
        interior > 2000,
        "sector should be dense, got {interior} interior cells"
    );
    // Biomes ran (physical pipeline completed).
    assert_eq!(child.climate.biome.len(), child.mesh.cell_count());
}

/// Refinement is a pure function of `(params, sector)` — byte-identical across
/// runs. (Cross-platform byte-identity is covered by the wasm golden.)
#[test]
fn refinement_is_deterministic() {
    let p = params();
    let sec = Sector {
        level: 2,
        sx: 0,
        sy: 1,
    };
    let parent = generate_full(p.clone());
    let a = refine_sector(&parent, sec, RefineParams::default());
    let b = refine_sector(&parent, sec, RefineParams::default());

    let hash = |w: &mapgen_core::WorldData| {
        let mut bytes = Vec::new();
        ciborium::into_writer(w, &mut bytes).unwrap();
        blake3::hash(&bytes).to_hex().to_string()
    };
    assert_eq!(hash(&a), hash(&b), "refinement must be deterministic");
}

/// Distinct sectors are genuinely different worlds (no accidental aliasing of
/// the per-sector seed).
#[test]
fn distinct_sectors_differ() {
    let p = params();
    let parent = generate_full(p.clone());
    let a = refine_sector(
        &parent,
        Sector {
            level: 2,
            sx: 0,
            sy: 0,
        },
        RefineParams::default(),
    );
    let b = refine_sector(
        &parent,
        Sector {
            level: 2,
            sx: 3,
            sy: 3,
        },
        RefineParams::default(),
    );
    assert_ne!(a.terrain.elevation, b.terrain.elevation);
}

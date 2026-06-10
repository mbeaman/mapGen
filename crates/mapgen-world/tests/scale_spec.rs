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
        periodic: false,
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
        // TIGHTENED with the cross-level anchor (the child refines the parent's
        // FINAL field): the old thresholds (MAD < 0.15, agreement > 0.90) let a
        // tenth of the map flip land↔sea across a zoom — the user-visible
        // "patterns do not represent the same content" defect. Anchored, the
        // measured values are MAD ≤ 0.03 / agreement ≥ 98%; thresholds sit just
        // below so re-introducing per-sector erosion (or any low-frequency
        // rewrite) reds this.
        assert!(
            mad < 0.05,
            "sector {sec:?}: elevation MAD {mad:.3} too high — the child is \
             rewriting the parent's terrain, not refining it"
        );
        assert!(
            agree > 0.96,
            "sector {sec:?}: land/sea agreement {:.1}% too low — coastlines \
             reshape across a zoom",
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

/// The planet preset is the root of the zoom-out hierarchy: refining one of its
/// sectors yields a consistent continental window of the *same* world (same
/// extent, a true sub-rectangle, physical pipeline run), via the same machinery
/// as any other refinement — so planet → continent → region is one mechanism.
#[test]
fn planet_root_refines_into_a_continental_sector() {
    let planet = generate_full(GenerateParams::planet(42));
    let sec = Sector {
        level: 2,
        sx: 1,
        sy: 1,
    };
    let region = refine_sector(&planet, sec, RefineParams::default());

    // Same global extent; the sector is a true sub-rectangle within it.
    assert_eq!(region.mesh.width, planet.mesh.width);
    assert_eq!(region.mesh.height, planet.mesh.height);
    let rect = region.mesh.region.expect("sector carries a region");
    let [vx, vy, vx1, vy1] = region.mesh.view_rect();
    assert!(vx >= 0.0 && vy >= 0.0);
    assert!(vx1 <= planet.mesh.width + 1.0 && vy1 <= planet.mesh.height + 1.0);
    assert!((rect[2] - rect[0] - planet.mesh.width / 4.0).abs() < 1.0); // level-2 → 1/4 wide
                                                                        // The physical pipeline ran over the sector.
    assert_eq!(region.climate.biome.len(), region.mesh.cell_count());
    assert!(region.mesh.cell_count() > 1000);
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

/// Seam-pinning: two horizontally-adjacent sectors agree along their shared
/// edge. Each runs its own erosion + sector-specific detail noise, so without
/// pinning their edges would diverge; pinning both back to the shared base field
/// at the boundary makes the seam match (low elevation MAD, land/sea agreement).
#[test]
fn adjacent_sectors_agree_at_their_seam() {
    let p = params();
    let parent = generate_full(p.clone());
    // Columns 1 and 2 of the level-2 grid share the vertical edge at x = width/2.
    let a = refine_sector(
        &parent,
        Sector {
            level: 2,
            sx: 1,
            sy: 1,
        },
        RefineParams::default(),
    );
    let b = refine_sector(
        &parent,
        Sector {
            level: 2,
            sx: 2,
            sy: 1,
        },
        RefineParams::default(),
    );

    let edge_x = p.width * 0.5; // boundary between col 1 ([w/4,w/2]) and col 2
    let cell_h = p.height / 4.0;
    let (y_lo, y_hi) = (cell_h + 8.0, cell_h * 2.0 - 8.0); // row 1, inset from corners

    let mut n = 0;
    let mut mad = 0.0f32;
    let mut sign = 0;
    for k in 0..40 {
        let y = y_lo + (y_hi - y_lo) * k as f32 / 39.0;
        let pt = [edge_x, y];
        let ea = a.terrain.elevation[nearest(&a.mesh.sites, pt)];
        let eb = b.terrain.elevation[nearest(&b.mesh.sites, pt)];
        n += 1;
        mad += (ea - eb).abs();
        if (ea > 0.0) == (eb > 0.0) {
            sign += 1;
        }
    }
    let mad = mad / n as f32;
    let agree = sign as f32 / n as f32;
    assert!(
        mad < 0.04,
        "seam elevation MAD {mad:.3} too high — sectors disagree"
    );
    assert!(
        agree > 0.9,
        "seam land/sea agreement {:.0}% too low",
        agree * 100.0
    );

    // Rivers (projected from the shared parent network) are continuous too: both
    // sectors agree on whether significant flow is present along the seam.
    let mut river_agree = 0;
    for k in 0..40 {
        let y = y_lo + (y_hi - y_lo) * k as f32 / 39.0;
        let pt = [edge_x, y];
        let fa = a.hydrology.flow[nearest(&a.mesh.sites, pt)];
        let fb = b.hydrology.flow[nearest(&b.mesh.sites, pt)];
        if (fa > 20.0) == (fb > 20.0) {
            river_agree += 1;
        }
    }
    assert!(
        river_agree >= 34,
        "seam river-presence agreement {river_agree}/40 too low"
    );
}

/// The other half of "refine, never rewrite": the child must genuinely REFINE —
/// add sub-parent detail — not just upsample the parent verbatim. Every
/// agreement test above is monotone in smoothness (a child that IS the smoothed
/// parent scores perfectly), so without this contrast a silently-broken detail
/// stage (detail_octaves effectively 0) would stay green everywhere — the
/// review-caught false-green. Self-calibrating: refine the same sector WITH the
/// default detail octaves and WITH zero, and assert the default differs from
/// the detail-free anchor by a real margin. MEASURED: the detail stage
/// contributes ~0.0043 mean |Δelevation| (the noise crate's fBm output is far
/// below its nominal 0.12 amplitude) — deliberately SUBTLE fine texture, which
/// also serves the cross-level alignment contract; an upsample-only child gives
/// ~0. Threshold 0.002 sits between. (If drilled sectors ever read as "just a
/// blurry parent", the detail strength is the tuning knob — see tuning_log.)
#[test]
fn a_refined_sector_adds_sub_parent_detail() {
    let p = params();
    let parent = generate_full(p);
    let sec = Sector {
        level: 2,
        sx: 1,
        sy: 1,
    };
    let with_detail = refine_sector(&parent, sec, RefineParams::default());
    let without = refine_sector(
        &parent,
        sec,
        RefineParams {
            detail_octaves: 0,
            ..RefineParams::default()
        },
    );
    // Same sector stream → identical mesh; the only delta is the detail stage.
    assert_eq!(
        with_detail.mesh.cell_count(),
        without.mesh.cell_count(),
        "fixture: both refines must share the mesh"
    );
    let n = with_detail.mesh.cell_count();
    let mean_abs_delta: f32 = (0..n)
        .map(|i| (with_detail.terrain.elevation[i] - without.terrain.elevation[i]).abs())
        .sum::<f32>()
        / n as f32;
    assert!(
        mean_abs_delta >= 0.002,
        "mean |Δelev| {mean_abs_delta:.4} — the detail stage adds no sub-parent detail \
         (the child is a bare upsample of the parent)"
    );
}

/// THE CROSS-LEVEL CONTRACT on the globe's own path: drilling a PLANET world
/// shows the SAME place at higher fidelity — land/sea and biomes must match the
/// parent the user was just looking at. Before the cross-level anchor, the
/// refine path re-derived its own terrain (pre-erosion base + its own erosion):
/// measured on this exact world, only 55–85% of cells kept their land/sea sign
/// and 34–58% their biome across a drill — zooming in visibly rewrote the map.
/// Anchored (child = smoothed parent FINAL elevation + zero-mean detail, no
/// child erosion), the same sectors measure 97–100% land/sea and 85–90% biome.
/// Thresholds sit between the two regimes: reverting the anchor reds this.
#[test]
fn drilling_a_planet_shows_the_same_place_at_higher_fidelity() {
    let mut p = GenerateParams::planet(8);
    p.cell_count = 2000;
    let parent = generate_full(p);
    for (sx, sy) in [(1u32, 3u32), (1, 4), (2, 4)] {
        let sec = Sector { level: 3, sx, sy };
        let child = refine_sector(&parent, sec, RefineParams::default());
        let rect = sec.rect(2048.0, 1024.0);
        let (mut n, mut sign, mut biome_same) = (0, 0, 0);
        for i in 0..parent.mesh.cell_count() {
            let pp = parent.mesh.sites[i];
            if !in_rect(pp, rect) {
                continue;
            }
            n += 1;
            let ci = nearest(&child.mesh.sites, pp);
            if (parent.terrain.elevation[i] > 0.0) == (child.terrain.elevation[ci] > 0.0) {
                sign += 1;
            }
            if parent.climate.biome[i] == child.climate.biome[ci] {
                biome_same += 1;
            }
        }
        assert!(n >= 20, "sector ({sx},{sy}): too few parent samples ({n})");
        let land_agree = sign as f32 / n as f32;
        let biome_agree = biome_same as f32 / n as f32;
        assert!(
            land_agree >= 0.92,
            "sector ({sx},{sy}): land/sea agreement {:.0}% — drilling rewrites the coastline",
            land_agree * 100.0
        );
        assert!(
            biome_agree >= 0.75,
            "sector ({sx},{sy}): biome agreement {:.0}% — drilling recolours the map",
            biome_agree * 100.0
        );
    }
}

/// Seam-pinning for the VISIBLE field: two adjacent sectors agree on the BIOME
/// along their shared edge. Elevation is pinned to the shared base (above), but
/// biomes derive from CLIMATE (temperature/precipitation), which each sector
/// marches independently on its own mesh — without `pin_climate_to_parent`, the
/// two sides land on different sides of biome-class thresholds and the seam
/// shows as a sharp straight line where nature unnaturally shifts (the
/// user-reported drilled-globe artifact: discrete biome colours swapping along
/// the sector edge). Fixture: the probe-picked WORST seam — the seed-8 planet
/// @2000 (the globe e2e world), L3 (1,4)|(2,4), a land seam that agreed only 87%
/// unpinned and 92% pinned (the residual is nearest-cell sampling granularity at
/// legitimate biome transitions, not seam error). Threshold 90% sits between —
/// removing the climate pin reds this. Behavioral fixture: re-derive the seam if
/// a sim change shifts the world.
#[test]
fn adjacent_sectors_agree_on_biome_at_their_seam() {
    let mut p = GenerateParams::planet(8);
    p.cell_count = 2000;
    let parent = generate_full(p);
    let a = refine_sector(
        &parent,
        Sector {
            level: 3,
            sx: 1,
            sy: 4,
        },
        RefineParams::default(),
    );
    let b = refine_sector(
        &parent,
        Sector {
            level: 3,
            sx: 2,
            sy: 4,
        },
        RefineParams::default(),
    );

    let edge_x = 2048.0 * 2.0 / 8.0; // boundary between L3 columns 1 and 2
    let row_h = 1024.0 / 8.0;
    let (y_lo, y_hi) = (row_h * 4.0 + 4.0, row_h * 5.0 - 4.0);

    let mut same = 0;
    let mut total = 0;
    for k in 0..60 {
        let y = y_lo + (y_hi - y_lo) * k as f32 / 59.0;
        let pt = [edge_x, y];
        let ba = a.climate.biome[nearest(&a.mesh.sites, pt)];
        let bb = b.climate.biome[nearest(&b.mesh.sites, pt)];
        total += 1;
        if ba == bb {
            same += 1;
        }
    }
    let agree = same as f32 / total as f32;
    assert!(
        agree >= 0.9,
        "seam biome agreement {:.0}% ({same}/{total}) — adjacent sectors classify the \
         same edge differently (unpinned climate → visible straight-line biome swaps)",
        agree * 100.0
    );
}

/// Golden hash of a fixed refined sector — the native anchor for the
/// cross-platform refine golden (`crates/mapgen-wasm/tests/cross_platform.rs`
/// hashes the same sector under wasm32 and compares to this file). Pins that the
/// whole refine path — sub-region mesh, projection, seam-pinning — is
/// deterministic; the wasm side pins it is *also* byte-identical across targets.
#[test]
fn refined_sector_golden_hash() {
    let parent = generate_full(params());
    let child = refine_sector(
        &parent,
        Sector {
            level: 2,
            sx: 1,
            sy: 1,
        },
        RefineParams::default(),
    );
    let mut bytes = Vec::new();
    ciborium::into_writer(&child, &mut bytes).unwrap();
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let committed = include_str!("golden/seed42_sector.blake3.txt").trim();
    assert_eq!(
        hash, committed,
        "refined sector drifted from committed golden"
    );
}

/// Robustness: refining *every* sector across several levels — including all-
/// ocean sectors, all-land inland sectors, world-edge corners, and deep levels —
/// never panics and yields a structurally valid world (biome ids in range,
/// rivers/settlements/control referencing real cells). The edge cases that bite
/// on-demand generation (a sector with no land, no rivers, or clamped halo at the
/// world border) are exactly what's swept here.
#[test]
fn refine_is_robust_across_sectors() {
    let p = params();
    let parent = generate_full(p.clone());

    let mut sectors: Vec<Sector> = Vec::new();
    for level in 1..=2u32 {
        let span = 1u32 << level;
        for sy in 0..span {
            for sx in 0..span {
                sectors.push(Sector { level, sx, sy });
            }
        }
    }
    // A handful of deep + corner sectors (clamped halos, tiny cell counts).
    for &(level, sx, sy) in &[
        (3, 0, 0),
        (3, 7, 3),
        (4, 0, 0),
        (4, 15, 9),
        (5, 0, 0),
        (5, 31, 19),
    ] {
        sectors.push(Sector { level, sx, sy });
    }

    for sec in sectors {
        assert!(sec.is_valid());
        let c = refine_sector(&parent, sec, RefineParams::default());
        let n = c.mesh.cell_count();
        assert!(n > 0, "{sec:?}: empty mesh");
        assert!(c.mesh.region.is_some(), "{sec:?}: no region");
        assert_eq!(c.climate.biome.len(), n, "{sec:?}: biome length");
        for &b in &c.climate.biome {
            assert!(b <= 15, "{sec:?}: biome id {b} out of range");
        }
        for r in &c.hydrology.rivers {
            for &cell in &r.cells {
                assert!(
                    (cell as usize) < n,
                    "{sec:?}: river cell {cell} out of range"
                );
            }
        }
        for s in &c.society.settlements {
            assert!(
                (s.cell as usize) < n,
                "{sec:?}: settlement cell out of range"
            );
            assert!(
                (s.polity_id as usize) < c.society.nations.len(),
                "{sec:?}: settlement polity out of range"
            );
        }
        for ctrl in c.society.control.iter().flatten() {
            assert!(
                (*ctrl as usize) < c.society.nations.len(),
                "{sec:?}: control polity {ctrl} out of range"
            );
        }
    }
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

//! Sea-lanes substrate spec — Phase 1, Step 3a of "The Sundered Lanes"
//! (`docs/inter_continental_design.md`).
//!
//! The substrate is the maritime layer that later carriers and the diffusion /
//! contact loops replay across. Two properties are load-bearing and tested here:
//!
//! 1. **Exact geometry → tier mapping** (synthetic fixture). A hand-built grid
//!    of three landmasses — two split by a narrow strait, two by a wide ocean —
//!    must produce exactly the lanes that geometry allows, with the *near* pair
//!    crossable (`min_naval <= 40`, the roster's ordinary naval ceiling) and the
//!    *far* pair a wall (`min_naval > 40`). Known geometry ⇒ a known answer the
//!    feature can't fake: an empty graph, a missing tier, or a mis-anchored gate
//!    all fail.
//!
//! 2. **Emergent both-tier substrate** (canonical seed 19). A real planet must
//!    grow inter-body lanes spanning *both* tiers — at least one crossable strait
//!    AND at least one open-ocean wall — so the relative calibration has a
//!    non-empty crossable set and wall set on a seed the Step-0 probe proved has
//!    both. Scoped to the canonical seed only: a *sundered* seed legitimately has
//!    no crossable lane, so asserting "has a crossing" across all seeds would be
//!    the anti-Goodhart trap the design cut.

use mapgen_core::{MeshData, TerrainData, WorldData};
use mapgen_testsupport::{CROSSING_SEEDS, SUNDERED_SEEDS};
use mapgen_world::{
    naming::connected_bodies,
    pipeline::{Pipeline, PipelineStage},
    sea_lanes::{self, SeaLanesParams},
    GenerateParams,
};
use std::collections::BTreeSet;

/// The roster's ordinary naval ceiling (Riverfolk = 40). A lane is *crossable*
/// by an ordinary culture iff `min_naval <= NAVAL_CEILING`.
const NAVAL_CEILING: u8 = 40;

// ── Synthetic three-landmass fixture ────────────────────────────────────────
//
// A `ROWS × COLS` grid at `SPACING` world units. Column bands (every row
// identical), x = col * SPACING:
//
//   A | strait |  B  |      wide ocean      |   C
//   0..=3  4..=5  6..=9      10..=24          25..=29
//
// The strait (cols 4–5) is enclosed by A and B; the wide ocean (cols 10–24) by
// B and C. So A and C share NO water — only A↔B (strait) and B↔C (ocean) can
// form lanes. Land-to-land gaps: A↔B = 30 units (crossable), B↔C = 160 (wall).

const ROWS: usize = 6;
const COLS: usize = 30;
const SPACING: f32 = 10.0;

/// Body label of a column by its band.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Body {
    A,
    Strait,
    B,
    Ocean,
    C,
}

fn band(col: usize) -> Body {
    match col {
        0..=3 => Body::A,
        4..=5 => Body::Strait,
        6..=9 => Body::B,
        10..=24 => Body::Ocean,
        25..=29 => Body::C,
        _ => unreachable!(),
    }
}

/// Land bands are A/B/C; strait + ocean are sea.
fn is_land_band(b: Body) -> bool {
    matches!(b, Body::A | Body::B | Body::C)
}

/// Build the synthetic grid world: 4-connected neighbors, sites on a regular
/// lattice, elevation +0.5 on land bands and -0.2 on sea bands.
fn fixture_world() -> WorldData {
    let n = ROWS * COLS;
    let mut sites = Vec::with_capacity(n);
    let mut neighbors = vec![Vec::new(); n];
    let mut elevation = vec![0.0_f32; n];
    let idx = |r: usize, c: usize| r * COLS + c;
    for r in 0..ROWS {
        for c in 0..COLS {
            let i = idx(r, c);
            sites.push([c as f32 * SPACING, r as f32 * SPACING]);
            elevation[i] = if is_land_band(band(c)) { 0.5 } else { -0.2 };
            if r > 0 {
                neighbors[i].push(idx(r - 1, c) as u32);
            }
            if r + 1 < ROWS {
                neighbors[i].push(idx(r + 1, c) as u32);
            }
            if c > 0 {
                neighbors[i].push(idx(r, c - 1) as u32);
            }
            if c + 1 < COLS {
                neighbors[i].push(idx(r, c + 1) as u32);
            }
        }
    }

    let mesh = MeshData {
        width: COLS as f32 * SPACING,
        height: ROWS as f32 * SPACING,
        sites,
        neighbors,
        ..Default::default()
    };
    WorldData {
        mesh,
        terrain: TerrainData {
            elevation,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Classify a lane endpoint cell into its body band (by x position).
fn body_of(world: &WorldData, cell: u32) -> Body {
    let x = world.mesh.sites[cell as usize][0];
    let col = (x / SPACING).round() as usize;
    band(col)
}

/// The unordered body pair a lane connects.
fn lane_bodies(world: &WorldData, lane: &mapgen_core::SeaLane) -> (Body, Body) {
    let (ba, bb) = (body_of(world, lane.a), body_of(world, lane.b));
    if (ba as u8) <= (bb as u8) {
        (ba, bb)
    } else {
        (bb, ba)
    }
}

#[test]
fn synthetic_fixture_maps_strait_to_crossable_and_ocean_to_wall() {
    let mut world = fixture_world();

    // Sanity: the elevation mask really yields three land bodies.
    let bodies = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] > 0.0);
    assert_eq!(bodies.len(), 3, "fixture should have exactly 3 landmasses");

    let mut rng = mapgen_core::StageRng::new(0).stream(mapgen_core::Stage::SeaLanes);
    sea_lanes::chart(&mut world, SeaLanesParams { min_body_cells: 10 }, &mut rng);

    let lanes = &world.sea_lanes.lanes;
    assert!(
        !lanes.is_empty(),
        "chart must produce inter-body lanes, got none"
    );

    // Canonical endpoints and sortedness.
    for l in lanes {
        assert!(l.a < l.b, "lane endpoints must be canonical a<b: {l:?}");
    }

    // Exactly the two lanes geometry allows: A↔B (strait) and B↔C (ocean).
    // A and C share no water, so no A↔C lane may appear.
    let pairs: Vec<(Body, Body)> = lanes.iter().map(|l| lane_bodies(&world, l)).collect();
    assert!(
        pairs.contains(&(Body::A, Body::B)),
        "expected an A↔B strait lane, got pairs {pairs:?}"
    );
    assert!(
        pairs.contains(&(Body::B, Body::C)),
        "expected a B↔C ocean lane, got pairs {pairs:?}"
    );
    assert!(
        !pairs.contains(&(Body::A, Body::C)),
        "A and C share no sea — no A↔C lane should exist, got {pairs:?}"
    );

    // The tiers: the strait is crossable, the ocean is a wall.
    let ab = lanes
        .iter()
        .find(|l| lane_bodies(&world, l) == (Body::A, Body::B))
        .unwrap();
    let bc = lanes
        .iter()
        .find(|l| lane_bodies(&world, l) == (Body::B, Body::C))
        .unwrap();
    // Exact gates, not loose tiers: the strait gap is 30 units, the ocean 160,
    // and the fixed curve is cost * (40/100) rounded. 30 → 12, 160 → 64. Asserting
    // the exact values pins the *calibration mechanism*, not just the tier: a
    // per-world quantile (the cut Goodhart trap) or a wrong slope would still land
    // a crossable/wall split here but could never hit 12/64, and a chart() that
    // swapped the tiers fails outright. (Costs are exact integers, so the f32
    // round is unambiguous and native↔wasm-stable.)
    assert_eq!(
        ab.min_naval, 12,
        "the 30-unit strait must gate at exactly round(30 * 0.4) = 12, got {}",
        ab.min_naval
    );
    assert_eq!(
        bc.min_naval, 64,
        "the 160-unit ocean must gate at exactly round(160 * 0.4) = 64, got {}",
        bc.min_naval
    );
    // And the tiers still hold (crossable strait, wall ocean).
    assert!(ab.min_naval <= NAVAL_CEILING && bc.min_naval > NAVAL_CEILING);
}

/// A one-cell-wide strait: `A`(col 0) `| sea`(col 1) `| B`(col 2), 4 rows,
/// spacing 10. Every middle sea cell touches BOTH coasts directly. The sea–sea
/// watershed scan can never see this crossing (the strait has no sea–sea edge
/// between differently-labelled basins — the lone sea column is one body's
/// basin); only the sea→land "pinch" scan catches it. This is the exact case
/// Reviewer 3 showed produced ZERO lanes before the pinch branch existed.
fn one_cell_strait_world() -> WorldData {
    const R: usize = 4;
    const C: usize = 3;
    const S: f32 = 10.0;
    let n = R * C;
    let mut sites = Vec::with_capacity(n);
    let mut neighbors = vec![Vec::new(); n];
    let mut elevation = vec![0.0_f32; n];
    let idx = |r: usize, c: usize| r * C + c;
    for r in 0..R {
        for c in 0..C {
            let i = idx(r, c);
            sites.push([c as f32 * S, r as f32 * S]);
            elevation[i] = if c == 1 { -0.2 } else { 0.5 }; // col 1 is the strait
            if r > 0 {
                neighbors[i].push(idx(r - 1, c) as u32);
            }
            if r + 1 < R {
                neighbors[i].push(idx(r + 1, c) as u32);
            }
            if c > 0 {
                neighbors[i].push(idx(r, c - 1) as u32);
            }
            if c + 1 < C {
                neighbors[i].push(idx(r, c + 1) as u32);
            }
        }
    }
    let mesh = MeshData {
        width: C as f32 * S,
        height: R as f32 * S,
        sites,
        neighbors,
        ..Default::default()
    };
    WorldData {
        mesh,
        terrain: TerrainData {
            elevation,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn one_cell_pinch_strait_forms_a_lane_the_sea_scan_alone_would_miss() {
    let mut world = one_cell_strait_world();
    // Two land bodies (col 0 and col 2), each 4 cells.
    let bodies = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] > 0.0);
    assert_eq!(
        bodies.len(),
        2,
        "expected two landmasses split by the strait"
    );

    let mut rng = mapgen_core::StageRng::new(0).stream(mapgen_core::Stage::SeaLanes);
    sea_lanes::chart(&mut world, SeaLanesParams { min_body_cells: 3 }, &mut rng);

    let lanes = &world.sea_lanes.lanes;
    // Without the pinch (sea→land) scan this is ZERO — the discriminating signal.
    assert_eq!(
        lanes.len(),
        1,
        "the one-cell strait must form exactly one lane (the pinch scan); got {lanes:?}"
    );
    // Pinch cost = coast→sea(10) + sea→coast(10) = 20 ⇒ round(20 * 0.4) = 8.
    assert_eq!(
        lanes[0].min_naval, 8,
        "one-cell strait gap is 20 units ⇒ min_naval round(20 * 0.4) = 8, got {}",
        lanes[0].min_naval
    );
    // Endpoints are coastal land on opposite shores (x = 0 and x = 20).
    let xs = [
        world.mesh.sites[lanes[0].a as usize][0],
        world.mesh.sites[lanes[0].b as usize][0],
    ];
    assert!(
        xs.contains(&0.0) && xs.contains(&20.0),
        "lane must join the two opposite coasts (x=0 and x=20), got {xs:?}"
    );
}

// ── Emergent both-tier substrate (canonical seed) ───────────────────────────

/// Step a planet pipeline through the SeaLanes stage and return the partial
/// world (cultures/history not yet run — SeaLanes precedes them).
fn world_through_sea_lanes(seed: u64) -> WorldData {
    let mut p = Pipeline::new(GenerateParams::planet(seed));
    loop {
        match p.step() {
            Some(PipelineStage::SeaLanes) => break,
            Some(_) => continue,
            None => panic!("pipeline finished before the SeaLanes stage ran"),
        }
    }
    p.into_world()
}

#[test]
fn every_crossing_seed_grows_lanes_spanning_both_tiers() {
    // A correct substrate surfaces BOTH a crossable strait (min_naval <= ceiling)
    // and an open-ocean wall (> ceiling) — not all crossings cheap, not all
    // impossible. Probed across all crossing seeds, the tier split is 11→6/9,
    // 19→4/6, 7→3/5, 4→3/5, so every one carries both. (Sundered seeds
    // legitimately have only walls; asserted absent in sundered_lanes_claims.)
    for &seed in CROSSING_SEEDS {
        let world = world_through_sea_lanes(seed);
        let lanes = &world.sea_lanes.lanes;
        let tiers = || lanes.iter().map(|l| l.min_naval).collect::<Vec<_>>();
        assert!(
            !lanes.is_empty(),
            "crossing seed {seed} must grow inter-body sea lanes"
        );
        assert!(
            lanes.iter().any(|l| l.min_naval <= NAVAL_CEILING),
            "crossing seed {seed} must have a crossable strait (min_naval <= {NAVAL_CEILING}); \
             tiers: {:?}",
            tiers()
        );
        assert!(
            lanes.iter().any(|l| l.min_naval > NAVAL_CEILING),
            "crossing seed {seed} must have an open-ocean wall (min_naval > {NAVAL_CEILING}); \
             tiers: {:?}",
            tiers()
        );
    }
}

#[test]
fn lanes_are_deterministic_for_a_fixed_seed() {
    // The lane graph is on the hashed `WorldData` path; the cross-platform golden
    // (`cross_platform.rs`) exercises seed 42, which is continental and grows no
    // lanes, so it does not cover the f32 cost→`min_naval` path. The path uses
    // only `fmath::sqrt`/`fmath::round` + IEEE-deterministic ops (the same fmath
    // primitives the seed-42 golden already pins cross-platform), and integer
    // `BTreeMap`/`BinaryHeap`/`Vec` ordering — so it is byte-stable. Pin that
    // in-process here: two identical runs must produce identical lanes (a/b/cost/
    // min_naval all compared via the derived `PartialEq`).
    let a = world_through_sea_lanes(19).sea_lanes.lanes;
    let b = world_through_sea_lanes(19).sea_lanes.lanes;
    assert!(!a.is_empty(), "expected lanes to compare");
    assert_eq!(a, b, "sea lanes must be identical for a fixed seed");
}

#[test]
fn only_the_open_ocean_bridges_landmasses_no_inland_pool_does() {
    // The latent "lake bridging" hazard: `sea_lanes` treats EVERY `<= 0.0` cell as
    // navigable, so a sea pool touching two sizable landmasses forges a lane
    // between them — even an enclosed lake, not just the open ocean. This pins
    // that it is NOT live: on every canonical planet seed the only sea component
    // adjacent to ≥2 sizable landmasses is the dominant (largest) one — the ocean.
    // A lake touching one body labels its cells with that body and creates no
    // crossing; a lake touching two would.
    //
    // The algorithmic fix (restrict the navigable mask to the ocean) is
    // DEFERRED on purpose: in this code a strait and a bridging-lake are
    // topologically identical — both are a sea pocket touching two bodies — and
    // the synthetic fixture above models a legit strait as a *disconnected* pool,
    // so excluding non-ocean pools would wrongly kill straits. Telling them apart
    // needs real ocean-connectivity geometry (a larger change). Until then this
    // guard fires the moment a real lake ever bridges two continents.
    let min_body = SeaLanesParams::default().min_body_cells;
    for &seed in CROSSING_SEEDS.iter().chain(SUNDERED_SEEDS.iter()) {
        let world = world_through_sea_lanes(seed);
        let n = world.mesh.cell_count();

        let lands = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] > 0.0);
        let mut body_of = vec![usize::MAX; n];
        let mut bid = 0usize;
        for comp in &lands {
            if comp.len() < min_body {
                continue;
            }
            for &x in comp {
                body_of[x] = bid;
            }
            bid += 1;
        }

        let mut sea = connected_bodies(&world.mesh, |i| world.terrain.elevation[i] <= 0.0);
        sea.sort_by_key(|b| std::cmp::Reverse(b.len())); // dominant ocean first
        for (rank, comp) in sea.iter().enumerate() {
            if rank == 0 {
                continue; // the dominant ocean is allowed (and expected) to bridge
            }
            let mut touched = BTreeSet::new();
            for &s in comp {
                for &nb in &world.mesh.neighbors[s] {
                    let b = body_of[nb as usize];
                    if b != usize::MAX {
                        touched.insert(b);
                    }
                }
            }
            assert!(
                touched.len() < 2,
                "seed {seed}: an inland sea pool ({} cells) borders {} sizable landmasses \
                 {touched:?} — the lake-bridging hazard is now LIVE; sea_lanes must restrict \
                 its navigable mask to the ocean",
                comp.len(),
                touched.len(),
            );
        }
    }
}

//! Phase 7 — nested multi-scale refinement.
//!
//! Generates a *finer* mesh over a sub-rectangle of the world ("a sector") that
//! reproduces the parent's large-scale structure and adds sub-parent detail, on
//! demand and statelessly. This is the keystone of the multi-scale atlas (see
//! `docs/adr/0001-multiscale-navigation.md`).
//!
//! ## How it stays consistent with the parent
//!
//! The child's elevation **anchors to the parent's FINAL eroded field** —
//! [`parent_anchor_elevation`] interpolates it at each child site (k-nearest
//! inverse-distance over the surrounding parent cells), so the content the user
//! was just looking at is exactly what gets refined: same coastlines, same
//! mountain belts, same valleys (which therefore align with the PROJECTED parent
//! rivers). The refine path runs NO erosion of its own — the parent's erosion is
//! already in the anchor, and re-deriving terrain (the old model: recompute
//! plates+noise, re-erode) is what made zooming rewrite the map. The coarsening
//! contract — the child reproduces the parent at the parent's own sample points —
//! is pinned tight in `tests/scale_spec.rs` (land/sea > 96%, MAD < 0.05, plus a
//! planet-path biome test).
//!
//! Sub-parent **detail** (extra high-frequency, zero-mean noise octaves) is drawn
//! from the **sector** seed ([`StageRng::sector`]), so it is reproducible per
//! (level, sx, sy) yet distinct per sector, and averages away under coarsening.
//!
//! ## Halo + seams
//!
//! Hydrology / climate are neighbour-coupled, so a bare sector would show edge
//! artifacts. We build the mesh over the sector grown by a halo margin and run
//! the stages over the whole thing; the output `mesh.region` is the *true*
//! sector, so the renderer's viewport clips the halo away. Seam *consistency*
//! between adjacent sectors is explicit: [`pin_edges_to_shared`] fades each
//! sector's own detail noise out toward the (parent-derived, hence shared)
//! anchor at the edges, and `pin_climate_to_parent` does the same for the
//! climate fields before biomes classify — so two neighbours agree along their
//! seam, with each other AND with the parent's coarse render.
//!
//! ## Scope (v1)
//!
//! Refines the *physical* map: anchored terrain → hydrology → ocean → climate →
//! biomes (+ soils). Two layers are **projected from the parent** rather than
//! re-rolled, so a drill-in shows *this* world (not a different one) and stays
//! seam-consistent (both neighbours sample the same global parent): society
//! ([`project_society`] — towns, borders, roads) and hydrology
//! ([`project_hydrology`] — the river network + flow, which a sector can't
//! compute correctly anyway since the upstream catchment is global). History
//! stays at world scale on the parent (a sector has no chronicle of its own).

use mapgen_core::{
    world_data::{CulturesData, Road, SocietyData},
    Stage, StageRng, WorldData, WorldMeta,
};
use mapgen_geom::{Mesh, RegionMeshParams};

use crate::{
    biomes, climate::ClimateParams, climate_seasonal, hydrology, noise, ocean, plates,
    GenerateParams,
};

/// A node in the world's quadtree. Level 0 is the whole world (one sector);
/// each level doubles the subdivision per axis. `(sx, sy)` are absolute
/// coordinates at that level, in `0..2^level`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Sector {
    pub level: u32,
    pub sx: u32,
    pub sy: u32,
}

impl Sector {
    /// The whole world.
    pub const ROOT: Sector = Sector {
        level: 0,
        sx: 0,
        sy: 0,
    };

    /// Sectors per axis at this level (`2^level`).
    pub fn span(self) -> u32 {
        1u32 << self.level
    }

    /// True if `(sx, sy)` are in range for the level.
    pub fn is_valid(self) -> bool {
        self.sx < self.span() && self.sy < self.span()
    }

    /// World-space rectangle `[x0, y0, x1, y1]` for full-world dims `(w, h)`.
    pub fn rect(self, w: f32, h: f32) -> [f32; 4] {
        let s = self.span() as f32;
        let cw = w / s;
        let ch = h / s;
        [
            self.sx as f32 * cw,
            self.sy as f32 * ch,
            (self.sx + 1) as f32 * cw,
            (self.sy + 1) as f32 * ch,
        ]
    }

    /// The child sub-sector at quadrant `(dx, dy)`, each in `{0, 1}`.
    pub fn child(self, dx: u32, dy: u32) -> Sector {
        Sector {
            level: self.level + 1,
            sx: self.sx * 2 + dx,
            sy: self.sy * 2 + dy,
        }
    }
}

/// Tunables for one refinement pass.
#[derive(Clone, Debug)]
pub struct RefineParams {
    /// Target cell count inside the sector proper (excludes the halo). Picking
    /// this similar to the parent's whole-world count makes each level ~4×
    /// denser per axis-halving.
    pub target_cells: usize,
    /// Halo margin added on each side, as a fraction of the sector's size.
    pub halo_fraction: f32,
    /// Extra fBM octaves of sub-parent detail to inject (0 = base field only).
    pub detail_octaves: usize,
}

impl Default for RefineParams {
    fn default() -> Self {
        Self {
            target_cells: 4_000,
            halo_fraction: 0.25,
            detail_octaves: 3,
        }
    }
}

fn grow(rect: [f32; 4], frac: f32, w: f32, h: f32) -> [f32; 4] {
    let [x0, y0, x1, y1] = rect;
    let mx = (x1 - x0) * frac;
    let my = (y1 - y0) * frac;
    [
        (x0 - mx).max(0.0),
        (y0 - my).max(0.0),
        (x1 + mx).min(w),
        (y1 + my).min(h),
    ]
}

fn area(rect: [f32; 4]) -> f32 {
    ((rect[2] - rect[0]) * (rect[3] - rect[1])).max(1.0)
}

/// Refine `sector` of `parent` (the root, level-0 world), returning a renderable
/// [`WorldData`] confined to that sector. The shared base field is recomputed
/// from the root's seed + world dims + plate count — all recovered from `parent`
/// (plate count = its plate-roster length) — and the parent's society is
/// projected onto the sector (see [`project_society`]). Pure and deterministic in
/// `(parent, sector, refine)`; see the module docs.
pub fn refine_sector(parent: &WorldData, sector: Sector, refine: RefineParams) -> WorldData {
    assert!(
        sector.is_valid(),
        "sector {sector:?} out of range for its level"
    );

    let params = GenerateParams {
        seed: parent.meta.seed,
        width: parent.mesh.width,
        height: parent.mesh.height,
        cell_count: 0, // unused — the sector's density is `refine.target_cells`
        plate_count: parent.terrain.plates.len(),
        nation_count: 0, // unused — polities come from the projected society
        periodic: false, // a drilled sector is a continental (non-wrapping) view, even of a planet
    };

    let root_rng = StageRng::new(params.seed);
    let sec_rng = root_rng.sector(sector.level, sector.sx, sector.sy);

    let rect = sector.rect(params.width, params.height);
    let halo = grow(rect, refine.halo_fraction, params.width, params.height);

    // 1. Finer mesh over the haloed sub-rectangle. Layout uses the sector's own
    // Mesh stream (it need not align cell-for-cell with the parent). Cell count
    // scales with the haloed area so the *sector* lands near `target_cells`.
    let cells = (refine.target_cells as f32 * area(halo) / area(rect)).round() as usize;
    let mesh = Mesh::build_region(
        RegionMeshParams {
            full_width: params.width,
            full_height: params.height,
            region: halo,
            target_cells: cells.max(64),
            lloyd_iterations: 2,
        },
        &mut sec_rng.stream(Stage::Mesh),
    );
    let mut mesh_data = mesh.into_mesh_data();
    // Render viewport is the *true* sector; the halo sits outside it.
    mesh_data.region = Some(rect);

    // 2. Plate field from the ROOT stream → identical plate set to the parent,
    // evaluated on the finer sub-mesh.
    let terrain = plates::generate(
        &mesh_data,
        params.plate_count,
        &mut root_rng.stream(Stage::Plates),
    );

    let mut world = WorldData {
        meta: WorldMeta::new(params.seed),
        mesh: mesh_data,
        terrain,
        ..Default::default()
    };

    // The anchor, the climate seam-pin, and the projections all sample parent
    // layers by "nearest parent cell" — compute that mapping once and share it
    // (the nearest search dominated the projection cost).
    //
    // PERIODIC parent (the planet): parent sampling uses the minimum-image x and
    // an x-UNCLAMPED sampling halo, so a sector at the antimeridian sees the
    // parents across the wrap — its seam edge then pins to the same parent
    // content the wrap-adjacent sector pins to, and the drilled detail is as
    // seamless as the base globe. (The MESH halo stays clamped to the world —
    // only the parent SAMPLING wraps.) Flat parents are byte-identical.
    let period = parent.mesh.periodic.then_some(parent.mesh.width);
    let sample_halo = if period.is_some() {
        let mx = (rect[2] - rect[0]) * refine.halo_fraction;
        let my = (rect[3] - rect[1]) * refine.halo_fraction;
        [
            rect[0] - mx,
            (rect[1] - my).max(0.0),
            rect[2] + mx,
            (rect[3] + my).min(params.height),
        ]
    } else {
        halo
    };
    let nearest_parent = nearest_parent_cells(parent, &world.mesh.sites, sample_halo, period);

    // 3. CROSS-LEVEL ANCHOR (the coarsening contract made literal): the child's
    // elevation REFINES the parent's FINAL eroded field — the content the user
    // was just looking at — rather than re-deriving its own. The old path
    // rebuilt plates+noise (the PRE-erosion base) and ran its OWN erosion, so
    // zooming in rewrote the map: measured on the globe e2e world, only 55–85%
    // of cells kept their land/sea sign and 34–58% their biome across a drill.
    // It also carved fictional valleys that didn't align with the PROJECTED
    // parent rivers drawn on top. Now: sample the parent's elevation at each
    // child site (k=4 inverse-distance interpolation over the surrounding parent
    // cells — continuous at every drill depth) and let the sector add only
    // zero-mean fine detail below. Child EROSION IS GONE — the parent's erosion
    // is already in the anchor (re-eroding was the drift). The anchor is also
    // the seam reference: adjacent sectors sample the SAME parent, so they agree
    // with each other AND with the coarse base render at every boundary.
    if let Some(anchor) = parent_anchor_elevation(parent, &world, sample_halo, period) {
        world.terrain.elevation = anchor;
    } else {
        // Physical-only parent (no usable elevation field): fall back to the
        // recomputed plates field + root noise so the sector is still terrain.
        noise::overlay(
            &mut world,
            noise::NoiseParams::default(),
            &mut root_rng.stream(Stage::Noise),
        );
    }
    let shared_elev = world.terrain.elevation.clone();

    // 4. Sub-parent detail from the SECTOR stream: extra octaves at the
    // refinement frequency. Zero-mean, so it averages away under coarsening.
    if refine.detail_octaves > 0 {
        let factor = sector.span() as f64;
        let base = noise::NoiseParams::default();
        noise::overlay(
            &mut world,
            noise::NoiseParams {
                amplitude: 0.12,
                frequency: base.frequency * factor,
                octaves: refine.detail_octaves,
                warp_amount: base.warp_amount / factor,
                warp_frequency: base.warp_frequency * factor,
            },
            &mut sec_rng.stream(Stage::Noise),
        );
    }

    // Seam-pinning: fade the sector's own detail noise out toward the anchor as
    // cells approach the sector edges, so two independently-generated neighbours
    // (whose detail noise comes from DIFFERENT sector streams) agree along their
    // shared boundary — and both agree with the parent's render there. Runs
    // before hydrology so coast is derived from the pinned terrain.
    pin_edges_to_shared(&mut world, rect, &shared_elev);

    hydrology::detect_coast(&mut world);
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);
    hydrology::extract_rivers(&mut world, &flow_dir, 0.05);
    ocean::run(&mut world);
    climate_seasonal::run(&mut world, ClimateParams::default());

    // Climate seam-pinning: elevation is pinned to the shared base above, but the
    // climate fields are marched per-sector on the sector's OWN mesh (upwind
    // moisture transport), so two adjacent sectors disagree slightly along their
    // shared edge — and BIOMES quantize that disagreement into discrete class
    // swaps along a perfectly straight line (the visible "nature unnaturally
    // shifts" seam). Blend each climate field toward the PARENT's value (nearest
    // parent cell — both neighbours sample the same parent) with the same
    // edge-distance smoothstep elevation uses, BEFORE biomes classify, so the two
    // sides of a seam classify from near-identical inputs at the edge.
    pin_climate_to_parent(&mut world, parent, rect, &nearest_parent);
    biomes::classify(&mut world);

    // Project the parent's river network onto the sector. Rivers are derived
    // from flow accumulation, which needs the *global* upstream catchment — a
    // sector only sees its own cells, so its rivers can't be globally correct or
    // seam-consistent. The parent's network is both; projecting it (like society)
    // gives correct drainage and makes a river cross a sector seam identically on
    // both sides. Runs after biomes (which used the sector's own flow), so this
    // only re-bases what the renderer draws.
    project_hydrology(parent, &mut world, sample_halo, period, &nearest_parent);

    // Project the parent's society onto the sector — the same towns, borders,
    // and roads, remapped to the finer mesh. Society is generated once at world
    // scale and never re-rolled per sector, so drilling in shows *this* world's
    // cities, not different ones.
    project_society(
        parent,
        &mut world,
        rect,
        sample_halo,
        period,
        &nearest_parent,
    );

    world
}

/// For each sector cell, the index of the nearest *parent* cell within the
/// haloed rect (`None` if no parent cell falls in the halo). Shared by the
/// hydrology and society projections, which both resample parent per-cell layers
/// onto the finer mesh — this nearest search dominated their cost, so it is done
/// once here. `O(sector_cells × parent_cells_in_halo)`; the halo restriction
/// keeps the parent set small.
///
/// `period` is `Some(width)` for a PERIODIC parent (the planet): both the halo
/// membership and the distances use the minimum-image x (`plates::wrap_dx`), so a
/// sector at the antimeridian samples the parents ACROSS the wrap — without this,
/// the two wrap-adjacent sector columns each saw one-sided context and met at the
/// seam with mismatched content (a hard vertical line in drilled detail, the
/// drilled twin of the base-globe seam the periodic arc removed). Flat parents
/// (`None`) are byte-identical to before.
fn nearest_parent_cells(
    parent: &WorldData,
    sites: &[[f32; 2]],
    halo: [f32; 4],
    period: Option<f32>,
) -> Vec<Option<u32>> {
    let parent_in_halo: Vec<u32> = (0..parent.mesh.cell_count() as u32)
        .filter(|&i| in_rect_p(parent.mesh.sites[i as usize], halo, period))
        .collect();
    sites
        .iter()
        .map(|&p| {
            let mut best: Option<u32> = None;
            let mut bd = f32::MAX;
            for &pc in &parent_in_halo {
                let s = parent.mesh.sites[pc as usize];
                let d = dist2_p(s, p, period);
                if d < bd {
                    bd = d;
                    best = Some(pc);
                }
            }
            best
        })
        .collect()
}

fn in_rect(p: [f32; 2], r: [f32; 4]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}

/// `in_rect` with minimum-image x for a periodic world: the rect's x-range may
/// conceptually extend past `[0, width]` (an unclamped sampling halo at the
/// antimeridian), so a point is inside if `x`, `x+width`, or `x−width` lands in
/// it. `None` → the flat test, byte-identical.
fn in_rect_p(p: [f32; 2], r: [f32; 4], period: Option<f32>) -> bool {
    match period {
        None => in_rect(p, r),
        Some(w) => {
            (p[1] >= r[1] && p[1] < r[3])
                && ((p[0] >= r[0] && p[0] < r[2])
                    || (p[0] + w >= r[0] && p[0] + w < r[2])
                    || (p[0] - w >= r[0] && p[0] - w < r[2]))
        }
    }
}

/// Squared distance with minimum-image x for a periodic world (`None` → flat).
fn dist2_p(a: [f32; 2], b: [f32; 2], period: Option<f32>) -> f32 {
    let dx = match period {
        Some(w) => crate::plates::wrap_dx(a[0] - b[0], w),
        None => a[0] - b[0],
    };
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

/// Seam-pinning (Phase 7): blend each cell's elevation back toward the shared
/// base field `shared` as it approaches the sector boundary, so adjacent sectors
/// — which recompute the identical shared field — agree along their seam. The
/// blend weight ramps via smoothstep from 0 at the rectangle edge (fully shared)
/// to 1 a short band inside (the sector's own eroded + detailed terrain). Cells
/// in the halo (outside the rectangle) stay fully shared.
fn pin_edges_to_shared(world: &mut WorldData, rect: [f32; 4], shared: &[f32]) {
    let [x0, y0, x1, y1] = rect;
    let blend = ((x1 - x0).min(y1 - y0) * 0.12).max(1.0);
    let sites = &world.mesh.sites;
    let elev = &mut world.terrain.elevation;
    let n = elev.len().min(shared.len()).min(sites.len());
    for i in 0..n {
        let p = sites[i];
        // Distance *inside* the rectangle to the nearest edge (≤ 0 in the halo).
        let d_in = (p[0] - x0).min(x1 - p[0]).min(p[1] - y0).min(y1 - p[1]);
        let t = (d_in / blend).clamp(0.0, 1.0);
        let w = t * t * (3.0 - 2.0 * t); // smoothstep
        elev[i] = shared[i] + (elev[i] - shared[i]) * w;
    }
}

/// The cross-level elevation anchor: the parent's FINAL (eroded) elevation
/// interpolated at each child site by INVERSE-DISTANCE WEIGHTING over the k=4
/// nearest parent cells. IDW gives continuous gradients INSIDE a parent cell, so
/// the anchor stays geographic at every drill depth — a nearest-cell sample
/// degrades to a CONSTANT once the sector is smaller than one parent cell
/// (L5–L6), turning coasts into anchorless noise speckle. The parent search
/// window is the halo grown by two parent-cell spacings, so even a deepest-drill
/// sector (smaller than a single parent cell) always sees its surrounding
/// parents — there is no "no parent in halo" failure depth. `None` only for a
/// physical-only parent (no elevation to anchor to). Pure f32 add/mul/div (+
/// IEEE-exact sqrt for the spacing) in deterministic mesh order —
/// native↔wasm byte-identical.
fn parent_anchor_elevation(
    parent: &WorldData,
    world: &WorldData,
    halo: [f32; 4],
    period: Option<f32>,
) -> Option<Vec<f32>> {
    let pn = parent.mesh.cell_count();
    if parent.terrain.elevation.len() != pn || pn == 0 {
        return None;
    }
    let spacing = (parent.mesh.width * parent.mesh.height / pn as f32).sqrt();
    let g = 2.0 * spacing;
    let search = [halo[0] - g, halo[1] - g, halo[2] + g, halo[3] + g];
    let cand: Vec<u32> = (0..pn as u32)
        .filter(|&i| in_rect_p(parent.mesh.sites[i as usize], search, period))
        .collect();
    if cand.is_empty() {
        return None;
    }
    const K: usize = 4;
    const EPS: f32 = 1e-6;
    let anchor = world
        .mesh
        .sites
        .iter()
        .map(|&p| {
            // k smallest squared distances (insertion sort into a fixed array —
            // deterministic tie-break by candidate order).
            let mut best: [(f32, u32); K] = [(f32::MAX, u32::MAX); K];
            for &pc in &cand {
                let s = parent.mesh.sites[pc as usize];
                let d = dist2_p(s, p, period);
                if d < best[K - 1].0 {
                    let mut k = K - 1;
                    while k > 0 && d < best[k - 1].0 {
                        best[k] = best[k - 1];
                        k -= 1;
                    }
                    best[k] = (d, pc);
                }
            }
            let (mut num, mut den) = (0.0f32, 0.0f32);
            for &(d, pc) in &best {
                if pc == u32::MAX {
                    continue;
                }
                let w = 1.0 / (d + EPS);
                num += parent.terrain.elevation[pc as usize] * w;
                den += w;
            }
            num / den
        })
        .collect();
    Some(anchor)
}

/// Climate seam-pinning (the biome half of `pin_edges_to_shared`): blend every
/// climate field toward the PARENT's value (nearest parent cell) as cells
/// approach the sector boundary, with the same smoothstep band elevation uses.
/// Two adjacent sectors sample the SAME parent cells along their shared edge, so
/// their climate — and therefore the quantized biome classification — agrees at
/// the seam instead of swapping classes along a straight line. The interior
/// (band inward) keeps the sector's own marched climate untouched. A
/// physical-only parent (no climate) is a no-op. Pure f32 mul/add — no RNG, no
/// transcendentals — so the refine path stays native↔wasm byte-identical.
fn pin_climate_to_parent(
    world: &mut WorldData,
    parent: &WorldData,
    rect: [f32; 4],
    nearest_parent: &[Option<u32>],
) {
    let [x0, y0, x1, y1] = rect;
    let blend = ((x1 - x0).min(y1 - y0) * 0.12).max(1.0);
    let n = world.mesh.cell_count();
    // (sector field, parent field) pairs — seasonal fields are present on a full
    // parent; each pair is skipped independently if either side is missing.
    let pairs: [(&mut Vec<f32>, &Vec<f32>); 6] = [
        (&mut world.climate.temperature, &parent.climate.temperature),
        (
            &mut world.climate.precipitation,
            &parent.climate.precipitation,
        ),
        (
            &mut world.climate.temperature_summer,
            &parent.climate.temperature_summer,
        ),
        (
            &mut world.climate.temperature_winter,
            &parent.climate.temperature_winter,
        ),
        (
            &mut world.climate.precipitation_summer,
            &parent.climate.precipitation_summer,
        ),
        (
            &mut world.climate.precipitation_winter,
            &parent.climate.precipitation_winter,
        ),
    ];
    for (field, parent_field) in pairs {
        if field.len() != n || parent_field.len() != parent.mesh.cell_count() {
            continue;
        }
        for i in 0..n {
            let Some(pi) = nearest_parent[i] else {
                continue;
            };
            let target = parent_field[pi as usize];
            let p = world.mesh.sites[i];
            let d_in = (p[0] - x0).min(x1 - p[0]).min(p[1] - y0).min(y1 - p[1]);
            let t = (d_in / blend).clamp(0.0, 1.0);
            let w = t * t * (3.0 - 2.0 * t); // smoothstep
            field[i] = target + (field[i] - target) * w;
        }
    }
}

/// Project the root world's hydrology onto a refined sector: per-cell `flow` and
/// `strahler` are sampled from the nearest parent cell, and the parent's rivers
/// and lakes are remapped (their in-halo cells → nearest sector cell), carrying
/// width / name / order / regime. Because the parent network is global and both
/// neighbours sample the *same* parent, a river crosses their shared seam
/// identically. A physical-only parent (no hydrology) is a no-op.
fn project_hydrology(
    parent: &WorldData,
    world: &mut WorldData,
    halo: [f32; 4],
    period: Option<f32>,
    nearest_parent: &[Option<u32>],
) {
    if parent.hydrology.flow.len() != parent.mesh.cell_count() {
        return;
    }
    let psite = |c: u32| parent.mesh.sites[c as usize];
    let in_halo = |c: u32| in_rect_p(psite(c), halo, period);
    let n = world.mesh.cell_count();

    // Per-cell flow + Strahler order, sampled from the nearest parent cell
    // (shared `nearest_parent` map) — drives river width / classing.
    let mut flow = vec![0.0_f32; n];
    let has_strahler = parent.hydrology.strahler.len() == parent.mesh.cell_count();
    let mut strahler = if has_strahler {
        vec![0u8; n]
    } else {
        Vec::new()
    };
    for (i, &maybe_pc) in nearest_parent.iter().enumerate() {
        if let Some(pc) = maybe_pc {
            flow[i] = parent.hydrology.flow[pc as usize];
            if has_strahler {
                strahler[i] = parent.hydrology.strahler[pc as usize];
            }
        }
    }

    // Remap parent rivers/lakes — keep each chain's in-halo cells, snapped to the
    // nearest sector cell.
    let sites = world.mesh.sites.clone();
    let nearest_sector = |p: [f32; 2]| -> u32 {
        let mut best = 0u32;
        let mut bd = f32::MAX;
        for (i, s) in sites.iter().enumerate() {
            let d = (s[0] - p[0]).powi(2) + (s[1] - p[1]).powi(2);
            if d < bd {
                bd = d;
                best = i as u32;
            }
        }
        best
    };
    let rivers = parent
        .hydrology
        .rivers
        .iter()
        .filter_map(|r| {
            let cells: Vec<u32> = r
                .cells
                .iter()
                .copied()
                .filter(|&c| in_halo(c))
                .map(|c| nearest_sector(psite(c)))
                .collect();
            if cells.len() < 2 {
                return None;
            }
            let mut r2 = r.clone();
            r2.cells = cells;
            Some(r2)
        })
        .collect();
    let lakes = parent
        .hydrology
        .lakes
        .iter()
        .filter_map(|l| {
            let cells: Vec<u32> = l
                .cells
                .iter()
                .copied()
                .filter(|&c| in_halo(c))
                .map(|c| nearest_sector(psite(c)))
                .collect();
            if cells.is_empty() {
                return None;
            }
            let mut l2 = l.clone();
            l2.cells = cells;
            Some(l2)
        })
        .collect();

    world.hydrology.flow = flow;
    if has_strahler {
        world.hydrology.strahler = strahler;
    }
    world.hydrology.rivers = rivers;
    world.hydrology.lakes = lakes;
}

/// Carry the root world's society into a refined sector. Settlements/roads in
/// view are kept and their cell indices remapped to the nearest sector cell;
/// per-cell `control` and `culture_id` are sampled from the nearest parent cell
/// (restricted to the haloed rect, which keeps this cheap); the polity and
/// culture rosters are copied verbatim. A physical-only parent (no society) is a
/// no-op.
fn project_society(
    parent: &WorldData,
    world: &mut WorldData,
    rect: [f32; 4],
    halo: [f32; 4],
    period: Option<f32>,
    nearest_parent: &[Option<u32>],
) {
    if parent.society.nations.is_empty() && parent.society.settlements.is_empty() {
        return;
    }
    let psite = |c: u32| parent.mesh.sites[c as usize];
    let sites = world.mesh.sites.clone();
    let n = sites.len();

    // Nearest sector cell to a world point — used for the handful of settlement
    // / capital / road cells, so brute force is fine.
    let nearest_sector = |p: [f32; 2]| -> u32 {
        let mut best = 0u32;
        let mut bd = f32::MAX;
        for (i, s) in sites.iter().enumerate() {
            let d = (s[0] - p[0]).powi(2) + (s[1] - p[1]).powi(2);
            if d < bd {
                bd = d;
                best = i as u32;
            }
        }
        best
    };

    // Per-cell controlling polity + culture, sampled from the nearest parent
    // cell (shared `nearest_parent` map).
    let mut control = vec![None; n];
    let has_cultures = parent.cultures.culture_id.len() == parent.mesh.cell_count();
    let mut culture_id = if has_cultures {
        vec![None; n]
    } else {
        Vec::new()
    };
    for (i, &maybe_pc) in nearest_parent.iter().enumerate() {
        if let Some(pc) = maybe_pc {
            control[i] = parent.society.control.get(pc as usize).copied().flatten();
            if has_cultures {
                culture_id[i] = parent.cultures.culture_id[pc as usize];
            }
        }
    }

    let settlements = parent
        .society
        .settlements
        .iter()
        .filter(|s| in_rect(psite(s.cell), rect))
        .map(|s| {
            let mut s2 = s.clone();
            s2.cell = nearest_sector(psite(s.cell));
            s2
        })
        .collect();

    // Keep all polities (so settlement/control `polity_id`s resolve); remap each
    // capital to the nearest sector cell for the founding-culture glyph lookup.
    let nations = parent
        .society
        .nations
        .iter()
        .map(|nn| {
            let mut n2 = nn.clone();
            n2.capital_cell = nearest_sector(psite(nn.capital_cell));
            n2
        })
        .collect();

    let roads = parent
        .society
        .roads
        .iter()
        .filter(|r| r.cells.iter().any(|&c| in_rect_p(psite(c), halo, period)))
        .map(|r| Road {
            cells: r.cells.iter().map(|&c| nearest_sector(psite(c))).collect(),
        })
        .collect();

    world.society = SocietyData {
        nations,
        settlements,
        roads,
        control,
    };
    if has_cultures {
        world.cultures = CulturesData {
            cultures: parent.cultures.cultures.clone(),
            culture_id,
        };
    }
}

//! Phase 7 — nested multi-scale refinement.
//!
//! Generates a *finer* mesh over a sub-rectangle of the world ("a sector") that
//! reproduces the parent's large-scale structure and adds sub-parent detail, on
//! demand and statelessly. This is the keystone of the multi-scale atlas (see
//! `docs/adr/0001-multiscale-navigation.md`).
//!
//! ## How it stays consistent with the parent
//!
//! The base elevation field is *position-deterministic*: plate centres are
//! scattered in world coordinates from the **root** `Stage::Plates` stream, and
//! the noise overlay is a pure function of world position from the **root**
//! `Stage::Noise` stream. A sector recomputes exactly those fields, just sampled
//! on a denser mesh confined to the sub-rectangle — so the coarse shape (coast
//! lines, plate uplift, mountain belts) matches the parent by construction. The
//! coarsening contract — averaging the child back down reproduces the parent —
//! is tested in `tests/scale_spec.rs` on this base field.
//!
//! Sub-parent **detail** (extra high-frequency noise octaves) and the stateful
//! stages' randomness (erosion, etc.) are drawn from the **sector** seed
//! ([`StageRng::sector`]), so they are reproducible per (level, sx, sy) yet
//! distinct per sector. Because the added octaves are zero-mean, they average
//! away under coarsening — the contract still holds.
//!
//! ## Halo
//!
//! Erosion / hydrology / climate are neighbour-coupled, so a bare sector would
//! show edge artifacts (rivers with nowhere to flow, clipped erosion). We build
//! the mesh over the sector grown by a halo margin and run the stages over the
//! whole thing; the output `mesh.region` is the *true* sector, so the renderer's
//! viewport clips the halo away. The halo is context, not a pinned boundary —
//! adjacent sectors therefore agree only approximately (they share the same base
//! field); exact seam-pinning across the stateful stages is future work (it
//! needs per-stage Dirichlet boundary masks). The drill-in use case views one
//! sector at a time, so approximate seams are not yet exercised.
//!
//! ## Scope (v1)
//!
//! Refines the *physical* map: terrain → erosion → hydrology → ocean → climate →
//! biomes (+ soils + rivers/Strahler/regime, which the stages carry). Society
//! (cultures / polities / history) stays at world scale on the parent; per-sector
//! settlements and roads are a follow-up.

use mapgen_core::{Stage, StageRng, WorldData, WorldMeta};
use mapgen_geom::{Mesh, RegionMeshParams};

use crate::{
    biomes, climate::ClimateParams, climate_seasonal, erosion, hydrology, noise, ocean, plates,
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

/// Refine `sector` of the world described by root `params`, returning a
/// renderable [`WorldData`] confined to that sector. `params` must be the same
/// `GenerateParams` the parent/root world was built with (seed + world dims +
/// plate count drive the shared base field). Pure and deterministic in
/// `(params, sector, refine)` — see the module docs.
pub fn refine_sector(params: GenerateParams, sector: Sector, refine: RefineParams) -> WorldData {
    assert!(
        sector.is_valid(),
        "sector {sector:?} out of range for its level"
    );

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

    // 3. Base noise from the ROOT stream → identical low-frequency field.
    noise::overlay(
        &mut world,
        noise::NoiseParams::default(),
        &mut root_rng.stream(Stage::Noise),
    );

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

    // 5. Physical pipeline over the sub-mesh (sector streams for the stateful
    // stages). Mirrors `generate_full`'s stage order; `mesh.width/height` stay
    // full-world so plate scatter and climate latitude remain global.
    erosion::run(
        &mut world,
        erosion::ErosionParams::default(),
        &mut sec_rng.stream(Stage::Erosion),
    );
    hydrology::detect_coast(&mut world);
    hydrology::fill_depressions(&mut world);
    let flow_dir = hydrology::flow_directions(&world);
    hydrology::accumulate_flow(&mut world, &flow_dir);
    hydrology::extract_rivers(&mut world, &flow_dir, 0.05);
    ocean::run(&mut world);
    climate_seasonal::run(&mut world, ClimateParams::default());
    biomes::classify(&mut world);

    world
}

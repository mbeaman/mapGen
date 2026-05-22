//! Phase 3e ornate-antique render. The "exotic map shop" aesthetic.
//!
//! MVP scope (what this commit ships):
//! * Parchment background.
//! * Multi-offset coastline ripples (jittered via deterministic per-
//!   coast-vertex noise — no `roughr` dep yet; closer to Imhof
//!   shaded-relief feel than to a true hand-drawn pen).
//! * Mountain glyphs: triangular icons on ALPINE / SNOW cells, scaled
//!   by elevation.
//! * Forest scatter: small tree-tuft triangles on TEMPERATE_FOREST /
//!   TEMPERATE_RAINFOREST / TAIGA / TROPICAL_RAINFOREST cells,
//!   positioned deterministically inside each cell polygon.
//! * Settlement glyphs: capitals are larger filled-square-with-crown
//!   marks colored by polity; towns are smaller circles.
//! * Roads: thin russet polylines along road-cell sequences.
//! * Sacred sites: small radiant marks layered above settlements.
//!
//! Deferred to follow-up commits (per the architecture's Phase 3e
//! sub-deliverables in `docs/TASKS.md`):
//! * `roughr` pen-jitter (real hand-drawn primitives — needs API probe).
//! * `<defs>`-embedded Cinzel / IM Fell English / EB Garamond fonts.
//! * Compass rose, corner cartouche, vignette, edge burn.
//! * Imhof-style label placement (settlement & polity labels).
//! * `Culture.settlement × Culture.architecture` glyph-shape derivation
//!   (today the same tier-based shapes apply to every culture).
//!
//! All randomness is derived from cell IDs and vertex indices — no RNG
//! is threaded through this module, so the output is byte-identical
//! across two runs of the same world.

use std::fmt::Write;

use mapgen_core::entities::SettlementTier;
use mapgen_core::WorldData;

pub fn render(world: &WorldData) -> String {
    let mesh = &world.mesh;
    let w = mesh.width;
    let h = mesh.height;

    let mut out = String::with_capacity(mesh.cell_count() * 128);
    write!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0} {h:.0}" width="{w:.0}" height="{h:.0}">"##
    )
    .unwrap();

    // Aged parchment with a subtle radial gradient for vignette-lite.
    out.push_str(
        r##"<defs>
<radialGradient id="parchment" cx="50%" cy="50%" r="75%">
<stop offset="0%" stop-color="#f0e3bf"/>
<stop offset="70%" stop-color="#dfca96"/>
<stop offset="100%" stop-color="#a88550"/>
</radialGradient>
</defs>"##,
    );
    out.push_str(r##"<rect width="100%" height="100%" fill="url(#parchment)"/>"##);

    render_land_fill(world, &mut out);
    render_coastline_ripples(world, &mut out);
    render_rivers(world, &mut out);
    render_mountains(world, &mut out);
    render_forest_scatter(world, &mut out);
    render_roads(world, &mut out);
    render_settlements(world, &mut out);
    render_sacred_sites(world, &mut out);

    out.push_str("</svg>");
    out
}

/// Soft biome-tinted fill, muted with parchment so the map reads as
/// "antique scroll" rather than "satellite photograph."
fn render_land_fill(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let biomes = &world.climate.biome;
    let elev = &world.terrain.elevation;
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() {
            continue;
        }
        let cell_elev = elev.get(i).copied().unwrap_or(0.0);
        let biome = biomes.get(i).copied().unwrap_or(0);
        let fill = ornate_biome_color(biome, cell_elev);
        out.push_str(r##"<polygon points=""##);
        for (k, &vi) in verts.iter().enumerate() {
            let v = mesh.vertices[vi as usize];
            if k > 0 {
                out.push(' ');
            }
            write!(out, "{:.1},{:.1}", v[0], v[1]).unwrap();
        }
        write!(out, r##"" fill="{fill}" fill-opacity="0.75"/>"##).unwrap();
    }
}

/// Three offset coastline strokes — outer (thick, faded), middle, inner
/// (crisp). Each offset's points are nudged inward (toward the cell
/// center on the land side) using a deterministic hash of the vertex
/// pair so the offsets look hand-drawn without an RNG.
fn render_coastline_ripples(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;

    // Outer-most ripple: thick, very faded blue, suggesting deep
    // water dropping off from coast. Inner ripples crisper.
    let ripples: &[(&str, f32, f32)] = &[
        ("#5a6a8a", 4.0, 0.18), // outermost, far offset
        ("#7a8aa8", 2.5, 0.10),
        ("#2a2418", 1.4, 0.0), // innermost: dark coastline outline
    ];

    for (color, width, offset) in ripples.iter().copied() {
        write!(
            out,
            r##"<g stroke="{color}" stroke-width="{width:.2}" stroke-linecap="round" fill="none" stroke-opacity="0.75">"##
        )
        .unwrap();
        for i in 0..mesh.cell_count() {
            let i_land = elev[i] > 0.0;
            for &nj in &mesh.neighbors[i] {
                let j = nj as usize;
                if j <= i {
                    continue;
                }
                let j_land = elev[j] > 0.0;
                if i_land == j_land {
                    continue;
                }
                let Some((va, vb)) = shared_edge(&mesh.cell_vertices[i], &mesh.cell_vertices[j])
                else {
                    continue;
                };
                let mut a = mesh.vertices[va as usize];
                let mut b = mesh.vertices[vb as usize];
                if offset > 0.0 {
                    // Push the line away from the land-side centroid.
                    let land_cell = if i_land { i } else { j };
                    let land_center = mesh.sites[land_cell];
                    let nx = (a[0] + b[0]) * 0.5 - land_center[0];
                    let ny = (a[1] + b[1]) * 0.5 - land_center[1];
                    let len = (nx * nx + ny * ny).sqrt().max(1e-4);
                    // Per-edge wobble so the ripple isn't a perfect copy.
                    let wobble = hash_offset(va, vb) * 1.5;
                    let push = offset * 14.0 + wobble;
                    let dx = nx / len * push;
                    let dy = ny / len * push;
                    a[0] += dx;
                    a[1] += dy;
                    b[0] += dx;
                    b[1] += dy;
                }
                write!(
                    out,
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                    a[0], a[1], b[0], b[1]
                )
                .unwrap();
            }
        }
        out.push_str("</g>");
    }
}

fn render_rivers(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let flow = &world.hydrology.flow;
    out.push_str(
        r##"<g stroke="#4a6b8a" fill="none" stroke-linecap="round" stroke-linejoin="round" stroke-opacity="0.85">"##,
    );
    for river in &world.hydrology.rivers {
        if river.cells.len() < 2 {
            continue;
        }
        for win in river.cells.windows(2) {
            let a = mesh.sites[win[0] as usize];
            let b = mesh.sites[win[1] as usize];
            let f = flow.get(win[1] as usize).copied().unwrap_or(1.0);
            let sw = (f.sqrt() * 0.22).clamp(0.6, 6.0);
            write!(
                out,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke-width="{:.2}"/>"##,
                a[0], a[1], b[0], b[1], sw
            )
            .unwrap();
        }
    }
    out.push_str("</g>");
}

/// Tolkien triangular mountains on ALPINE / SNOW cells. Size scales
/// with elevation. Placed at the cell's site; wobble x position by a
/// per-cell hash for non-grid look.
fn render_mountains(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let biomes = &world.climate.biome;
    let elev = &world.terrain.elevation;

    out.push_str(r##"<g fill="#3a2f25" stroke="#1a140e" stroke-width="0.6" stroke-linejoin="round" fill-opacity="0.92">"##);
    for i in 0..mesh.cell_count() {
        let biome = biomes.get(i).copied().unwrap_or(0);
        if biome != ALPINE && biome != SNOW {
            continue;
        }
        let e = elev.get(i).copied().unwrap_or(0.0).max(0.0);
        let site = mesh.sites[i];
        let base = 8.0 + e * 10.0;
        let height = base * 1.2;
        let cx = site[0] + hash_offset(i as u32, 0) * 4.0;
        let cy = site[1] + hash_offset(i as u32, 1) * 2.0;
        // Triangle: base-left, base-right, peak. Peak slightly off-
        // center for hand-drawn feel.
        let bx_l = cx - base * 0.5;
        let bx_r = cx + base * 0.5;
        let by = cy + height * 0.4;
        let peak_x = cx + hash_offset(i as u32, 2) * base * 0.15;
        let peak_y = cy - height * 0.6;
        write!(
            out,
            r##"<polygon points="{bx_l:.1},{by:.1} {bx_r:.1},{by:.1} {peak_x:.1},{peak_y:.1}"/>"##
        )
        .unwrap();
        // Snowcap on the upper triangle for SNOW or tall ALPINE.
        if biome == SNOW || e > 0.6 {
            let scx_l = cx - base * 0.2;
            let scx_r = cx + base * 0.2;
            let scy = cy - height * 0.15;
            write!(
                out,
                r##"<polygon points="{scx_l:.1},{scy:.1} {scx_r:.1},{scy:.1} {peak_x:.1},{peak_y:.1}" fill="#e8e2d4" stroke="none"/>"##
            )
            .unwrap();
        }
    }
    out.push_str("</g>");
}

/// Small triangular tree tufts on forest-class biomes. Multiple per
/// cell, positioned deterministically along the cell polygon's
/// half-edges.
fn render_forest_scatter(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let biomes = &world.climate.biome;

    out.push_str(r##"<g fill="#2d4a2b" stroke="#1a2e19" stroke-width="0.3" fill-opacity="0.85">"##);
    for i in 0..mesh.cell_count() {
        let biome = biomes.get(i).copied().unwrap_or(0);
        let (n_trees, color) = match biome {
            TEMPERATE_FOREST => (3, "#2d4a2b"),
            TEMPERATE_RAINFOREST => (4, "#1f3a1d"),
            TAIGA => (2, "#1e3a2c"),
            TROPICAL_RAINFOREST => (4, "#1c4422"),
            _ => continue,
        };
        let site = mesh.sites[i];
        for t in 0..n_trees {
            // Per-tree offset from cell site.
            let ox = hash_offset(i as u32, (10 + t) as u32) * 14.0;
            let oy = hash_offset(i as u32, (20 + t) as u32) * 10.0;
            let cx = site[0] + ox;
            let cy = site[1] + oy;
            let size = 2.5 + hash_offset(i as u32, (30 + t) as u32).abs() * 1.5;
            // Tree = small triangle.
            let bx_l = cx - size;
            let bx_r = cx + size;
            let by = cy + size * 0.8;
            let peak_y = cy - size * 1.5;
            write!(
                out,
                r##"<polygon points="{bx_l:.1},{by:.1} {bx_r:.1},{by:.1} {cx:.1},{peak_y:.1}" fill="{color}"/>"##
            )
            .unwrap();
        }
    }
    out.push_str("</g>");
}

fn render_roads(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    out.push_str(
        r##"<g stroke="#6b4423" stroke-width="1.4" fill="none" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="4 2" stroke-opacity="0.8">"##,
    );
    for road in &world.society.roads {
        if road.cells.len() < 2 {
            continue;
        }
        out.push_str(r##"<polyline points=""##);
        for (k, &c) in road.cells.iter().enumerate() {
            let p = mesh.sites[c as usize];
            if k > 0 {
                out.push(' ');
            }
            write!(out, "{:.1},{:.1}", p[0], p[1]).unwrap();
        }
        out.push_str(r##""/>"##);
    }
    out.push_str("</g>");
}

/// Settlement glyphs: capitals get a square-with-crown (filled by
/// polity color + dark border + small crown above), towns get a circle.
/// Sized by tier.
fn render_settlements(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    for s in &world.society.settlements {
        let site = mesh.sites[s.cell as usize];
        let color = world
            .society
            .nations
            .get(s.polity_id as usize)
            .map(|n| format!("#{:02x}{:02x}{:02x}", n.color[0], n.color[1], n.color[2]))
            .unwrap_or_else(|| "#5a3a25".to_string());
        match s.tier {
            SettlementTier::Capital => {
                // 8×8 filled square + crown notch above.
                let cx = site[0];
                let cy = site[1];
                write!(
                    out,
                    r##"<rect x="{:.1}" y="{:.1}" width="8" height="8" fill="{color}" stroke="#1a140e" stroke-width="0.9"/>"##,
                    cx - 4.0,
                    cy - 4.0
                )
                .unwrap();
                // Three-tooth crown.
                write!(
                    out,
                    r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{color}" stroke="#1a140e" stroke-width="0.7"/>"##,
                    cx - 4.0, cy - 4.0,
                    cx - 4.0, cy - 7.0,
                    cx - 1.5, cy - 4.5,
                    cx,       cy - 8.0,
                    cx + 1.5, cy - 4.5,
                    cx + 4.0, cy - 7.0,
                    cx + 4.0, cy - 4.0,
                )
                .unwrap();
            }
            SettlementTier::Town => {
                write!(
                    out,
                    r##"<circle cx="{:.1}" cy="{:.1}" r="3.5" fill="{color}" stroke="#1a140e" stroke-width="0.7"/>"##,
                    site[0], site[1]
                )
                .unwrap();
            }
            SettlementTier::Village => {
                write!(
                    out,
                    r##"<circle cx="{:.1}" cy="{:.1}" r="2" fill="{color}" stroke="#1a140e" stroke-width="0.5"/>"##,
                    site[0], site[1]
                )
                .unwrap();
            }
        }
    }
}

/// Sacred sites — small 4-point radiant markers above their cell.
fn render_sacred_sites(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    out.push_str(r##"<g stroke="#cc9933" stroke-width="0.8" fill="#ffcc66" fill-opacity="0.85">"##);
    for religion in &world.religions.religions {
        for &cell in &religion.sacred_sites {
            let p = mesh.sites[cell as usize];
            // Diamond outline.
            write!(
                out,
                r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}"/>"##,
                p[0],
                p[1] - 3.5,
                p[0] + 3.0,
                p[1],
                p[0],
                p[1] + 3.5,
                p[0] - 3.0,
                p[1],
            )
            .unwrap();
        }
    }
    out.push_str("</g>");
}

/// Muted ornate palette — biomes tinted toward parchment so the
/// underlying map reads "old hand-drawn" not "Phase-2 data debug."
fn ornate_biome_color(biome: u8, elev: f32) -> &'static str {
    if elev <= 0.0 {
        return if elev < -0.3 { "#6a85a0" } else { "#9bb5c8" };
    }
    match biome {
        SNOW => "#e8e2d4",
        TUNDRA => "#bfbca8",
        TAIGA => "#5e7a55",
        TEMPERATE_FOREST => "#7a9e6e",
        TEMPERATE_GRASSLAND => "#c2b870",
        TEMPERATE_RAINFOREST => "#5d8a65",
        DESERT => "#dcc080",
        SAVANNA => "#c9a55a",
        TROPICAL_RAINFOREST => "#4a8a4a",
        TROPICAL_DRY_FOREST => "#9aa050",
        SHRUBLAND => "#a59868",
        ALPINE => "#9a9a8a",
        RIPARIAN => "#6a9a4a",
        _ => "#b8a880",
    }
}

const SNOW: u8 = 0;
const TUNDRA: u8 = 1;
const TAIGA: u8 = 2;
const TEMPERATE_FOREST: u8 = 3;
const TEMPERATE_GRASSLAND: u8 = 4;
const TEMPERATE_RAINFOREST: u8 = 5;
const DESERT: u8 = 6;
const SAVANNA: u8 = 7;
const TROPICAL_RAINFOREST: u8 = 8;
const TROPICAL_DRY_FOREST: u8 = 9;
const SHRUBLAND: u8 = 10;
const ALPINE: u8 = 11;
const RIPARIAN: u8 = 14;

/// Per-element deterministic offset in `[-1, 1]`, derived from a u32
/// pair via SplitMix64-style hashing. Reproducible across runs.
fn hash_offset(a: u32, b: u32) -> f32 {
    let mut x = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= (b as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 30;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 27;
    // Take high 24 bits, normalize to [-1, 1].
    let u = (x >> 40) as u32;
    ((u as f32) / (1u32 << 23) as f32) - 1.0
}

fn shared_edge(a: &[u32], b: &[u32]) -> Option<(u32, u32)> {
    let mut hits: [u32; 2] = [0, 0];
    let mut n = 0;
    for &x in a {
        if b.contains(&x) {
            hits[n] = x;
            n += 1;
            if n == 2 {
                return Some((hits[0], hits[1]));
            }
        }
    }
    None
}

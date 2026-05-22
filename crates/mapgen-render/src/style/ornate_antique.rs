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
//! * Settlement glyphs: derived from
//!   `Culture.settlement × Culture.architecture × SettlementTier`. Each
//!   icon family has its own silhouette (8 base shapes); architecture
//!   tweaks stroke / fill / corner-radius. Capital adds a pennant
//!   above; village collapses to a tinted dot.
//! * Roads: thin russet polylines along road-cell sequences.
//! * Sacred sites: small radiant marks layered above settlements.
//!
//! Deferred to follow-up commits (per the architecture's Phase 3e
//! sub-deliverables in `docs/TASKS.md`):
//! * `roughr` pen-jitter (real hand-drawn primitives — needs API probe).
//! * `<defs>`-embedded Cinzel / IM Fell English / EB Garamond fonts.
//! * Compass rose, corner cartouche, vignette, edge burn.
//! * Imhof-style label placement (settlement & polity labels).
//! * (deferred) `roughr` Bezier perturbation for true pen-jitter — the
//!   per-vertex wobble only approximates the look.
//!
//! All randomness is derived from cell IDs and vertex indices — no RNG
//! is threaded through this module, so the output is byte-identical
//! across two runs of the same world.

use std::fmt::Write;

use mapgen_core::entities::{Architecture, SettlementIcon, SettlementTier};
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
    render_polity_labels(world, &mut out);
    render_settlement_labels(world, &mut out);
    render_sacred_site_labels(world, &mut out);

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

/// Settlement glyphs derived from
/// `Culture.settlement × Culture.architecture × SettlementTier`. Each
/// settlement is wrapped in a `<g class="settlement icon-X arch-Y
/// tier-Z">` group — the class attribute lets tests pin the dispatch
/// without coupling to exact path geometry, and usvg/resvg silently
/// ignore unrecognized attributes during PNG rasterization.
///
/// Lookup chain per polity: `Nation.capital_cell → cultures.culture_id
/// [cell] → cultures.cultures[idx]`. Computed once and cached so a
/// polity's frontier town carries the *founding* culture's glyph (the
/// natural reading of "settlement × architecture") even if the cell
/// the town sits on happens to belong to a neighbor's culture. Falls
/// back to `(Castle, Classical)` if any link is missing.
fn render_settlements(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;

    // Per polity, the founding culture's (icon, architecture) — looked
    // up once so a polity's settlements share their founder's
    // silhouette regardless of which cell each one sits on.
    let polity_glyph: Vec<(SettlementIcon, Architecture)> = world
        .society
        .nations
        .iter()
        .map(|n| {
            world
                .cultures
                .culture_id
                .get(n.capital_cell as usize)
                .copied()
                .flatten()
                .and_then(|idx| world.cultures.cultures.get(idx as usize))
                .map(|c| (c.settlement, c.architecture))
                .unwrap_or((SettlementIcon::Castle, Architecture::Classical))
        })
        .collect();

    for s in &world.society.settlements {
        let site = mesh.sites[s.cell as usize];
        let (icon, arch) = polity_glyph
            .get(s.polity_id as usize)
            .copied()
            .unwrap_or((SettlementIcon::Castle, Architecture::Classical));
        let polity_color = world
            .society
            .nations
            .get(s.polity_id as usize)
            .map(|n| n.color)
            .unwrap_or([90, 58, 37]);
        let style = arch_style(arch);
        let fill = darken_color(polity_color, style.fill_darken);

        write!(
            out,
            r##"<g class="settlement icon-{icon} arch-{arch} tier-{tier}">"##,
            icon = icon_class(icon),
            arch = arch_class(arch),
            tier = tier_class(s.tier),
        )
        .unwrap();

        let scale = match s.tier {
            SettlementTier::Capital => 1.0,
            SettlementTier::Town => 0.75,
            SettlementTier::Village => 0.0, // village uses a uniform tinted dot
        };

        if s.tier == SettlementTier::Village {
            // Per advisor: villages collapse to a small icon-family-
            // tinted square. At 4-pixel scale, distinguishing 8 icon
            // silhouettes is wasted detail — the polity color carries
            // the identity instead.
            write!(
                out,
                r##"<rect x="{x:.1}" y="{y:.1}" width="3" height="3" fill="{fill}" stroke="#1a140e" stroke-width="0.5"/>"##,
                x = site[0] - 1.5,
                y = site[1] - 1.5,
            )
            .unwrap();
        } else {
            draw_glyph(icon, site[0], site[1], scale, &fill, &style, out);
            if s.tier == SettlementTier::Capital {
                draw_capital_pennant(site[0], site[1], scale, &fill, out);
            }
        }

        out.push_str("</g>");
    }
}

/// Architecture-driven styling axis: stroke weight, fill darkening,
/// rect corner-radius. Classical is the clean baseline; Gothic adds
/// stroke + slight darkening; Organic rounds corners + slightly fades;
/// Megalithic uses heavy stroke + darker fill to read as "carved
/// stone."
#[derive(Copy, Clone)]
struct ArchStyle {
    stroke_width: f32,
    fill_darken: f32,
    rx: f32,
    /// Gothic adds a thin vertical accent line up the middle of body-
    /// based silhouettes (Castle, Tower, Hall, Gate, Longhouse, Yurt).
    /// Triangle-based silhouettes (Spire, Treehouse) ignore it.
    accent: bool,
}

fn arch_style(arch: Architecture) -> ArchStyle {
    match arch {
        Architecture::Classical => ArchStyle {
            stroke_width: 0.8,
            fill_darken: 1.0,
            rx: 0.0,
            accent: false,
        },
        Architecture::Gothic => ArchStyle {
            stroke_width: 0.9,
            fill_darken: 0.88,
            rx: 0.0,
            accent: true,
        },
        Architecture::Organic => ArchStyle {
            stroke_width: 0.7,
            fill_darken: 0.95,
            rx: 2.0,
            accent: false,
        },
        Architecture::Megalithic => ArchStyle {
            stroke_width: 1.5,
            fill_darken: 0.75,
            rx: 0.0,
            accent: false,
        },
    }
}

fn icon_class(icon: SettlementIcon) -> &'static str {
    match icon {
        SettlementIcon::Castle => "castle",
        SettlementIcon::Tower => "tower",
        SettlementIcon::Hall => "hall",
        SettlementIcon::Spire => "spire",
        SettlementIcon::Longhouse => "longhouse",
        SettlementIcon::Treehouse => "treehouse",
        SettlementIcon::Gate => "gate",
        SettlementIcon::Yurt => "yurt",
    }
}

fn arch_class(arch: Architecture) -> &'static str {
    match arch {
        Architecture::Classical => "classical",
        Architecture::Gothic => "gothic",
        Architecture::Organic => "organic",
        Architecture::Megalithic => "megalithic",
    }
}

fn tier_class(tier: SettlementTier) -> &'static str {
    match tier {
        SettlementTier::Capital => "capital",
        SettlementTier::Town => "town",
        SettlementTier::Village => "village",
    }
}

fn darken_color(color: [u8; 3], factor: f32) -> String {
    let r = ((color[0] as f32) * factor) as u8;
    let g = ((color[1] as f32) * factor) as u8;
    let b = ((color[2] as f32) * factor) as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn draw_glyph(
    icon: SettlementIcon,
    cx: f32,
    cy: f32,
    s: f32,
    fill: &str,
    st: &ArchStyle,
    out: &mut String,
) {
    match icon {
        SettlementIcon::Castle => draw_castle(cx, cy, s, fill, st, out),
        SettlementIcon::Tower => draw_tower(cx, cy, s, fill, st, out),
        SettlementIcon::Hall => draw_hall(cx, cy, s, fill, st, out),
        SettlementIcon::Spire => draw_spire(cx, cy, s, fill, st, out),
        SettlementIcon::Longhouse => draw_longhouse(cx, cy, s, fill, st, out),
        SettlementIcon::Treehouse => draw_treehouse(cx, cy, s, fill, st, out),
        SettlementIcon::Gate => draw_gate(cx, cy, s, fill, st, out),
        SettlementIcon::Yurt => draw_yurt(cx, cy, s, fill, st, out),
    }
}

/// Castle: square body with three merlons on the top edge —
/// the existing capital glyph generalized so every Castle-family
/// settlement reads as "fortified."
fn draw_castle(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let w = 8.0 * s;
    let h = 8.0 * s;
    let merlon_w = w * 0.22;
    let merlon_h = h * 0.22;
    let x0 = cx - w * 0.5;
    let y0 = cy - h * 0.5;
    write!(
        out,
        r##"<rect x="{x0:.1}" y="{y0:.1}" width="{w:.1}" height="{h:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        rx = st.rx,
        sw = st.stroke_width,
    )
    .unwrap();
    for col in [0.0_f32, 0.4, 0.8] {
        let mx = x0 + w * col + (w - merlon_w * 3.0) * 0.05;
        let my = y0 - merlon_h;
        write!(
            out,
            r##"<rect x="{mx:.1}" y="{my:.1}" width="{merlon_w:.1}" height="{merlon_h:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
            sw = st.stroke_width,
        )
        .unwrap();
    }
    if st.accent {
        write!(
            out,
            r##"<line x1="{cx:.1}" y1="{y1:.1}" x2="{cx:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="0.5"/>"##,
            y1 = y0 + h * 0.15,
            y2 = y0 + h * 0.95,
        )
        .unwrap();
    }
}

/// Tower: narrow tall body with a conical (triangular) roof.
fn draw_tower(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let w = 4.0 * s;
    let body_h = 9.0 * s;
    let roof_h = 4.0 * s;
    let body_top = cy - body_h * 0.4;
    write!(
        out,
        r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{body_h:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        x = cx - w * 0.5,
        y = body_top,
        rx = st.rx,
        sw = st.stroke_width,
    )
    .unwrap();
    write!(
        out,
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        cx - w * 0.65,
        body_top,
        cx + w * 0.65,
        body_top,
        cx,
        body_top - roof_h,
        sw = st.stroke_width,
    )
    .unwrap();
    if st.accent {
        write!(
            out,
            r##"<line x1="{cx:.1}" y1="{y1:.1}" x2="{cx:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="0.5"/>"##,
            y1 = body_top + body_h * 0.15,
            y2 = body_top + body_h * 0.9,
        )
        .unwrap();
    }
}

/// Hall: wide low body with a peaked gable roof. Reads as
/// "community building" — the Riverfolk / Greendale glyph.
fn draw_hall(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let w = 10.0 * s;
    let body_h = 5.0 * s;
    let roof_h = 3.5 * s;
    let body_top = cy - body_h * 0.2;
    write!(
        out,
        r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{body_h:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        x = cx - w * 0.5,
        y = body_top,
        rx = st.rx,
        sw = st.stroke_width,
    )
    .unwrap();
    write!(
        out,
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        cx - w * 0.55,
        body_top,
        cx + w * 0.55,
        body_top,
        cx,
        body_top - roof_h,
        sw = st.stroke_width,
    )
    .unwrap();
    if st.accent {
        write!(
            out,
            r##"<line x1="{cx:.1}" y1="{y1:.1}" x2="{cx:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="0.5"/>"##,
            y1 = body_top - roof_h * 0.4,
            y2 = body_top + body_h * 0.9,
        )
        .unwrap();
    }
}

/// Spire: tall sharp triangle — wizard's pinnacle / temple peak.
fn draw_spire(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let w = 5.0 * s;
    let h = 12.0 * s;
    write!(
        out,
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        cx - w * 0.5,
        cy + h * 0.4,
        cx + w * 0.5,
        cy + h * 0.4,
        cx,
        cy - h * 0.6,
        sw = st.stroke_width,
    )
    .unwrap();
}

/// Longhouse: very wide low rectangle with a ridge line down the
/// center — the Orc / Burning Horde glyph.
fn draw_longhouse(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let w = 12.0 * s;
    let body_h = 4.5 * s;
    let x0 = cx - w * 0.5;
    let y0 = cy - body_h * 0.5;
    write!(
        out,
        r##"<rect x="{x0:.1}" y="{y0:.1}" width="{w:.1}" height="{body_h:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        rx = st.rx,
        sw = st.stroke_width,
    )
    .unwrap();
    write!(
        out,
        r##"<line x1="{x1:.1}" y1="{cy:.1}" x2="{x2:.1}" y2="{cy:.1}" stroke="#1a140e" stroke-width="0.5"/>"##,
        x1 = x0 + w * 0.1,
        x2 = x0 + w * 0.9,
    )
    .unwrap();
    if st.accent {
        write!(
            out,
            r##"<line x1="{cx:.1}" y1="{y1:.1}" x2="{cx:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="0.5"/>"##,
            y1 = y0 + body_h * 0.1,
            y2 = y0 + body_h * 0.9,
        )
        .unwrap();
    }
}

/// Treehouse: leaf-canopy circle with two short trunks below — the
/// Wood-Elf / Wildwood Kin glyph.
fn draw_treehouse(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let canopy_r = 4.0 * s;
    let trunk_h = 4.0 * s;
    let canopy_cy = cy - trunk_h * 0.2;
    write!(
        out,
        r##"<circle cx="{cx:.1}" cy="{canopy_cy:.1}" r="{canopy_r:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        sw = st.stroke_width,
    )
    .unwrap();
    for &dx in &[-1.2_f32, 1.2] {
        write!(
            out,
            r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
            x1 = cx + dx * s,
            y1 = canopy_cy + canopy_r * 0.6,
            x2 = cx + dx * s,
            y2 = canopy_cy + canopy_r + trunk_h,
            sw = st.stroke_width,
        )
        .unwrap();
    }
}

/// Gate: stocky square body with a notched arch cutout in the lower
/// half — the Dwarven / Iron Hold mountain-hold glyph.
fn draw_gate(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let w = 9.0 * s;
    let h = 9.0 * s;
    let x0 = cx - w * 0.5;
    let y0 = cy - h * 0.5;
    write!(
        out,
        r##"<rect x="{x0:.1}" y="{y0:.1}" width="{w:.1}" height="{h:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        rx = st.rx,
        sw = st.stroke_width,
    )
    .unwrap();
    // Arch cutout: a darker trapezoid taking up the lower middle.
    let arch_w = w * 0.4;
    let arch_h = h * 0.55;
    write!(
        out,
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="#1a140e" stroke="none"/>"##,
        cx - arch_w * 0.5,
        y0 + h,
        cx + arch_w * 0.5,
        y0 + h,
        cx + arch_w * 0.35,
        y0 + h - arch_h,
        cx - arch_w * 0.35,
        y0 + h - arch_h,
    )
    .unwrap();
    if st.accent {
        write!(
            out,
            r##"<line x1="{cx:.1}" y1="{y1:.1}" x2="{cx:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="0.5"/>"##,
            y1 = y0 + h * 0.1,
            y2 = y0 + h * 0.4,
        )
        .unwrap();
    }
}

/// Yurt: trapezoid base with a peaked-dome top — the steppe-nomad
/// glyph reserved for the Halfling-Pastoral archetype's likely
/// future cousin.
fn draw_yurt(cx: f32, cy: f32, s: f32, fill: &str, st: &ArchStyle, out: &mut String) {
    let base_w = 9.0 * s;
    let top_w = 5.0 * s;
    let body_h = 5.0 * s;
    let dome_h = 3.0 * s;
    let y_bottom = cy + body_h * 0.4;
    let y_top = y_bottom - body_h;
    write!(
        out,
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        cx - base_w * 0.5,
        y_bottom,
        cx + base_w * 0.5,
        y_bottom,
        cx + top_w * 0.5,
        y_top,
        cx - top_w * 0.5,
        y_top,
        sw = st.stroke_width,
    )
    .unwrap();
    write!(
        out,
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{fill}" stroke="#1a140e" stroke-width="{sw:.2}"/>"##,
        cx - top_w * 0.5,
        y_top,
        cx + top_w * 0.5,
        y_top,
        cx,
        y_top - dome_h,
        sw = st.stroke_width,
    )
    .unwrap();
    if st.accent {
        write!(
            out,
            r##"<line x1="{cx:.1}" y1="{y1:.1}" x2="{cx:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="0.5"/>"##,
            y1 = y_top + body_h * 0.1,
            y2 = y_top + body_h * 0.9,
        )
        .unwrap();
    }
}

/// Capital ornament — a small polity-colored pennant on a pole above
/// the glyph. Same shape across all icons so capitals read as
/// "important" regardless of culture.
fn draw_capital_pennant(cx: f32, cy: f32, s: f32, fill: &str, out: &mut String) {
    let pole_top = cy - 6.0 * s - 9.0 * s;
    write!(
        out,
        r##"<line x1="{cx:.1}" y1="{y1:.1}" x2="{cx:.1}" y2="{y2:.1}" stroke="#1a140e" stroke-width="0.7"/>"##,
        y1 = cy - 6.0 * s,
        y2 = pole_top,
    )
    .unwrap();
    write!(
        out,
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{fill}" stroke="#1a140e" stroke-width="0.6"/>"##,
        cx,
        pole_top,
        cx + 5.0 * s,
        pole_top + 1.5 * s,
        cx,
        pole_top + 3.0 * s,
    )
    .unwrap();
}

/// Settlement name labels. Capitals get large serif text above-right of
/// the glyph; towns get smaller text below. Basic positioning — no
/// Imhof simulated-annealing optimization (architecture defers full SA
/// to post-MVP). Accepts label overlap rather than silently dropping
/// labels; the reader can usually disambiguate from glyph proximity.
fn render_settlement_labels(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    out.push_str(
        r##"<g font-family="Georgia, 'Times New Roman', serif" fill="#1a140e" stroke="#f0e3bf" paint-order="stroke" stroke-width="2.0" stroke-linejoin="round">"##,
    );
    for s in &world.society.settlements {
        if s.name.is_empty() {
            continue;
        }
        let site = mesh.sites[s.cell as usize];
        let (dx, dy, size, weight) = match s.tier {
            SettlementTier::Capital => (8.0_f32, -10.0_f32, 15.0_f32, "bold"),
            SettlementTier::Town => (0.0, 14.0, 10.0, "normal"),
            SettlementTier::Village => (0.0, 11.0, 8.0, "normal"),
        };
        let anchor = match s.tier {
            SettlementTier::Capital => "start",
            _ => "middle",
        };
        write!(
            out,
            r##"<text x="{:.1}" y="{:.1}" font-size="{size:.1}" font-weight="{weight}" text-anchor="{anchor}">{}</text>"##,
            site[0] + dx,
            site[1] + dy,
            xml_escape(&s.name),
        )
        .unwrap();
    }
    out.push_str("</g>");
}

/// Polity-name labels at the territorial centroid (computed from
/// `world.society.control` — the per-cell polity_id) so the name lands
/// in the middle of the realm rather than at the capital. Large
/// italicized serif, tinted by polity color so it reads as "this
/// region belongs to this polity."
fn render_polity_labels(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    out.push_str(
        r##"<g font-family="Georgia, 'Times New Roman', serif" font-style="italic" font-weight="bold" text-anchor="middle" fill-opacity="0.85" stroke="#f0e3bf" paint-order="stroke" stroke-width="3.0" stroke-linejoin="round">"##,
    );
    for (p, polity) in world.society.nations.iter().enumerate() {
        if polity.name.is_empty() {
            continue;
        }
        // Compute centroid of cells controlled by this polity.
        let mut sum = [0.0_f32, 0.0_f32];
        let mut count = 0u32;
        for (cell, &slot) in world.society.control.iter().enumerate() {
            if slot == Some(p as u32) {
                let site = mesh.sites[cell];
                sum[0] += site[0];
                sum[1] += site[1];
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        let cx = sum[0] / count as f32;
        let cy = sum[1] / count as f32;
        // Darken the polity color slightly for legibility against the
        // softer territorial fill underneath.
        let r = ((polity.color[0] as u16 * 7) / 10) as u8;
        let g = ((polity.color[1] as u16 * 7) / 10) as u8;
        let b = ((polity.color[2] as u16 * 7) / 10) as u8;
        write!(
            out,
            r##"<text x="{cx:.1}" y="{cy:.1}" font-size="22" fill="#{r:02x}{g:02x}{b:02x}">{}</text>"##,
            xml_escape(&polity.name)
        )
        .unwrap();
    }
    out.push_str("</g>");
}

/// Religion-name labels — tiny italic text above each sacred site
/// diamond. Stays decorative rather than dominant; capital and polity
/// labels are the load-bearing readability layer.
fn render_sacred_site_labels(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    out.push_str(
        r##"<g font-family="Georgia, 'Times New Roman', serif" font-style="italic" font-size="9" text-anchor="middle" fill="#7a5a25" stroke="#f0e3bf" paint-order="stroke" stroke-width="1.6" stroke-linejoin="round">"##,
    );
    for religion in &world.religions.religions {
        if religion.name.is_empty() {
            continue;
        }
        // Place above the first sacred site (deterministic — sacred
        // sites are stored highest-elevation-first by the religions
        // stage).
        let Some(&first_cell) = religion.sacred_sites.first() else {
            continue;
        };
        let site = mesh.sites[first_cell as usize];
        write!(
            out,
            r##"<text x="{:.1}" y="{:.1}">{}</text>"##,
            site[0],
            site[1] - 8.0,
            xml_escape(&religion.name),
        )
        .unwrap();
    }
    out.push_str("</g>");
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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

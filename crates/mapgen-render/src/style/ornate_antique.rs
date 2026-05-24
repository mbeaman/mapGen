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

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::LazyLock;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use euclid::default::Point2D;
use mapgen_core::entities::{Architecture, PantheonPattern, SettlementIcon, SettlementTier};
use mapgen_core::WorldData;
use roughr::core::{OpSetType, OptionsBuilder};
use roughr::generator::Generator;

use crate::FONTS_TTF;

/// Pre-encoded `<style>` block carrying base64 `@font-face` rules for
/// each vendored font. Computed once per process — encoding ~200 KB
/// of TTF bytes is ~milliseconds, but we pay it lazily so the cost
/// only lands on processes that actually render ornate maps.
static FONT_FACE_BLOCK: LazyLock<String> = LazyLock::new(|| {
    let mut s = String::from("<style>");
    for (name, bytes) in FONTS_TTF {
        let b64 = STANDARD.encode(bytes);
        // `format("truetype")` keeps the browser hint consistent with
        // what usvg's fontdb sees on the CLI side — both render paths
        // load identical typography.
        write!(
            s,
            r#"@font-face{{font-family:"{name}";src:url(data:font/ttf;base64,{b64}) format("truetype");}}"#,
        )
        .unwrap();
    }
    s.push_str("</style>");
    s
});

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

    // Aged parchment + vendored display typography. Fonts are embedded
    // as base64 TTFs in an `@font-face` block so browser-rendered SVGs
    // carry the same typography the CLI sees through usvg's fontdb
    // (single source of truth: `mapgen_render::FONTS_TTF`).
    out.push_str(r##"<defs>"##);
    out.push_str(&FONT_FACE_BLOCK);
    out.push_str(
        r##"<radialGradient id="parchment" cx="50%" cy="50%" r="75%">
<stop offset="0%" stop-color="#f0e3bf"/>
<stop offset="70%" stop-color="#dfca96"/>
<stop offset="100%" stop-color="#a88550"/>
</radialGradient>
<radialGradient id="edge-burn" cx="50%" cy="50%" r="78%">
<stop offset="40%" stop-color="#2a1c0d" stop-opacity="0"/>
<stop offset="78%" stop-color="#2a1c0d" stop-opacity="0.30"/>
<stop offset="100%" stop-color="#1a1208" stop-opacity="0.72"/>
</radialGradient>
</defs>"##,
    );
    out.push_str(r##"<rect width="100%" height="100%" fill="url(#parchment)"/>"##);

    render_land_fill(world, &mut out);
    render_ocean_hatching(world, &mut out);
    render_coastline_ripples(world, &mut out);
    render_rivers(world, &mut out);
    render_mountains(world, &mut out);
    render_forest_scatter(world, &mut out);
    render_roads(world, &mut out);
    render_polity_borders(world, &mut out);
    render_settlements(world, &mut out);
    render_sacred_sites(world, &mut out);
    render_polity_labels(world, &mut out);
    render_settlement_labels(world, &mut out);
    render_sacred_site_labels(world, &mut out);
    render_feature_labels(world, &mut out);
    render_mountain_range_labels(world, &mut out);

    // Decorative top-of-stack overlays. The edge-burn overlay darkens
    // the periphery (intentionally fading edge labels into "aged"
    // shadow); compass + cartouche sit on top, untouched.
    render_edge_burn(w, h, &mut out);
    render_compass(w, h, &mut out);
    render_cartouche(w, h, &mut out);

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

/// Antique ocean engraving: short faint horizontal hatch dashes
/// scattered across sea cells, the way old maps lined the water with
/// fine horizontal strokes to give it texture without dominating the
/// land. Sparse (~40% of sea cells) and low-opacity so the parchment
/// still reads through. Dash length / vertical jitter are hash-driven
/// for a hand-engraved, broken-line feel; drawn over the flat sea fill
/// but under the coastline ripples so the coast stays crisp.
fn render_ocean_hatching(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;
    out.push_str(
        r##"<g class="ocean-hatch" stroke="#5a7088" stroke-width="0.5" stroke-opacity="0.28" stroke-linecap="round">"##,
    );
    for i in 0..mesh.cell_count() {
        if elev.get(i).copied().unwrap_or(0.0) > 0.0 {
            continue; // land
        }
        if hash_offset(i as u32, 7) < 0.2 {
            continue; // sparse: ~40% of sea cells carry a dash
        }
        let site = mesh.sites[i];
        let len = 7.0 + hash_offset(i as u32, 8).abs() * 6.0;
        let y = site[1] + hash_offset(i as u32, 9) * 4.0;
        write!(
            out,
            r##"<line x1="{:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}"/>"##,
            site[0] - len * 0.5,
            site[0] + len * 0.5,
        )
        .unwrap();
    }
    out.push_str("</g>");
}

/// Four offset coastline strokes hand-perturbed by `roughr` (the Rust
/// port of Rough.js). Continuous coastline polylines are traced from
/// the cell-edge graph, then each ripple draws each polyline as a
/// roughr `linear_path` with its own seed + roughness — so the four
/// ripples are visually independent scratchy passes rather than four
/// parallel copies.
///
/// Per-ripple seeds give deterministic byte-identical output across
/// runs. ARCHITECTURE.md §Phase 3e calls for 4 ripples; we ship 4.
fn render_coastline_ripples(world: &WorldData, out: &mut String) {
    let polylines = extract_coastline_polylines(world);
    if polylines.is_empty() {
        return;
    }

    // Per ripple: (color, stroke-width, roughness, bowing, seed).
    // Outer ripples thicker + jitterier + faded (suggesting shallow
    // water haze); inner ripples crisper for the coastline outline.
    let ripples: &[(&str, f32, f32, f32, u64)] = &[
        ("#5a6a8a", 4.0, 2.2, 2.0, 91_001), // outermost, deepest haze
        ("#7a8aa8", 2.8, 1.8, 2.0, 91_002),
        ("#a09cb8", 1.9, 1.4, 1.5, 91_003), // 4th ripple per arch spec
        ("#2a2418", 1.2, 0.6, 1.0, 91_004), // innermost: crisp dark outline
    ];

    let gen = Generator::default();

    for (color, width, roughness, bowing, seed) in ripples.iter().copied() {
        write!(
            out,
            r##"<g stroke="{color}" stroke-width="{width:.2}" stroke-linecap="round" fill="none" stroke-opacity="0.78">"##
        )
        .unwrap();

        let opts = OptionsBuilder::default()
            .roughness(roughness)
            .bowing(bowing)
            .seed(seed)
            .disable_multi_stroke(true)
            .preserve_vertices(true)
            .build()
            .expect("OptionsBuilder must build with valid params");
        let opts_some = Some(opts);

        for (chain, closed) in &polylines {
            if chain.len() < 2 {
                continue;
            }
            let points: Vec<Point2D<f32>> =
                chain.iter().map(|&[x, y]| Point2D::new(x, y)).collect();
            let drawable = gen.linear_path(&points, *closed, &opts_some);
            for op_set in &drawable.sets {
                if matches!(op_set.op_set_type, OpSetType::Path) {
                    let path = roughr_path_with_move_fix(op_set.clone());
                    if !path.is_empty() {
                        write!(out, r##"<path d="{path}"/>"##).unwrap();
                    }
                }
            }
        }

        out.push_str("</g>");
    }
}

/// Convert a roughr `OpSet<f32>` to an SVG `d=` attribute string,
/// fixing the upstream roughr 0.12 bug where `OpType::Move` is
/// incorrectly serialized as `L` (lineto) instead of `M` (moveto).
/// Without this fix the path starts with an implicit moveto from
/// origin and many renderers draw a stray stroke from (0,0) to the
/// path's actual starting point.
///
/// See `roughr::generator::Generator::ops_to_path` line 479:
/// `write!(&mut path, "L{} {} ", ...)` should write `M` for Move.
fn roughr_path_with_move_fix(op_set: roughr::core::OpSet<f32>) -> String {
    let raw = Generator::ops_to_path(op_set, Some(1));
    if let Some(rest) = raw.strip_prefix('L') {
        format!("M{rest}")
    } else {
        raw
    }
}

/// Trace continuous coastline polylines from the land/sea cell-edge
/// graph. Each output is `(chain, closed)` where `closed=true` means
/// the polyline is a loop (typical case — coastlines wrap around
/// islands/continents) and `closed=false` means it terminates at the
/// canvas edge.
///
/// Algorithm: collect every shared edge between a land cell and a sea
/// cell; build vertex adjacency; walk each unvisited edge to extract
/// the chain it belongs to (forward then backward). Closed loops are
/// detected by returning to the starting vertex; open chains by
/// hitting a degree-1 endpoint.
///
/// Vertices of degree >2 (where 3+ coastline edges meet — rare in a
/// Voronoi mesh) cause the walk to pick the first unvisited neighbor;
/// the remaining edges become their own chain on a later pass.
fn extract_coastline_polylines(world: &WorldData) -> Vec<(Vec<[f32; 2]>, bool)> {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;

    // Collect coastline edges as sorted (va, vb) pairs.
    let mut edges: Vec<(u32, u32)> = Vec::new();
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
            if let Some((va, vb)) = shared_edge(&mesh.cell_vertices[i], &mesh.cell_vertices[j]) {
                edges.push((va.min(vb), va.max(vb)));
            }
        }
    }
    if edges.is_empty() {
        return Vec::new();
    }

    // Vertex adjacency.
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(a, b) in &edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }

    // Walk chains. For each unused edge, follow forward until we
    // return to start (closed loop) or hit a dead end (open chain),
    // then extend backward from the start in the open-chain case.
    let mut used: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut chains: Vec<Vec<u32>> = Vec::new();

    let edge_key = |x: u32, y: u32| (x.min(y), x.max(y));

    for &(a0, b0) in &edges {
        if used.contains(&edge_key(a0, b0)) {
            continue;
        }
        used.insert(edge_key(a0, b0));
        let mut chain = vec![a0, b0];

        // Extend forward from b0.
        let mut prev = a0;
        let mut current = b0;
        while let Some(neighbors) = adj.get(&current) {
            let Some(next) = neighbors
                .iter()
                .copied()
                .find(|&v| v != prev && !used.contains(&edge_key(current, v)))
            else {
                break;
            };
            used.insert(edge_key(current, next));
            chain.push(next);
            prev = current;
            current = next;
            if current == a0 {
                break; // closed loop
            }
        }

        // If chain didn't close, extend backward from a0.
        let closed = chain.first() == chain.last() && chain.len() > 2;
        if !closed {
            let mut prev = b0;
            let mut current = a0;
            while let Some(neighbors) = adj.get(&current) {
                let Some(next) = neighbors
                    .iter()
                    .copied()
                    .find(|&v| v != prev && !used.contains(&edge_key(current, v)))
                else {
                    break;
                };
                used.insert(edge_key(current, next));
                chain.insert(0, next);
                prev = current;
                current = next;
            }
        }

        chains.push(chain);
    }

    // Convert vertex indices to coordinates. Drop the duplicate
    // trailing vertex on closed loops — roughr's `close=true` adds it
    // back.
    chains
        .into_iter()
        .map(|chain| {
            let closed = chain.first() == chain.last() && chain.len() > 2;
            let drop_last = if closed { 1 } else { 0 };
            let points: Vec<[f32; 2]> = chain[..chain.len() - drop_last]
                .iter()
                .map(|&v| mesh.vertices[v as usize])
                .collect();
            (points, closed)
        })
        .collect()
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

    out.push_str(r##"<g class="mountains" fill="#3a2f25" stroke="#1a140e" stroke-width="0.6" stroke-linejoin="round" fill-opacity="0.92">"##);
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
        // Depth shadow: the same triangle offset down-right and painted
        // semi-transparent dark *under* the peak, so mountains read as
        // 3D land features lit from the upper-left rather than flat
        // stickers. Scales with the peak so big mountains throw bigger
        // shadows. Drawn first → the main triangle paints over it.
        let sh = (base * 0.18).clamp(1.5, 4.0);
        write!(
            out,
            r##"<polygon class="mtn-shadow" points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="#160f08" fill-opacity="0.30" stroke="none"/>"##,
            bx_l + sh,
            by + sh,
            bx_r + sh,
            by + sh,
            peak_x + sh,
            peak_y + sh,
        )
        .unwrap();
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

/// Roads, drawn per-segment with stroke width scaled by how many roads
/// share each cell. The polities stage builds town→capital paths with a
/// reuse discount, so cells near a capital sit on many overlapping
/// paths; counting per-cell road membership recovers that trunk-and-
/// branch structure and renders trunk segments visibly thicker than the
/// branches that feed them — without a schema change (the count is
/// derived from the existing `Road.cells`).
fn render_roads(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;

    // Per-cell traversal frequency across all roads.
    let mut traversal: HashMap<u32, u32> = HashMap::new();
    for road in &world.society.roads {
        for &c in &road.cells {
            *traversal.entry(c).or_default() += 1;
        }
    }
    let weight = |c: u32| traversal.get(&c).copied().unwrap_or(1);

    out.push_str(
        r##"<g class="roads" stroke="#6b4423" fill="none" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="4 2" stroke-opacity="0.8">"##,
    );
    for road in &world.society.roads {
        if road.cells.len() < 2 {
            continue;
        }
        for win in road.cells.windows(2) {
            let a = mesh.sites[win[0] as usize];
            let b = mesh.sites[win[1] as usize];
            // A segment is trunk-like only if BOTH endpoints are
            // heavily shared, so take the min — a spur joining a trunk
            // stays thin until it merges.
            let shared = weight(win[0]).min(weight(win[1]));
            let sw = (1.0 + (shared as f32).sqrt() * 0.7).clamp(1.0, 4.5);
            write!(
                out,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke-width="{sw:.2}"/>"##,
                a[0], a[1], b[0], b[1],
            )
            .unwrap();
        }
    }
    out.push_str("</g>");
}

/// Dashed political borders between adjacent polities. For every pair
/// of neighboring land cells controlled by *different* polities, the
/// shared Voronoi edge is drawn as a dash-dot line — the classic
/// antique-map convention for a frontier. Coast and unclaimed-land
/// edges are left to the coastline layer, so this draws only the
/// genuine inter-polity boundaries that resolve "which realm owns this
/// peninsula." Borders sit under settlements so glyphs stay on top.
fn render_polity_borders(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let control = &world.society.control;
    if control.is_empty() {
        return;
    }
    out.push_str(
        r##"<g class="polity-borders" stroke="#4a3520" stroke-width="1.1" fill="none" stroke-linecap="round" stroke-dasharray="5 3 1 3" stroke-opacity="0.7">"##,
    );
    for i in 0..mesh.cell_count() {
        let Some(ci) = control.get(i).copied().flatten() else {
            continue;
        };
        for &nj in &mesh.neighbors[i] {
            let j = nj as usize;
            if j <= i {
                continue; // each undirected edge once
            }
            let Some(cj) = control.get(j).copied().flatten() else {
                continue;
            };
            if ci == cj {
                continue; // same polity — interior, no border
            }
            if let Some((va, vb)) = shared_edge(&mesh.cell_vertices[i], &mesh.cell_vertices[j]) {
                let a = mesh.vertices[va as usize];
                let b = mesh.vertices[vb as usize];
                write!(
                    out,
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                    a[0], a[1], b[0], b[1],
                )
                .unwrap();
            }
        }
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
            SettlementTier::Town => town_scale(s.population),
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

/// Town glyph scale as a function of the settlement's normalized
/// population proxy in `(0, 1]`. Towns range 0.6×–0.95× the capital
/// glyph so the Christaller hierarchy's population spread is legible at
/// a glance instead of every town rendering at a fixed 0.75×. Capitals
/// are always 1.0 (handled by the caller); villages collapse to a dot.
fn town_scale(population: f32) -> f32 {
    (0.55 + population * 0.4).clamp(0.6, 0.95)
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

/// Axis-aligned box (top-left origin) used for label placement.
#[derive(Copy, Clone)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn centered(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Rect {
            x: cx - w * 0.5,
            y: cy - h * 0.5,
            w,
            h,
        }
    }
    /// Intersection area with another box (0 if disjoint).
    fn overlap(&self, o: &Rect) -> f32 {
        let ix = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let iy = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        if ix > 0.0 && iy > 0.0 {
            ix * iy
        } else {
            0.0
        }
    }
}

/// Per-label candidate offset directions, in Imhof preference order
/// (upper-right best, then the other diagonals, then the cardinals).
/// Lower index = more preferred; the SA cost adds a small penalty per
/// index so ties break toward the cartographic convention.
const LABEL_DIRS: [(f32, f32); 8] = [
    (1.0, -1.0),
    (-1.0, -1.0),
    (1.0, 1.0),
    (-1.0, 1.0),
    (0.0, -1.4),
    (0.0, 1.4),
    (1.4, 0.0),
    (-1.4, 0.0),
];

/// Settlement name labels with Imhof-style simulated-annealing
/// placement. Each label has 8 candidate positions around its glyph; SA
/// minimizes an energy summing label-label overlap, label-glyph overlap,
/// and a small position-preference penalty, so on dense maps labels
/// slide to open space instead of stacking. The anneal uses a fixed-seed
/// xorshift and a no-exp (linear-cooling) acceptance rule, so placement
/// is byte-identical across runs and free of transcendentals.
///
/// Capitals use Cinzel (display caps); towns/villages use EB Garamond.
fn render_settlement_labels(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;

    struct Spec {
        ax: f32,
        ay: f32,
        text: String,
        size: f32,
        weight: &'static str,
        family: &'static str,
    }

    let mut specs: Vec<Spec> = Vec::new();
    for s in &world.society.settlements {
        if s.name.is_empty() {
            continue;
        }
        let site = mesh.sites[s.cell as usize];
        let (size, weight, family) = match s.tier {
            SettlementTier::Capital => (15.0_f32, "bold", r##""Cinzel", Georgia, serif"##),
            SettlementTier::Town => (10.0, "normal", r##""EB Garamond", Georgia, serif"##),
            SettlementTier::Village => (8.0, "normal", r##""EB Garamond", Georgia, serif"##),
        };
        specs.push(Spec {
            ax: site[0],
            ay: site[1],
            text: xml_escape(&s.name),
            size,
            weight,
            family,
        });
    }
    if specs.is_empty() {
        return;
    }

    // One glyph obstacle box per anchor + 8 candidate boxes per label.
    let glyphs: Vec<Rect> = specs
        .iter()
        .map(|s| Rect::centered(s.ax, s.ay, 13.0, 13.0))
        .collect();
    let gap = 7.0_f32;
    let candidates: Vec<Vec<Rect>> = specs
        .iter()
        .map(|s| {
            let tw = s.text.chars().count() as f32 * s.size * 0.55;
            let th = s.size;
            LABEL_DIRS
                .iter()
                .map(|&(dx, dy)| {
                    let cx = s.ax + dx * (tw * 0.5 + gap);
                    let cy = s.ay + dy * (th * 0.5 + gap);
                    Rect::centered(cx, cy, tw, th)
                })
                .collect()
        })
        .collect();

    let chosen = anneal_labels(&candidates, &glyphs);

    out.push_str(
        r##"<g class="settlement-labels" fill="#1a140e" stroke="#f0e3bf" paint-order="stroke" stroke-width="2.0" stroke-linejoin="round" text-anchor="middle">"##,
    );
    for (i, s) in specs.iter().enumerate() {
        let b = &candidates[i][chosen[i]];
        write!(
            out,
            r##"<text x="{:.1}" y="{:.1}" font-family='{family}' font-size="{size:.1}" font-weight="{weight}">{text}</text>"##,
            b.x + b.w * 0.5,
            b.y + b.h * 0.78,
            family = s.family,
            size = s.size,
            weight = s.weight,
            text = s.text,
        )
        .unwrap();
    }
    out.push_str("</g>");
}

/// Simulated-annealing assignment of one candidate index per label,
/// minimizing total overlap energy. Deterministic: fixed-seed xorshift +
/// linear-cooling acceptance (no `exp`). Returns the chosen index per
/// label.
fn anneal_labels(candidates: &[Vec<Rect>], glyphs: &[Rect]) -> Vec<usize> {
    let n = candidates.len();
    let mut state = vec![0usize; n]; // start at each label's preferred slot

    // Energy contributed by label i: position preference + overlap with
    // every other label's chosen box + overlap with all glyph obstacles
    // except its own anchor.
    let cost = |i: usize, state: &[usize]| -> f32 {
        let bi = &candidates[i][state[i]];
        let mut c = state[i] as f32 * 1.5;
        for (g, gb) in glyphs.iter().enumerate() {
            if g != i {
                c += bi.overlap(gb) * 0.6;
            }
        }
        for (j, cj) in candidates.iter().enumerate() {
            if j != i {
                c += bi.overlap(&cj[state[j]]) * 1.0;
            }
        }
        c
    };

    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    let mut t = 6.0_f32;
    for _sweep in 0..240 {
        for _ in 0..n {
            let i = (next() % n as u64) as usize;
            let slots = candidates[i].len();
            if slots < 2 {
                continue;
            }
            let old = state[i];
            let cand = (next() % slots as u64) as usize;
            if cand == old {
                continue;
            }
            let before = cost(i, &state);
            state[i] = cand;
            let after = cost(i, &state);
            let d = after - before;
            if d > 0.0 {
                // Linear-cooling acceptance: take a worse move only while
                // it's smaller than the temperature, with probability
                // (t - d)/t. No exp → deterministic + transcendental-free.
                let r = (next() % 1000) as f32 / 1000.0;
                if !(d < t && r < (t - d) / t) {
                    state[i] = old; // reject
                }
            }
        }
        t *= 0.97;
    }
    state
}

/// Polity-name labels at the territorial centroid (computed from
/// `world.society.control` — the per-cell polity_id) so the name lands
/// in the middle of the realm rather than at the capital. Large
/// italicized serif, tinted by polity color so it reads as "this
/// region belongs to this polity."
fn render_polity_labels(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    // Polity names use Cinzel (display) for monumental territorial
    // labels — fits the "ancient cartographer" voice better than a
    // generic serif. Italic styling is synthesized when the loaded
    // face has no native italic.
    out.push_str(
        r##"<g font-family='"Cinzel", Georgia, serif' font-style="italic" font-weight="bold" text-anchor="middle" fill-opacity="0.85" stroke="#f0e3bf" paint-order="stroke" stroke-width="3.0" stroke-linejoin="round">"##,
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
    // Sacred sites use IM Fell English (italic) — an early-modern
    // typeface with hand-pressed irregularities that reads "old
    // religious manuscript" without being intrusive at small size.
    out.push_str(
        r##"<g font-family='"IM Fell English", Georgia, serif' font-style="italic" font-size="9" text-anchor="middle" fill="#7a5a25" stroke="#f0e3bf" paint-order="stroke" stroke-width="1.6" stroke-linejoin="round">"##,
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

/// Names for major natural features. River names follow the channel via
/// `<textPath>` over a constructed (invisible) path so the label curves
/// with the water; lake names sit at the lake centroid. Both use the
/// IM Fell English italic in a watery blue, with a parchment halo so
/// they stay legible over the sea fill and ocean hatching. Only features
/// the naming stage labeled (major rivers / sizeable lakes) appear.
fn render_feature_labels(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;

    // Rivers — emit one invisible path per named river, then a textPath
    // label group referencing them by id.
    let named_rivers: Vec<(usize, &mapgen_core::world_data::River)> = world
        .hydrology
        .rivers
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.name.is_empty() && r.cells.len() >= 2)
        .collect();

    if !named_rivers.is_empty() {
        for (i, r) in &named_rivers {
            write!(
                out,
                r##"<path id="riverlbl-{i}" fill="none" stroke="none" d=""##
            )
            .unwrap();
            for (k, &c) in r.cells.iter().enumerate() {
                let p = mesh.sites[c as usize];
                write!(
                    out,
                    "{}{:.1},{:.1}",
                    if k == 0 { "M" } else { " L" },
                    p[0],
                    p[1]
                )
                .unwrap();
            }
            out.push_str(r##""/>"##);
        }
        out.push_str(
            r##"<g class="river-labels" font-family='"IM Fell English", Georgia, serif' font-style="italic" font-size="9" fill="#33597d" stroke="#f0e3bf" paint-order="stroke" stroke-width="1.4" stroke-linejoin="round">"##,
        );
        for (i, r) in &named_rivers {
            write!(
                out,
                r##"<text><textPath href="#riverlbl-{i}" startOffset="45%">{}</textPath></text>"##,
                xml_escape(&r.name),
            )
            .unwrap();
        }
        out.push_str("</g>");
    }

    // Lakes — centroid point labels.
    let has_named_lake = world.hydrology.lakes.iter().any(|l| !l.name.is_empty());
    if has_named_lake {
        out.push_str(
            r##"<g class="lake-labels" font-family='"IM Fell English", Georgia, serif' font-style="italic" font-size="9" text-anchor="middle" fill="#33597d" stroke="#f0e3bf" paint-order="stroke" stroke-width="1.4" stroke-linejoin="round">"##,
        );
        for lake in &world.hydrology.lakes {
            if lake.name.is_empty() || lake.cells.is_empty() {
                continue;
            }
            let (mut sx, mut sy) = (0.0_f32, 0.0_f32);
            for &c in &lake.cells {
                let p = mesh.sites[c as usize];
                sx += p[0];
                sy += p[1];
            }
            let n = lake.cells.len() as f32;
            write!(
                out,
                r##"<text x="{:.1}" y="{:.1}">{}</text>"##,
                sx / n,
                sy / n,
                xml_escape(&lake.name),
            )
            .unwrap();
        }
        out.push_str("</g>");
    }
}

/// Mountain-range names laid across each range's spine. A range is a
/// named cluster of peak cells; the label follows a west→east spine
/// (westmost peak → centroid → eastmost peak) via `<textPath>`, in
/// wide-tracked faded Cinzel italic — the classic antique convention of
/// a range name stretched across the chain. Spaced and low-opacity so
/// it sits behind the settlement labels in visual weight.
fn render_mountain_range_labels(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let named: Vec<(usize, &mapgen_core::world_data::MountainRange)> = world
        .mountain_ranges
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.name.is_empty() && r.cells.len() >= 2)
        .collect();
    if named.is_empty() {
        return;
    }

    for (i, r) in &named {
        let pts: Vec<[f32; 2]> = r.cells.iter().map(|&c| mesh.sites[c as usize]).collect();
        let west = pts
            .iter()
            .copied()
            .reduce(|a, b| if b[0] < a[0] { b } else { a })
            .unwrap();
        let east = pts
            .iter()
            .copied()
            .reduce(|a, b| if b[0] > a[0] { b } else { a })
            .unwrap();
        let cx = pts.iter().map(|p| p[0]).sum::<f32>() / pts.len() as f32;
        let cy = pts.iter().map(|p| p[1]).sum::<f32>() / pts.len() as f32;
        // West→east so the baseline reads left-to-right (never inverted).
        write!(
            out,
            r##"<path id="rangelbl-{i}" fill="none" stroke="none" d="M{:.1},{:.1} L{cx:.1},{cy:.1} L{:.1},{:.1}"/>"##,
            west[0], west[1], east[0], east[1],
        )
        .unwrap();
    }

    out.push_str(
        r##"<g class="range-labels" font-family='"Cinzel", Georgia, serif' font-style="italic" font-size="14" letter-spacing="4" fill="#5a4a32" fill-opacity="0.65" stroke="#f0e3bf" paint-order="stroke" stroke-width="1.6" stroke-linejoin="round">"##,
    );
    for (i, r) in &named {
        write!(
            out,
            r##"<text text-anchor="middle"><textPath href="#rangelbl-{i}" startOffset="50%">{}</textPath></text>"##,
            xml_escape(&r.name),
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

/// Edge-burn vignette — radial-gradient overlay painted on top of the
/// map content so the periphery fades into aged-paper shadow. Stronger
/// than the parchment gradient underneath (which tints the background);
/// this one darkens labels and glyphs that sit near the canvas edge,
/// matching the look of a real burnt-edge antique map.
fn render_edge_burn(w: f32, h: f32, out: &mut String) {
    out.push_str(
        r##"<g class="edge-burn"><rect width="100%" height="100%" fill="url(#edge-burn)" pointer-events="none"/>"##,
    );
    // Irregular burn stains: a handful of soft dark blobs scattered
    // around the perimeter so the aging reads as real scorching — some
    // corners darker than others, occasional splotch mid-edge — rather
    // than the mathematically uniform radial gradient underneath.
    // Positions / sizes / opacities are hash-driven (deterministic).
    const STAINS: u32 = 7;
    for k in 0..STAINS {
        let t = (k as f32 + 0.5) / STAINS as f32; // spread around perimeter
        let (px, py) = perimeter_point(t, w, h);
        let cx = px + hash_offset(900 + k, 1) * w * 0.05;
        let cy = py + hash_offset(900 + k, 2) * h * 0.05;
        let r = w.min(h) * 0.05 * (0.6 + hash_offset(900 + k, 3).abs() * 0.9);
        let op = 0.16 + hash_offset(900 + k, 4).abs() * 0.22;
        write!(
            out,
            r##"<ellipse class="edge-stain" cx="{cx:.1}" cy="{cy:.1}" rx="{r:.1}" ry="{ry:.1}" fill="#160f06" fill-opacity="{op:.2}" pointer-events="none"/>"##,
            ry = r * 0.7,
        )
        .unwrap();
    }
    out.push_str("</g>");
}

/// Map `t` in `[0, 1)` to a point walking clockwise around the canvas
/// rectangle perimeter, starting at the top-left corner. Used to scatter
/// edge-burn stains along the border.
fn perimeter_point(t: f32, w: f32, h: f32) -> (f32, f32) {
    let p = t.fract() * 2.0 * (w + h);
    if p < w {
        (p, 0.0)
    } else if p < w + h {
        (w, p - w)
    } else if p < 2.0 * w + h {
        (w - (p - w - h), h)
    } else {
        (0.0, h - (p - 2.0 * w - h))
    }
}

/// Compass rose in the NW corner — an 8-point star with 4 long
/// cardinal spikes and 4 shorter inter-cardinal spikes, centered
/// medallion, and "N" letter above. Sized for legibility across
/// 1024–2048 px canvas widths.
fn render_compass(w: f32, h: f32, out: &mut String) {
    // Place a comfortable inset from the NW corner. Scale modestly
    // with canvas size so the rose stays readable on wider exports.
    let scale = (w.min(h) / 1280.0).clamp(0.7, 1.4);
    let cx = 90.0 * scale;
    let cy = 90.0 * scale;
    let r_long = 42.0 * scale;
    let r_short = 18.0 * scale;
    let r_inner = 4.5 * scale;

    write!(
        out,
        r##"<g class="compass" transform="translate({cx:.1} {cy:.1})">"##,
    )
    .unwrap();

    // Cardinal star: 4 long spikes meeting at center. Polygon traces
    // each spike out-and-back with a small inset between spikes so the
    // outline reads as a star rather than a plus sign.
    write!(
        out,
        r##"<polygon points="{rl:.1},0 {ri:.1},{ri:.1} 0,{rl:.1} -{ri:.1},{ri:.1} -{rl:.1},0 -{ri:.1},-{ri:.1} 0,-{rl:.1} {ri:.1},-{ri:.1}" fill="#2a2418" stroke="#1a140e" stroke-width="0.8"/>"##,
        rl = r_long,
        ri = r_inner,
    )
    .unwrap();

    // Inter-cardinal star, rotated 45°. Shorter spikes layered on top
    // give the recognizable compass-rose silhouette.
    let r_off = r_short / std::f32::consts::SQRT_2;
    let i_off = r_inner / std::f32::consts::SQRT_2;
    write!(
        out,
        r##"<polygon points="{r1:.1},{r1:.1} {i:.1},0 {r1:.1},-{r1:.1} 0,-{i:.1} -{r1:.1},-{r1:.1} -{i:.1},0 -{r1:.1},{r1:.1} 0,{i:.1}" fill="#5a3a25" stroke="#1a140e" stroke-width="0.6"/>"##,
        r1 = r_off,
        i = i_off,
    )
    .unwrap();

    // Centered medallion.
    write!(
        out,
        r##"<circle cx="0" cy="0" r="{rm:.1}" fill="#dfca96" stroke="#1a140e" stroke-width="0.9"/>"##,
        rm = r_inner * 1.4,
    )
    .unwrap();

    // North marker letter above the rose, in Cinzel.
    let n_y = -(r_long + 14.0 * scale);
    write!(
        out,
        r##"<text x="0" y="{n_y:.1}" font-family='"Cinzel", Georgia, serif' font-size="{fs:.1}" text-anchor="middle" font-weight="bold" fill="#2a2418">N</text>"##,
        fs = 13.0 * scale,
    )
    .unwrap();

    out.push_str("</g>");
}

/// Decorative cartouche in the SE corner — title block in a scroll-
/// like frame. Today carries a generic "A Map of the Known World"
/// title; future work may parameterize the title text on world
/// metadata (oldest polity name, hemisphere, etc.).
fn render_cartouche(w: f32, h: f32, out: &mut String) {
    let scale = (w.min(h) / 1280.0).clamp(0.7, 1.6);
    let box_w = 360.0 * scale;
    let box_h = 78.0 * scale;
    let x0 = w - box_w - 50.0 * scale;
    let y0 = h - box_h - 50.0 * scale;
    let cx = x0 + box_w * 0.5;
    let cy = y0 + box_h * 0.5;

    write!(out, r##"<g class="cartouche">"##).unwrap();

    // Outer scroll: rounded plaque shape with a slightly darker
    // parchment fill so it reads as separate from the map background.
    write!(
        out,
        r##"<rect x="{x0:.1}" y="{y0:.1}" width="{box_w:.1}" height="{box_h:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="#e8d8a8" stroke="#2a2418" stroke-width="{sw:.1}" fill-opacity="0.92"/>"##,
        rx = 6.0 * scale,
        sw = 1.4 * scale,
    )
    .unwrap();

    // Inner border line — adds the engraved double-line look.
    let inset = 5.0 * scale;
    write!(
        out,
        r##"<rect x="{x:.1}" y="{y:.1}" width="{ww:.1}" height="{hh:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="none" stroke="#2a2418" stroke-width="{sw:.1}" stroke-opacity="0.55"/>"##,
        x = x0 + inset,
        y = y0 + inset,
        ww = box_w - inset * 2.0,
        hh = box_h - inset * 2.0,
        rx = 4.0 * scale,
        sw = 0.7 * scale,
    )
    .unwrap();

    // Title text — Cinzel small-caps for the antique typeset feel.
    write!(
        out,
        r##"<text x="{cx:.1}" y="{cy:.1}" font-family='"Cinzel", Georgia, serif' font-size="{fs:.1}" font-weight="bold" text-anchor="middle" dominant-baseline="middle" fill="#2a2418">A MAP OF THE KNOWN WORLD</text>"##,
        fs = 18.0 * scale,
    )
    .unwrap();

    out.push_str("</g>");
}

/// Sacred sites, with a per-pantheon symbol so faiths read distinctly
/// on multi-religion worlds instead of all sharing one gold diamond.
/// The symbol is keyed off each religion's `PantheonPattern`:
/// Mono → cross, Poly → many-rayed sun, Dual → light/dark split disc,
/// Animism → leaf, Ancestor → memorial tablet, CosmicOrder → ring.
/// Each site is wrapped in `<g class="sacred {pantheon}">` so the
/// dispatch is test-pinnable without coupling to glyph geometry.
fn render_sacred_sites(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    out.push_str(
        r##"<g class="sacred-sites" stroke="#cc9933" stroke-width="0.9" fill="#ffcc66" fill-opacity="0.9" stroke-linejoin="round">"##,
    );
    for religion in &world.religions.religions {
        for &cell in &religion.sacred_sites {
            let p = mesh.sites[cell as usize];
            write!(
                out,
                r##"<g class="sacred {}">"##,
                pantheon_class(religion.pantheon)
            )
            .unwrap();
            draw_sacred_glyph(religion.pantheon, p[0], p[1], out);
            out.push_str("</g>");
        }
    }
    out.push_str("</g>");
}

fn pantheon_class(p: PantheonPattern) -> &'static str {
    match p {
        PantheonPattern::Mono => "mono",
        PantheonPattern::Poly => "poly",
        PantheonPattern::Dual => "dual",
        PantheonPattern::Animism => "animism",
        PantheonPattern::Ancestor => "ancestor",
        PantheonPattern::CosmicOrder => "cosmic",
    }
}

/// Unit-circle directions at 22.5° steps (cos, sin), precomputed so the
/// Poly sun-star needs no runtime trigonometry. `SQ` is 1/√2, the 45°
/// component (named to dodge clippy's approx-constant lint).
const SQ: f32 = std::f32::consts::FRAC_1_SQRT_2;
#[rustfmt::skip]
const UNIT16: [(f32, f32); 16] = [
    (1.0, 0.0), (0.92388, 0.38268), (SQ, SQ), (0.38268, 0.92388),
    (0.0, 1.0), (-0.38268, 0.92388), (-SQ, SQ), (-0.92388, 0.38268),
    (-1.0, 0.0), (-0.92388, -0.38268), (-SQ, -SQ), (-0.38268, -0.92388),
    (0.0, -1.0), (0.38268, -0.92388), (SQ, -SQ), (0.92388, -0.38268),
];

/// Per-pantheon sacred-site symbol, ~4 px radius, centered on `(cx, cy)`.
/// Inherits the gold fill/stroke from the enclosing group.
fn draw_sacred_glyph(p: PantheonPattern, cx: f32, cy: f32, out: &mut String) {
    let r = 4.0_f32;
    match p {
        // Latin cross — taller vertical bar, crossbar near the top.
        PantheonPattern::Mono => {
            write!(
                out,
                r##"<line x1="{cx:.1}" y1="{:.1}" x2="{cx:.1}" y2="{:.1}" stroke-width="1.6"/><line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke-width="1.6"/>"##,
                cy - r,
                cy + r,
                cx - r * 0.6,
                cy - r * 0.35,
                cx + r * 0.6,
                cy - r * 0.35,
            )
            .unwrap();
        }
        // Many-rayed sun: an 8-point star polygon. Vertices come from a
        // precomputed 16-step unit table (no runtime trig — keeps the
        // renderer free of transcendentals), alternating full radius for
        // the spikes and 0.45× for the notches between them.
        PantheonPattern::Poly => {
            out.push_str(r##"<polygon points=""##);
            for (k, &(ux, uy)) in UNIT16.iter().enumerate() {
                let rad = if k % 2 == 0 { r } else { r * 0.45 };
                if k > 0 {
                    out.push(' ');
                }
                write!(out, "{:.1},{:.1}", cx + rad * ux, cy + rad * uy).unwrap();
            }
            out.push_str(r##""/>"##);
        }
        // Duality: a disc with its right half darkened.
        PantheonPattern::Dual => {
            write!(out, r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}"/>"##).unwrap();
            write!(
                out,
                r##"<path d="M {cx:.1} {:.1} A {r:.1} {r:.1} 0 0 1 {cx:.1} {:.1} Z" fill="#7a5a1a" stroke="none"/>"##,
                cy - r,
                cy + r,
            )
            .unwrap();
        }
        // Leaf: a pointed ellipse-ish lens with a center vein.
        PantheonPattern::Animism => {
            write!(
                out,
                r##"<path d="M {cx:.1} {:.1} Q {:.1} {cy:.1} {cx:.1} {:.1} Q {:.1} {cy:.1} {cx:.1} {:.1} Z"/><line x1="{cx:.1}" y1="{:.1}" x2="{cx:.1}" y2="{:.1}" stroke-width="0.6"/>"##,
                cy - r,
                cx + r * 0.7,
                cy + r,
                cx - r * 0.7,
                cy - r,
                cy - r * 0.8,
                cy + r * 0.8,
            )
            .unwrap();
        }
        // Ancestor tablet: a tall rounded-top stele.
        PantheonPattern::Ancestor => {
            write!(
                out,
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" ry="{:.1}"/>"##,
                cx - r * 0.55,
                cy - r,
                r * 1.1,
                r * 2.0,
                r * 0.55,
                r * 0.55,
            )
            .unwrap();
        }
        // Cosmic order: a plain ring with a center dot.
        PantheonPattern::CosmicOrder => {
            write!(
                out,
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="none" stroke-width="1.4"/><circle cx="{cx:.1}" cy="{cy:.1}" r="0.9"/>"##
            )
            .unwrap();
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn town_scale_grows_with_population_and_stays_bounded() {
        // The whole point of the feature: a more-populous town renders a
        // visibly bigger glyph than a small one.
        assert!(
            town_scale(0.9) > town_scale(0.3),
            "higher population must produce a larger town glyph"
        );
        // Stay inside the documented 0.6×–0.95× band so towns never
        // out-size a capital (1.0×) or shrink into village territory.
        for p in [0.0_f32, 0.1, 0.5, 0.9, 1.0] {
            let s = town_scale(p);
            assert!(
                (0.6..=0.95).contains(&s),
                "town_scale({p}) = {s} escaped the [0.6, 0.95] band"
            );
        }
    }
}

//! Phase 2 data-honest render. Shows everything Phase 2 produces:
//! biome-colored cells, a traced coastline, river polylines.
//! No ornate styling — this is the dev view that proves the data is right.

use std::fmt::Write;

use mapgen_core::world_data::WorldData;

pub fn render(world: &WorldData) -> String {
    let mesh = &world.mesh;
    let w = mesh.width;
    let h = mesh.height;

    let mut out = String::with_capacity(mesh.cell_count() * 96);
    write!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0} {h:.0}" width="{w:.0}" height="{h:.0}">"##
    )
    .unwrap();

    // Parchment background so unfilled gaps don't show neon white.
    out.push_str(r##"<rect width="100%" height="100%" fill="#f4ebd0"/>"##);

    // --- Biome-filled cell polygons --------------------------------------
    let biomes = &world.climate.biome;
    let elev = &world.terrain.elevation;
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() {
            continue;
        }
        let fill = biome_color(
            *biomes.get(i).unwrap_or(&0),
            elev.get(i).copied().unwrap_or(0.0),
        );
        out.push_str(r##"<polygon points=""##);
        for (k, &vi) in verts.iter().enumerate() {
            let v = mesh.vertices[vi as usize];
            if k > 0 {
                out.push(' ');
            }
            write!(out, "{:.1},{:.1}", v[0], v[1]).unwrap();
        }
        write!(out, r##"" fill="{fill}"/>"##).unwrap();
    }

    // --- Coastline trace -------------------------------------------------
    // For every undirected neighbor pair (i, j) with one land + one sea,
    // emit the shared cell-boundary edge.
    out.push_str(r##"<g stroke="#2a2418" stroke-width="1.2" stroke-linecap="round" fill="none">"##);
    for i in 0..mesh.cell_count() {
        let i_land = elev[i] > 0.0;
        for &nj in &mesh.neighbors[i] {
            let j = nj as usize;
            if j <= i {
                continue; // each edge once
            }
            let j_land = elev[j] > 0.0;
            if i_land == j_land {
                continue;
            }
            if let Some((va, vb)) = shared_edge(&mesh.cell_vertices[i], &mesh.cell_vertices[j]) {
                let a = mesh.vertices[va as usize];
                let b = mesh.vertices[vb as usize];
                write!(
                    out,
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                    a[0], a[1], b[0], b[1]
                )
                .unwrap();
            }
        }
    }
    out.push_str("</g>");

    // --- Rivers ----------------------------------------------------------
    //
    // Each consecutive cell-pair in a river chain is its own segment
    // with a width derived from the upstream cell's flow value, so the
    // river visibly widens at every confluence. Without this, the
    // single per-river width misses the entire "river growth" story.
    out.push_str(
        r##"<g stroke="#3b6d99" fill="none" stroke-linecap="round" stroke-linejoin="round">"##,
    );
    let flow = &world.hydrology.flow;
    for river in &world.hydrology.rivers {
        if river.cells.len() < 2 {
            continue;
        }
        for w in river.cells.windows(2) {
            let a = mesh.sites[w[0] as usize];
            let b = mesh.sites[w[1] as usize];
            // Width from the flow at the downstream cell — captures growth.
            let f = flow.get(w[1] as usize).copied().unwrap_or(1.0);
            let sw = (f.sqrt() * 0.18).clamp(0.5, 5.0);
            write!(
                out,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke-width="{:.2}"/>"##,
                a[0], a[1], b[0], b[1], sw
            )
            .unwrap();
        }
    }
    out.push_str("</g>");

    out.push_str("</svg>");
    out
}

/// Find the (up to) two vertex indices shared by two adjacent cells.
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

/// Whittaker-derived palette + sea by depth. Uses biome IDs from
/// `mapgen_world::biomes` (we replicate them here to keep this crate
/// independent of mapgen-world).
fn biome_color(biome: u8, elev: f32) -> &'static str {
    // Sea overrides — depth gradient.
    if elev <= 0.0 {
        return if elev < -0.3 { "#3a5d85" } else { "#7da6c8" };
    }
    match biome {
        0 => "#f5f8fc",  // SNOW
        1 => "#c4cdb7",  // TUNDRA
        2 => "#2d5e3e",  // TAIGA
        3 => "#5d8a4e",  // TEMPERATE_FOREST
        4 => "#c2c97c",  // TEMPERATE_GRASSLAND
        5 => "#3a6b3c",  // TEMPERATE_RAINFOREST
        6 => "#e8d49a",  // DESERT
        7 => "#d8ba6b",  // SAVANNA
        8 => "#2e6b3e",  // TROPICAL_RAINFOREST
        9 => "#8aa057",  // TROPICAL_DRY_FOREST
        10 => "#b5b07a", // SHRUBLAND
        11 => "#b8b3a8", // ALPINE
        12 => "#7da6c8", // SEA_SHALLOW
        13 => "#3a5d85", // SEA_DEEP
        14 => "#4d8a3a", // RIPARIAN — saturated green river corridor
        _ => "#a0a0a0",  // unassigned / unknown
    }
}

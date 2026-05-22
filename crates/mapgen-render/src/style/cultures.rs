//! Phase 3a data-honest render. Cells colored by the `Race` of the
//! assigned culture. Coastline + river overlays are unchanged from the
//! biomes style — culture territories sit on top of geography so the
//! eye can still trace the underlying landform.
//!
//! Not the ornate render (that's Phase 3e). This is the dev view that
//! proves cultures::populate produced sensible territories. The
//! per-race color palette is stable across seeds so seed comparison
//! works.

use std::fmt::Write;

use mapgen_core::entities::Race;
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

    // Parchment background — visible in the gaps between polygons and on
    // any unassigned land.
    out.push_str(r##"<rect width="100%" height="100%" fill="#f4ebd0"/>"##);

    // Resolve each cell's color: sea by depth, land by culture's race,
    // unassigned land falls through to a pale parchment-aligned grey so
    // it reads as "no culture here" rather than as the dominant
    // anything-else hue.
    let elev = &world.terrain.elevation;
    let culture_id = &world.cultures.culture_id;
    let cultures = &world.cultures.cultures;
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() {
            continue;
        }
        let cell_elev = elev.get(i).copied().unwrap_or(0.0);
        let fill = if cell_elev <= 0.0 {
            sea_color(cell_elev)
        } else {
            // Land cell: look up culture_id, then race, then color.
            culture_id
                .get(i)
                .and_then(|slot| *slot)
                .and_then(|id| cultures.get(id as usize))
                .map(|culture| race_color(culture.race))
                .unwrap_or("#d4cca8") // unassigned-land parchment
        };
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

    // Coastline trace — same algorithm as the biomes renderer; for every
    // land/sea neighbor pair, emit the shared cell-boundary edge.
    out.push_str(r##"<g stroke="#2a2418" stroke-width="1.2" stroke-linecap="round" fill="none">"##);
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

    // Rivers — flow-width segments, identical to the biomes renderer.
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

/// Stable per-race palette. Hues chosen so the five MVP archetypes
/// (Human / Elf / Dwarf / Orc / Halfling) read as visually distinct
/// territories — no two adjacent in the hue wheel. The four extra races
/// (Lizardfolk / SeaFolk / Underdark / Giant) have placeholder colors
/// for when later phases load them via CSV; pick-against the existing
/// hues if you tune.
fn race_color(race: Race) -> &'static str {
    match race {
        Race::Human => "#6b8bb5",      // slate blue — river-valley humans
        Race::Elf => "#6b8e23",        // olive — wood elves
        Race::Dwarf => "#a0522d",      // sienna — mountain dwarves
        Race::Orc => "#b04a4a",        // dusty red — marginal-land orcs
        Race::Halfling => "#daa520",   // goldenrod — pastoral halflings
        Race::Lizardfolk => "#4a7c59", // swamp green
        Race::SeaFolk => "#2e8b8b",    // teal
        Race::Underdark => "#6a4a7c",  // muted purple
        Race::Giant => "#a3c4d4",      // ice blue
    }
}

fn sea_color(elev: f32) -> &'static str {
    if elev < -0.3 {
        "#3a5d85" // deep
    } else {
        "#7da6c8" // shallow
    }
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

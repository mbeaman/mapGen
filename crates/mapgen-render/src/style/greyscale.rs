//! Phase 1 dev visualization: filled Voronoi polygons colored by elevation.
//! Ocean = blue ramp; land = green/brown/white ramp. No styling, just enough
//! to verify the geography pipeline.

use mapgen_core::{fmath, world_data::WorldData};
use std::fmt::Write;

pub fn render(world: &WorldData) -> String {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;
    let w = mesh.width;
    let h = mesh.height;

    let mut out = String::with_capacity(mesh.cell_count() * 96);
    write!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0} {h:.0}" width="{w:.0}" height="{h:.0}">"##
    )
    .unwrap();
    // Sea background — paints between the polygons (which fill cell area).
    out.push_str(r##"<rect width="100%" height="100%" fill="#1e3a5f"/>"##);

    for (cid, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() {
            continue;
        }
        let e = elev.get(cid).copied().unwrap_or(0.0);
        let fill = color_for(e);
        out.push_str(r##"<polygon points=""##);
        for (k, &vi) in verts.iter().enumerate() {
            let v = mesh.vertices[vi as usize];
            if k > 0 {
                out.push(' ');
            }
            // 1-decimal precision — keeps the SVG small without visibly
            // losing the antialiased boundaries.
            write!(out, "{:.1},{:.1}", v[0], v[1]).unwrap();
        }
        write!(out, r##"" fill="{fill}"/>"##).unwrap();
    }

    out.push_str("</svg>");
    out
}

/// Elevation → CSS color. Blue ramp under sea level, green→brown→white above.
fn color_for(e: f32) -> String {
    let e = fmath::clamp(e, -1.0, 1.0);
    if e < 0.0 {
        // -1 → deep blue (10,30,60); 0 → light blue (90,140,180).
        let t = (e + 1.0).clamp(0.0, 1.0); // 0 deep → 1 shallow
        let r = lerp(10.0, 90.0, t);
        let g = lerp(30.0, 140.0, t);
        let b = lerp(60.0, 180.0, t);
        rgb(r, g, b)
    } else {
        // 0 → tan (200,190,140); 0.5 → green (90,140,70); 1 → snow (250,250,250)
        if e < 0.5 {
            let t = e / 0.5;
            let r = lerp(200.0, 90.0, t);
            let g = lerp(190.0, 140.0, t);
            let b = lerp(140.0, 70.0, t);
            rgb(r, g, b)
        } else {
            let t = (e - 0.5) / 0.5;
            let r = lerp(90.0, 250.0, t);
            let g = lerp(140.0, 250.0, t);
            let b = lerp(70.0, 250.0, t);
            rgb(r, g, b)
        }
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn rgb(r: f32, g: f32, b: f32) -> String {
    format!("#{:02x}{:02x}{:02x}", r as u8, g as u8, b as u8)
}

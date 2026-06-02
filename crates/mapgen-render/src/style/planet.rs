//! Planet / world-overview render — the zoomed-*out* "planisphere".
//!
//! Where `ornate_antique` draws one continent at reading distance (forests,
//! coastline ripples, settlement clusters), this draws the whole world as an
//! antique world map: biome-tinted continents over a depth-shaded sea, a
//! lat/long graticule, the major rivers and ranges only, and engraved
//! continent / ocean labels. It deliberately drops the per-cell clutter that
//! would be illegible (and slow) at planetary scale.
//!
//! It shares the parchment palette, embedded typography, compass and cartouche
//! with the ornate style (reused via `pub(crate)` helpers there), so the planet
//! reads as the same atlas one zoom band out — drill into any sector with the
//! Phase-7 refinement to get the continental ornate view of that region.

use std::fmt::Write;

use mapgen_core::WorldData;

use super::ornate_antique::{
    ornate_biome_color, render_cartouche, render_compass, FONT_FACE_BLOCK,
};

pub fn render(world: &WorldData) -> String {
    let mesh = &world.mesh;
    let [vx, vy, vx1, vy1] = mesh.view_rect();
    let (w, h) = (vx1 - vx, vy1 - vy);

    let mut out = String::with_capacity(mesh.cell_count() * 80 + 4096);
    write!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vx:.0} {vy:.0} {w:.0} {h:.0}" width="{w:.0}" height="{h:.0}">"##
    )
    .unwrap();
    out.push_str("<defs>");
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
    write!(
        out,
        r##"<rect x="{vx:.0}" y="{vy:.0}" width="{w:.0}" height="{h:.0}" fill="url(#parchment)"/>"##
    )
    .unwrap();

    // Every world-space coordinate below is run through the Mollweide `project`
    // (world → the oval planisphere) via this view rect; the decorative
    // compass/cartouche/legend/vignette stay in screen space.
    let proj = Proj::new([vx, vy, w, h]);
    render_fill(world, &proj, &mut out);
    // Political wash over the inhabited third, under the linework. Empty until a
    // world has society; with the time-slider it animates empires rise + fall
    // (renderAtYear swaps control before re-rendering, so this gets it for free).
    render_political(world, &proj, &mut out);
    render_coast(world, &proj, &mut out);
    render_graticule(&proj, &mut out);
    render_major_rivers(world, &proj, &mut out);
    render_major_ranges(world, &proj, &mut out);
    render_labels(world, &proj, &mut out);

    // A soft radial vignette (the ornate edge-burn gradient) without the ornate
    // ink-stain blobs, which read as blemishes on a clean planisphere.
    write!(
        out,
        r##"<rect x="{vx:.0}" y="{vy:.0}" width="{w:.0}" height="{h:.0}" fill="url(#edge-burn)" pointer-events="none"/>"##
    )
    .unwrap();
    render_compass(w, h, &mut out);
    render_cartouche(w, h, "ORBIS TERRARUM", &mut out);
    render_nation_legend(world, w, h, &mut out);
    out.push_str("</svg>");
    out
}

/// Per-cell fill: land in its biome colour (matching the ornate detailed view),
/// sea shaded by depth so basins and shelves read.
fn render_fill(world: &WorldData, proj: &Proj, out: &mut String) {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;
    let biomes = &world.climate.biome;
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() {
            continue;
        }
        let e = elev.get(i).copied().unwrap_or(0.0);
        let fill = if e > 0.0 {
            ornate_biome_color(biomes.get(i).copied().unwrap_or(0), e).to_string()
        } else {
            sea_color(e)
        };
        write_polygon(mesh, verts, &fill, 0.85, proj, out);
    }
}

/// Shallow shelf → deep basin, a muted antique blue-grey (depth = -elev in [0,1]).
fn sea_color(elev: f32) -> String {
    let d = (-elev).clamp(0.0, 1.0);
    // shelf #acc2c6 → abyss #5d7a86
    let mix = |a: f32, b: f32| (a + (b - a) * d).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        mix(172.0, 93.0),
        mix(194.0, 122.0),
        mix(198.0, 134.0)
    )
}

/// A sepia stroke along the coast: outline every land cell that touches the sea.
/// (At planetary scale a per-cell coast band reads as a clean shoreline.)
fn render_coast(world: &WorldData, proj: &Proj, out: &mut String) {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;
    out.push_str(r##"<g fill="none" stroke="#5a4326" stroke-width="0.6" stroke-opacity="0.7">"##);
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() || elev.get(i).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let coastal = mesh
            .neighbors
            .get(i)
            .map(|ns| {
                ns.iter()
                    .any(|&n| elev.get(n as usize).copied().unwrap_or(0.0) <= 0.0)
            })
            .unwrap_or(false);
        if coastal {
            write_polygon_outline(mesh, verts, proj, out);
        }
    }
    out.push_str("</g>");
}

/// A faint lat/long graticule — the planisphere grid. Each meridian/parallel is
/// a *projected* polyline (subdivided), so meridians bow toward the pinched poles
/// and parallels compress with latitude — the visible globe-edge cue.
fn render_graticule(proj: &Proj, out: &mut String) {
    let (vx, vy, w, h) = (proj.vx, proj.vy, proj.vw, proj.vh);
    let (nx, ny) = (8u32, 4u32);
    let samples = 32u32; // subdivisions per line — enough for a smooth curve
    out.push_str(r##"<g fill="none" stroke="#5a4326" stroke-width="0.5" stroke-opacity="0.22">"##);
    // Meridians: constant world-x (longitude), sampled down world-y.
    for k in 0..=nx {
        let wx = vx + w * k as f32 / nx as f32;
        out.push_str(r##"<polyline points=""##);
        for s in 0..=samples {
            let wy = vy + h * s as f32 / samples as f32;
            let (px, py) = proj.project(wx, wy);
            if s > 0 {
                out.push(' ');
            }
            write!(out, "{px:.1},{py:.1}").unwrap();
        }
        out.push_str(r##""/>"##);
    }
    // Parallels: constant world-y (latitude), sampled across world-x.
    for k in 0..=ny {
        let wy = vy + h * k as f32 / ny as f32;
        out.push_str(r##"<polyline points=""##);
        for s in 0..=samples {
            let wx = vx + w * s as f32 / samples as f32;
            let (px, py) = proj.project(wx, wy);
            if s > 0 {
                out.push(' ');
            }
            write!(out, "{px:.1},{py:.1}").unwrap();
        }
        out.push_str(r##""/>"##);
    }
    out.push_str("</g>");
}

/// Major rivers only (Strahler ≥ 4) as thin polylines — tributaries are noise
/// at this scale.
fn render_major_rivers(world: &WorldData, proj: &Proj, out: &mut String) {
    let mesh = &world.mesh;
    out.push_str(r##"<g fill="none" stroke="#4f6f86" stroke-opacity="0.8" stroke-linejoin="round" stroke-linecap="round">"##);
    for river in &world.hydrology.rivers {
        if river.strahler < 4 || river.cells.len() < 2 {
            continue;
        }
        let width = 0.5 + 0.35 * (river.strahler as f32 - 3.0);
        write!(out, r##"<polyline stroke-width="{width:.1}" points=""##).unwrap();
        for (k, &c) in river.cells.iter().enumerate() {
            let p = mesh.sites[c as usize];
            let (px, py) = proj.project(p[0], p[1]);
            if k > 0 {
                out.push(' ');
            }
            write!(out, "{px:.1},{py:.1}").unwrap();
        }
        out.push_str(r##""/>"##);
    }
    out.push_str("</g>");
}

/// The largest mountain ranges as a small glyph + engraved name.
fn render_major_ranges(world: &WorldData, proj: &Proj, out: &mut String) {
    let mesh = &world.mesh;
    let mut ranges: Vec<&mapgen_core::MountainRange> = world
        .mountain_ranges
        .iter()
        .filter(|r| r.cells.len() >= 6)
        .collect();
    ranges.sort_by_key(|r| std::cmp::Reverse(r.cells.len()));
    out.push_str(r##"<g fill="#5a4326">"##);
    for r in ranges.iter().take(8) {
        let (wcx, wcy) = centroid(mesh, r.cells.iter().map(|&c| c as usize));
        // Project the anchor, then place the glyph + label offsets in SCREEN space
        // so the peak mark and text stay upright and unscaled under the projection.
        let (cx, cy) = proj.project(wcx, wcy);
        // A small triangular peak mark.
        write!(
            out,
            r##"<path d="M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z" fill-opacity="0.8"/>"##,
            cx - 5.0,
            cy + 4.0,
            cx,
            cy - 6.0,
            cx + 5.0,
            cy + 4.0,
        )
        .unwrap();
        write!(
            out,
            r##"<text x="{cx:.1}" y="{:.1}" font-family='"IM Fell English", Georgia, serif' font-size="11" font-style="italic" text-anchor="middle" fill="#3a2c18" fill-opacity="0.85">{}</text>"##,
            cy + 17.0,
            escape(&r.name),
        )
        .unwrap();
    }
    out.push_str("</g>");
}

/// Engraved continent + ocean labels — read straight from the names the naming
/// stage grounded in each body's dominant culture (`world.continents` /
/// `world.oceans`), uppercased for the antique-map register. The flood-fill that
/// used to run here now lives in the pipeline, so the renderer just places the
/// labels at the stored centroids.
fn render_labels(world: &WorldData, proj: &Proj, out: &mut String) {
    let n = world.mesh.cell_count().max(1);
    out.push_str(
        r##"<g font-family='"Cinzel", Georgia, serif' text-anchor="middle" fill="#3a2c18">"##,
    );

    // Continents: the largest named landmasses (the stage already dropped specks
    // and ordered them largest-first), sized by their share of the world.
    for cont in world.continents.iter().take(6) {
        if cont.name.is_empty() {
            continue;
        }
        let (cx, cy) = proj.project(cont.centroid[0], cont.centroid[1]);
        let size = (12.0 + (cont.cell_count as f32 / n as f32) * 60.0).min(34.0);
        // Escape after uppercasing — to_uppercase would turn "&amp;" into "&AMP;".
        let label = escape(&cont.name.to_uppercase());
        write!(
            out,
            r##"<text x="{cx:.1}" y="{cy:.1}" font-size="{size:.0}" letter-spacing="2" fill-opacity="0.5">{label}</text>"##,
        )
        .unwrap();
    }

    // The widest ocean gets one engraved label: "MARE" — the cartographer's Latin
    // for "sea" — plus the proper noun grounded in its coastal cultures.
    if let Some(ocean) = world.oceans.first() {
        if !ocean.name.is_empty() {
            let (cx, cy) = proj.project(ocean.centroid[0], ocean.centroid[1]);
            let label = escape(&ocean.name.to_uppercase());
            write!(
                out,
                r##"<text x="{cx:.1}" y="{cy:.1}" font-size="22" font-style="italic" letter-spacing="3" fill="#3a4e57" fill-opacity="0.5">MARE {label}</text>"##,
            )
            .unwrap();
        }
    }
    out.push_str("</g>");
}

/// Political wash: each controlled land cell filled by its realm's colour at low
/// opacity, so the biome continents still read through. Empty (early-returns)
/// until a world has society. Mirrors `ornate_antique::render_political`; with
/// the time-slider (`renderAtYear` swaps `control` before re-rendering) it
/// animates empires rise and fall across the planisphere.
fn render_political(world: &WorldData, proj: &Proj, out: &mut String) {
    let mesh = &world.mesh;
    let control = &world.society.control;
    if control.is_empty() {
        return;
    }
    let elev = &world.terrain.elevation;
    out.push_str(r##"<g class="planet-political">"##);
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() || elev.get(i).copied().unwrap_or(0.0) <= 0.0 {
            continue; // land only
        }
        let Some(pid) = control.get(i).copied().flatten() else {
            continue;
        };
        let Some(nation) = world.society.nations.get(pid as usize) else {
            continue;
        };
        let [r, g, b] = nation.color;
        write_polygon(
            mesh,
            verts,
            &format!("#{r:02x}{g:02x}{b:02x}"),
            0.40,
            proj,
            out,
        );
    }
    out.push_str("</g>");
}

/// A key in the SW corner: each realm that holds land, by colour + name — so the
/// political wash reads as "the realm of X" rather than anonymous blobs. Reflects
/// the *rendered* control, so with the time-slider it tracks who exists each
/// year. Empty (early-returns) until a world has society.
fn render_nation_legend(world: &WorldData, _w: f32, h: f32, out: &mut String) {
    let control = &world.society.control;
    let elev = &world.terrain.elevation;
    // Realms holding ≥1 land cell in the rendered control, by ascending id.
    let mut seen = std::collections::BTreeSet::new();
    for (i, c) in control.iter().enumerate() {
        if elev.get(i).copied().unwrap_or(0.0) > 0.0 {
            if let Some(pid) = *c {
                seen.insert(pid);
            }
        }
    }
    let realms: Vec<u32> = seen.into_iter().collect();
    if realms.is_empty() {
        return;
    }

    let scale = (h / 1024.0).clamp(0.7, 1.4);
    let row_h = 22.0 * scale;
    let pad = 12.0 * scale;
    let sw = 15.0 * scale; // swatch edge
    let fs = 13.0 * scale;
    let box_w = 200.0 * scale;
    let box_h = pad * 2.0 + row_h * (realms.len() as f32 + 1.0); // +1 title row
    let x0 = 50.0 * scale;
    let y0 = h - box_h - 50.0 * scale;
    let tx = x0 + pad;

    write!(out, r##"<g class="nation-legend">"##).unwrap();
    write!(
        out,
        r##"<rect x="{x0:.1}" y="{y0:.1}" width="{box_w:.1}" height="{box_h:.1}" rx="6" fill="#f0e3bf" fill-opacity="0.82" stroke="#5a3a25" stroke-width="1.2"/>"##
    )
    .unwrap();
    let mut ty = y0 + pad + row_h * 0.7;
    write!(
        out,
        r##"<text x="{tx:.1}" y="{ty:.1}" font-family='"Cinzel", Georgia, serif' font-size="{fs:.1}" font-weight="bold" letter-spacing="2" fill="#2a2418">REALMS</text>"##
    )
    .unwrap();
    for pid in realms {
        ty += row_h;
        let Some(nation) = world.society.nations.get(pid as usize) else {
            continue;
        };
        let [r, g, b] = nation.color;
        let sy = ty - sw * 0.85;
        write!(
            out,
            r##"<rect x="{tx:.1}" y="{sy:.1}" width="{sw:.1}" height="{sw:.1}" fill="#{r:02x}{g:02x}{b:02x}" stroke="#2a2418" stroke-width="0.8"/>"##
        )
        .unwrap();
        let name = escape(&nation.name);
        let nx = tx + sw + 8.0 * scale;
        write!(
            out,
            r##"<text x="{nx:.1}" y="{ty:.1}" font-family='"EB Garamond", Georgia, serif' font-size="{fs:.1}" fill="#2a2418">{name}</text>"##
        )
        .unwrap();
    }
    out.push_str("</g>");
}

// ---- helpers ----------------------------------------------------------------

fn centroid(mesh: &mapgen_core::MeshData, cells: impl IntoIterator<Item = usize>) -> (f32, f32) {
    let (mut sx, mut sy, mut k) = (0.0f32, 0.0f32, 0usize);
    for c in cells {
        let p = mesh.sites[c];
        sx += p[0];
        sy += p[1];
        k += 1;
    }
    let k = k.max(1) as f32;
    (sx / k, sy / k)
}

fn write_polygon(
    mesh: &mapgen_core::MeshData,
    verts: &[u32],
    fill: &str,
    opacity: f32,
    proj: &Proj,
    out: &mut String,
) {
    out.push_str(r##"<polygon points=""##);
    for (k, &vi) in verts.iter().enumerate() {
        let v = mesh.vertices[vi as usize];
        let (px, py) = proj.project(v[0], v[1]);
        if k > 0 {
            out.push(' ');
        }
        write!(out, "{px:.1},{py:.1}").unwrap();
    }
    write!(out, r##"" fill="{fill}" fill-opacity="{opacity}"/>"##).unwrap();
}

fn write_polygon_outline(
    mesh: &mapgen_core::MeshData,
    verts: &[u32],
    proj: &Proj,
    out: &mut String,
) {
    out.push_str(r##"<polygon points=""##);
    for (k, &vi) in verts.iter().enumerate() {
        let v = mesh.vertices[vi as usize];
        let (px, py) = proj.project(v[0], v[1]);
        if k > 0 {
            out.push(' ');
        }
        write!(out, "{px:.1},{py:.1}").unwrap();
    }
    out.push_str(r##""/>"##);
}

/// Minimal XML-text escaping for label content.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---- Mollweide projection (the globe-edge look) -----------------------------
//
// World coords are equirectangular in the view rect `[vx, vy, vw, vh]`; this
// maps them onto the Mollweide oval inscribed in the same rect (equal-area,
// whole world, pinched poles). The frontend's `mollweideProject` /
// `mollweideUnproject` in `web/src/sector.ts` mirror this EXACTLY so a map click
// can be un-projected back to world space for the continent drill — cross-
// language agreement is pinned by shared reference points in the tests of both.
// Projection trig is not determinism-load-bearing (the planet SVG is never
// hashed), but routes through `fmath` for workspace house-style.

use mapgen_core::fmath;
use std::f32::consts::PI;

/// Solve `2θ + sin2θ = π·sin(lat)` for the Mollweide auxiliary angle (Newton).
/// The poles (`θ = ±π/2`) are special-cased — there the derivative vanishes.
fn mollweide_theta(lat: f32) -> f32 {
    let half_pi = PI / 2.0;
    if lat.abs() >= half_pi - 1e-6 {
        return lat.signum() * half_pi;
    }
    let mut theta = lat;
    let target = PI * fmath::sin(lat);
    for _ in 0..8 {
        let den = 2.0 + 2.0 * fmath::cos(2.0 * theta);
        if den.abs() < 1e-9 {
            break;
        }
        theta -= (2.0 * theta + fmath::sin(2.0 * theta) - target) / den;
    }
    theta
}

/// Forward Mollweide projection with a precomputed per-row table. Mollweide is
/// separable: a row at latitude φ keeps its screen-y and scales horizontally by
/// `cos θ(φ)` about the central meridian — i.e. `sx = cx0 + (wx − cx0)·cosθ`,
/// `sy = f(wy)`. Both depend only on `wy`, so the per-vertex hot path becomes a
/// table lookup + lerp with NO trig (the Newton solve runs once per row at
/// build time). Keeps the projected 18k-cell render near the un-projected cost.
struct Proj {
    vx: f32,
    vy: f32,
    vw: f32,
    vh: f32,
    cx0: f32,            // central-meridian screen x = vx + vw/2
    cos_theta: Vec<f32>, // cos θ per sampled row
    sy: Vec<f32>,        // screen y per sampled row
}

impl Proj {
    /// Samples on `[vy, vy+vh]`; 2048 gives ~2 rows/pixel at planet height, so
    /// linear interpolation is sub-pixel for the interior rows. The two
    /// pole-boundary rows can't be interpolated (cos θ cusps to 0 at the pole)
    /// and are computed exactly in `project`.
    const N: usize = 2048;

    fn new(vr: [f32; 4]) -> Self {
        let [vx, vy, vw, vh] = vr;
        let mut cos_theta = Vec::with_capacity(Self::N + 1);
        let mut sy = Vec::with_capacity(Self::N + 1);
        for i in 0..=Self::N {
            let wy = vy + vh * i as f32 / Self::N as f32;
            let lat = (0.5 - (wy - vy) / vh) * PI;
            let theta = mollweide_theta(lat);
            cos_theta.push(fmath::cos(theta));
            // my = √2·sinθ → sy = vy + vh·(0.5 − my/(2√2)) = vy + vh·(0.5 − sinθ/2).
            sy.push(vy + vh * (0.5 - fmath::sin(theta) / 2.0));
        }
        Proj {
            vx,
            vy,
            vw,
            vh,
            cx0: vx + vw / 2.0,
            cos_theta,
            sy,
        }
    }

    fn project(&self, wx: f32, wy: f32) -> (f32, f32) {
        let t = ((wy - self.vy) / self.vh).clamp(0.0, 1.0) * Self::N as f32;
        let i = (t as usize).min(Self::N - 1);
        let (ct, sy) = if i == 0 || i == Self::N - 1 {
            // cos θ has an infinite-slope cusp at the poles, so the two
            // pole-boundary intervals can't be linearly interpolated (lerping
            // toward the singular cos=0 pole node over-collapses a row's interior
            // toward the central meridian — ~20 px at the oval edge). Compute
            // these two rows exactly; interior rows use the cheap table.
            let theta = mollweide_theta((0.5 - (wy - self.vy) / self.vh) * PI);
            (
                fmath::cos(theta),
                self.vy + self.vh * (0.5 - fmath::sin(theta) / 2.0),
            )
        } else {
            let frac = t - i as f32;
            (
                self.cos_theta[i] + (self.cos_theta[i + 1] - self.cos_theta[i]) * frac,
                self.sy[i] + (self.sy[i + 1] - self.sy[i]) * frac,
            )
        };
        (self.cx0 + (wx - self.cx0) * ct, sy)
    }
}

#[cfg(test)]
mod tests {
    use super::Proj;

    /// The committed Mollweide vectors (`tests/mollweide_vectors.txt`) are the
    /// SINGLE SOURCE OF TRUTH for the projection, asserted here AND in the
    /// frontend (`web/src/sector.test.ts`). A drifted constant in either language
    /// fails its side against the shared grid — without this, both round-trip
    /// suites can pass while a click lands in the wrong ocean. The grid spans the
    /// oval (centre, edges, all quadrants, and the exactly-computed pole-boundary
    /// rows), so a wrong sign/scale/axis can't sneak through the way it could past
    /// a handful of hand-picked points.
    #[test]
    fn project_matches_the_committed_cross_language_vectors() {
        let proj = Proj::new([0.0, 0.0, 2048.0, 1024.0]);
        let vectors = include_str!("../../tests/mollweide_vectors.txt");
        let mut checked = 0;
        for line in vectors.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let v: Vec<f32> = line
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            let (wx, wy, ex, ey) = (v[0], v[1], v[2], v[3]);
            let (px, py) = proj.project(wx, wy);
            // Loose tolerance: Rust uses the per-row LUT (interior < 0.12px error,
            // pole rows exact); the vectors are exact f64. A drifted constant is
            // off by far more than this.
            assert!(
                (px - ex).abs() < 0.6 && (py - ey).abs() < 0.6,
                "project({wx},{wy}) = ({px:.3},{py:.3}), committed ~({ex},{ey})"
            );
            checked += 1;
        }
        assert!(
            checked >= 15,
            "expected the full vector grid, got {checked}"
        );
    }
}

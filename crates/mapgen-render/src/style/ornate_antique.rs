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
use mapgen_core::{fmath, WorldData};
use roughr::core::{OpSetType, OptionsBuilder};
use roughr::generator::Generator;

use crate::FONTS_TTF;

/// Pre-encoded `<style>` block carrying base64 `@font-face` rules for
/// each vendored font. Computed once per process — encoding ~200 KB
/// of TTF bytes is ~milliseconds, but we pay it lazily so the cost
/// only lands on processes that actually render ornate maps.
pub(crate) static FONT_FACE_BLOCK: LazyLock<String> = LazyLock::new(|| {
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
    // Viewport is the cells' actual world-space rectangle: the whole world for a
    // level-0 mesh, or the sub-rectangle for a refined sector (Phase 7).
    let [vx, vy, vx1, vy1] = mesh.view_rect();
    let w = vx1 - vx;
    let h = vy1 - vy;

    let mut out = String::with_capacity(mesh.cell_count() * 128);
    write!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vx:.0} {vy:.0} {w:.0} {h:.0}" width="{w:.0}" height="{h:.0}">"##
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
</radialGradient>"##,
    );
    // The overlay legend gradients are generated from the same ramp tables the
    // tints sample (`THERMAL`/`HYPSO`/`PRECIP`), so legend and map can't drift.
    write_legend_gradient(&mut out, "thermal", &THERMAL);
    write_legend_gradient(&mut out, "hypso", &HYPSO);
    write_legend_gradient(&mut out, "precip", &PRECIP);
    write_legend_gradient(&mut out, "prosperity", &PROSPERITY);
    out.push_str("</defs>");
    out.push_str(LAYER_STYLE);
    write!(
        out,
        r##"<rect x="{vx:.0}" y="{vy:.0}" width="{w:.0}" height="{h:.0}" fill="url(#parchment)"/>"##
    )
    .unwrap();

    // Scale-dependent render fidelity (Phase 7 LOD): how far zoomed in we are
    // vs. the whole world — 1.0 at level 0, 2/4/8… for refined sectors. Feature
    // glyphs gain detail when zoomed in; at 1.0 the world renders byte-identical.
    let detail = (mesh.width / w).max(1.0);

    // Each step is wrapped in a named, toggleable layer group (see `layer`).
    // The default (no root classes) rasterizes identically to before — features
    // visible, the `political` data overlay hidden — so level-0 stays byte-stable;
    // the frontend flips layers via the `LAYER_STYLE` rules.
    layer(&mut out, "land", false, |o| render_land_fill(world, o));
    layer(&mut out, "political", true, |o| render_political(world, o));
    layer(&mut out, "faith", true, |o| render_faith(world, o));
    layer(&mut out, "prosperity", true, |o| {
        render_prosperity(world, o)
    });
    layer(&mut out, "trade", true, |o| render_trade_routes(world, o));
    layer(&mut out, "climate", true, |o| render_climate(world, o));
    layer(&mut out, "relief", true, |o| render_relief(world, o));
    layer(&mut out, "precip", true, |o| render_precip(world, o));
    layer(&mut out, "ocean", false, |o| {
        render_ocean_hatching(world, o)
    });
    layer(&mut out, "coastline", false, |o| {
        render_coastline_ripples(world, o, detail)
    });
    layer(&mut out, "rivers", false, |o| render_rivers(world, o));
    layer(&mut out, "mountains", false, |o| {
        render_mountains(world, o, detail)
    });
    layer(&mut out, "forests", false, |o| {
        render_forest_scatter(world, o, detail)
    });
    layer(&mut out, "roads", false, |o| render_roads(world, o));
    layer(&mut out, "borders", false, |o| {
        render_polity_borders(world, o)
    });
    layer(&mut out, "settlements", false, |o| {
        render_settlements(world, o, detail)
    });
    layer(&mut out, "sacred", false, |o| render_sacred_sites(world, o));
    layer(&mut out, "labels", false, |o| {
        render_polity_labels(world, o);
        render_settlement_labels(world, o);
        render_sacred_site_labels(world, o);
        render_feature_labels(world, o);
        render_mountain_range_labels(world, o);
    });

    // Decorative top-of-stack overlays. The edge-burn overlay darkens
    // the periphery (intentionally fading edge labels into "aged"
    // shadow); compass + cartouche sit on top, untouched.
    render_edge_burn(vx, vy, w, h, &mut out);
    // Overlay legends, drawn after the edge-burn so they stay crisp. Each is
    // hidden unless its `on-<overlay>` root class is set; they share the
    // bottom-left anchor (one overlay shows at a time under the presets).
    for spec in [
        LegendSpec {
            class: "climate",
            title: "TEMPERATURE",
            grad_id: "thermal",
            lo: "Frigid",
            hi: "Torrid",
        },
        LegendSpec {
            class: "relief",
            title: "ELEVATION",
            grad_id: "hypso",
            lo: "Lowland",
            hi: "Peaks",
        },
        LegendSpec {
            class: "precip",
            title: "RAINFALL",
            grad_id: "precip",
            lo: "Arid",
            hi: "Humid",
        },
        LegendSpec {
            class: "prosperity",
            title: "PROSPERITY",
            grad_id: "prosperity",
            lo: "Poor",
            hi: "Rich",
        },
    ] {
        render_overlay_legend(&mut out, vx, vy, w, h, spec);
    }
    // The compass and title cartouche are whole-world chrome anchored to the
    // canvas corners; a refined sector (Phase 7) is a detail view, so it gets a
    // clean framed map without them. Level-0 worlds are unaffected.
    if mesh.region.is_none() {
        render_compass(w, h, &mut out);
        render_cartouche(w, h, "A MAP OF THE KNOWN WORLD", &mut out);
    }

    out.push_str("</svg>");
    out
}

/// Browser-side layer toggles. A feature layer hides when the root `<svg>`
/// carries `off-<layer>`; the `political` data overlay (off by default via a
/// `display` attribute resvg also honours) shows when the root carries
/// `on-political`. resvg ignores these class selectors, so the *rasterized*
/// default — features on, overlays off — is byte-stable. Keep the layer names in
/// sync with the frontend panel (`web/src/layers.ts`).
const LAYER_STYLE: &str = r##"<style>
svg.off-land .layer-land,svg.off-ocean .layer-ocean,svg.off-coastline .layer-coastline,svg.off-rivers .layer-rivers,svg.off-mountains .layer-mountains,svg.off-forests .layer-forests,svg.off-roads .layer-roads,svg.off-borders .layer-borders,svg.off-settlements .layer-settlements,svg.off-sacred .layer-sacred,svg.off-labels .layer-labels{display:none}
svg.on-political .layer-political{display:inline !important}
svg.on-faith .layer-faith{display:inline !important}
svg.on-prosperity .layer-prosperity,svg.on-prosperity .legend-prosperity{display:inline !important}
svg.on-trade .layer-trade{display:inline !important}
svg.on-climate .layer-climate,svg.on-climate .legend-climate{display:inline !important}
svg.on-relief .layer-relief,svg.on-relief .legend-relief{display:inline !important}
svg.on-precip .layer-precip,svg.on-precip .legend-precip{display:inline !important}
</style>"##;

/// Wrap a render step in a named, toggleable layer group. `hidden` adds a
/// `display="none"` presentation attribute — honoured by both resvg and the
/// browser — so a data overlay starts off; the frontend flips it via CSS.
fn layer(out: &mut String, name: &str, hidden: bool, f: impl FnOnce(&mut String)) {
    let attr = if hidden { r##" display="none""## } else { "" };
    write!(out, r##"<g class="layer-{name}"{attr}>"##).unwrap();
    f(out);
    out.push_str("</g>");
}

/// Political-territory overlay (a data layer, off by default): each controlled
/// land cell washed in its realm's colour. Pairs with the time-slider to show
/// empires rise and fall. Mirrors `render_land_fill`'s per-cell polygon.
fn render_political(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let control = &world.society.control;
    if control.is_empty() {
        return;
    }
    let elev = &world.terrain.elevation;
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
        out.push_str(r##"<polygon points=""##);
        for (k, &vi) in verts.iter().enumerate() {
            let v = mesh.vertices[vi as usize];
            if k > 0 {
                out.push(' ');
            }
            write!(out, "{:.1},{:.1}", v[0], v[1]).unwrap();
        }
        write!(
            out,
            r##"" fill="#{r:02x}{g:02x}{b:02x}" fill-opacity="0.42"/>"##
        )
        .unwrap();
    }
}

/// Faith overlay (a data layer, off by default): each land cell washed in its
/// religion's colour — the continental view of where each faith reaches. Mirrors
/// `render_political`, coloured by `religion_id` instead of polity control.
fn render_faith(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let religion_id = &world.religions.religion_id;
    if religion_id.is_empty() {
        return;
    }
    let elev = &world.terrain.elevation;
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() || elev.get(i).copied().unwrap_or(0.0) <= 0.0 {
            continue; // land only
        }
        let Some(rid) = religion_id.get(i).copied().flatten() else {
            continue;
        };
        let [r, g, b] = super::faith_color(rid);
        out.push_str(r##"<polygon points=""##);
        for (k, &vi) in verts.iter().enumerate() {
            let v = mesh.vertices[vi as usize];
            if k > 0 {
                out.push(' ');
            }
            write!(out, "{:.1},{:.1}", v[0], v[1]).unwrap();
        }
        write!(
            out,
            r##"" fill="#{r:02x}{g:02x}{b:02x}" fill-opacity="0.42"/>"##
        )
        .unwrap();
    }
}

/// Trade-routes overlay (a data layer, off by default): every crossable
/// inter-continental sea lane drawn as a line between its two coastal anchor
/// cells, in this view's raw world coordinates. The maritime substrate
/// (`sea_lanes`) is a planet-scale product — a continental / refined-sector world
/// carries none, so this group is typically empty here (the planet view is where
/// the Sundered Lanes read). The `layer()` wrapper still emits the group, which
/// `atlas.rs` requires and the `on-trade` lens reveals; it simply draws whatever
/// lanes fall in range. Mirrors the planet `render_trade_routes`.
fn render_trade_routes(world: &WorldData, out: &mut String) {
    let mesh = &world.mesh;
    let control = &world.society.control;
    out.push_str(
        r##"<g fill="none" stroke="#8c2f1a" stroke-width="1.6" stroke-opacity="0.85" stroke-linecap="round">"##,
    );
    let prosperity_of = |cell: usize| {
        control
            .get(cell)
            .copied()
            .flatten()
            .and_then(|pid| world.society.nations.get(pid as usize))
            .map(|n| n.prosperity)
    };
    for lane in &world.sea_lanes.lanes {
        if lane.min_naval > super::MAX_CROSSABLE_NAVAL {
            continue; // not crossable — an abyss no seafarer of this world reaches
        }
        let (a, b) = (lane.a as usize, lane.b as usize);
        let (Some(&pa), Some(&pb)) = (mesh.sites.get(a), mesh.sites.get(b)) else {
            continue;
        };
        // Tint by the average prosperity of the realms the lane binds, through the
        // SAME `PROSPERITY` ramp the prosperity wash uses — the trade→prosperity
        // loop made legible (mirrors the planet `render_trade_routes`). A lane with
        // no controlled endpoint keeps the group's fallback carmine.
        let tint = match (prosperity_of(a), prosperity_of(b)) {
            (Some(pa), Some(pb)) => Some((pa + pb) / 2.0),
            (Some(p), None) | (None, Some(p)) => Some(p),
            (None, None) => None,
        };
        match tint {
            Some(t) => {
                let [r, g, bl] = ramp(&PROSPERITY, t);
                write!(
                    out,
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#{r:02x}{g:02x}{bl:02x}"/>"##,
                    pa[0], pa[1], pb[0], pb[1]
                )
                .unwrap();
            }
            None => {
                write!(
                    out,
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                    pa[0], pa[1], pb[0], pb[1]
                )
                .unwrap();
            }
        }
    }
    out.push_str("</g>");
}

// ---- Data overlays (off by default; revealed by `on-<name>` root classes) ----
//
// Each is a per-cell choropleth drawn under the linework, so coastline / rivers
// / labels still read on top — toggle off `land`+`forests` (or use the Climate
// lens preset) for a clean thematic view. The matching legend rides top-of-
// stack via the same `on-<name>` class. All three share `fill_cells` (the
// polygon loop), `ramp` (stop interpolation), and `render_overlay_legend`.
//
// KNOWN LIMITATION (deliberate): each overlay normalizes to *this world's* own
// min/max (`field_range`), maximizing within-map contrast. The trade-off is
// that a Phase-7 refined sector normalizes to the sector's range, so the same
// physical value can map to a different colour at world vs. zoomed-in scale —
// the qualitative legend ("Frigid…Torrid") hides this, it doesn't resolve it.
// A cross-scale-stable variant would thread an explicit `(lo, hi)` range down
// from the parent; deferred as a product call (see docs/BACKLOG.md → overlays).

/// Colour-ramp stops: `(t, [r,g,b])` with `t` ascending over [0,1].
type Stops = [(f32, [f32; 3])];

/// Cold→hot: indigo → glacial cyan → pale gold → ember orange → oxblood.
const THERMAL: [(f32, [f32; 3]); 5] = [
    (0.0, [38.0, 54.0, 120.0]),
    (0.25, [58.0, 150.0, 190.0]),
    (0.5, [214.0, 199.0, 132.0]),
    (0.75, [216.0, 130.0, 56.0]),
    (1.0, [168.0, 44.0, 30.0]),
];
/// Hypsometric land tint: lowland green → gold → ochre → dark brown → snow.
const HYPSO: [(f32, [f32; 3]); 5] = [
    (0.0, [90.0, 138.0, 74.0]),
    (0.35, [200.0, 190.0, 126.0]),
    (0.65, [154.0, 106.0, 62.0]),
    (0.85, [110.0, 74.0, 48.0]),
    (1.0, [242.0, 240.0, 236.0]),
];
/// Arid→humid: parched tan → dry grass → green → wet teal.
const PRECIP: [(f32, [f32; 3]); 4] = [
    (0.0, [216.0, 200.0, 154.0]),
    (0.4, [184.0, 200.0, 122.0]),
    (0.7, [106.0, 168.0, 106.0]),
    (1.0, [42.0, 122.0, 106.0]),
];
/// Poor→rich: pale parchment-gold → amber → ember → oxblood. The prosperity ramp
/// (v21) — a per-realm heatmap of final relative population, so trade's "realms
/// grow" / embargo's "impoverish" reads at a glance. Already normalized to [0,1]
/// upstream, so unlike the climate/precip overlays it is sampled directly (no
/// per-world `field_range` re-normalization).
const PROSPERITY: [(f32, [f32; 3]); 4] = [
    (0.0, [238.0, 222.0, 180.0]),
    (0.4, [222.0, 178.0, 110.0]),
    (0.7, [196.0, 120.0, 60.0]),
    (1.0, [140.0, 56.0, 36.0]),
];

/// Temperature overlay — a cold→hot wash normalized to the world's own min/max,
/// surfacing the latitude bands + orographic cooling the base map only implies.
fn render_climate(world: &WorldData, out: &mut String) {
    let temp = &world.climate.temperature;
    let (lo, hi) = field_range(temp);
    let span = (hi - lo).max(1e-3);
    fill_cells(world, out, 0.6, |i| {
        temp.get(i).map(|&t| ramp(&THERMAL, (t - lo) / span))
    });
}

/// Relief overlay — a hypsometric tint on land (green→snow by height) over a
/// bathymetric blue on water (shallow→deep), each on its own scale so both read.
fn render_relief(world: &WorldData, out: &mut String) {
    let elev = &world.terrain.elevation;
    let (min_e, max_e) = field_range(elev);
    fill_cells(world, out, 0.8, |i| {
        elev.get(i).map(|&e| relief_color(e, min_e, max_e))
    });
}

/// Precipitation overlay — an arid→humid wash normalized to the world's min/max.
fn render_precip(world: &WorldData, out: &mut String) {
    let precip = &world.climate.precipitation;
    let (lo, hi) = field_range(precip);
    let span = (hi - lo).max(1e-3);
    fill_cells(world, out, 0.6, |i| {
        precip.get(i).map(|&p| ramp(&PRECIP, (p - lo) / span))
    });
}

/// Prosperity overlay (v21) — each controlled land cell tinted by its realm's
/// final relative prosperity (`nations[control[cell]].prosperity`, already in
/// [0,1]) through the [`PROSPERITY`] ramp. Unlike the climate/relief/precip
/// overlays this is keyed by the controlling polity (cell → control → nation), not
/// a per-cell scalar field, and is sampled DIRECTLY (no `field_range` — the value
/// is already normalized upstream). Unclaimed land / sea cells get no fill.
fn render_prosperity(world: &WorldData, out: &mut String) {
    let control = &world.society.control;
    if control.is_empty() {
        return;
    }
    let elev = &world.terrain.elevation;
    let nations = &world.society.nations;
    fill_cells(world, out, 0.7, |i| {
        if elev.get(i).copied().unwrap_or(0.0) <= 0.0 {
            return None; // land only
        }
        let pid = control.get(i).copied().flatten()?;
        let nation = nations.get(pid as usize)?;
        Some(ramp(&PROSPERITY, nation.prosperity))
    });
}

/// Emit a per-cell choropleth: for each non-empty cell, `color(i)` yields an
/// optional RGB fill drawn at `opacity`. Mirrors `render_land_fill`'s polygon
/// loop; shared by the scalar data overlays above.
fn fill_cells(
    world: &WorldData,
    out: &mut String,
    opacity: f32,
    color: impl Fn(usize) -> Option<[u8; 3]>,
) {
    let mesh = &world.mesh;
    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        if verts.is_empty() {
            continue;
        }
        let Some([r, g, b]) = color(i) else { continue };
        out.push_str(r##"<polygon points=""##);
        for (k, &vi) in verts.iter().enumerate() {
            let v = mesh.vertices[vi as usize];
            if k > 0 {
                out.push(' ');
            }
            write!(out, "{:.1},{:.1}", v[0], v[1]).unwrap();
        }
        write!(
            out,
            r##"" fill="#{r:02x}{g:02x}{b:02x}" fill-opacity="{opacity}"/>"##
        )
        .unwrap();
    }
}

/// Finite min/max of a per-cell field, falling back to [0,1] if empty/non-finite.
fn field_range(field: &[f32]) -> (f32, f32) {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for &t in field {
        if t.is_finite() {
            lo = lo.min(t);
            hi = hi.max(t);
        }
    }
    if lo.is_finite() {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

/// Linear interpolation of `t` in [0,1] through `stops` (ascending). No
/// transcendentals — the same colours as the matching `<linearGradient>` legend.
fn ramp(stops: &Stops, t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let mut k = 0;
    while k + 1 < stops.len() && t > stops[k + 1].0 {
        k += 1;
    }
    let (t0, c0) = stops[k];
    let (t1, c1) = stops[(k + 1).min(stops.len() - 1)];
    let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    let mix = |a: f32, b: f32| (a + (b - a) * f).round().clamp(0.0, 255.0) as u8;
    [mix(c0[0], c1[0]), mix(c0[1], c1[1]), mix(c0[2], c1[2])]
}

/// Emit a `<linearGradient id="{id}">` whose stops are exactly `stops`. The
/// overlay legend bars reference these, so a legend always matches the tint its
/// overlay paints with `ramp(stops, …)` — one source of colour, no drift.
fn write_legend_gradient(out: &mut String, id: &str, stops: &Stops) {
    write!(
        out,
        r##"<linearGradient id="{id}" x1="0%" y1="0%" x2="100%" y2="0%">"##
    )
    .unwrap();
    for &(t, [r, g, b]) in stops {
        write!(
            out,
            r##"<stop offset="{:.0}%" stop-color="#{:02x}{:02x}{:02x}"/>"##,
            t * 100.0,
            r as u8,
            g as u8,
            b as u8,
        )
        .unwrap();
    }
    out.push_str("</linearGradient>");
}

/// Two-part relief colour: bathymetry (≤ sea level) shallow→deep blue, land
/// (> sea level) through the hypsometric ramp, each normalized on its own side.
fn relief_color(elev: f32, min_e: f32, max_e: f32) -> [u8; 3] {
    if elev <= 0.0 {
        let d = if min_e < 0.0 {
            (elev / min_e).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // shallow #9fc2dd → deep #16335f
        let mix = |a: f32, b: f32| (a + (b - a) * d).round().clamp(0.0, 255.0) as u8;
        [mix(159.0, 22.0), mix(194.0, 51.0), mix(221.0, 95.0)]
    } else {
        let t = if max_e > 0.0 {
            (elev / max_e).clamp(0.0, 1.0)
        } else {
            0.0
        };
        ramp(&HYPSO, t)
    }
}

/// Legend for a data overlay: a gradient bar (`#{grad_id}`) with qualitative end
/// labels — the scales are normalized per-world, not absolute units. Hidden by
/// default; revealed alongside its tint by the `on-{class}` root class. All
/// share the bottom-left anchor (clearing the compass NW + cartouche SE); since
/// the presets enable one overlay at a time, they don't visually collide.
/// The text + gradient descriptors for one overlay's legend (the geometry is
/// passed separately so all legends share the bottom-left anchor).
struct LegendSpec {
    class: &'static str,
    title: &'static str,
    grad_id: &'static str,
    lo: &'static str,
    hi: &'static str,
}

fn render_overlay_legend(out: &mut String, vx: f32, vy: f32, w: f32, h: f32, spec: LegendSpec) {
    let LegendSpec {
        class,
        title,
        grad_id,
        lo,
        hi,
    } = spec;
    let scale = (w.min(h) / 1280.0).clamp(0.7, 1.5);
    let bar_w = 200.0 * scale;
    let bar_h = 12.0 * scale;
    let pad = 10.0 * scale;
    let box_w = bar_w + pad * 2.0;
    let box_h = bar_h + 34.0 * scale;
    let x0 = vx + 50.0 * scale;
    let y0 = vy + h - box_h - 50.0 * scale;

    write!(out, r##"<g class="legend-{class}" display="none">"##).unwrap();
    write!(
        out,
        r##"<rect x="{x0:.1}" y="{y0:.1}" width="{box_w:.1}" height="{box_h:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="#e8d8a8" stroke="#2a2418" stroke-width="{sw:.1}" fill-opacity="0.92"/>"##,
        rx = 5.0 * scale,
        sw = 1.2 * scale,
    )
    .unwrap();
    write!(
        out,
        r##"<text x="{tx:.1}" y="{ty:.1}" font-family="Cinzel, Georgia, serif" font-size="{fs:.1}" letter-spacing="1" fill="#2a2418">{title}</text>"##,
        tx = x0 + pad,
        ty = y0 + pad + 8.0 * scale,
        fs = 9.0 * scale,
    )
    .unwrap();
    let bx = x0 + pad;
    let by = y0 + pad + 12.0 * scale;
    write!(
        out,
        r##"<rect x="{bx:.1}" y="{by:.1}" width="{bar_w:.1}" height="{bar_h:.1}" fill="url(#{grad_id})" stroke="#2a2418" stroke-width="{sw:.1}"/>"##,
        sw = 0.6 * scale,
    )
    .unwrap();
    let ly = by + bar_h + 11.0 * scale;
    write!(
        out,
        r##"<text x="{bx:.1}" y="{ly:.1}" font-family="EB Garamond, serif" font-size="{fs:.1}" fill="#2a2418">{lo}</text>"##,
        fs = 9.0 * scale,
    )
    .unwrap();
    write!(
        out,
        r##"<text x="{rx:.1}" y="{ly:.1}" text-anchor="end" font-family="EB Garamond, serif" font-size="{fs:.1}" fill="#2a2418">{hi}</text>"##,
        rx = bx + bar_w,
        fs = 9.0 * scale,
    )
    .unwrap();
    out.push_str("</g>");
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
fn render_coastline_ripples(world: &WorldData, out: &mut String, detail: f32) {
    let polylines = extract_coastline_polylines(world);
    if polylines.is_empty() {
        return;
    }

    // Ripple widths/roughness are world-unit; left unscaled they'd bloat into a
    // thick haze when zoomed in (the viewBox magnifies them), drowning out the
    // finer coastline the denser sector mesh produces. Scaling by 1/detail keeps
    // them screen-consistent so the crisper coast shows through (Phase 7 LOD).
    // `s == 1` at world scale → byte-identical level-0 render.
    let s = (1.0 / detail).clamp(0.25, 1.0);

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
        let width = width * s;
        write!(
            out,
            r##"<g stroke="{color}" stroke-width="{width:.2}" stroke-linecap="round" fill="none" stroke-opacity="0.78">"##
        )
        .unwrap();

        let opts = OptionsBuilder::default()
            .roughness(roughness * s)
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
pub(crate) fn extract_coastline_polylines(world: &WorldData) -> Vec<(Vec<[f32; 2]>, bool)> {
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
    use mapgen_core::world_data::river_regime;
    let mesh = &world.mesh;
    let flow = &world.hydrology.flow;
    out.push_str(
        r##"<g stroke="#4a6b8a" fill="none" stroke-linecap="round" stroke-linejoin="round" stroke-opacity="0.85">"##,
    );
    for river in &world.hydrology.rivers {
        if river.cells.len() < 2 {
            continue;
        }
        // Intermittent (ephemeral) watercourses get the standard cartographic
        // dashed line; perennial rivers stay solid (6.1.5).
        let dash = if river.regime == river_regime::EPHEMERAL {
            r##" stroke-dasharray="4 3""##
        } else {
            ""
        };
        for win in river.cells.windows(2) {
            let a = mesh.sites[win[0] as usize];
            let b = mesh.sites[win[1] as usize];
            // Width from √flow — at 4k cells Strahler order tops out at ~3, so
            // flow accumulation gives a smoother, wider creek→trunk gradient.
            let f = flow.get(win[1] as usize).copied().unwrap_or(1.0);
            let sw = (f.sqrt() * 0.22).clamp(0.6, 6.0);
            write!(
                out,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke-width="{:.2}"{dash}/>"##,
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
fn render_mountains(world: &WorldData, out: &mut String, detail: f32) {
    let mesh = &world.mesh;
    let biomes = &world.climate.biome;
    let elev = &world.terrain.elevation;
    let rich = detail >= 2.0;

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
        // Zoomed in (Phase 7 LOD): a subordinate peak beside the main one so a
        // cell reads as a ridge rather than a lone sticker. Drawn before the
        // main triangle so the main peak overlaps it.
        if rich {
            let s2 = base * 0.62;
            let dir = if hash_offset(i as u32, 6) < 0.0 {
                -1.0
            } else {
                1.0
            };
            let c2x = cx + dir * base * (0.45 + hash_offset(i as u32, 5).abs() * 0.3);
            let p2x = c2x + hash_offset(i as u32, 7) * s2 * 0.2;
            let p2y = cy - height * 0.4;
            write!(
                out,
                r##"<polygon points="{:.1},{by:.1} {:.1},{by:.1} {p2x:.1},{p2y:.1}"/>"##,
                c2x - s2 * 0.5,
                c2x + s2 * 0.5,
            )
            .unwrap();
        }
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

/// Tree tufts on forest-class biomes. At world scale (`detail == 1`) each tree
/// is a single small triangle — unchanged, so the level-0 render stays
/// byte-identical. Zoomed into a refined sector (`detail >= 2`), the canopy
/// densifies and each tree gains a trunk + layered crown (conifers as stacked
/// tiers, broadleaf as a lobed canopy), so a forest reads as a *detailed* wood
/// rather than a denser sprinkle of the same marks (Phase 7 LOD).
fn render_forest_scatter(world: &WorldData, out: &mut String, detail: f32) {
    let mesh = &world.mesh;
    let biomes = &world.climate.biome;
    let rich = detail >= 2.0;

    out.push_str(r##"<g fill="#2d4a2b" stroke="#1a2e19" stroke-width="0.3" fill-opacity="0.85">"##);
    for i in 0..mesh.cell_count() {
        let biome = biomes.get(i).copied().unwrap_or(0);
        let (base_trees, color, conifer) = match biome {
            TEMPERATE_FOREST => (3, "#2d4a2b", false),
            TEMPERATE_RAINFOREST => (4, "#1f3a1d", true),
            TAIGA => (2, "#1e3a2c", true),
            TROPICAL_RAINFOREST => (4, "#1c4422", false),
            _ => continue,
        };
        // Zoomed in, fill the now-larger cell with a fuller canopy. Capped so
        // the SVG stays sane at deep zoom.
        let n_trees = if rich {
            (base_trees as f32 * 1.6).round().min(8.0) as i32
        } else {
            base_trees
        };
        let site = mesh.sites[i];
        for t in 0..n_trees {
            let ox = hash_offset(i as u32, (10 + t) as u32) * 14.0;
            let oy = hash_offset(i as u32, (20 + t) as u32) * 10.0;
            let cx = site[0] + ox;
            let cy = site[1] + oy;
            let size = 2.5 + hash_offset(i as u32, (30 + t) as u32).abs() * 1.5;
            if rich {
                rich_tree(out, cx, cy, size, color, conifer);
            } else {
                // World scale: the original single triangle (unchanged bytes).
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
    }
    out.push_str("</g>");
}

/// A single detailed tree for the zoomed-in forest canopy: a brown trunk plus a
/// layered crown — three stacked tiers for a conifer, three overlapping lobes
/// for a broadleaf. Coordinates are in world units (the viewBox scales them up).
fn rich_tree(out: &mut String, cx: f32, cy: f32, size: f32, color: &str, conifer: bool) {
    let tw = (size * 0.22).max(0.6);
    let th = size * 0.9;
    write!(
        out,
        r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#5a4326" stroke="none"/>"##,
        cx - tw * 0.5,
        cy + size * 0.45,
        tw,
        th,
    )
    .unwrap();
    if conifer {
        // Stacked triangles, each tier smaller and higher than the last.
        for k in 0..3 {
            let s = size * (1.0 - k as f32 * 0.26);
            let tier_y = cy + size * 0.3 - size * 0.6 * k as f32;
            let bx_l = cx - s;
            let bx_r = cx + s;
            let peak_y = tier_y - s * 1.25;
            write!(
                out,
                r##"<polygon points="{bx_l:.1},{tier_y:.1} {bx_r:.1},{tier_y:.1} {cx:.1},{peak_y:.1}" fill="{color}"/>"##
            )
            .unwrap();
        }
    } else {
        // Broadleaf: a lobed crown from three overlapping circles.
        let cap_y = cy - size * 0.2;
        for (dx, dy, r) in [(0.0, -0.35, 1.15), (-0.7, 0.15, 0.78), (0.7, 0.15, 0.78)] {
            write!(
                out,
                r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{color}"/>"##,
                cx + dx * size,
                cap_y + dy * size,
                r * size,
            )
            .unwrap();
        }
    }
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
fn render_settlements(world: &WorldData, out: &mut String, detail: f32) {
    let mesh = &world.mesh;
    // Zoomed to local scale (Phase 7 LOD): settlements bloom from a single glyph
    // into a town footprint — a cluster of buildings (and a wall ring for
    // capitals) around the landmark glyph.
    let urban = detail >= 4.0;

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

    // Sea-cell positions, for siting a harbour on a coastal town's seaward edge
    // (only needed at city-plan zoom).
    let sea_sites: Vec<[f32; 2]> = if detail >= 8.0 {
        (0..mesh.cell_count())
            .filter(|&i| world.terrain.elevation.get(i).copied().unwrap_or(1.0) <= 0.0)
            .map(|i| mesh.sites[i])
            .collect()
    } else {
        Vec::new()
    };

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
            if urban {
                // A hamlet: a few buildings instead of a single dot.
                draw_urban_halo(out, site[0], site[1], 3.5, 3, false, &fill, s.cell);
            } else {
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
            }
        } else {
            let capital = s.tier == SettlementTier::Capital;
            if detail >= 8.0 {
                // Deepest zoom: a full town plan — wall, streets, quarters,
                // plus a harbour if the sea is close.
                let r = if capital {
                    14.0
                } else {
                    8.0 + s.population * 4.0
                };
                let harbour = nearest_sea_dir(site, &sea_sites, r * 1.6);
                draw_city_plan(
                    out, site[0], site[1], r, arch, capital, &fill, s.cell, harbour,
                );
            } else if urban {
                // Mid zoom: a building cluster under the landmark glyph.
                let (radius, n, walled) = if capital {
                    (6.0 + scale * 9.0, 13, true)
                } else {
                    (5.0 + scale * 7.0, 6, false)
                };
                draw_urban_halo(out, site[0], site[1], radius, n, walled, &fill, s.cell);
            }
            draw_glyph(icon, site[0], site[1], scale, &fill, &style, out);
            if capital {
                draw_capital_pennant(site[0], site[1], scale, &fill, out);
            }
        }

        out.push_str("</g>");
    }
}

/// A ring of small buildings around a settlement glyph (Phase 7 LOD, drawn only
/// at local zoom). `radius` is the spread in world units; `n` the building count;
/// `walled` adds a dashed perimeter ring (capitals). Building positions are
/// hash-driven off the settlement `seed` (its cell), so they're stable across
/// renders. Rectangular jitter (no trig) keeps it off the fmath hot path.
#[allow(clippy::too_many_arguments)]
fn draw_urban_halo(
    out: &mut String,
    x: f32,
    y: f32,
    radius: f32,
    n: u32,
    walled: bool,
    fill: &str,
    seed: u32,
) {
    if walled {
        write!(
            out,
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="{:.1}" fill="none" stroke="#5a4326" stroke-width="0.7" stroke-opacity="0.55" stroke-dasharray="2.5 1.5"/>"##,
            radius * 1.3,
        )
        .unwrap();
    }
    for k in 0..n {
        let bx = x + hash_offset(seed, 40 + k) * radius;
        let by = y + hash_offset(seed, 60 + k) * radius * 0.72;
        let bw = 1.3 + hash_offset(seed, 80 + k).abs() * 1.4;
        write!(
            out,
            r##"<rect x="{:.1}" y="{:.1}" width="{bw:.1}" height="{bw:.1}" fill="{fill}" stroke="#1a140e" stroke-width="0.3"/>"##,
            bx - bw * 0.5,
            by - bw * 0.5,
        )
        .unwrap();
    }
}

/// Unit direction from `site` toward the nearest sea cell within `max_dist`
/// (world units), or `None` if the settlement is inland — sites a harbour on a
/// coastal town's seaward edge.
fn nearest_sea_dir(site: [f32; 2], sea_sites: &[[f32; 2]], max_dist: f32) -> Option<[f32; 2]> {
    let mut best_d2 = max_dist * max_dist;
    let mut best: Option<[f32; 2]> = None;
    for &s in sea_sites {
        let d2 = (s[0] - site[0]).powi(2) + (s[1] - site[1]).powi(2);
        if d2 < best_d2 {
            best_d2 = d2;
            best = Some(s);
        }
    }
    best.map(|s| {
        let (dx, dy) = (s[0] - site[0], s[1] - site[1]);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        [dx / len, dy / len]
    })
}

/// A hand-drawn town plan for a settlement at the deepest zoom (Phase 7 LOD):
/// an enclosing wall (capitals), a street network (a grid for planned cultures,
/// radial spokes + a ring road for organic ones), quarters of small buildings, a
/// market plaza, and a harbour on the seaward edge if `harbour` is `Some(dir)`.
/// The caller draws the settlement's landmark glyph over the centre as the
/// citadel. Everything is hash-driven off `seed` (the cell), so a town's plan is
/// stable across renders and distinct between towns. `r` is the town radius in
/// world units; trig routes through `fmath` (the purity guard forbids raw
/// `sin`/`cos` even in the renderer).
#[allow(clippy::too_many_arguments)]
fn draw_city_plan(
    out: &mut String,
    cx: f32,
    cy: f32,
    r: f32,
    arch: Architecture,
    capital: bool,
    fill: &str,
    seed: u32,
    harbour: Option<[f32; 2]>,
) {
    use std::f32::consts::{PI, TAU};
    let grid = matches!(arch, Architecture::Classical | Architecture::Megalithic);

    // 1. Walled enclosure (capitals): an irregular polygon, faintly filled to
    //    read as a built-up area, stroked as a stone wall.
    if capital {
        let n_wall = 16u32;
        let mut pts = String::new();
        for k in 0..n_wall {
            let a = TAU * k as f32 / n_wall as f32;
            let rr = r * (0.9 + hash_offset(seed, 100 + k).abs() * 0.13);
            let px = cx + rr * fmath::cos(a);
            let py = cy + rr * fmath::sin(a);
            write!(pts, "{}{px:.1},{py:.1}", if k == 0 { "" } else { " " }).unwrap();
        }
        write!(
            out,
            r##"<polygon points="{pts}" fill="#cdbf9c" fill-opacity="0.40" stroke="#3a2f22" stroke-width="1.4" stroke-linejoin="round"/>"##
        )
        .unwrap();
    }

    // 2. Streets.
    out.push_str(
        r##"<g stroke="#7a5836" stroke-width="0.55" stroke-opacity="0.7" fill="none" stroke-linecap="round">"##,
    );
    if grid {
        // Horizontal + vertical streets as chords of the town circle.
        for m in -2i32..=2 {
            let off = m as f32 * r * 0.32;
            if off.abs() >= r {
                continue;
            }
            let half = (r * r - off * off).sqrt();
            write!(
                out,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                cx - half,
                cy + off,
                cx + half,
                cy + off,
            )
            .unwrap();
            write!(
                out,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                cx + off,
                cy - half,
                cx + off,
                cy + half,
            )
            .unwrap();
        }
    } else {
        // Radial spokes from the centre + a ring road.
        let spokes = 5 + seed % 3;
        for k in 0..spokes {
            let a = TAU * k as f32 / spokes as f32 + hash_offset(seed, 200 + k) * 0.18;
            write!(
                out,
                r##"<line x1="{cx:.1}" y1="{cy:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                cx + r * fmath::cos(a),
                cy + r * fmath::sin(a),
            )
            .unwrap();
        }
        write!(
            out,
            r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}"/>"##,
            r * 0.55,
        )
        .unwrap();
    }
    out.push_str("</g>");

    // 3. Quarters: small buildings filling the town, uniform over the disk
    //    (sqrt radius), leaving the centre clear for the citadel glyph.
    let n_b = if capital { 34 } else { 20 };
    write!(
        out,
        r##"<g fill="{fill}" stroke="#1a140e" stroke-width="0.25" fill-opacity="0.92">"##
    )
    .unwrap();
    for k in 0..n_b {
        let ang = hash_offset(seed, 300 + k) * PI;
        let d = hash_offset(seed, 400 + k).abs().sqrt() * r * 0.82;
        if d < r * 0.16 {
            continue; // keep the centre for the citadel
        }
        let bx = cx + d * fmath::cos(ang);
        let by = cy + d * fmath::sin(ang);
        let bw = 1.1 + hash_offset(seed, 500 + k).abs() * 1.3;
        write!(
            out,
            r##"<rect x="{:.1}" y="{:.1}" width="{bw:.1}" height="{:.1}" fill="{fill}" stroke="#1a140e" stroke-width="0.25"/>"##,
            bx - bw * 0.5,
            by - bw * 0.4,
            bw * 0.8,
        )
        .unwrap();
    }
    out.push_str("</g>");

    // 4. Market plaza: a small open square just off the citadel, with stalls.
    let ang = hash_offset(seed, 700) * PI;
    let md = r * 0.34;
    let (mx, my) = (cx + md * fmath::cos(ang), cy + md * fmath::sin(ang));
    let ps = r * 0.13;
    write!(
        out,
        r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#cdbf9c" fill-opacity="0.7" stroke="#6b4a2a" stroke-width="0.4"/>"##,
        mx - ps,
        my - ps,
        ps * 2.0,
        ps * 2.0,
    )
    .unwrap();
    for q in 0..3u32 {
        let sx = mx + hash_offset(seed, 710 + q) * ps * 0.7;
        let sy = my + hash_offset(seed, 720 + q) * ps * 0.7;
        write!(
            out,
            r##"<rect x="{:.1}" y="{:.1}" width="1.0" height="1.0" fill="#7a5836" stroke="none"/>"##,
            sx - 0.5,
            sy - 0.5,
        )
        .unwrap();
    }

    // 5. Harbour on the seaward edge (coastal towns): piers reaching into the
    //    water, with a moored boat or two.
    if let Some([dx, dy]) = harbour {
        let (wx, wy) = (cx + dx * r * 0.95, cy + dy * r * 0.95);
        let (qx, qy) = (-dy, dx); // along-shore (perpendicular to the sea dir)
        out.push_str(
            r##"<g stroke="#5a4326" stroke-width="0.8" stroke-linecap="round" fill="none">"##,
        );
        for m in -1i32..=1 {
            let sx = wx + qx * m as f32 * r * 0.22;
            let sy = wy + qy * m as f32 * r * 0.22;
            write!(
                out,
                r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{:.1}" y2="{:.1}"/>"##,
                sx + dx * r * 0.38,
                sy + dy * r * 0.38,
            )
            .unwrap();
        }
        out.push_str("</g>");
        for b in 0..2u32 {
            let off = r * (0.42 + 0.18 * b as f32);
            let j = hash_offset(seed, 600 + b) * r * 0.18;
            let bx = wx + dx * off + qx * j;
            let by = wy + dy * off + qy * j;
            write!(
                out,
                r##"<ellipse cx="{bx:.1}" cy="{by:.1}" rx="{:.1}" ry="{:.1}" fill="#3a2f22" stroke="none"/>"##,
                r * 0.07,
                r * 0.035,
            )
            .unwrap();
        }
    }

    // 6. A named ward or two — small antique labels inside the walls. The
    //    seaward ward gets a docks name; capitals get a second ward opposite.
    let primary = if harbour.is_some() {
        DOCK_NAMES[seed as usize % DOCK_NAMES.len()]
    } else {
        WARD_NAMES[seed as usize % WARD_NAMES.len()]
    };
    let (lx, ly) = match harbour {
        Some([dx, dy]) => (cx + dx * r * 0.52, cy + dy * r * 0.52),
        None => (mx, my - ps - r * 0.14),
    };
    draw_district_label(out, lx, ly, primary, r * 0.22);
    if capital {
        let second =
            WARD_NAMES[(seed as usize).wrapping_mul(31).wrapping_add(7) % WARD_NAMES.len()];
        draw_district_label(out, cx - r * 0.3, cy - r * 0.42, second, r * 0.22);
    }
}

/// Small ward labels for a city plan. A curated antique pool, hash-picked per
/// town; `DOCK_NAMES` for the seaward ward.
const WARD_NAMES: &[&str] = &[
    "Old Town",
    "High Ward",
    "Low Quarter",
    "Stonegate",
    "The Rows",
    "Inner Ward",
    "Kingsreach",
    "New Town",
    "The Heights",
    "Greymarket",
];
const DOCK_NAMES: &[&str] = &["The Docks", "Harbour Ward", "Wharfside", "Saltgate"];

/// One ward label: small EB Garamond italic with a parchment halo for legibility
/// against the busy town fabric.
fn draw_district_label(out: &mut String, x: f32, y: f32, name: &str, size: f32) {
    write!(
        out,
        r##"<text x="{x:.1}" y="{y:.1}" font-family='"EB Garamond", Georgia, serif' font-style="italic" font-size="{size:.1}" fill="#2a2018" fill-opacity="0.9" stroke="#f0e3bf" stroke-width="{:.2}" paint-order="stroke" stroke-linejoin="round" text-anchor="middle">{}</text>"##,
        size * 0.18,
        xml_escape(name),
    )
    .unwrap();
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
pub(crate) fn render_edge_burn(vx: f32, vy: f32, w: f32, h: f32, out: &mut String) {
    write!(
        out,
        r##"<g class="edge-burn"><rect x="{vx:.0}" y="{vy:.0}" width="{w:.0}" height="{h:.0}" fill="url(#edge-burn)" pointer-events="none"/>"##
    )
    .unwrap();
    // Irregular burn stains: a handful of soft dark blobs scattered
    // around the perimeter so the aging reads as real scorching — some
    // corners darker than others, occasional splotch mid-edge — rather
    // than the mathematically uniform radial gradient underneath.
    // Positions / sizes / opacities are hash-driven (deterministic).
    const STAINS: u32 = 7;
    for k in 0..STAINS {
        let t = (k as f32 + 0.5) / STAINS as f32; // spread around perimeter
        let (px, py) = perimeter_point(t, w, h);
        let cx = vx + px + hash_offset(900 + k, 1) * w * 0.05;
        let cy = vy + py + hash_offset(900 + k, 2) * h * 0.05;
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
pub(crate) fn render_compass(w: f32, h: f32, out: &mut String) {
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
pub(crate) fn render_cartouche(w: f32, h: f32, title: &str, out: &mut String) {
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
        r##"<text x="{cx:.1}" y="{cy:.1}" font-family='"Cinzel", Georgia, serif' font-size="{fs:.1}" font-weight="bold" text-anchor="middle" dominant-baseline="middle" fill="#2a2418">{title}</text>"##,
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
pub(crate) fn ornate_biome_color(biome: u8, elev: f32) -> &'static str {
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
        WETLAND => "#7e9483", // marsh — muted reedy green-grey
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
const WETLAND: u8 = 15;

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

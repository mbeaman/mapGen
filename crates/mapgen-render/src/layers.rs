//! Map layers, curated presets, and the function that bakes a layer state into
//! a rendered SVG.
//!
//! The ornate renderer (`style::ornate_antique`) emits every component as a
//! `<g class="layer-NAME">` group plus a `<style>` block keyed off root-`<svg>`
//! classes; the browser frontend flips those classes live (instant, no
//! re-render). This module is the *native* counterpart: [`bake_layer_state`]
//! writes a chosen layer state directly into the SVG by toggling each group's
//! `display` attribute, yielding a self-contained SVG that renders correctly
//! with no CSS at all (resvg, PDF, static embedding) — the basis of the
//! `mapgen atlas` export and any server-side per-lens rasterization.
//!
//! [`LAYERS`] and [`PRESETS`] mirror `web/src/layers.ts`; keep the two in sync.
//! (A test below asserts every layer/preset name here actually appears in the
//! ornate render, which catches drift on this side.)

/// One toggleable layer. An `overlay` data layer is off by default (the frontend
/// reveals it with `on-NAME`); a feature layer is on by default (hidden with
/// `off-NAME`). `legend` marks the scalar overlays that emit a companion
/// `legend-NAME` group revealed alongside the tint.
pub struct LayerInfo {
    pub name: &'static str,
    pub overlay: bool,
    pub legend: bool,
}

const fn feature(name: &'static str) -> LayerInfo {
    LayerInfo {
        name,
        overlay: false,
        legend: false,
    }
}
const fn overlay(name: &'static str, legend: bool) -> LayerInfo {
    LayerInfo {
        name,
        overlay: true,
        legend,
    }
}

/// Every layer the ornate renderer emits, mirroring `web/src/layers.ts`.
pub const LAYERS: &[LayerInfo] = &[
    overlay("political", false),
    overlay("climate", true),
    overlay("relief", true),
    overlay("precip", true),
    feature("labels"),
    feature("settlements"),
    feature("sacred"),
    feature("borders"),
    feature("roads"),
    feature("rivers"),
    feature("forests"),
    feature("mountains"),
    feature("coastline"),
    feature("ocean"),
    feature("land"),
];

/// A named "lens": the exact set of layers enabled for one view.
pub struct Preset {
    pub name: &'static str,
    pub label: &'static str,
    /// One-line description of what the view shows (atlas caption).
    pub caption: &'static str,
    pub enabled: &'static [&'static str],
}

/// Curated views, mirroring `web/src/layers.ts` (`caption` is atlas-only).
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "antique",
        label: "Antique",
        caption: "The cartographer's full hand — every feature, no thematic wash.",
        enabled: &[
            "land",
            "ocean",
            "coastline",
            "rivers",
            "mountains",
            "forests",
            "roads",
            "borders",
            "settlements",
            "sacred",
            "labels",
        ],
    },
    Preset {
        name: "political",
        label: "Political",
        caption: "Realms washed in their colours over a decluttered base.",
        enabled: &[
            "land",
            "ocean",
            "coastline",
            "rivers",
            "roads",
            "borders",
            "settlements",
            "sacred",
            "labels",
            "political",
        ],
    },
    Preset {
        name: "physical",
        label: "Physical",
        caption: "The natural world alone — terrain, water, woods and peaks.",
        enabled: &[
            "land",
            "ocean",
            "coastline",
            "rivers",
            "mountains",
            "forests",
            "labels",
        ],
    },
    Preset {
        name: "climate",
        label: "Climate",
        caption: "Temperature, frigid poles to torrid equator, over land and sea.",
        enabled: &["coastline", "rivers", "labels", "climate"],
    },
    Preset {
        name: "relief",
        label: "Relief",
        caption: "Elevation as hypsometric tint, with bathymetry beneath the waves.",
        enabled: &["coastline", "rivers", "mountains", "labels", "relief"],
    },
    Preset {
        name: "rainfall",
        label: "Rainfall",
        caption: "Precipitation, parched drylands to humid green.",
        enabled: &["coastline", "rivers", "labels", "precip"],
    },
];

/// Bake a layer state into a rendered ornate SVG: reveal the enabled data
/// overlays (and their legends) and hide the disabled feature layers, by
/// toggling each group's `display` attribute. The result is self-contained — it
/// renders correctly without the root-class CSS machinery (resvg, PDF, static
/// embedding). Baking the `antique` preset is a no-op (it equals the default).
pub fn bake_layer_state(svg: &str, enabled: &[&str]) -> String {
    let mut s = svg.to_string();
    for layer in LAYERS {
        let on = enabled.contains(&layer.name);
        if layer.overlay {
            if on {
                s = reveal(s, &format!("layer-{}", layer.name));
                if layer.legend {
                    s = reveal(s, &format!("legend-{}", layer.name));
                }
            }
        } else if !on {
            s = hide(s, &format!("layer-{}", layer.name));
        }
    }
    s
}

/// Drop a group's default `display="none"` so it renders.
fn reveal(s: String, group: &str) -> String {
    s.replace(
        &format!(r#"<g class="{group}" display="none">"#),
        &format!(r#"<g class="{group}">"#),
    )
}
/// Add `display="none"` to a default-visible group so it stops rendering.
fn hide(s: String, group: &str) -> String {
    s.replace(
        &format!(r#"<g class="{group}">"#),
        &format!(r#"<g class="{group}" display="none">"#),
    )
}

/// Like [`bake_layer_state`], but also *removes* the now-hidden layer/legend
/// groups entirely (not just `display="none"`), so a static export carries only
/// what it shows — markedly smaller HTML and faster in-browser rendering. Use
/// this for the atlas / one-shot exports, never for interactive toggling (the
/// dropped groups can't be toggled back on without a re-render).
pub fn bake_layer_state_pruned(svg: &str, enabled: &[&str]) -> String {
    let mut s = bake_layer_state(svg, enabled);
    // After baking, every still-hidden layer/legend group is exactly one of
    // these opening tags (each emitted at most once). Cut each `<g …>…</g>`
    // span with balanced-depth matching so nested groups inside are removed too.
    let mut tags: Vec<String> = LAYERS
        .iter()
        .map(|l| format!(r#"<g class="layer-{}" display="none">"#, l.name))
        .collect();
    for l in LAYERS {
        if l.legend {
            tags.push(format!(r#"<g class="legend-{}" display="none">"#, l.name));
        }
    }
    for open in tags {
        if let Some(start) = s.find(&open) {
            let end = balanced_group_end(&s, start);
            s.replace_range(start..end, "");
        }
    }
    s
}

/// Given `start` at the `<` of a `<g …>` opening tag, return the byte index just
/// past its matching `</g>` (accounting for nested `<g>`). Byte-based so it is
/// safe across the UTF-8 in place-name labels.
fn balanced_group_end(s: &str, start: usize) -> usize {
    let b = s.as_bytes();
    let mut i = start;
    let mut depth = 0i32;
    while i < b.len() {
        if b[i..].starts_with(b"</g>") {
            depth -= 1;
            i += 4;
            if depth == 0 {
                return i;
            }
        } else if b[i..].starts_with(b"<g") {
            depth += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    b.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_reference_only_known_layers() {
        let known: Vec<&str> = LAYERS.iter().map(|l| l.name).collect();
        for p in PRESETS {
            for &name in p.enabled {
                assert!(
                    known.contains(&name),
                    "preset {} names unknown layer {name}",
                    p.name
                );
            }
        }
    }

    #[test]
    fn bake_toggles_groups() {
        // A synthetic SVG with one feature, one overlay, and its legend.
        let svg = concat!(
            r#"<g class="layer-land">L</g>"#,
            r#"<g class="layer-climate" display="none">C</g>"#,
            r#"<g class="legend-climate" display="none">K</g>"#,
        );
        // Enable only the climate overlay: land (feature) hidden, climate +
        // its legend revealed.
        let out = bake_layer_state(svg, &["climate"]);
        assert!(out.contains(r#"<g class="layer-land" display="none">"#));
        assert!(out.contains(r#"<g class="layer-climate">"#));
        assert!(out.contains(r#"<g class="legend-climate">"#));
    }

    #[test]
    fn antique_preset_is_a_noop() {
        let svg = r#"<g class="layer-land">L</g><g class="layer-political" display="none">P</g>"#;
        let antique = PRESETS.iter().find(|p| p.name == "antique").unwrap();
        assert_eq!(bake_layer_state(svg, antique.enabled), svg);
    }

    #[test]
    fn pruning_drops_hidden_groups_and_keeps_shown_ones() {
        // `layer-labels` nests a <g> to exercise balanced matching.
        let svg = concat!(
            r#"<rect/>"#,
            r#"<g class="layer-land"><polygon/></g>"#,
            r#"<g class="layer-climate" display="none"><polygon/></g>"#,
            r#"<g class="legend-climate" display="none"><rect/></g>"#,
            r#"<g class="layer-labels"><g><text/></g></g>"#,
        );
        // Climate lens: only climate + labels on.
        let out = bake_layer_state_pruned(svg, &["climate", "labels"]);
        assert!(!out.contains("layer-land"), "hidden feature not pruned");
        assert!(
            out.contains(r#"<g class="layer-climate">"#),
            "enabled overlay dropped"
        );
        assert!(
            out.contains(r#"<g class="legend-climate">"#),
            "enabled legend dropped"
        );
        assert!(out.contains("<text/>"), "nested label content lost");
        assert!(out.starts_with("<rect/>"), "non-layer content disturbed");
    }
}

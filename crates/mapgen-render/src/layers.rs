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
//! This module is the **single source of truth** for the layer + preset lists.
//! The frontend does not re-declare them: [`manifest_json`] serializes them to
//! `web/src/layers.manifest.json` (a committed, generated file that
//! `web/src/layers.ts` imports), and `web_manifest_is_in_sync` (below) fails if
//! that file drifts from this module — so the two can never silently diverge. A
//! separate test in `mapgen-cli/tests/atlas.rs` asserts every name here is
//! actually emitted by the ornate render, catching drift against the renderer.

use serde::Serialize;

/// One toggleable layer. An `overlay` data layer is off by default (the frontend
/// reveals it with `on-NAME`); a feature layer is on by default (hidden with
/// `off-NAME`). `legend` marks the scalar overlays that emit a companion
/// `legend-NAME` group revealed alongside the tint. `label` is the UI caption.
#[derive(Serialize)]
pub struct LayerInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub overlay: bool,
    pub legend: bool,
}

const fn feature(name: &'static str, label: &'static str) -> LayerInfo {
    LayerInfo {
        name,
        label,
        overlay: false,
        legend: false,
    }
}
const fn overlay(name: &'static str, label: &'static str, legend: bool) -> LayerInfo {
    LayerInfo {
        name,
        label,
        overlay: true,
        legend,
    }
}

/// Every layer the ornate renderer emits. The order is the panel's top-to-bottom
/// order (overlays first), independent of the SVG draw order.
pub const LAYERS: &[LayerInfo] = &[
    overlay("political", "Political territory", false),
    overlay("faith", "Faith", false),
    overlay("climate", "Temperature", true),
    overlay("relief", "Elevation", true),
    overlay("precip", "Rainfall", true),
    feature("labels", "Labels"),
    feature("settlements", "Settlements"),
    feature("sacred", "Sacred sites"),
    feature("borders", "Borders"),
    feature("roads", "Roads"),
    feature("rivers", "Rivers"),
    feature("forests", "Forests"),
    feature("mountains", "Mountains"),
    feature("coastline", "Coastline"),
    feature("ocean", "Ocean hatching"),
    feature("land", "Land fill"),
];

/// A named "lens": the exact set of layers enabled for one view.
#[derive(Serialize)]
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
        name: "faith",
        label: "Faith",
        caption: "Faiths washed over a decluttered base — where each religion reaches.",
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
            "faith",
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

/// The layer + preset lists serialized as pretty JSON — the exact bytes of the
/// committed `web/src/layers.manifest.json`, which the frontend imports instead
/// of re-declaring the lists. Regenerate that file from this whenever the lists
/// change (the `web_manifest_is_in_sync` test below enforces it).
pub fn manifest_json() -> String {
    #[derive(Serialize)]
    struct Manifest {
        layers: &'static [LayerInfo],
        presets: &'static [Preset],
    }
    serde_json::to_string_pretty(&Manifest {
        layers: LAYERS,
        presets: PRESETS,
    })
    .expect("layer manifest serializes")
}

/// Bake a layer state into a rendered ornate SVG: reveal the enabled data
/// overlays (and their legends) and hide the disabled feature layers, by
/// toggling each group's `display` attribute. The result is self-contained — it
/// renders correctly without the root-class CSS machinery (resvg, PDF, static
/// embedding). Baking the `antique` preset is a no-op (it equals the default).
///
/// This is a deliberately small, targeted string transform — *not* an XML
/// parser — so it depends on the exact group-tag spelling the `layer()` helper
/// in `ornate_antique.rs` emits. That coupling is guarded by the
/// `render_emits_every_declared_layer_group` test in `mapgen-cli/tests/atlas.rs`,
/// which fails the moment the emitted shape and this consumer drift apart.
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
    fn antique_preset_enables_exactly_all_features() {
        // The default view must list every feature layer and no overlay — so a
        // newly-added feature can't silently vanish from the default / atlas.
        let antique = PRESETS.iter().find(|p| p.name == "antique").unwrap();
        let mut want: Vec<&str> = LAYERS
            .iter()
            .filter(|l| !l.overlay)
            .map(|l| l.name)
            .collect();
        let mut got: Vec<&str> = antique.enabled.to_vec();
        want.sort_unstable();
        got.sort_unstable();
        assert_eq!(
            got, want,
            "antique preset must equal exactly the feature layers"
        );
    }

    #[test]
    fn web_manifest_is_in_sync() {
        // The frontend imports this committed file instead of re-declaring the
        // lists. If it drifts from the Rust source, regenerate it:
        //   the manifest is `manifest_json()` + a trailing newline.
        let committed = include_str!("../../../web/src/layers.manifest.json");
        assert_eq!(
            committed.trim_end(),
            manifest_json().trim_end(),
            "web/src/layers.manifest.json is stale — regenerate from manifest_json()"
        );
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

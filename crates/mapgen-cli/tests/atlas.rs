//! The atlas export rests on two facts: every layer the presets reference is
//! actually emitted by the ornate render, and `bake_layer_state` toggles those
//! groups on real output. (The bake *mechanics* are unit-tested in
//! mapgen-render against a synthetic SVG; this pins them to a real render and
//! catches drift between `layers::LAYERS` and what the renderer emits.)

use mapgen_render::{
    layers::{bake_layer_state, bake_layer_state_pruned, LAYERS, PRESETS},
    render,
    style::Style,
};
use mapgen_world::{generate_full, GenerateParams};

fn world_svg() -> String {
    let world = generate_full(GenerateParams {
        seed: 42,
        cell_count: 2_000,
        nation_count: 6,
        ..Default::default()
    });
    render(&world, Style::OrnateAntique).expect("ornate render")
}

#[test]
fn render_emits_every_declared_layer_group() {
    let svg = world_svg();
    for l in LAYERS {
        assert!(
            svg.contains(&format!(r#"class="layer-{}""#, l.name)),
            "ornate render is missing the layer-{} group declared in layers::LAYERS",
            l.name
        );
        if l.legend {
            assert!(
                svg.contains(&format!(r#"class="legend-{}""#, l.name)),
                "scalar overlay {} declares a legend but the render emits none",
                l.name
            );
        }
    }
}

#[test]
fn every_preset_bakes_a_self_contained_view() {
    let svg = world_svg();
    for p in PRESETS {
        let baked = bake_layer_state(&svg, p.enabled);
        for l in LAYERS {
            let on = p.enabled.contains(&l.name);
            if l.overlay && on {
                assert!(
                    baked.contains(&format!(r#"<g class="layer-{}">"#, l.name)),
                    "preset {}: overlay {} should be revealed",
                    p.name,
                    l.name
                );
            }
            if !l.overlay && !on {
                assert!(
                    baked.contains(&format!(r#"<g class="layer-{}" display="none">"#, l.name)),
                    "preset {}: feature {} should be hidden",
                    p.name,
                    l.name
                );
            }
        }

        // The atlas ships the pruned form: disabled groups are gone entirely,
        // enabled ones remain, and it's never larger than the toggled form.
        let pruned = bake_layer_state_pruned(&svg, p.enabled);
        assert!(
            pruned.len() <= baked.len(),
            "preset {}: pruning grew the SVG",
            p.name
        );
        for l in LAYERS {
            let on = p.enabled.contains(&l.name);
            if !on {
                assert!(
                    !pruned.contains(&format!(r#"class="layer-{}""#, l.name)),
                    "preset {}: disabled layer {} survived pruning",
                    p.name,
                    l.name
                );
            } else {
                assert!(
                    pruned.contains(&format!(r#"class="layer-{}""#, l.name)),
                    "preset {}: enabled layer {} was pruned",
                    p.name,
                    l.name
                );
            }
        }
    }
}

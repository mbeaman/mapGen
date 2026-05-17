//! Renderer invariants. The `Biomes` style emits a Phase-2 data-honest
//! SVG; these tests pin the structural and content guarantees a caller
//! (the CLI, the WASM frontend, the eventual sweep harness) is allowed
//! to rely on.
//!
//! Scope: structural (envelope, polygon count, no NaN coords) +
//! content (every biome color in the 14+RIPARIAN palette renders at
//! least once on a representative world). Full XML validation is
//! deliberately not pulled in as a dep — the renderer hand-writes a
//! known structure, so heuristic checks catch the realistic failure
//! modes (truncation, format-string bugs, NaN propagation,
//! palette-class collapse) without dragging in a parser.
//!
//! Reference world: seed 42, 4_000 cells. Same params used by
//! `phase2_spec` / `realism_spec` / the golden-hash anchor.

use mapgen_render::{render, style::Style};
use mapgen_world::{generate_full, GenerateParams};

fn ref_params() -> GenerateParams {
    GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    }
}

fn render_ref_world() -> (mapgen_core::WorldData, String) {
    let world = generate_full(ref_params());
    let svg = render(&world, Style::Biomes);
    (world, svg)
}

#[test]
fn svg_envelope_is_well_formed() {
    let (_, svg) = render_ref_world();
    assert!(
        svg.starts_with("<svg"),
        "SVG does not start with <svg: first 40 bytes = {:?}",
        &svg[..svg.len().min(40)]
    );
    assert!(
        svg.trim_end().ends_with("</svg>"),
        "SVG does not end with </svg>: last 40 bytes = {:?}",
        &svg[svg.len().saturating_sub(40)..]
    );
    assert!(svg.contains("viewBox=\""), "SVG missing viewBox attribute");
    assert!(
        svg.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "SVG missing xmlns declaration"
    );
}

#[test]
fn polygon_count_matches_cell_count() {
    let (world, svg) = render_ref_world();
    let polygon_count = svg.matches("<polygon").count();
    // The biomes renderer skips cells whose `cell_vertices` is empty, so
    // we allow non-empty cells to be the upper bound. In practice every
    // mesh cell has a polygon — assert exact match and let any future
    // mesh change surface here.
    let nonempty = world
        .mesh
        .cell_vertices
        .iter()
        .filter(|v| !v.is_empty())
        .count();
    assert_eq!(
        polygon_count,
        nonempty,
        "polygon count {polygon_count} ≠ non-empty cell count {nonempty} \
         (mesh.cell_count() = {})",
        world.mesh.cell_count()
    );
}

#[test]
fn no_nan_coordinates_in_output() {
    let (_, svg) = render_ref_world();
    // Rust's `f32::Display` writes "NaN" for non-finite values. The
    // renderer uses `{:.1}` formatting, which also produces "NaN".
    // A `NaN` substring anywhere would indicate float corruption
    // upstream (mesh build, erosion, hydrology).
    assert!(
        !svg.contains("NaN"),
        "found NaN in rendered SVG — float corruption upstream"
    );
    // "inf" is the other non-finite signal worth catching.
    assert!(
        !svg.contains("inf"),
        "found 'inf' in rendered SVG — float corruption upstream"
    );
}

#[test]
fn always_present_biome_colors_emitted_on_reference_world() {
    // What this asserts and what it does not:
    //
    // The 14-biome palette is documented as "all colors emit on a
    // representative world." Empirically, on seed 42 / 4k cells via the
    // current Köppen-Geiger path, two biome IDs are *never assigned*
    // because no `KoppenClass` maps to them in `mapgen-world::koppen`:
    //
    //   * TEMPERATE_GRASSLAND (id 4)
    //   * TROPICAL_DRY_FOREST (id 9)
    //
    // The Whittaker fallback emits both, but it only runs when seasonal
    // climate data is absent. This is the known palette-collapse issue
    // tracked as "Wider biome palette" in `docs/BACKLOG.md` (revival
    // trigger: render needs to distinguish humid-subtropical from
    // oceanic-temperate visually).
    //
    // Likewise, several Köppen-reachable biomes (SNOW, TAIGA,
    // TEMPERATE_RAINFOREST) require narrow climate-band conditions —
    // they show on some seeds but not seed 42 at 4k cells.
    //
    // What we *do* enforce: the common-and-Köppen-reachable subset of
    // the palette renders. A regression that collapses any of these
    // into another color, removes a color from the palette, or breaks
    // the riparian override fires this test.
    let (_, svg) = render_ref_world();
    let must_emit: &[(&str, &str)] = &[
        ("#5d8a4e", "TEMPERATE_FOREST"),
        ("#e8d49a", "DESERT"),
        ("#d8ba6b", "SAVANNA"),
        ("#2e6b3e", "TROPICAL_RAINFOREST"),
        ("#b5b07a", "SHRUBLAND"),
        ("#b8b3a8", "ALPINE"),
        ("#7da6c8", "SEA_SHALLOW"),
        ("#3a5d85", "SEA_DEEP"),
        ("#4d8a3a", "RIPARIAN"),
        ("#c4cdb7", "TUNDRA"),
    ];

    let missing: Vec<&str> = must_emit
        .iter()
        .filter(|(hex, _)| !svg.contains(hex))
        .map(|(_, name)| *name)
        .collect();

    assert!(
        missing.is_empty(),
        "seed-42 reference world missing biome color(s) from rendered SVG: {missing:?}"
    );

    // Also assert no unassigned-grey leak: every land cell should
    // resolve to a real biome. "#a0a0a0" is the unassigned fallback.
    assert!(
        !svg.contains("#a0a0a0"),
        "unassigned-biome fallback color (#a0a0a0) appeared in SVG — \
         a land cell escaped classification"
    );
}

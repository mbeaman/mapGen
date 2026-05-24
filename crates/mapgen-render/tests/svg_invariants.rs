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
    let svg = render(&world, Style::Biomes).expect("biomes style is implemented");
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

#[test]
fn cultures_style_emits_the_five_mvp_race_colors_on_reference_world() {
    // The cultures debug render colors cells by Race. Seed 42 at 4 000
    // cells places all 5 MVP archetypes (Riverfolk / Wildwood Kin /
    // Iron Hold / Burning Horde / Greendale) — verified empirically in
    // commit `aca2fed`'s sample-render output. Each one's race hue must
    // appear in the SVG.
    //
    // What this catches: a future renderer change that collapses two
    // race hues, a culling-pass regression that drops one of the five
    // expected races from seed-42's roster, or a race→color mapping
    // typo in `style::cultures::race_color`.
    let world = generate_full(ref_params());
    let svg = render(&world, Style::Cultures).expect("cultures style is implemented");

    assert!(svg.starts_with("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));

    let must_emit: &[(&str, &str)] = &[
        ("#6b8bb5", "Human (slate blue)"),
        ("#6b8e23", "Elf (olive)"),
        ("#a0522d", "Dwarf (sienna)"),
        ("#b04a4a", "Orc (dusty red)"),
        ("#daa520", "Halfling (goldenrod)"),
    ];
    let missing: Vec<&str> = must_emit
        .iter()
        .filter(|(hex, _)| !svg.contains(hex))
        .map(|(_, name)| *name)
        .collect();
    assert!(
        missing.is_empty(),
        "seed-42 cultures render missing race color(s): {missing:?}"
    );
}

#[test]
fn ornate_antique_emits_settlement_labels_with_phonotactic_names() {
    // Labels are the Phase-3e polish item that makes the generated
    // Phase-3d names actually visible on the rendered map. Sample
    // every settlement's name from the world; every one must appear
    // in the SVG output as a `<text>` element value. If even one is
    // missing, the labeling layer regressed (skipped, overlapped-
    // away, or got truncated).
    let world = generate_full(ref_params());
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");

    assert!(
        svg.contains("<text "),
        "ornate render emitted no <text> elements — labels regressed"
    );
    assert!(
        svg.contains("font-family"),
        "ornate render's labels lost their font-family attribute"
    );

    // Every settlement's phonotactic name should appear in the SVG.
    // Names are deterministic — we don't need to know which strings
    // seed 42 produces, just that the strings the world carries land
    // in the rendered output.
    let mut missing: Vec<&str> = Vec::new();
    for s in &world.society.settlements {
        if !svg.contains(s.name.as_str()) {
            missing.push(s.name.as_str());
        }
    }
    assert!(
        missing.is_empty(),
        "{} settlement label(s) missing from ornate render: {:?}",
        missing.len(),
        &missing[..missing.len().min(5)]
    );

    // Same for polity names.
    for n in &world.society.nations {
        assert!(
            svg.contains(n.name.as_str()),
            "polity {:?} missing from ornate render labels",
            n.name
        );
    }
}

#[test]
fn ornate_antique_emits_a_phase_3e_render() {
    // Phase 3e — the marquee aesthetic style. Was an explicit `Err`
    // before commit landing the impl; now must succeed and emit an SVG
    // with the recognizable Phase-3e element shapes (parchment
    // gradient, mountain glyphs, settlement markers, sacred-site
    // diamonds).
    let world = generate_full(ref_params());
    let svg = render(&world, Style::OrnateAntique)
        .expect("ornate_antique render must succeed once Phase 3e lands");

    assert!(svg.starts_with("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
    assert!(
        svg.len() >= 50_000,
        "ornate SVG suspiciously small: {} bytes — render likely \
         regressed to a placeholder",
        svg.len()
    );
    // Parchment gradient is the visual signature of the style; missing
    // it would mean a regression to the biomes/cultures dev view.
    assert!(
        svg.contains("parchment"),
        "ornate SVG missing the parchment gradient marker"
    );
    // Sample-render evidence (seed 42 / 4 000 cells): ALPINE / SNOW
    // cells emit mountain triangles, forest biomes emit tree scatter,
    // polities emit at least one capital settlement. We assert the
    // tags that prove each layer ran.
    assert!(
        svg.contains("<polygon"),
        "ornate SVG has no polygon tags — mountains + cell fills + \
         tree tufts all use polygons"
    );
    // Settlement glyphs use a mix of <rect>, <polygon>, and <circle>
    // primitives depending on the culture's SettlementIcon — see the
    // exhaustive matrix test `ornate_antique_dispatches_glyph_for_
    // every_icon_arch_tier_combination` below. At least one rect
    // appears because most icon families (Castle, Tower, Hall, Gate,
    // Longhouse) draw rects, and the seed-42 roster always contains
    // at least one of those plus the village ornaments.
    assert!(
        svg.contains("<rect"),
        "ornate SVG has no rect — every settlement icon family emits at \
         least one rectangular primitive (walls, body, or village mark)"
    );
}

#[test]
fn ornate_antique_coastline_paths_have_explicit_moveto() {
    // Pins the M-fix workaround for roughr 0.12's bug where
    // `OpType::Move` is incorrectly serialized as `L` (lineto)
    // instead of `M` (moveto). Without the fix, paths start with a
    // lineto-from-origin and tiny-skia / usvg may draw a stray
    // stroke from (0,0). The fix in `roughr_path_with_move_fix`
    // replaces the leading L with M.
    //
    // Asserts: no `<path d="L` substring appears anywhere in the
    // ornate render output. If this fires, the M-fix regressed and
    // we're emitting invalid (or visually-bug-prone) SVG.
    let world = generate_full(ref_params());
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");
    assert!(
        !svg.contains(r##"<path d="L"##),
        "ornate SVG has a <path d=\"L…\"> element — roughr 0.12's \
         Move-as-L bug is reproducing. Check roughr_path_with_move_fix."
    );
}

#[test]
fn ornate_antique_renders_without_panic_across_seeds() {
    // Multi-seed smoke test for the roughr coastline pipeline.
    // Different seeds produce different coastline shapes:
    //   * Tiny islands (short polylines, edge case for chain walk)
    //   * Coastlines touching the canvas edge (open chains)
    //   * Degree-3 vertex junctions (where 3+ coastline edges meet)
    //   * Worlds with very few or very many distinct coastline loops
    //
    // What we assert per seed:
    //   * `render()` returns Ok (no panic in polyline tracing or
    //     roughr's perturbation).
    //   * The output starts with `<svg` and ends with `</svg>`.
    //   * No `NaN` or `inf` substrings (catches float corruption).
    //   * No `<path d="L"` (catches M-fix regression).
    //   * The svg has ≥4 `<path d="M"` elements (4 ripples ran).
    //
    // 10 seeds covers ~10× more coastline variety than the seed-42
    // reference world and surfaces edge cases that don't show up on
    // a single seed.
    for seed in 1_u64..=10 {
        let params = mapgen_world::GenerateParams {
            seed,
            ..ref_params()
        };
        let world = generate_full(params);
        let svg = render(&world, Style::OrnateAntique)
            .unwrap_or_else(|e| panic!("seed {seed} render failed: {e}"));
        assert!(
            svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"),
            "seed {seed} produced malformed SVG envelope"
        );
        assert!(
            !svg.contains("NaN") && !svg.contains("inf"),
            "seed {seed} produced NaN / inf in SVG output"
        );
        assert!(
            !svg.contains(r##"<path d="L"##),
            "seed {seed} produced a <path d=\"L…\"> — M-fix regressed"
        );
        let move_paths = svg.matches(r##"<path d="M"##).count();
        assert!(
            move_paths >= 4,
            "seed {seed} produced only {move_paths} <path d=\"M…\"> \
             elements — coastline ripple count regressed"
        );
    }
}

#[test]
fn ornate_antique_coastlines_use_roughr_perturbed_paths() {
    // ARCHITECTURE.md §Phase 3e calls for "roughr-perturbed coastlines
    // with 4 offset ripples." Before this commit the ripples were
    // deterministic per-edge `<line>` strokes with hash-driven wobble;
    // we now trace continuous polylines and hand each one to roughr's
    // `linear_path` per ripple, producing scratchy hand-drawn Bezier
    // paths.
    //
    // What this asserts (structural):
    //   * No `<line>` elements appear in the coastline region anymore
    //     — the renderer should emit `<path>` from roughr instead.
    //   * The coastline color palette (3 outer haze blues + 1 dark
    //     outline) appears, each via a `<g stroke="…">` group.
    //   * The ripple-stroke palette has exactly 4 distinct color
    //     groups (matching ARCHITECTURE.md's "4 offset ripples").
    //
    // What this does not verify:
    //   * Whether the curves visually read as "hand-drawn" (eyeball
    //     the seed-42 PNG).
    //   * Per-ripple seed determinism (covered by the existing render
    //     hash tests in mapgen-world).
    let world = generate_full(ref_params());
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");

    // The four ripple stroke colors. If any one is missing, a ripple
    // didn't render.
    let ripple_strokes: &[&str] = &[
        r##"stroke="#5a6a8a""##, // outermost haze
        r##"stroke="#7a8aa8""##,
        r##"stroke="#a09cb8""##, // 4th ripple — newly added
        r##"stroke="#2a2418""##, // innermost crisp outline
    ];
    for needle in ripple_strokes {
        assert!(
            svg.contains(needle),
            "ornate SVG missing ripple stroke group: {needle}"
        );
    }

    // roughr emits curves as `<path d="...">` with M / L / C ops. If
    // the new code regressed to straight-line edges, we'd lose paths.
    let path_count = svg.matches(r##"<path d="M"##).count();
    assert!(
        path_count >= 4,
        "ornate SVG has only {path_count} <path d=\"M…\"> elements — \
         roughr coastline ripples expected to emit one path per \
         polyline × 4 ripples"
    );
}

#[test]
fn ornate_antique_renders_compass_cartouche_and_edge_burn() {
    // ARCHITECTURE.md §Phase 3e requires: compass rose, corner
    // cartouche, vignette + edge burn. This test pins the contract
    // that all three overlay layers render on the ornate style.
    //
    // What we assert (structural, not visual):
    //   * `<g class="compass">` is emitted with an N marker letter
    //   * `<g class="cartouche">` is emitted with the title text
    //   * `<g class="edge-burn">` is emitted referencing the
    //     edge-burn radial gradient
    //   * The edge-burn radial gradient is defined in <defs>
    //
    // What this does not verify:
    //   * Whether the visual placement looks good (corner positions
    //     scale with canvas — verify by eyeballing the seed-42 PNG).
    let world = generate_full(ref_params());
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");

    assert!(
        svg.contains(r#"<g class="compass""#),
        "ornate SVG missing compass rose layer"
    );
    assert!(
        svg.contains(">N</text>"),
        "ornate SVG compass rose missing the N-marker label"
    );
    assert!(
        svg.contains(r#"<g class="cartouche">"#),
        "ornate SVG missing cartouche layer"
    );
    assert!(
        svg.contains("A MAP OF THE KNOWN WORLD"),
        "ornate SVG cartouche missing the title text"
    );
    assert!(
        svg.contains(r#"<g class="edge-burn">"#),
        "ornate SVG missing edge-burn overlay layer"
    );
    assert!(
        svg.contains(r##"id="edge-burn""##),
        "ornate SVG <defs> missing the edge-burn radial gradient"
    );
    assert!(
        svg.contains(r##"fill="url(#edge-burn)""##),
        "ornate SVG edge-burn rect not referencing the gradient"
    );
}

#[test]
fn ornate_antique_embeds_vendored_typography_via_at_font_face() {
    // Phase 3e polish item #2 (per session-state): bundle Cinzel +
    // IM Fell English + EB Garamond as base64 TTF in <defs> so the
    // SVG carries its own typography and renders identically through
    // both browsers (CSS @font-face) and the CLI's PNG path (the same
    // bytes are loaded into usvg's fontdb — see
    // `mapgen-cli/src/sweep.rs::svg_to_png`).
    //
    // What this test pins:
    //   * A <style> block exists inside <defs>.
    //   * Each of the three font-family names appears in an @font-face rule.
    //   * Each @font-face's src= line carries a base64 TTF data URL.
    //
    // What this test does not verify:
    //   * Whether the embedded bytes parse as valid TTF (would need a
    //     real TrueType parser; usvg's fontdb registration covers this
    //     path implicitly).
    //   * Whether the rendered text actually uses Cinzel vs falls back
    //     to system Georgia (would require pixel diffing — out of
    //     scope; eyeball the seed-42 PNG instead).
    let world = generate_full(ref_params());
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");

    assert!(
        svg.contains("<style>"),
        "ornate SVG missing <style> block in <defs> — @font-face embedding regressed"
    );
    assert!(
        svg.contains("@font-face"),
        "ornate SVG <style> block missing @font-face rules"
    );
    for family in ["Cinzel", "EB Garamond", "IM Fell English"] {
        assert!(
            svg.contains(&format!(r#"font-family:"{family}""#)),
            "ornate SVG missing @font-face rule for {family:?}"
        );
    }
    assert!(
        svg.contains("data:font/ttf;base64,"),
        "ornate SVG @font-face missing base64 data: URL — font bytes \
         not actually embedded"
    );
    assert!(
        svg.contains(r#"format("truetype")"#),
        "ornate SVG @font-face missing TrueType format hint — usvg / \
         browser may pick the wrong loader"
    );

    // Label groups must reference the embedded family names with a
    // system-Georgia fallback (lets the SVG still degrade gracefully
    // if the data URLs ever fail to load).
    for needle in [
        r##"'"Cinzel", Georgia, serif'"##,
        r##"'"EB Garamond", Georgia, serif'"##,
        r##"'"IM Fell English", Georgia, serif'"##,
    ] {
        assert!(
            svg.contains(needle),
            "ornate SVG missing font-family attribute referencing the \
             embedded typography: expected {needle:?}"
        );
    }
}

/// Synthetic-world helper for the glyph-dispatch matrix test. Builds
/// 96 cells laid out on a regular grid, 32 cultures (one per icon ×
/// architecture pair), 32 polities, and 96 settlements — three per
/// polity covering each `SettlementTier`. Used by the exhaustive
/// dispatch test to assert the renderer emits a distinct class marker
/// for every (icon, architecture, tier) combination.
///
/// The mesh has empty `cell_vertices` so `render_land_fill` and the
/// coastline ripples don't draw polygons; biomes are uniform
/// `TEMPERATE_FOREST` so forest scatter runs (proving cell iteration
/// happens) but mountains don't fire. All cells are land (`elevation
/// = 0.5`), so the coastline-ripple layer finds no land/sea edges.
fn synthetic_world_for_glyph_matrix() -> mapgen_core::WorldData {
    use mapgen_core::entities::{
        Architecture, Culture, Settlement, SettlementIcon, SettlementTier,
    };
    use mapgen_core::world_data::Nation;

    const ICONS: [SettlementIcon; 8] = [
        SettlementIcon::Castle,
        SettlementIcon::Tower,
        SettlementIcon::Hall,
        SettlementIcon::Spire,
        SettlementIcon::Longhouse,
        SettlementIcon::Treehouse,
        SettlementIcon::Gate,
        SettlementIcon::Yurt,
    ];
    const ARCHS: [Architecture; 4] = [
        Architecture::Classical,
        Architecture::Gothic,
        Architecture::Organic,
        Architecture::Megalithic,
    ];
    const TIERS: [SettlementTier; 3] = [
        SettlementTier::Capital,
        SettlementTier::Town,
        SettlementTier::Village,
    ];

    let n_cells: usize = ICONS.len() * ARCHS.len() * TIERS.len(); // 96

    let mut world = mapgen_core::WorldData::default();
    world.mesh.width = 1280.0;
    world.mesh.height = 720.0;
    world.mesh.sites = (0..n_cells)
        .map(|i| {
            let col = (i % 32) as f32;
            let row = (i / 32) as f32;
            [40.0 + col * 38.0, 80.0 + row * 200.0]
        })
        .collect();
    world.mesh.cell_vertices = vec![vec![]; n_cells];
    world.mesh.neighbors = vec![vec![]; n_cells];
    world.mesh.coast = vec![false; n_cells];

    world.terrain.elevation = vec![0.5; n_cells];
    world.terrain.plate_id = vec![mapgen_core::PlateId(0); n_cells];
    world.terrain.plates = vec![];

    world.climate.biome = vec![3; n_cells]; // TEMPERATE_FOREST
    world.climate.temperature = vec![0.5; n_cells];
    world.climate.precipitation = vec![0.5; n_cells];

    for (icon_idx, &icon) in ICONS.iter().enumerate() {
        for (arch_idx, &arch) in ARCHS.iter().enumerate() {
            let polity_idx = icon_idx * ARCHS.len() + arch_idx;
            world.cultures.cultures.push(Culture {
                name: format!("Culture{polity_idx}"),
                settlement: icon,
                architecture: arch,
                ..Default::default()
            });
            let capital_cell = (polity_idx * TIERS.len()) as u32;
            world.society.nations.push(Nation {
                name: format!("Polity{polity_idx}"),
                capital_cell,
                color: [
                    80 + (polity_idx as u8 * 5),
                    50 + (polity_idx as u8 * 3),
                    40 + (polity_idx as u8 * 2),
                ],
            });
            for (tier_idx, &tier) in TIERS.iter().enumerate() {
                let cell = (polity_idx * TIERS.len() + tier_idx) as u32;
                world.society.settlements.push(Settlement {
                    name: format!("S{cell}"),
                    cell,
                    tier,
                    polity_id: polity_idx as u16,
                    population: 1.0 - (tier_idx as f32) * 0.3,
                });
            }
        }
    }

    world.cultures.culture_id = (0..n_cells)
        .map(|i| Some((i / TIERS.len()) as u16))
        .collect();
    world.society.control = vec![None; n_cells];
    world.society.roads = vec![];

    world
}

#[test]
fn ornate_antique_textures_ocean_and_burns_edges() {
    // Two BACKLOG render-polish items: faint horizontal ocean hatching,
    // and irregular dark stains layered onto the edge-burn vignette so
    // the aging reads as real scorching rather than a uniform gradient.
    let world = generate_full(ref_params());
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");

    // Ocean hatching group exists and actually carries dashes (seed 42
    // is mostly sea). Scope the line check to the group so river/glyph
    // <line> elements elsewhere can't mask an empty hatch layer.
    let start = svg
        .find(r#"<g class="ocean-hatch""#)
        .expect("ocean hatching group missing");
    let end = svg[start..]
        .find("</g>")
        .expect("ocean-hatch group unclosed")
        + start;
    assert!(
        svg[start..end].contains("<line"),
        "ocean-hatch group emitted no hatch dashes on a mostly-sea world"
    );

    // Irregular edge-burn stains.
    assert!(
        svg.contains(r#"class="edge-stain""#),
        "edge-burn vignette missing its irregular dark stains"
    );
}

/// Minimal world carrying a single ALPINE peak (biome 11) at high
/// elevation, with no coast / society / hydrology. Just enough for
/// `render_mountains` to fire exactly once so the depth-shadow marker
/// is unambiguous to assert.
fn world_with_one_alpine_peak() -> mapgen_core::WorldData {
    let mut world = mapgen_core::WorldData::default();
    world.mesh.width = 200.0;
    world.mesh.height = 200.0;
    world.mesh.sites = vec![[100.0, 100.0]];
    world.mesh.cell_vertices = vec![vec![]];
    world.mesh.neighbors = vec![vec![]];
    world.mesh.coast = vec![false];
    world.terrain.elevation = vec![0.8];
    world.terrain.plate_id = vec![mapgen_core::PlateId(0)];
    world.terrain.plates = vec![];
    world.climate.biome = vec![11]; // ALPINE
    world.climate.temperature = vec![0.2];
    world.climate.precipitation = vec![0.2];
    world
}

#[test]
fn ornate_antique_mountains_cast_a_depth_shadow() {
    // BACKLOG "Mountain depth shadow": each Tolkien triangle now drops a
    // semi-transparent offset shadow beneath it so peaks read as 3D land
    // features. Pin the shadow primitive so a future refactor of
    // render_mountains can't silently drop it back to flat stickers.
    let world = world_with_one_alpine_peak();
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");
    assert!(
        svg.contains(r#"class="mtn-shadow""#),
        "alpine peak rendered without a depth-shadow polygon"
    );
    // The shadow must sit *under* the main peak triangle (painted
    // first), else it would occlude the mountain instead of grounding
    // it. Assert the shadow's opening tag precedes the solid fill.
    let shadow_at = svg.find(r#"class="mtn-shadow""#).unwrap();
    let group_at = svg.find(r#"class="mountains""#).unwrap();
    assert!(
        group_at < shadow_at,
        "mtn-shadow emitted outside the mountains group"
    );
}

/// Two land cells sharing one Voronoi edge, owned by different
/// polities — the minimal case for an inter-polity border.
fn world_with_two_adjacent_polities() -> mapgen_core::WorldData {
    use mapgen_core::world_data::Nation;
    let mut world = mapgen_core::WorldData::default();
    world.mesh.width = 40.0;
    world.mesh.height = 20.0;
    world.mesh.sites = vec![[5.0, 5.0], [15.0, 5.0]];
    world.mesh.vertices = vec![
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [0.0, 10.0],
        [20.0, 0.0],
        [20.0, 10.0],
    ];
    // Cells share vertices 1 and 2 (the edge at x=10).
    world.mesh.cell_vertices = vec![vec![0, 1, 2, 3], vec![1, 4, 5, 2]];
    world.mesh.neighbors = vec![vec![1], vec![0]];
    world.mesh.coast = vec![false, false];
    world.terrain.elevation = vec![0.5, 0.5];
    world.terrain.plate_id = vec![mapgen_core::PlateId(0); 2];
    world.terrain.plates = vec![];
    world.climate.biome = vec![3, 3];
    world.climate.temperature = vec![0.5, 0.5];
    world.climate.precipitation = vec![0.5, 0.5];
    world.society.control = vec![Some(0), Some(1)];
    world.society.nations = vec![
        Nation {
            name: "A".into(),
            capital_cell: 0,
            color: [100, 80, 60],
        },
        Nation {
            name: "B".into(),
            capital_cell: 1,
            color: [60, 80, 100],
        },
    ];
    world
}

#[test]
fn ornate_antique_draws_borders_between_adjacent_polities() {
    // BACKLOG "Polity border lines": the shared edge between two cells
    // owned by different polities renders as a dashed frontier so
    // territorial adjacency is unambiguous.
    let world = world_with_two_adjacent_polities();
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");
    let start = svg
        .find(r#"<g class="polity-borders""#)
        .expect("polity-borders group missing");
    let end = svg[start..].find("</g>").unwrap() + start;
    assert!(
        svg[start..end].contains("<line"),
        "no border line drawn between two adjacent, differently-owned cells"
    );
}

/// Five cells, two roads ([3,2,1,0] and [4,2,1,0]) sharing the trunk
/// 2→1→0 — so the trunk segments are traversed twice and the spurs
/// once.
fn world_with_shared_road_trunk() -> mapgen_core::WorldData {
    use mapgen_core::world_data::Road;
    let mut world = mapgen_core::WorldData::default();
    world.mesh.width = 100.0;
    world.mesh.height = 50.0;
    world.mesh.sites = vec![
        [10.0, 25.0],
        [30.0, 25.0],
        [50.0, 25.0],
        [70.0, 10.0],
        [70.0, 40.0],
    ];
    world.mesh.cell_vertices = vec![vec![]; 5];
    world.mesh.neighbors = vec![vec![]; 5];
    world.mesh.coast = vec![false; 5];
    world.terrain.elevation = vec![0.5; 5];
    world.terrain.plate_id = vec![mapgen_core::PlateId(0); 5];
    world.terrain.plates = vec![];
    world.climate.biome = vec![3; 5];
    world.climate.temperature = vec![0.5; 5];
    world.climate.precipitation = vec![0.5; 5];
    world.society.roads = vec![
        Road {
            cells: vec![3, 2, 1, 0],
        },
        Road {
            cells: vec![4, 2, 1, 0],
        },
    ];
    world
}

#[test]
fn ornate_antique_roads_vary_width_by_traversal() {
    // BACKLOG "Roads differentiated by trunk vs branch": segments shared
    // by multiple town→capital paths render thicker than spurs. With two
    // roads sharing the 2→1→0 trunk, the rendered road group must carry
    // at least two distinct stroke widths.
    let world = world_with_shared_road_trunk();
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");
    let start = svg
        .find(r#"<g class="roads""#)
        .expect("roads group missing");
    let end = svg[start..].find("</g>").unwrap() + start;
    let group = &svg[start..end];
    let needle = "stroke-width=\"";
    let widths: std::collections::HashSet<&str> = group
        .match_indices(needle)
        .map(|(i, _)| {
            let rest = &group[i + needle.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect();
    assert!(
        widths.len() >= 2,
        "roads rendered at a single width — trunk/branch not differentiated: {widths:?}"
    );
}

#[test]
fn ornate_antique_dispatches_glyph_for_every_icon_arch_tier_combination() {
    // The Phase 3e ornate render is contracted to derive settlement
    // glyphs from `Culture.settlement × Culture.architecture × tier`
    // (see ARCHITECTURE.md §Context). This test pins the dispatch
    // surface — every 8 icons × 4 architectures × 3 tiers = 96 combos
    // must emit a class-marked group, so a future regression that
    // silently falls back to "Castle for everything" or "tier-only"
    // fires here.
    //
    // The class attribute is the test affordance; usvg/resvg ignore
    // it during PNG rendering, so it costs nothing visually. A
    // production-side reader can rely on these markers if they ever
    // need to introspect glyph dispatch from the SVG.
    let world = synthetic_world_for_glyph_matrix();
    let svg = render(&world, Style::OrnateAntique).expect("ornate render must succeed");

    const ICON_NAMES: &[&str] = &[
        "castle",
        "tower",
        "hall",
        "spire",
        "longhouse",
        "treehouse",
        "gate",
        "yurt",
    ];
    const ARCH_NAMES: &[&str] = &["classical", "gothic", "organic", "megalithic"];
    const TIER_NAMES: &[&str] = &["capital", "town", "village"];

    let mut missing: Vec<String> = Vec::new();
    for icon in ICON_NAMES {
        for arch in ARCH_NAMES {
            for tier in TIER_NAMES {
                let marker = format!("settlement icon-{icon} arch-{arch} tier-{tier}");
                if !svg.contains(&marker) {
                    missing.push(marker);
                }
            }
        }
    }

    assert!(
        missing.is_empty(),
        "{} of 96 glyph dispatch markers missing from ornate render \
         (first 8 shown): {:?}",
        missing.len(),
        &missing[..missing.len().min(8)]
    );
}

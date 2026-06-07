//! Cartographic generalisation (ADR 0001 §3) — the zoom-OUT overview simplifies
//! natural lines it can't resolve at scale. Increment 1: Visvalingam–Whyatt on the
//! planet/globe major rivers. Render-only (the planet SVG is never hashed; WorldData
//! is untouched), so this never moves a golden.

use mapgen_render::{render, style::Style};
use mapgen_testsupport::planet_params;
use mapgen_world::generate_full;

/// Seed 11 is the canonical multi-continent fixture and the only canonical planet
/// seed with Strahler≥4 rivers (12 of them, 183 raw cells) — so the overview
/// actually draws major rivers to simplify.
const RIVER_SEED: u64 = 11;

/// Raw vertex count the major rivers WOULD draw with no simplification: the sum of
/// cells over Strahler≥4 rivers (cells ≥ 2), matching `render_major_rivers`' filter.
fn raw_river_points(world: &mapgen_core::WorldData) -> usize {
    world
        .hydrology
        .rivers
        .iter()
        .filter(|r| r.strahler >= 4 && r.cells.len() >= 2)
        .map(|r| r.cells.len())
        .sum()
}

/// The rivers `<g>` (its stroke colour `#4f6f86` is unique to rivers — coast and
/// graticule are `#5a4326`), up to its closing tag (no nested groups inside).
fn rivers_group(svg: &str) -> &str {
    let start = svg
        .find(r##"<g fill="none" stroke="#4f6f86""##)
        .expect("planet render must have a rivers group");
    let rest = &svg[start..];
    let end = rest.find("</g>").expect("rivers group must close");
    &rest[..end]
}

/// Total `x,y` points across every `points="…"` polyline in a group.
fn count_points(group: &str) -> usize {
    let mut total = 0;
    let mut rest = group;
    while let Some(i) = rest.find("points=\"") {
        rest = &rest[i + "points=\"".len()..];
        let end = rest.find('"').expect("points attr closes");
        total += rest[..end].split_whitespace().count();
        rest = &rest[end..];
    }
    total
}

#[test]
fn overview_major_rivers_are_visvalingam_simplified() {
    let world = generate_full(planet_params(RIVER_SEED));
    let raw = raw_river_points(&world);
    assert!(raw > 0, "fixture precondition: seed 11 must have major rivers to simplify");

    let svg = render(&world, Style::Planet).expect("planet renders");
    let drawn = count_points(rivers_group(&svg));

    // The overview still draws the rivers...
    assert!(drawn > 0, "the overview must still draw the major rivers");
    // ...but with sub-scale wiggle dropped, so fewer points than raw cells. Remove
    // the `visvalingam` call in render_major_rivers and drawn == raw → this trips.
    assert!(
        drawn < raw,
        "Visvalingam must drop sub-scale river vertices: drew {drawn} points from {raw} raw cells"
    );
}

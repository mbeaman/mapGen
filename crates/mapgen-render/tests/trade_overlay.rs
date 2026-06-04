//! Observable claim for the Trade-route lens: the planisphere draws the
//! *crossable* inter-continental sea lanes (the "Sundered Lanes"). The planet
//! render emits a `planet-trade` group holding exactly one `<line>` per crossable
//! lane (a lane whose `min_naval` gate is within a seafarer's reach), plus the
//! lens CSS that swaps it in under `on-trade` — so toggling Trade makes the lanes
//! that bind the continents visible.
//!
//! BOTH renders draw one `<line>` per crossable lane in the world they're handed —
//! the planet wash (`planet-trade`, Mollweide-projected) and the ornate continental
//! layer (`layer-trade`, world coords). The ornate group is only empty for a world
//! with no lanes (e.g. a single-landmass continent); handed a lane-bearing world it
//! draws them, so the count claim is asserted against BOTH styles below.
//! (`mapgen-cli/tests/atlas.rs` pins the ornate group's existence; the web e2e pins
//! its display swap.)

use mapgen_render::{render, style::Style};
use mapgen_testsupport::{planet_params, CROSSING_SEEDS};
use mapgen_world::generate_full;

/// The crossable-lane gate the renderer uses (`render_trade_routes` filters
/// `min_naval <= 40`). Pinned here so the test can't drift from the render.
const MAX_CROSSABLE_NAVAL: u8 = 40;

/// Inner content of the first `<g class="{class}" ...>…</g>`, tolerating extra
/// attrs (e.g. `display="none"`, stroke) on the open tag and balancing nested
/// `<g>`. Panics if the group is absent (so a missing group fails the test loud).
fn group<'a>(svg: &'a str, class: &str) -> &'a str {
    let open = format!(r#"<g class="{class}""#);
    let start = svg
        .find(&open)
        .unwrap_or_else(|| panic!("no <g class=\"{class}\"> group in render"));
    let after = start + svg[start..].find('>').map(|i| i + 1).unwrap_or(0);
    let b = svg.as_bytes();
    let mut i = after;
    let mut depth = 1i32;
    while i < b.len() {
        if b[i..].starts_with(b"</g>") {
            depth -= 1;
            if depth == 0 {
                break;
            }
            i += 4;
        } else if b[i..].starts_with(b"<g") {
            depth += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    &svg[after..i.min(svg.len())]
}

#[test]
fn the_planet_trade_group_draws_a_line_per_crossable_lane() {
    let mut drawn = 0usize;
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));

        // Count the crossable lanes straight from the world the render consumes —
        // the claim is tied to the data, not a magic number.
        let crossable = world
            .sea_lanes
            .lanes
            .iter()
            .filter(|l| l.min_naval <= MAX_CROSSABLE_NAVAL)
            .count();
        assert!(
            crossable > 0,
            "seed {seed}: a crossing seed has no crossable lane — the lens claim isn't exercised"
        );

        let svg = render(&world, Style::Planet).expect("planet render");

        // The lens machinery is present: the lane group + the CSS that swaps it in
        // under `on-trade` (so the frontend toggle has something to reveal).
        assert!(
            svg.contains(r##"<g class="planet-trade""##),
            "seed {seed}: no planet-trade lane group"
        );
        assert!(
            svg.contains("svg.on-trade .planet-trade"),
            "seed {seed}: the Trade lens CSS (TRADE_LENS_STYLE) is missing"
        );

        // The group holds EXACTLY one <line> per crossable lane. `==` (not `>=`)
        // also catches an over-inclusive filter that would draw impassable abysses.
        let trade = group(&svg, "planet-trade");
        let lines = trade.matches("<line").count();
        assert_eq!(
            lines, crossable,
            "seed {seed}: planet-trade drew {lines} lines for {crossable} crossable lanes"
        );
        drawn += lines;
    }
    assert!(
        drawn > 0,
        "no crossable lane was drawn on any crossing seed — the lens never drew anything"
    );
}

#[test]
fn the_ornate_trade_layer_draws_a_line_per_crossable_lane() {
    // The ornate continental `layer-trade` group is NOT empty for a lane-bearing
    // world — it draws each crossable lane in world coords. The atlas test only
    // checks the GROUP exists, so an empty/broken ornate render would ship silently;
    // this pins one `<line>` per crossable lane (`==`, catching an over-inclusive
    // filter), the ornate counterpart of the planet claim above.
    let mut drawn = 0usize;
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let crossable = world
            .sea_lanes
            .lanes
            .iter()
            .filter(|l| l.min_naval <= MAX_CROSSABLE_NAVAL)
            .count();
        assert!(crossable > 0, "seed {seed}: no crossable lane to draw");

        let svg = render(&world, Style::OrnateAntique).expect("ornate render");
        let trade = group(&svg, "layer-trade");
        let lines = trade.matches("<line").count();
        assert_eq!(
            lines, crossable,
            "seed {seed}: ornate layer-trade drew {lines} lines for {crossable} crossable lanes"
        );
        drawn += lines;
    }
    assert!(drawn > 0, "the ornate trade layer never drew a lane");
}

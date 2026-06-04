//! Observable claim for Phase 2 Diffusion: the Faith lens surfaces a faith that
//! crossed water. A religion that diffused onto a second landmass (spans ≥2
//! sizable bodies, per the data-layer `diffusion_claims`) appears in the
//! planet-faith wash as its own `data-religion` group — so toggling Faith makes
//! the crossing visible, the render-level counterpart to the data-level claim.

use mapgen_render::{render, style::Style};
use mapgen_testsupport::{
    planet_params, reference_params, religions_spanning_multiple_landmasses, CROSSING_SEEDS,
    REFERENCE_SEED,
};
use mapgen_world::generate_full;

/// Inner content of the first `<g class="{class}" ...>…</g>`, tolerating extra
/// attrs (e.g. `display="none"`) on the open tag and balancing nested `<g>`.
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

/// Inner content of `<g class="layer-{name}" …>…</g>` (the ornate layer groups).
fn layer_group<'a>(svg: &'a str, name: &str) -> &'a str {
    group(svg, &format!("layer-{name}"))
}

#[test]
fn the_faith_wash_surfaces_every_faith_that_crossed_water() {
    let mut surfaced = 0;
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let svg = render(&world, Style::Planet).expect("planet render");

        // The lens machinery is present: a faith wash group + the CSS that swaps
        // it in under `on-faith` (so the frontend toggle has something to reveal).
        assert!(
            svg.contains(r##"<g class="planet-faith""##),
            "seed {seed}: no planet-faith wash group"
        );
        assert!(
            svg.contains("svg.on-faith .planet-faith"),
            "seed {seed}: the Faith lens CSS (FAITH_LENS_STYLE) is missing"
        );

        // Every faith that crossed water (spans ≥2 sizable bodies) is surfaced as
        // its own data-religion group in the wash.
        for rid in religions_spanning_multiple_landmasses(&world) {
            assert!(
                svg.contains(&format!(r#"data-religion="{rid}""#)),
                "seed {seed}: faith {rid} crossed water but isn't drawn in the faith wash"
            );
            surfaced += 1;
        }
    }
    assert!(
        surfaced > 0,
        "no faith crossed water on any crossing seed — the surfacing claim was never exercised"
    );
}

#[test]
fn the_ornate_faith_layer_draws_the_faith_wash() {
    // The continental (ornate) faith layer must actually emit faith polygons —
    // the atlas test only checks the `layer-faith` GROUP exists, so an empty wash
    // would ship silently. Assert the group carries cells. (Mutation-verified:
    // early-returning ornate render_faith empties the group → this trips.)
    let world = generate_full(reference_params(REFERENCE_SEED));
    assert!(
        !world.religions.religions.is_empty(),
        "reference world should have religions to wash"
    );
    let svg = render(&world, Style::OrnateAntique).expect("ornate render");
    let faith = layer_group(&svg, "faith");
    assert!(
        faith.contains("<polygon"),
        "the ornate layer-faith group drew no faith cells (render_faith emitted nothing)"
    );
}

#[test]
fn the_planet_faith_wash_redraws_for_a_past_year() {
    // Render-level half of the Faith time-slider (Substage 3): the faith wash is a
    // function of `religion_id`, so rendering a reconstructed PAST faith map
    // (`religion_at_year`, before any diffusion) yields a DIFFERENT `planet-faith`
    // wash than the present — which is exactly what wasm `render_at_year` swaps in
    // live to animate the slider. Scoped to the wash group so an unrelated change
    // can't pass it. (Mutation-verified: a `render_faith` that ignores religion_id,
    // or a `religion_at_year` that ignores the year, collapses the two → trips.)
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let first = world
            .history
            .faith_changes
            .iter()
            .map(|c| c.year)
            .min()
            .expect("a crossing seed records diffusion");

        let present = render(&world, Style::Planet).expect("planet render");
        let mut past = world.clone();
        past.religions.religion_id = world.religion_at_year(first - 1); // pre-diffusion
        let past_svg = render(&past, Style::Planet).expect("planet render");

        assert_ne!(
            group(&present, "planet-faith"),
            group(&past_svg, "planet-faith"),
            "seed {seed}: the planet-faith wash is identical at founding and present — the \
             wash doesn't track religion_id, so the Faith slider can't animate"
        );
    }
}

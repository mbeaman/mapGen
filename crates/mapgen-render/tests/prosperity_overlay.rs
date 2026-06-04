//! Render-level claim for the v21 Prosperity heatmap. The prosperity overlay tints
//! each controlled land cell by its realm's `Nation::prosperity` through a
//! sequential pale→deep ramp, so trade's growth / war's stagnation reads as colour.
//!
//! The discriminating signal is that the COLOUR carries the prosperity value, not
//! merely that the group draws polygons: a wash keyed on a constant (or that
//! ignored `prosperity`) would still emit `<polygon>` at t=0. So we hand two
//! realms DIFFERENT prosperities (1.0 vs 0.1) on adjacent cells and assert the
//! planet-prosperity group paints TWO DISTINCT fills — only a ramp keyed on
//! `nations[pid].prosperity` produces that. (Mutation-verified: keying the ramp on
//! a constant, or on `0.0`, collapses both cells to one fill → trips.)

use mapgen_core::world_data::Nation;
use mapgen_render::{render, style::Style};
use mapgen_testsupport::{planet_params, CROSSING_SEEDS};
use mapgen_world::generate_full;

/// Inner content of the first `<g class="{class}" …>…</g>`, tolerating extra attrs
/// on the open tag and balancing nested `<g>`.
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

/// Distinct `fill="#rrggbb"` values appearing in `s`, in first-seen order.
fn distinct_fills(s: &str) -> Vec<String> {
    let mut seen = Vec::new();
    let mut rest = s;
    let needle = "fill=\"#";
    while let Some(p) = rest.find(needle) {
        let start = p + "fill=\"".len();
        let tail = &rest[start..];
        if let Some(end) = tail.find('"') {
            let f = tail[..end].to_string();
            if !seen.contains(&f) {
                seen.push(f);
            }
            rest = &tail[end..];
        } else {
            break;
        }
    }
    seen
}

/// Two adjacent land cells, one per realm; the realms carry different prosperity.
fn world_with_two_realms(prosperity: [f32; 2]) -> mapgen_core::WorldData {
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
    world.mesh.cell_vertices = vec![vec![0, 1, 2, 3], vec![1, 4, 5, 2]];
    world.mesh.neighbors = vec![vec![1], vec![0]];
    world.mesh.coast = vec![false, false];
    world.terrain.elevation = vec![0.5, 0.5]; // both land
    world.terrain.plate_id = vec![mapgen_core::PlateId(0); 2];
    world.climate.biome = vec![3, 3];
    world.climate.temperature = vec![0.5, 0.5];
    world.climate.precipitation = vec![0.5, 0.5];
    world.society.control = vec![Some(0), Some(1)];
    world.society.nations = vec![
        Nation {
            name: "Rich".into(),
            capital_cell: 0,
            color: [100, 80, 60],
            prosperity: prosperity[0],
        },
        Nation {
            name: "Poor".into(),
            capital_cell: 1,
            color: [60, 80, 100],
            prosperity: prosperity[1],
        },
    ];
    world
}

#[test]
fn planet_prosperity_wash_colours_cells_by_realm_prosperity() {
    let world = world_with_two_realms([1.0, 0.1]);
    let svg = render(&world, Style::Planet).expect("planet render");

    // The lens machinery is present so the frontend toggle has something to reveal.
    assert!(
        svg.contains(r##"<g class="planet-prosperity""##),
        "no planet-prosperity wash group"
    );
    assert!(
        svg.contains("svg.on-prosperity .planet-prosperity"),
        "the Prosperity lens CSS (PROSPERITY_LENS_STYLE) is missing"
    );

    let wash = group(&svg, "planet-prosperity");
    assert!(
        wash.contains("<polygon"),
        "the planet-prosperity group drew no cells"
    );

    // The colour must CARRY the prosperity value: two realms at 1.0 vs 0.1 must
    // paint two distinct fills. A constant-keyed ramp would collapse to one.
    let fills = distinct_fills(wash);
    assert!(
        fills.len() >= 2,
        "planet-prosperity used {} distinct fill(s) for two realms of different \
         prosperity (1.0 vs 0.1) — the ramp isn't keyed on Nation::prosperity: {fills:?}",
        fills.len()
    );
}

#[test]
fn equal_prosperity_paints_one_fill_but_unequal_paints_two() {
    // Tightens the discriminator: with EQUAL prosperity the two cells share a
    // fill (the colour is a pure function of prosperity, not of cell/polity id);
    // with UNEQUAL prosperity they diverge. Only a ramp keyed on the value yields
    // both halves — a per-polity constant would always give two.
    let equal = group(
        &render(&world_with_two_realms([0.5, 0.5]), Style::Planet).expect("render"),
        "planet-prosperity",
    )
    .to_string();
    let unequal = group(
        &render(&world_with_two_realms([1.0, 0.0]), Style::Planet).expect("render"),
        "planet-prosperity",
    )
    .to_string();
    assert_eq!(
        distinct_fills(&equal).len(),
        1,
        "equal prosperity should paint a single fill, got {:?}",
        distinct_fills(&equal)
    );
    assert_eq!(
        distinct_fills(&unequal).len(),
        2,
        "unequal prosperity should paint two fills, got {:?}",
        distinct_fills(&unequal)
    );
}

#[test]
fn ornate_equal_prosperity_paints_one_fill_but_unequal_paints_two() {
    // The ornate counterpart of the planet equal/unequal test — and the EQUAL half
    // is load-bearing. A `>= 2`-only check (two realms at differing prosperity →
    // two fills) is survived by a ramp keyed on polity/cell id, since pid 0 and pid
    // 1 paint two fills regardless. The equal case (both 0.5 → ONE fill) demands the
    // colour be a pure function of `Nation::prosperity`, killing a pid-keyed ramp.
    // (Mutation-verified: keying the ornate ramp on `pid` leaves the equal case
    // painting two fills → this trips.)
    let equal = group(
        &render(&world_with_two_realms([0.5, 0.5]), Style::OrnateAntique).expect("render"),
        "layer-prosperity",
    )
    .to_string();
    let unequal = group(
        &render(&world_with_two_realms([1.0, 0.0]), Style::OrnateAntique).expect("render"),
        "layer-prosperity",
    )
    .to_string();
    assert_eq!(
        distinct_fills(&equal).len(),
        1,
        "ornate: equal prosperity should paint a single fill, got {:?}",
        distinct_fills(&equal)
    );
    assert_eq!(
        distinct_fills(&unequal).len(),
        2,
        "ornate: unequal prosperity should paint two fills, got {:?}",
        distinct_fills(&unequal)
    );
}

#[test]
fn ornate_prosperity_layer_draws_a_graded_wash_on_a_real_world() {
    // The continental (ornate) prosperity layer must emit prosperity polygons on a
    // generated world — the atlas test only checks the GROUP exists, so an empty
    // wash would ship silently. Use a crossing seed (its history populates
    // prosperity) and assert the layer-prosperity group carries cells.
    // (Mutation-verified: skip the history write-back → all prosperity 0 still
    // draws polygons, so this test is paired with the DATA test's max==1.0 guard,
    // and the planet test's two-fill guard, which DO trip on that mutation.)
    let seed = CROSSING_SEEDS[0];
    let world = generate_full(planet_params(seed));
    assert!(
        !world.society.nations.is_empty(),
        "seed {seed}: no polities to wash"
    );
    let svg = render(&world, Style::OrnateAntique).expect("ornate render");
    let layer = group(&svg, "layer-prosperity");
    assert!(
        layer.contains("<polygon"),
        "seed {seed}: the ornate layer-prosperity group drew no cells"
    );
}

//! Cross-lens correlation (Phase 3 surfacing): the Trade lens tints each crossable
//! sea lane by the prosperity of the realms it connects — through the SAME ramp
//! the Prosperity wash uses — so the trade→prosperity causal loop is legible
//! across lenses. A lane binding rich shores is drawn deeper than one binding poor
//! shores. Render-path test: it asserts the rendered <line> strokes, not the
//! per-lane prosperity the render reads (which would re-test the data, vacuous).

use std::collections::BTreeSet;

use mapgen_render::{render, style::Style};
use mapgen_testsupport::{planet_params, CROSSING_SEEDS, SUNDERED_SEEDS};
use mapgen_world::generate_full;

const MAX_CROSSABLE_NAVAL: u8 = 40; // mirrors mapgen_render::style::MAX_CROSSABLE_NAVAL

/// Inner content of the `<g class="{class}" …>…</g>` group (balanced `<g>`, so
/// the ornate `layer-trade` wrapper's inner stroke group is included).
fn trade_group<'a>(svg: &'a str, class: &str) -> &'a str {
    let open = format!(r#"<g class="{class}""#);
    let start = svg
        .find(&open)
        .unwrap_or_else(|| panic!("no {class} group in render"));
    let after = start + svg[start..].find('>').map(|i| i + 1).unwrap_or(0);
    let b = svg.as_bytes();
    let (mut i, mut depth) = (after, 1i32);
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

/// The `stroke="#rrggbb"` of each `<line>` in document order; `None` if a line
/// carries no explicit stroke (it then inherits the group's fallback carmine).
fn line_strokes(group: &str) -> Vec<Option<String>> {
    group
        .match_indices("<line")
        .map(|(i, _)| {
            let end = group[i..].find("/>").map(|e| i + e).unwrap_or(group.len());
            let tag = &group[i..end];
            tag.find("stroke=\"").map(|s| {
                let rest = &tag[s + 8..];
                rest[..rest.find('"').unwrap_or(rest.len())].to_string()
            })
        })
        .collect()
}

/// Sum of the R, G, B channels of a `#rrggbb` colour — a monotone darkness proxy:
/// the prosperity ramp runs pale (high sum) → deep (low sum), so a deeper tint
/// (richer realms) has the LOWER sum.
fn rgb_sum(hex: &str) -> u32 {
    let h = hex.trim_start_matches('#');
    (0..3)
        .map(|k| u32::from_str_radix(&h[k * 2..k * 2 + 2], 16).unwrap_or(0))
        .sum()
}

/// Per crossable lane, in the SAME order the renderer draws them, the average
/// prosperity of the realms it connects. Used ONLY to select which lanes to
/// compare (the rich one, the poor one) — never to recompute the rendered colour.
fn lane_avg_prosperity(world: &mapgen_core::WorldData) -> Vec<Option<f32>> {
    let control = &world.society.control;
    let mesh = &world.mesh;
    let prosperity_of = |cell: usize| {
        control
            .get(cell)
            .copied()
            .flatten()
            .and_then(|pid| world.society.nations.get(pid as usize))
            .map(|n| n.prosperity)
    };
    world
        .sea_lanes
        .lanes
        .iter()
        .filter(|lane| lane.min_naval <= MAX_CROSSABLE_NAVAL)
        .filter(|lane| {
            mesh.sites.get(lane.a as usize).is_some() && mesh.sites.get(lane.b as usize).is_some()
        })
        .map(|lane| {
            match (
                prosperity_of(lane.a as usize),
                prosperity_of(lane.b as usize),
            ) {
                (Some(pa), Some(pb)) => Some((pa + pb) / 2.0),
                (Some(p), None) | (None, Some(p)) => Some(p),
                (None, None) => None,
            }
        })
        .collect()
}

#[test]
fn the_trade_lens_tints_each_lane_by_the_prosperity_it_connects() {
    // Fixture: seed 19 (probe-picked) grows 4 crossable lanes whose connected-realm
    // prosperity spans ~0.13→0.97 — wide enough that the richest and poorest lane
    // land on visibly different ramp colours. Asserted for BOTH render styles: the
    // planet `planet-trade` wash and the ornate `layer-trade` layer each tint via
    // the SAME prosperity ramp their own wash uses (different stops, both pale→deep,
    // so the *direction* holds across styles even though the exact hex differs).
    let seed = 19u64;
    assert!(
        CROSSING_SEEDS.contains(&seed),
        "fixture must be a crossing seed"
    );
    let world = generate_full(planet_params(seed));
    let avgs = lane_avg_prosperity(&world);

    // The richest- and poorest-connected lane (by input prosperity) — used only to
    // SELECT which two rendered lanes to compare, identically for every style.
    let idx_of = |pick: fn(&f32, &f32) -> bool| -> usize {
        avgs.iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|v| (i, v)))
            .reduce(|a, b| if pick(&a.1, &b.1) { a } else { b })
            .expect("a crossable lane with prosperity")
            .0
    };
    let rich_i = idx_of(|a, b| a >= b); // max avg
    let poor_i = idx_of(|a, b| a <= b); // min avg

    for (style, class) in [
        (Style::Planet, "planet-trade"),
        (Style::OrnateAntique, "layer-trade"),
    ] {
        let svg = render(&world, style).unwrap_or_else(|e| panic!("{class} render failed: {e}"));
        let strokes = line_strokes(trade_group(&svg, class));
        assert_eq!(
            strokes.len(),
            avgs.len(),
            "{class}: the test's lane filter must align with the renderer's (one <line> per crossable lane)"
        );
        assert!(
            strokes.len() >= 2,
            "{class}: fixture needs ≥2 crossable lanes"
        );

        // M1 — tinting actually happens: the lanes do NOT all share one stroke.
        // (Mutation: a constant per-lane stroke, or dropping the per-line stroke,
        // collapses this set to 1 → trips.)
        let distinct: BTreeSet<&String> = strokes.iter().flatten().collect();
        assert!(
            distinct.len() >= 2,
            "{class}: all crossable lanes share one stroke — no prosperity tint; got {strokes:?}"
        );

        // Identity — the colour MAPS to prosperity, in the right direction: the lane
        // binding the richest realms is drawn DEEPER (lower RGB sum, the ramp's rich
        // end) than the lane binding the poorest. Input prosperity only SELECTS the
        // two lanes; the assertion is on the rendered strokes. (Mutation: invert the
        // tint, or tint by a non-prosperity field → the richest-prosperity lane is no
        // longer the deepest → trips. Verified for BOTH styles.)
        let rich = strokes[rich_i]
            .as_ref()
            .unwrap_or_else(|| panic!("{class}: the richest lane is untinted"));
        let poor = strokes[poor_i]
            .as_ref()
            .unwrap_or_else(|| panic!("{class}: the poorest lane is untinted"));
        assert!(
            rgb_sum(rich) < rgb_sum(poor),
            "{class}: the lane binding the richest realms (avg {:.2}) must be deeper than the poorest (avg {:.2}); rich={rich} poor={poor}",
            avgs[rich_i].unwrap(),
            avgs[poor_i].unwrap()
        );
    }
}

#[test]
fn a_sundered_world_draws_no_lanes_to_tint() {
    // Both-directions guard: a sundered seed grows no crossable lane, so the trade
    // group draws zero <line> — there is nothing to tint. (A tint that somehow
    // fabricated lanes would trip here.)
    let seed = SUNDERED_SEEDS[0]; // 23
    let world = generate_full(planet_params(seed));
    let svg = render(&world, Style::Planet).expect("planet render");
    let group = trade_group(&svg, "planet-trade");
    assert!(
        !group.contains("<line"),
        "a sundered world must draw no trade lanes; got: {group}"
    );
}

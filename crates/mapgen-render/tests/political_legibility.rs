//! Observable-layer claims for "The Sundered Lanes" (see `docs/CLAIMS.md`): the
//! political wash is *legible* — you can actually tell two bordering realms
//! apart, and in particular an overseas exclave is distinct from the realm it
//! landed next to. The render fills each land cell with `nation.color`
//! (`planet.rs:310`, `ornate_antique.rs:246`), so "renders distinctly" is
//! exactly "bordering polities hold different `nation.color`".
//!
//! These were RED before the post-history `recolor_political` step: `polity_color`
//! wrapped a 5-colour palette mod 5, so on a ~20-polity planet many neighbours —
//! exclaves included — collided.
//!
//! The adjacency the test checks is rebuilt here independently from the final
//! control map, NOT shared with the recolour code, so the test cannot pass by
//! reading the colourer's own bookkeeping.

use mapgen_render::{render, style::Style};
use mapgen_testsupport::{
    an_earned_overseas_seizure, planet_params, polities_spanning_multiple_landmasses,
    CROSSING_SEEDS, SUNDERED_SEEDS,
};
use mapgen_world::generate_full;
use std::collections::BTreeSet;

/// Land-holding realms (≥1 land cell), ascending — mirrors `render_nation_legend`.
fn land_holding_realms(world: &mapgen_core::WorldData) -> BTreeSet<u32> {
    let mut realms = BTreeSet::new();
    for (i, c) in world.society.control.iter().enumerate() {
        if world.terrain.elevation.get(i).copied().unwrap_or(0.0) > 0.0 {
            if let Some(p) = *c {
                realms.insert(p);
            }
        }
    }
    realms
}

/// Polity ids carried by `<g class="realm exclave" data-polity="N">` groups in a slice.
fn exclave_polities(slice: &str) -> BTreeSet<u32> {
    let marker = r#"class="realm exclave" data-polity=""#;
    let mut s = BTreeSet::new();
    let mut rest = slice;
    while let Some(i) = rest.find(marker) {
        let after = &rest[i + marker.len()..];
        let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(p) = id.parse::<u32>() {
            s.insert(p);
        }
        rest = &rest[i + marker.len()..];
    }
    s
}

/// Unordered pairs of polities that share a border on the final control map.
fn bordering_polities(world: &mapgen_core::WorldData) -> BTreeSet<(u32, u32)> {
    let control = &world.society.control;
    let mut pairs = BTreeSet::new();
    for cell in 0..world.mesh.cell_count() {
        let Some(a) = control.get(cell).copied().flatten() else {
            continue;
        };
        let Some(nbrs) = world.mesh.neighbors.get(cell) else {
            continue;
        };
        for &nb in nbrs {
            if let Some(b) = control.get(nb as usize).copied().flatten() {
                if a != b {
                    pairs.insert((a.min(b), a.max(b)));
                }
            }
        }
    }
    pairs
}

#[test]
fn bordering_realms_render_in_distinct_colours() {
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let nations = &world.society.nations;
        for (a, b) in bordering_polities(&world) {
            assert_ne!(
                nations[a as usize].color, nations[b as usize].color,
                "seed {seed}: bordering realms {a} ({}) and {b} ({}) share fill colour {:?} — \
                 the political wash is ambiguous along that border",
                nations[a as usize].name, nations[b as usize].name, nations[a as usize].color,
            );
        }
    }
}

#[test]
fn the_overseas_exclave_is_colour_distinct_from_the_realms_it_borders() {
    // The headline payoff, exercised at the exclave specifically (so this can't
    // pass vacuously if no exclave happens to border a foreign realm). The
    // exclave cell must hold a colour different from every foreign realm it
    // touches, or the sundering is invisible.
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let Some((p, cell, _year)) = an_earned_overseas_seizure(&world) else {
            panic!("crossing seed {seed}: no earned overseas seizure to check for legibility");
        };
        let nations = &world.society.nations;
        let mut checked_a_foreign_border = false;
        for &nb in world
            .mesh
            .neighbors
            .get(cell as usize)
            .into_iter()
            .flatten()
        {
            if let Some(q) = world.society.control.get(nb as usize).copied().flatten() {
                if q != p {
                    checked_a_foreign_border = true;
                    assert_ne!(
                        nations[p as usize].color, nations[q as usize].color,
                        "seed {seed}: overseas exclave cell {cell} of realm {p} ({}) shares its \
                         colour with the realm {q} ({}) it borders — you cannot see the sundering",
                        nations[p as usize].name, nations[q as usize].name,
                    );
                }
            }
        }
        assert!(
            checked_a_foreign_border,
            "seed {seed}: the earned exclave cell {cell} borders no foreign realm — the \
             legibility check was vacuous (pick a different earned cell)"
        );
    }
}

#[test]
fn every_territorial_change_flips_the_rendered_colour() {
    // The time-slider animates by re-rendering past control with the SAME
    // `nation.color`. A cell that passes from realm X to realm Y must therefore
    // change color as you scrub across that year — but X and Y need not still
    // border each other on the present map (X may have retreated), so present
    // adjacency alone can leave them the same color, freezing the slider for that
    // conquest. (This is the regression the seed-4 @ 2000 e2e caught: the whole
    // founding-vs-present wash came out identical.) The configs below include
    // that exact e2e world.
    for (seed, cells) in [(4u64, 2000usize), (11, 18_000), (19, 18_000), (7, 18_000)] {
        let mut p = planet_params(seed);
        p.cell_count = cells;
        let world = generate_full(p);
        let nations = &world.society.nations;
        for ch in &world.history.border_changes {
            if let (Some(x), Some(y)) = (ch.from, ch.to) {
                if x != y {
                    assert_ne!(
                        nations[x as usize].color, nations[y as usize].color,
                        "seed {seed}/{cells}c: cell {} passes from realm {x} to realm {y} in year \
                         {}, but they share a color — the slider won't show that conquest",
                        ch.cell, ch.year,
                    );
                }
            }
        }
    }
}

/// The `<g class="{class}">…</g>` slice, balancing nested `<g>`/`</g>` so a
/// sub-group's `</g>` (e.g. an exclave realm group inside the wash) doesn't end
/// the scan early. Empty string if the group isn't present.
fn group_slice<'a>(svg: &'a str, class: &str) -> &'a str {
    let open = format!(r#"<g class="{class}">"#);
    let Some(start) = svg.find(&open) else {
        return "";
    };
    let b = svg.as_bytes();
    let mut i = start;
    let mut depth = 0i32;
    let mut end = svg.len();
    while i < b.len() {
        if b[i..].starts_with(b"</g>") {
            depth -= 1;
            i += 4;
            if depth == 0 {
                end = i;
                break;
            }
        } else if b[i..].starts_with(b"<g") {
            depth += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    &svg[start..end]
}

/// Distinct `fill="#rrggbb"` values inside a `<g class="{class}">` group.
fn distinct_fills(svg: &str, group_class: &str) -> BTreeSet<String> {
    let group = group_slice(svg, group_class);
    let needle = "fill=\"#";
    let mut fills = BTreeSet::new();
    let mut rest = group;
    while let Some(i) = rest.find(needle) {
        let after = &rest[i + needle.len()..];
        let hex: String = after.chars().take(6).collect();
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            fills.insert(hex.to_ascii_lowercase());
        }
        rest = &rest[i + needle.len()..];
    }
    fills
}

#[test]
fn the_political_wash_renders_exactly_the_realms_legible_colours() {
    // Purely observable: parse the rendered SVG and confirm it surfaces the
    // legible coloring *verbatim* — the set of distinct fills in the wash equals
    // the set of distinct `nation.color`s among realms that hold land. This is
    // the bridge from the data-level legibility (the tests above, on
    // `nation.color`) to what actually paints: a render that dropped a realm,
    // collapsed colors, or invented its own scheme would fail here.
    //
    // (A well graph-colored, planar-ish map uses FEW colors — fewer than the old
    // mod-5 spread — so "many fills" is the wrong signal; adjacency-distinctness,
    // asserted above, is the right one. This test only checks faithfulness.)
    let world = generate_full(planet_params(CROSSING_SEEDS[0]));
    let svg = render(&world, Style::Planet).expect("planet render");
    let rendered = distinct_fills(&svg, "planet-political");

    let elev = &world.terrain.elevation;
    let mut expected: BTreeSet<String> = BTreeSet::new();
    for cell in 0..world.mesh.cell_count() {
        if elev.get(cell).copied().unwrap_or(0.0) <= 0.0 {
            continue; // wash draws land only
        }
        if let Some(pid) = world.society.control.get(cell).copied().flatten() {
            if let Some(n) = world.society.nations.get(pid as usize) {
                let [r, g, b] = n.color;
                expected.insert(format!("{r:02x}{g:02x}{b:02x}"));
            }
        }
    }
    assert_eq!(
        rendered, expected,
        "the wash's fills do not match the land-holding realms' colors — the render is not \
         a faithful surface of the political coloring"
    );
}

#[test]
fn the_wash_groups_each_realm_and_tags_overseas_exclaves() {
    // The DOM-surfacing claim (lets an e2e point at THE exclave): every realm
    // holding land is a `data-polity` group, and a polity holding land on a
    // second sizable landmass gets a `class="realm exclave" data-polity` group.
    // Exclave-tagged polities must equal exactly the polities that span ≥2
    // sizable landmasses, and the regrouping must drop/duplicate no cell.
    for &seed in CROSSING_SEEDS {
        let world = generate_full(planet_params(seed));
        let svg = render(&world, Style::Planet).expect("planet render");
        let wash = group_slice(&svg, "planet-political");

        for pid in land_holding_realms(&world) {
            assert!(
                wash.contains(&format!(r#"data-polity="{pid}""#)),
                "seed {seed}: realm {pid} holds land but has no data-polity group"
            );
        }

        let spanning: BTreeSet<u32> = polities_spanning_multiple_landmasses(&world)
            .into_iter()
            .collect();
        assert_eq!(
            exclave_polities(wash),
            spanning,
            "seed {seed}: exclave-tagged polities must be exactly those spanning ≥2 sizable \
             landmasses"
        );

        // Faithfulness: the regrouped wash draws exactly the cells the render
        // emits — non-empty verts, land, controlled by a polity that EXISTS
        // (the render skips a dangling control id, so we must too, or the count
        // diverges spuriously).
        let expected_polys = world
            .mesh
            .cell_vertices
            .iter()
            .enumerate()
            .filter(|(i, v)| {
                !v.is_empty()
                    && world.terrain.elevation.get(*i).copied().unwrap_or(0.0) > 0.0
                    && world
                        .society
                        .control
                        .get(*i)
                        .copied()
                        .flatten()
                        .is_some_and(|pid| world.society.nations.get(pid as usize).is_some())
            })
            .count();
        assert_eq!(
            wash.matches("<polygon").count(),
            expected_polys,
            "seed {seed}: wash polygon count != controlled land cells (regrouping lost cells)"
        );
    }

    for &seed in SUNDERED_SEEDS {
        let world = generate_full(planet_params(seed));
        let svg = render(&world, Style::Planet).expect("planet render");
        let wash = group_slice(&svg, "planet-political");
        // Pin that the wash actually rendered the confined society FIRST — else
        // "no exclave" passes vacuously on a dropped/empty wash (a false-green:
        // a sundered-specific path that drops the wash would slip through).
        let realms = land_holding_realms(&world);
        assert!(
            !realms.is_empty(),
            "sundered seed {seed}: expected land-holding realms to wash in"
        );
        for pid in &realms {
            assert!(
                wash.contains(&format!(r#"data-polity="{pid}""#)),
                "sundered seed {seed}: realm {pid} holds land but has no data-polity group"
            );
        }
        // ...and none of them is an exclave (society is landmass-confined).
        assert!(
            !wash.contains(r#"class="realm exclave""#),
            "sundered seed {seed}: society is landmass-confined — no exclave should render"
        );
    }
}

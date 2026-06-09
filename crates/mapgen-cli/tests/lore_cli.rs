//! CLI end-to-end: the multi-strand FAR-SHORE chronicle through the `mapgen`
//! binary. `select_shore` / `narrate_shore` are pinned at library level by
//! `mapgen-lore/tests/shore.rs`; this exercises the thin CLI glue around them —
//! the part that library tests can't reach:
//!
//!   * `mapgen generate --planet` must persist a LANED, multi-continent world
//!     (a default continental generate is laneless, so `auto-shore` would have
//!     nothing to chronicle — this was the fixture friction that kept the test
//!     off the suite).
//!   * `mapgen lore --event auto-shore` must read that gzipped world back, route
//!     to `narrate_shore` (not the focal-event path), weave the chronicle, and
//!     print every strand NAMING the shore — so the `far_shore` tags survive the
//!     full generate → gzip → read → weave → print round-trip.
//!
//! Uses seed 9 (`GenerateParams::planet`), the only canonical THREE-strand shore
//! (faith + colony + sword) — the same fixture `shore.rs` uses, but driven
//! entirely through the CLI. A planet world is ~18k cells, so this is heavier
//! than `roundtrip.rs` (4k); it is the cost of a laned fixture. Output goes under
//! `env::temp_dir()` with a unique suffix per process so concurrent runs do not
//! collide.

use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_dir() -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("mapgen-lore-cli-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// A real planet seed whose far shore is reached by TWO strands (faith + sword).
/// Under the periodic planet no seed reaches a single shore by all three strands
/// (see `mapgen-lore/tests/shore.rs` — the 3-strand weave is now pinned
/// synthetically there). This test's job is the CLI glue + gzip round-trip on a
/// REAL generated world, so it uses the production 2-strand reality rather than a
/// synthetic world (the binary generates from `--seed`, so a hand-built world
/// can't be injected here).
const FAR_SHORE_SEED: &str = "9";

#[test]
fn lore_auto_shore_weaves_the_far_shore_chronicle_through_the_cli() {
    let bin = env!("CARGO_BIN_EXE_mapgen");
    let dir = unique_dir();
    let world_path = dir.join("planet9.json.gz");

    // Persist a LANED planet world through the CLI. `--planet` selects the
    // multi-continent preset (the only scale that grows sea lanes + far_shore
    // tags); a default continental generate would be laneless.
    let gen_out = Command::new(bin)
        .args(["generate", "--planet", "--seed", FAR_SHORE_SEED, "--out"])
        .arg(&world_path)
        .output()
        .expect("spawn mapgen generate --planet");
    assert!(
        gen_out.status.success(),
        "mapgen generate --planet failed: status={:?} stderr={}",
        gen_out.status,
        String::from_utf8_lossy(&gen_out.stderr)
    );
    let meta = fs::metadata(&world_path).expect("planet world file written");
    assert!(
        meta.len() > 1024,
        "planet world file suspiciously small: {} bytes",
        meta.len()
    );

    // Weave the multi-strand far-shore chronicle through the CLI (default voice =
    // monastic-chronicle, so the title is "The Annal of the Reaching of <shore>").
    let lore_out = Command::new(bin)
        .args(["lore", "--event", "auto-shore", "--in"])
        .arg(&world_path)
        .output()
        .expect("spawn mapgen lore --event auto-shore");
    assert!(
        lore_out.status.success(),
        "mapgen lore --event auto-shore failed: status={:?} stderr={}",
        lore_out.status,
        String::from_utf8_lossy(&lore_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&lore_out.stdout);

    // The title NAMES the shore — and the name reaches the page ONLY because the
    // weaver read the `far_shore` tags (no event summary carries it). Capture it
    // from the title rather than hard-coding the generated name, then assert it
    // recurs in the body beats.
    const TITLE: &str = "# The Annal of the Reaching of ";
    let title_line = stdout
        .lines()
        .find(|l| l.starts_with(TITLE))
        .unwrap_or_else(|| panic!("no far-shore title in lore output:\n{stdout}"));
    let shore = title_line[TITLE.len()..].trim();
    assert!(
        !shore.is_empty(),
        "the chronicle must name a shore; title was {title_line:?}"
    );

    // seed 9 reaches its shore by faith AND the sword, each beat NAMING the shore —
    // and the name reaches the page ONLY because the weaver read the `far_shore`
    // tags (no event summary carries it), proven here to survive the generate →
    // gzip → read-back → weave CLI round-trip. (The 3-strand weave with the colony
    // beat is pinned at library level in `shore.rs`; under the periodic planet no
    // real seed grows all three strands on one shore.)
    for beat in [
        format!("faith first took root upon the shore of {shore}"),
        format!("sword first won a foothold upon the shore of {shore}"),
    ] {
        assert!(
            stdout.contains(&beat),
            "missing strand beat {beat:?} in lore output:\n{stdout}"
        );
    }

    // Tidy up — best-effort, not asserted.
    let _ = fs::remove_dir_all(&dir);
}

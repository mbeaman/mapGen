//! CLI end-to-end roundtrip. Drives the `mapgen` binary the same way a
//! user would: `mapgen generate ...` writes a gzipped world; `mapgen
//! render ...` reads that world and writes an SVG. This catches:
//!
//!   * Subcommand argument-parsing regressions (clap definitions out of
//!     sync with what the test passes).
//!   * Serialization roundtrip breakage (`WorldData` writes that fail
//!     to deserialize, missing `#[serde(default)]`, gz envelope bugs).
//!   * Wiring regressions between `mapgen-world::generate_full`,
//!     `mapgen-render::render`, and the file-IO glue.
//!
//! Uses 4000 cells (same as the realism specs) for speed — the default
//! 15000 cells generate fine in ~10s in release but bloat the integration
//! test budget. Output goes under `env::temp_dir()` with a unique suffix
//! per process so concurrent test runs do not collide.

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
    let dir = std::env::temp_dir().join(format!("mapgen-roundtrip-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn generate_then_render_produces_valid_svg() {
    let bin = env!("CARGO_BIN_EXE_mapgen");
    let dir = unique_dir();
    let world_path = dir.join("tmp.json.gz");
    let svg_path = dir.join("tmp.svg");

    let gen_out = Command::new(bin)
        .args(["generate", "--seed", "42", "--cells", "4000", "--out"])
        .arg(&world_path)
        .output()
        .expect("spawn mapgen generate");
    assert!(
        gen_out.status.success(),
        "mapgen generate failed: status={:?} stderr={}",
        gen_out.status,
        String::from_utf8_lossy(&gen_out.stderr)
    );
    let world_meta = fs::metadata(&world_path).expect("world file written");
    assert!(
        world_meta.len() > 1024,
        "world file suspiciously small: {} bytes",
        world_meta.len()
    );

    let render_out = Command::new(bin)
        .args(["render", "--in"])
        .arg(&world_path)
        .args(["--style", "biomes", "--out"])
        .arg(&svg_path)
        .output()
        .expect("spawn mapgen render");
    assert!(
        render_out.status.success(),
        "mapgen render failed: status={:?} stderr={}",
        render_out.status,
        String::from_utf8_lossy(&render_out.stderr)
    );

    let svg = fs::read_to_string(&svg_path).expect("svg file written");
    assert!(
        svg.len() >= 100 * 1024,
        "SVG below 100KB floor: {} bytes (expected ≥102400)",
        svg.len()
    );
    assert!(
        svg.starts_with("<?xml") || svg.starts_with("<svg"),
        "SVG does not start with <?xml or <svg: first 40 bytes = {:?}",
        &svg[..svg.len().min(40)]
    );

    // Tidy up — best-effort, not asserted.
    let _ = fs::remove_dir_all(&dir);
}

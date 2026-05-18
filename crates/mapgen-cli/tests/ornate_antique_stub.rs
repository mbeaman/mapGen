//! `mapgen render --style ornate_antique` must exit non-zero with an
//! informative message until the Phase 3e implementation lands. The
//! Style enum still accepts `ornate_antique` so we don't have to
//! re-litigate naming when 3e arrives, but accepting the name and
//! silently producing a placeholder SVG would mislead anyone trying
//! the documented variant.
//!
//! Pair test: `mapgen-render/tests/svg_invariants.rs::
//! ornate_antique_is_an_explicit_err_until_phase_3e` pins the library
//! contract; this test pins the CLI-level surface (error propagation,
//! non-zero exit, stderr carries the explanation).

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
    let dir =
        std::env::temp_dir().join(format!("mapgen-ornate-stub-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn render_ornate_antique_exits_nonzero_with_explanation() {
    let bin = env!("CARGO_BIN_EXE_mapgen");
    let dir = unique_dir();
    let world_path = dir.join("tmp.json.gz");
    let svg_path = dir.join("tmp.svg");

    let gen_out = Command::new(bin)
        .args(["generate", "--seed", "1", "--cells", "1500", "--out"])
        .arg(&world_path)
        .output()
        .expect("spawn mapgen generate");
    assert!(
        gen_out.status.success(),
        "prerequisite `mapgen generate` failed: status={:?} stderr={}",
        gen_out.status,
        String::from_utf8_lossy(&gen_out.stderr)
    );

    let render_out = Command::new(bin)
        .args(["render", "--in"])
        .arg(&world_path)
        .args(["--style", "ornate_antique", "--out"])
        .arg(&svg_path)
        .output()
        .expect("spawn mapgen render");

    assert!(
        !render_out.status.success(),
        "mapgen render --style ornate_antique unexpectedly succeeded — \
         the stub should refuse, not silently emit a placeholder"
    );

    let stderr = String::from_utf8_lossy(&render_out.stderr);
    assert!(
        stderr.to_lowercase().contains("ornate_antique"),
        "stderr should name the unimplemented style: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("not yet implemented")
            || stderr.to_lowercase().contains("phase 3e"),
        "stderr should explain *why* (not-implemented / Phase 3e): {stderr}"
    );

    // No partial SVG was written.
    assert!(
        !svg_path.exists(),
        "SVG file was written despite render error: {}",
        svg_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

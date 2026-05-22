//! CLI surface for the Phase 3e ornate render — `mapgen render --style
//! ornate_antique` must produce a valid SVG end-to-end, not the
//! placeholder it returned before commit landing the impl.
//!
//! Filename is a holdover from the pre-3e stub guard (this test
//! formerly asserted non-zero exit on an unimplemented style). Kept as
//! the CLI-level companion to the library-level pin
//! `mapgen-render/tests/svg_invariants.rs::
//! ornate_antique_emits_a_phase_3e_render`.

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
    let dir = std::env::temp_dir().join(format!("mapgen-ornate-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn render_ornate_antique_writes_a_valid_svg_end_to_end() {
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
        render_out.status.success(),
        "mapgen render --style ornate_antique failed: status={:?} stderr={}",
        render_out.status,
        String::from_utf8_lossy(&render_out.stderr)
    );

    let svg = fs::read_to_string(&svg_path).expect("svg file written");
    assert!(
        svg.len() >= 30_000,
        "ornate SVG suspiciously small ({} bytes) on a 1 500-cell world — \
         a regression to the placeholder or to an early-exit path?",
        svg.len()
    );
    assert!(
        svg.starts_with("<?xml") || svg.starts_with("<svg"),
        "ornate SVG does not start with <?xml or <svg: first 40 bytes = {:?}",
        &svg[..svg.len().min(40)]
    );
    assert!(
        svg.contains("parchment"),
        "ornate SVG missing the parchment gradient marker — likely \
         a regression to a non-ornate style"
    );

    let _ = fs::remove_dir_all(&dir);
}

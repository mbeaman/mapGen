//! `mapgen sweep` end-to-end. Complements `roundtrip.rs` (which covers
//! the `generate` → `render` pipeline) by driving the sweep subcommand
//! through the binary and asserting the artifacts it should leave
//! behind: two PNGs (real PNG magic bytes), two SVGs (correct
//! envelope), one `index.html` that references both PNGs.
//!
//! Smallest meaningful invocation: 2 steps, 1500 cells, range crossing
//! the default. Keeps the test under a few seconds in release.

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
    let dir = std::env::temp_dir().join(format!(
        "mapgen-sweep-roundtrip-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn sweep_emits_pngs_svgs_and_index_html() {
    let bin = env!("CARGO_BIN_EXE_mapgen");
    let dir = unique_dir();

    let out = Command::new(bin)
        .args([
            "sweep",
            "--seed",
            "42",
            "--knob",
            "erosion_rate",
            "--range",
            "0.02..0.06",
            "--steps",
            "2",
            "--cells",
            "1500",
            "--style",
            "biomes",
            "--out",
        ])
        .arg(&dir)
        .output()
        .expect("spawn mapgen sweep");
    assert!(
        out.status.success(),
        "mapgen sweep failed: status={:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let mut entries: Vec<String> = fs::read_dir(&dir)
        .expect("read temp dir")
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    entries.sort();

    let pngs: Vec<&String> = entries.iter().filter(|n| n.ends_with(".png")).collect();
    let svgs: Vec<&String> = entries.iter().filter(|n| n.ends_with(".svg")).collect();
    assert_eq!(
        pngs.len(),
        2,
        "expected 2 PNG steps, got {pngs:?} in {entries:?}"
    );
    assert_eq!(
        svgs.len(),
        2,
        "expected 2 SVG steps, got {svgs:?} in {entries:?}"
    );
    assert!(
        entries.iter().any(|n| n == "index.html"),
        "no index.html in {entries:?}"
    );

    // Each PNG must start with the 8-byte PNG signature. Catches PNG
    // pipeline breakage (resvg/usvg/tiny-skia API drift, swapped writer,
    // empty output) without needing an image decoder.
    for png in &pngs {
        let bytes = fs::read(dir.join(png)).expect("read png");
        assert!(
            bytes.len() > 1024,
            "PNG {png} suspiciously small: {} bytes",
            bytes.len()
        );
        assert!(
            bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
            "PNG {png} missing magic bytes; got {:02x?}",
            &bytes[..bytes.len().min(8)]
        );
    }

    // SVGs share the same envelope guarantees as `svg_invariants.rs` —
    // light check here just confirms the sweep writes them at all.
    for svg in &svgs {
        let content = fs::read_to_string(dir.join(svg)).expect("read svg");
        assert!(content.starts_with("<svg"), "SVG {svg} missing envelope");
        assert!(
            content.trim_end().ends_with("</svg>"),
            "SVG {svg} truncated"
        );
    }

    // The index must reference each PNG by filename — a broken HTML
    // template wouldn't.
    let index = fs::read_to_string(dir.join("index.html")).expect("read index");
    for png in &pngs {
        assert!(
            index.contains(png.as_str()),
            "index.html does not reference {png}"
        );
    }
    assert!(
        index.contains("erosion_rate"),
        "index.html does not mention the swept knob"
    );

    let _ = fs::remove_dir_all(&dir);
}

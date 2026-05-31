//! Determinism guard: no crate in the workspace may call raw `f32`/`f64`
//! transcendentals — they break the native ↔ wasm32 byte-identical contract
//! that the multi-scale atlas depends on (a sector regenerated in the browser
//! must match the CLI bit-for-bit). Anything transcendental must route through
//! `mapgen_core::fmath` (libm-backed, so identical on every target).
//!
//! This guard earns its keep: 6.2 wired the first real cross-platform golden
//! (`crates/mapgen-wasm/tests/cross_platform.rs`) and immediately caught two
//! escapes this scan now forbids — a `.exp()` in `climate::band_precip` and a
//! `.powf()` in `cultures` — that diverged because *native* std libm and the
//! wasm libm round transcendentals differently in the last ULP. The scan lives
//! in mapgen-history for historical reasons but walks every crate's `src/`.
//!
//! `sqrt` is deliberately NOT forbidden: IEEE-754 *requires* `sqrt` to be
//! correctly rounded, so x86 SSE2 `sqrtss` and the wasm `f32.sqrt` instruction
//! produce bit-identical results (unlike sin/cos/exp/ln/pow, which have no such
//! mandate). `.powi(` is excluded too — integer power is exact multiplication.

use std::fs;
use std::path::Path;

/// Crates whose `src/` is *not* part of the WorldData-generation determinism
/// contract, so raw transcendentals there don't threaten the native↔wasm32
/// byte-identical guarantee. Keep this list tight — every entry is a
/// promise that nothing in that crate's `src/` feeds back into the
/// generation pipeline.
///
/// `mapgen-viewer` is purely render-side: camera projection, vertex layout,
/// shader uniforms. It consumes a `WorldData` produced elsewhere and never
/// writes back to it. Forcing its trig (FOV → distance, yaw/pitch →
/// camera basis) through `fmath` would bloat a determinism-critical
/// module with helpers only the viewer needs.
const EXCLUDED_CRATES: &[&str] = &["mapgen-viewer"];

/// Method-call (`x.exp()`) and associated (`f32::exp`) forms of the genuinely
/// target-divergent transcendentals `fmath` owns. See module docs for why
/// `sqrt` and `powi` are absent.
const FORBIDDEN: &[&str] = &[
    ".sin(",
    ".cos(",
    ".tan(",
    ".asin(",
    ".acos(",
    ".atan(",
    ".atan2(",
    ".exp(",
    ".exp2(",
    ".ln(",
    ".log(",
    ".log2(",
    ".log10(",
    ".powf(",
    ".cbrt(",
    ".hypot(",
    "f32::sin",
    "f32::cos",
    "f32::tan",
    "f32::exp",
    "f32::ln",
    "f32::powf",
    "f32::hypot",
    "f64::sin",
    "f64::cos",
    "f64::tan",
    "f64::exp",
    "f64::ln",
    "f64::powf",
];

fn scan(dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            scan(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            // `fmath.rs` is the one place libm is called; its bodies route
            // through `libm::sinf` etc. (which don't match these patterns),
            // but skip it explicitly so the intent is unmistakable.
            if path.file_name().map(|f| f == "fmath.rs").unwrap_or(false) {
                continue;
            }
            let src = fs::read_to_string(&path).expect("read source");
            for (i, line) in src.lines().enumerate() {
                // Ignore line comments to avoid flagging prose.
                let code = line.split("//").next().unwrap_or("");
                for pat in FORBIDDEN {
                    if code.contains(pat) {
                        out.push(format!("{}:{} — `{pat}`", path.display(), i + 1));
                    }
                }
            }
        }
    }
}

#[test]
fn workspace_uses_no_raw_transcendentals() {
    // CARGO_MANIFEST_DIR is .../crates/mapgen-history; its parent is the
    // `crates/` root, so we scan every crate's `src/` (test/example code is
    // excluded — only the determinism-critical library paths matter).
    let crates_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ root");
    let mut violations = Vec::new();
    let mut scanned = 0usize;
    for entry in fs::read_dir(crates_root).expect("read crates dir") {
        let crate_dir = entry.expect("dir entry").path();
        let crate_name = crate_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if EXCLUDED_CRATES.contains(&crate_name) {
            continue;
        }
        let src = crate_dir.join("src");
        if src.is_dir() {
            scanned += 1;
            scan(&src, &mut violations);
        }
    }
    assert!(
        scanned >= 6,
        "expected to scan the workspace crates, saw {scanned}"
    );
    assert!(
        violations.is_empty(),
        "raw f32/f64 transcendentals found (route through mapgen_core::fmath to \
         keep native↔wasm32 byte-identical — see crates/mapgen-wasm/tests/\
         cross_platform.rs):\n{}",
        violations.join("\n")
    );
}

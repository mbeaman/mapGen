//! Determinism guard (review finding #2): the history sim must not call raw
//! `f32`/`f64` transcendentals — they break the native ↔ wasm32 byte-identical
//! contract. Anything transcendental must route through `mapgen_core::fmath`
//! (which `fmath::sin` etc. do via `libm`). 4a–4h get by on pure arithmetic;
//! this test fails the moment a future loop reaches for `x.exp()` directly.

use std::fs;
use std::path::Path;

/// Method-call (`x.exp()`) and associated (`f32::exp`) forms of the
/// transcendental / irrational functions `fmath` owns. `.powi(` is deliberately
/// excluded — integer power is exact multiplication, not a libm call.
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
    ".sqrt(",
    ".cbrt(",
    ".hypot(",
    "f32::sin",
    "f32::cos",
    "f32::tan",
    "f32::exp",
    "f32::ln",
    "f32::powf",
    "f32::sqrt",
    "f32::hypot",
    "f64::sin",
    "f64::cos",
    "f64::tan",
    "f64::exp",
    "f64::ln",
    "f64::powf",
    "f64::sqrt",
];

fn scan(dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            scan(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
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
fn mapgen_history_uses_no_raw_transcendentals() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    scan(&src, &mut violations);
    assert!(
        violations.is_empty(),
        "raw f32/f64 transcendentals in mapgen-history (route through \
         mapgen_core::fmath to keep native↔wasm32 byte-identical):\n{}",
        violations.join("\n")
    );
}

//! Phase A — off-main-thread RGBA rasterize. Proves `WorldHandle::render_rgba`
//! compiles to **wasm32** (with resvg/usvg/tiny-skia, fontless) and produces a
//! sane, fully-opaque RGBA buffer whose lens variant DIFFERS from the baseline —
//! the worker-side path the 3D globe streams instead of shipping SVG strings to
//! the main thread for a ~300 ms `drawImage`. The native twin
//! (`mapgen-cli/tests/visual_regression.rs`
//! `globe_texture_rasterizes_and_its_lens_class_is_honored`) pins the same
//! lens-honored fact off-GPU; this pins that the wasm method itself works.
//!
//! Run: `wasm-pack test --node crates/mapgen-wasm`

use mapgen_wasm::generate;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn render_rgba_is_sane_opaque_and_the_lens_differs() {
    // A continent world (full pipeline → has religions, so the faith lens has a
    // wash to swap in). Modest cells + a small raster keep the node wasm run quick.
    let world = generate(42, 6_000, 8);
    let (w, h) = (512u32, 256u32);

    let Ok(base) = world.render_rgba("globe", "", w, h) else {
        panic!("render_rgba baseline failed");
    };
    assert_eq!(
        base.len(),
        (w * h * 4) as usize,
        "RGBA buffer must be w*h*4 bytes"
    );

    // Fully opaque (opaque deep-sea backdrop) + non-uniform + has sea AND land.
    let px = (w * h) as f64;
    let (mut opaque, mut water, mut warm, mut sum, mut sumsq) = (0u64, 0u64, 0u64, 0.0f64, 0.0f64);
    for c in base.chunks_exact(4) {
        if c[3] == 255 {
            opaque += 1;
        }
        let (r, g, b) = (c[0] as f64, c[1] as f64, c[2] as f64);
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        sum += lum;
        sumsq += lum * lum;
        if b > r + 8.0 {
            water += 1;
        }
        if r > b + 8.0 {
            warm += 1;
        }
    }
    let mean = sum / px;
    let variance = (sumsq / px - mean * mean).max(0.0);
    assert!(
        opaque as f64 / px > 0.99,
        "render_rgba not opaque ({opaque} / {px}) — the deep-sea backdrop should cover every texel"
    );
    assert!(
        variance > 100.0,
        "render_rgba ~uniform (variance {variance})"
    );
    assert!(
        water as f64 / px > 0.03,
        "render_rgba has no sea ({water} px)"
    );
    assert!(
        warm as f64 / px > 0.01,
        "render_rgba has no land ({warm} px)"
    );

    // The lens is honored INSIDE the worker path: same world + size, `on-faith`
    // vs baseline must DIFFER (political wash → faith wash). False-green guard: do
    // NOT assert merely len-equal / both-opaque — those pass when the lens is
    // ignored. This is the wasm twin of the native lens-honored test.
    let Ok(faith) = world.render_rgba("globe", "on-faith", w, h) else {
        panic!("render_rgba faith failed");
    };
    assert_eq!(faith.len(), base.len());
    let differing = base
        .iter()
        .zip(faith.iter())
        .filter(|(a, b)| a != b)
        .count();
    let diff_frac = differing as f64 / base.len() as f64;
    assert!(
        diff_frac > 0.01,
        "render_rgba lens variant barely differs from baseline ({diff_frac}) — the lens \
         class selector was not honored on the worker path"
    );
}

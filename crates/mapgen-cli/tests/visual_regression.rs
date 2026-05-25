//! Raster sanity for the marquee ornate render. The structure invariants in
//! `mapgen-render/tests/svg_invariants.rs` check the SVG *text*; nothing else
//! verifies the SVG → pixels step. This rasterizes the ornate render (the same
//! resvg/usvg/tiny-skia path the CLI's PNG export uses, with the same embedded
//! fonts) and asserts the image is *sane*: opaque, non-uniform, with parchment
//! background, ink linework, and water present.
//!
//! Deliberately a machine-robust bucket/variance check with generous
//! thresholds — NOT an absolute pixel baseline. A strict perceptual baseline
//! would risk flaking across machines' antialiasing/font-hinting (the same
//! reason the native↔wasm golden is deferred to `wasm-bindgen-test`); it can be
//! added once CI rasterization determinism is confirmed. What this catches:
//! an SVG resvg can't parse, font-loading failures, a blank/uniform canvas,
//! content rendered off-canvas, or a missing major colour layer — none of which
//! the SVG-text invariants can see.

use mapgen_render::{render, style::Style, FONTS_TTF};
use mapgen_world::{generate_full, GenerateParams};

fn rasterize(svg: &str) -> tiny_skia::Pixmap {
    let mut opts = usvg::Options::default();
    for (_, bytes) in FONTS_TTF {
        opts.fontdb_mut().load_font_data(bytes.to_vec());
    }
    let tree = usvg::Tree::from_str(svg, &opts).expect("usvg should parse the ornate SVG");
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).expect("tiny-skia pixmap");
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap
}

#[test]
fn ornate_render_rasterizes_to_a_sane_image() {
    let world = generate_full(GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 4_000,
        plate_count: 12,
        nation_count: 6,
    });
    let svg = render(&world, Style::OrnateAntique).expect("ornate render");
    let pixmap = rasterize(&svg);

    let total = (pixmap.width() * pixmap.height()) as f64;
    assert!(total > 0.0, "empty pixmap");

    let (mut opaque, mut dark, mut warm, mut water) = (0u64, 0u64, 0u64, 0u64);
    let (mut sum, mut sumsq) = (0.0f64, 0.0f64);
    for p in pixmap.data().chunks_exact(4) {
        let (r, g, b, a) = (p[0] as f64, p[1] as f64, p[2] as f64, p[3]);
        if a > 200 {
            opaque += 1;
        }
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        sum += lum;
        sumsq += lum * lum;
        if lum < 70.0 {
            dark += 1;
        }
        if r > b + 8.0 {
            warm += 1; // parchment / sepia is warm-toned (red over blue)
        }
        if b > r + 10.0 && b > g + 10.0 {
            water += 1; // the ocean
        }
    }
    let mean = sum / total;
    let variance = (sumsq / total - mean * mean).max(0.0);
    let frac = |n: u64| n as f64 / total;
    eprintln!(
        "raster metrics: {}x{}  opaque={:.3} warm={:.3} dark={:.4} water={:.4} var={:.0} mean={:.0}",
        pixmap.width(),
        pixmap.height(),
        frac(opaque),
        frac(warm),
        frac(dark),
        frac(water),
        variance,
        mean
    );

    // Generous thresholds: catch catastrophic regressions, never flake on AA.
    assert!(
        frac(opaque) > 0.8,
        "image is not opaque ({:.3}) — render or rasterize failed",
        frac(opaque)
    );
    assert!(
        variance > 300.0,
        "image is ~uniform (variance {variance:.0}) — likely blank"
    );
    assert!(
        (80.0..=200.0).contains(&mean),
        "mean luminance {mean:.0} outside the parchment midtone band — wrong palette?"
    );
    assert!(
        frac(warm) > 0.4,
        "parchment (warm-toned) background appears missing ({:.3} warm pixels)",
        frac(warm)
    );
    assert!(
        frac(dark) > 0.005,
        "no ink / linework rendered ({:.4} dark pixels)",
        frac(dark)
    );
    assert!(
        frac(water) > 0.003,
        "no water rendered ({:.4} blue pixels)",
        frac(water)
    );
}

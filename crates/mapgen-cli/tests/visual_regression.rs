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

use mapgen_render::{
    layers::{bake_layer_state_pruned, PRESETS},
    render,
    style::Style,
    FONTS_TTF,
};
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
        periodic: false,
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

    // BASELINE BANDS (quality hunt #5): the original floors only caught
    // CATASTROPHIC failure — a renderer that lost 99% of its water (0.0213 →
    // 0.004) still passed `water > 0.003`. These bands pin each metric to its
    // RECORDED value ± a generous relative tolerance (±40–50%, far beyond any
    // cross-machine AA/font-hinting wobble on million-pixel aggregates, well
    // inside a real layer loss). Recorded on seed 42 @ 4000 cells:
    // opaque=1.000 warm=0.698 dark=0.0597 water=0.0213 var=1424 mean=148.
    // A legitimate art-direction change re-records the constants.
    assert!(
        frac(opaque) > 0.95,
        "image is not opaque ({:.3}) — render or rasterize failed",
        frac(opaque)
    );
    assert!(
        (700.0..=2900.0).contains(&variance),
        "variance {variance:.0} outside the recorded band (~1424 ±~2×) — blank or noise-bombed"
    );
    assert!(
        (110.0..=190.0).contains(&mean),
        "mean luminance {mean:.0} outside the recorded parchment band (~148 ±25%)"
    );
    assert!(
        (0.40..=0.95).contains(&frac(warm)),
        "warm-pixel fraction {:.3} outside the recorded band (~0.70 ±40%) — parchment lost or flooded",
        frac(warm)
    );
    assert!(
        (0.030..=0.120).contains(&frac(dark)),
        "ink fraction {:.4} outside the recorded band (~0.060 ±50%) — linework lost or smeared",
        frac(dark)
    );
    assert!(
        (0.010..=0.045).contains(&frac(water)),
        "water fraction {:.4} outside the recorded band (~0.021 ±50%+) — the ocean layer regressed",
        frac(water)
    );
}

/// The planet / planisphere overview must rasterize sane: opaque, non-uniform,
/// multi-coloured (continents + sea), on the parchment band. Guards the zoom-out
/// render against a blank/uniform/off-canvas regression the SVG invariants miss.
#[test]
fn planet_render_rasterizes_to_a_sane_image() {
    let world = generate_full(GenerateParams::planet(42));
    let pixmap = rasterize(&render(&world, Style::Planet).expect("planet render"));
    let total = (pixmap.width() * pixmap.height()) as f64;
    let (mut opaque, mut sum, mut sumsq, mut water) = (0u64, 0.0f64, 0.0f64, 0u64);
    let mut colours = std::collections::HashSet::new();
    for px in pixmap.data().chunks_exact(4) {
        if px[3] > 200 {
            opaque += 1;
        }
        let (r, g, b) = (px[0] as f64, px[1] as f64, px[2] as f64);
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        sum += lum;
        sumsq += lum * lum;
        if b > r + 8.0 {
            water += 1; // the sea reads blue-grey
        }
        if colours.len() < 1_000 {
            colours.insert([px[0], px[1], px[2]]);
        }
    }
    let mean = sum / total;
    let variance = (sumsq / total - mean * mean).max(0.0);
    assert!(opaque as f64 / total > 0.8, "planet not opaque");
    assert!(variance > 150.0, "planet ~uniform (variance {variance:.0})");
    assert!(
        (40.0..=225.0).contains(&mean),
        "planet mean luminance {mean:.0} off-band"
    );
    assert!(
        colours.len() > 20,
        "planet only {} distinct colours",
        colours.len()
    );
    assert!(
        water as f64 / total > 0.05,
        "planet has no sea ({} blue px)",
        water
    );
}

/// Every atlas preset must rasterize to a *sane* image — opaque, non-uniform,
/// multi-coloured, on the parchment midtone band. The palette-specific checks
/// above can't be reused (a thermal lens is blue→red, not warm parchment), so
/// these are deliberately palette-agnostic; what they catch is exactly what the
/// structure tests can't: a baked overlay that rasterizes blank, off-canvas, or
/// as a single flat fill — i.e. a broken tint, ramp, or prune.
#[test]
fn every_preset_rasterizes_to_a_sane_image() {
    let world = generate_full(GenerateParams {
        seed: 42,
        width: 1024.0,
        height: 640.0,
        cell_count: 3_000,
        plate_count: 12,
        nation_count: 8,
        periodic: false,
    });
    let base = render(&world, Style::OrnateAntique).expect("ornate render");

    for p in PRESETS {
        let pixmap = rasterize(&bake_layer_state_pruned(&base, p.enabled));
        let total = (pixmap.width() * pixmap.height()) as f64;
        let (mut opaque, mut sum, mut sumsq) = (0u64, 0.0f64, 0.0f64);
        let mut colours = std::collections::HashSet::new();
        for px in pixmap.data().chunks_exact(4) {
            if px[3] > 200 {
                opaque += 1;
            }
            let lum = 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
            sum += lum;
            sumsq += lum * lum;
            if colours.len() < 1_000 {
                colours.insert([px[0], px[1], px[2]]);
            }
        }
        let mean = sum / total;
        let variance = (sumsq / total - mean * mean).max(0.0);
        let opaque_frac = opaque as f64 / total;

        assert!(
            opaque_frac > 0.8,
            "preset {}: not opaque ({opaque_frac:.3})",
            p.name
        );
        assert!(
            variance > 150.0,
            "preset {}: ~uniform (variance {variance:.0}) — blank/broken tint",
            p.name
        );
        assert!(
            (40.0..=225.0).contains(&mean),
            "preset {}: mean luminance {mean:.0} off the midtone band",
            p.name
        );
        assert!(
            colours.len() > 20,
            "preset {}: only {} distinct colours — likely a flat fill",
            p.name,
            colours.len()
        );
    }
}

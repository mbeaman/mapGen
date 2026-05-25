//! Phase 7 — a refined sector renders to a valid, non-blank ornate map whose
//! viewport is the sector's own sub-rectangle. Rasterizes through the same
//! resvg/usvg/tiny-skia path as the PNG export (machine-robust thresholds, not a
//! pixel baseline — see `visual_regression.rs`).

use mapgen_render::{render, style::Style, FONTS_TTF};
use mapgen_world::{
    generate_full,
    scale::{refine_sector, RefineParams, Sector},
    GenerateParams,
};

fn rasterize(svg: &str) -> tiny_skia::Pixmap {
    let mut opts = usvg::Options::default();
    for (_, bytes) in FONTS_TTF {
        opts.fontdb_mut().load_font_data(bytes.to_vec());
    }
    let tree = usvg::Tree::from_str(svg, &opts).expect("usvg parses the sector SVG");
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).expect("pixmap");
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap
}

#[test]
fn refined_sector_renders_its_own_viewport_and_is_not_blank() {
    let params = GenerateParams {
        seed: 42,
        plate_count: 14,
        ..Default::default()
    };
    let sector = Sector {
        level: 2,
        sx: 2,
        sy: 1,
    };
    let parent = generate_full(params.clone());
    let world = refine_sector(&parent, sector, RefineParams::default());
    let svg = render(&world, Style::OrnateAntique).expect("ornate render");

    // viewBox is the sector's world-space rectangle, not the whole world.
    let [x0, y0, x1, y1] = sector.rect(params.width, params.height);
    let expected = format!("viewBox=\"{x0:.0} {y0:.0} {:.0} {:.0}\"", x1 - x0, y1 - y0);
    assert!(
        svg.contains(&expected),
        "sector SVG must carry its own viewBox {expected}"
    );
    // Whole-world chrome is suppressed for a sector.
    assert!(
        !svg.contains("A MAP OF THE KNOWN WORLD"),
        "sector should not draw the world title cartouche"
    );

    // Rasterize and sanity-check it's a real map, not a blank/uniform canvas.
    let pm = rasterize(&svg);
    let px = pm.data();
    let n = (px.len() / 4) as f64;
    let (mut opaque, mut sum) = (0u64, 0f64);
    let mut water = 0u64;
    for c in px.chunks_exact(4) {
        let (r, g, b, a) = (c[0] as f64, c[1] as f64, c[2] as f64, c[3]);
        if a > 0 {
            opaque += 1;
        }
        sum += (r + g + b) / 3.0;
        // Bluish pixels = sea/river.
        if b > r + 12.0 && b > g + 6.0 {
            water += 1;
        }
    }
    let mean = sum / n;
    assert!(opaque as f64 / n > 0.8, "canvas should be painted");
    assert!(
        mean > 80.0 && mean < 200.0,
        "parchment-toned mean, got {mean:.0}"
    );
    assert!(
        water as f64 / n > 0.002,
        "a coastal sector should show some water"
    );

    // Variance guard against a flat fill.
    let mut var = 0f64;
    for c in px.chunks_exact(4) {
        let lum = (c[0] as f64 + c[1] as f64 + c[2] as f64) / 3.0;
        var += (lum - mean).powi(2);
    }
    assert!(var / n > 300.0, "image should have real tonal variation");
}

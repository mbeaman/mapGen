//! The 3D globe TEXTURE (`Style::GlobeTexture`) — an equirectangular, fontless
//! skin for the sphere: the planet's parchment fill + depth-shaded sea + political
//! control wash + coastline + major rivers on a flat lon/lat grid. It is the
//! richer replacement for the flat `Biomes` texture the sphere used to wear.
//!
//! Distinct from the 2D planisphere (`Style::Planet`), which is the Mollweide oval
//! WITH chrome (graticule, labels, compass, cartouche, legends) and embedded fonts
//! — none of which belongs on a sphere skin (the oval can't tile sphere UVs, and
//! the labels rasterize unreliably as an `<img>`).

use mapgen_render::{render, style::Style};
use mapgen_testsupport::planet_params;
use mapgen_world::generate_full;

/// A multi-continent crossing seed: real realms (a political wash to show) and a
/// fully-tiled rect (so the far-NE corner has cells — the equirect discriminator).
const SEED: u64 = 11;

/// Every `x,y` coordinate across the SVG's `points="..."` attributes.
fn points(svg: &str) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("points=\"") {
        rest = &rest[i + "points=\"".len()..];
        let Some(end) = rest.find('"') else { break };
        for tok in rest[..end].split_whitespace() {
            if let Some((x, y)) = tok.split_once(',') {
                if let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) {
                    out.push((x, y));
                }
            }
        }
        rest = &rest[end..];
    }
    out
}

#[test]
fn globe_texture_is_equirect_fontless_and_washes_realms() {
    let world = generate_full(planet_params(SEED));
    let svg = render(&world, Style::GlobeTexture).expect("globe texture renders");

    // A 2:1 equirectangular grid — the lon/lat layout the sphere UVs expect, not
    // the inscribed oval.
    assert!(
        svg.contains(r#"viewBox="0 0 2048 1024""#),
        "globe texture must be a 2048x1024 equirectangular grid"
    );

    // Fontless: no embedded @font-face and no <text>. This is the whole reason the
    // sphere needs its own skin — the labelled styles rasterize unreliably as an
    // <img>, so the texture carries no glyphs.
    assert!(
        !svg.contains("@font-face"),
        "globe texture must embed no fonts"
    );
    assert!(!svg.contains("<text"), "globe texture must draw no text");

    // The political control wash is present (≥1 realm group over the fill), so
    // `render_at_year` animates empires on the sphere for free.
    assert!(
        svg.contains(r#"class="planet-political""#) && svg.contains(r#"class="realm"#),
        "globe texture must carry the political wash"
    );

    // EQUIRECT, not Mollweide. The mesh tiles the whole rect, so a far-NE corner
    // cell has a vertex near (2048, 0). The identity projection keeps it there;
    // Mollweide pinches EVERY north-edge longitude toward the central meridian
    // (x≈1024). A point in the top-right is therefore equirect-only — flip
    // `render_globe_texture` back to the Mollweide `Proj` and it vanishes.
    let pts = points(&svg);
    assert!(
        pts.iter().any(|&(x, y)| x >= 1900.0 && y <= 170.0),
        "equirect must place a point in the far-NE corner; Mollweide would pinch it inward"
    );
}

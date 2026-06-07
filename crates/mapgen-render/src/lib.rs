//! Map renderer. Pure function of `WorldData` → SVG string. Style modules are
//! pluggable; the same world re-renders in any style without re-simulating.

pub mod layers;
pub mod simplify;
pub mod style;

use mapgen_core::WorldData;
use style::Style;

/// Vendored display typography for the Phase 3e ornate style. Single
/// source of truth — the renderer base64-embeds these bytes in the
/// SVG `<defs>` via `@font-face` so browsers render with the right
/// font; the CLI's PNG path (`mapgen-cli::sweep::svg_to_png`)
/// registers the same bytes in `usvg::Options::fontdb` so rasterized
/// PNGs see the same typography.
///
/// Static single-weight TTFs from Google Fonts (licensed SIL OFL 1.1
/// — see `crates/mapgen-render/fonts/OFL-*.txt`). Variable-font
/// upstreams were 851 KB for EB Garamond alone; subset-static via
/// Google webfonts-helper cuts each to <120 KB.
pub const FONTS_TTF: &[(&str, &[u8])] = &[
    ("Cinzel", include_bytes!("../fonts/Cinzel-Regular.ttf")),
    (
        "EB Garamond",
        include_bytes!("../fonts/EBGaramond-Regular.ttf"),
    ),
    (
        "IM Fell English",
        include_bytes!("../fonts/IMFellEnglish-Italic.ttf"),
    ),
];

/// Render a world to an SVG string in the requested style.
///
/// Every style currently in the enum is implemented; the `Result`
/// return shape is retained for future styles whose implementation may
/// not have landed yet. Callers hardcoding an implemented variant can
/// `.expect("style implemented")`.
pub fn render(world: &WorldData, style: Style) -> Result<String, String> {
    match style {
        Style::Greyscale => Ok(style::greyscale::render(world)),
        Style::Biomes => Ok(style::biomes::render(world)),
        Style::Cultures => Ok(style::cultures::render(world)),
        Style::OrnateAntique => Ok(style::ornate_antique::render(world)),
        Style::Planet => Ok(style::planet::render(world)),
        Style::GlobeTexture => Ok(style::planet::render_globe_texture(world)),
    }
}

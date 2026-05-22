//! Map renderer. Pure function of `WorldData` → SVG string. Style modules are
//! pluggable; the same world re-renders in any style without re-simulating.

pub mod style;

use mapgen_core::WorldData;
use style::Style;

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
    }
}

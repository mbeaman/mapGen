//! Map renderer. Pure function of `WorldData` → SVG string. Style modules are
//! pluggable; the same world re-renders in any style without re-simulating.

pub mod style;

use mapgen_core::WorldData;
use style::Style;

/// Render a world to an SVG string in the requested style.
///
/// Returns `Err` for styles whose implementation has not landed yet
/// (currently only [`Style::OrnateAntique`], which is reserved for Phase
/// 3e). Implemented styles always succeed; callers that hardcode an
/// implemented variant can `.expect("style implemented")`.
pub fn render(world: &WorldData, style: Style) -> Result<String, String> {
    match style {
        Style::Greyscale => Ok(style::greyscale::render(world)),
        Style::Biomes => Ok(style::biomes::render(world)),
        Style::OrnateAntique => {
            Err("ornate_antique: not yet implemented — reserved for Phase 3e".into())
        }
    }
}

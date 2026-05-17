//! Map renderer. Pure function of `WorldData` → SVG string. Style modules are
//! pluggable; the same world re-renders in any style without re-simulating.

pub mod style;

use mapgen_core::WorldData;
use style::Style;

pub fn render(world: &WorldData, style: Style) -> String {
    match style {
        Style::Greyscale => style::greyscale::render(world),
        Style::OrnateAntique => style::ornate_antique::render(world),
    }
}

//! Browser-facing façade. Does NOT depend on `mapgen-lore` — Claude API
//! traffic flows through a native sidecar to keep the API key off the wire.

use mapgen_render::style::Style;
use mapgen_world::GenerateParams;
use wasm_bindgen::prelude::*;

/// Generate and render a world in a single round trip across the JS/WASM
/// boundary. Returns the SVG string.
#[wasm_bindgen]
pub fn generate_and_render(seed: u64, cells: usize, nations: usize) -> String {
    let world = mapgen_world::generate(GenerateParams {
        seed,
        cell_count: cells,
        nation_count: nations,
        ..Default::default()
    });
    mapgen_render::render(&world, Style::Greyscale).expect("greyscale style is implemented")
}

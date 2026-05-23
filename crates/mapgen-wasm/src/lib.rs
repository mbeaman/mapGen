//! Browser-facing façade. Does NOT depend on `mapgen-lore` — Claude API
//! traffic flows through a native sidecar to keep the API key off the wire.
//!
//! The API is split into `generate` (expensive: full pipeline) and
//! `WorldHandle::render` (cheap: style-only) so the frontend can switch
//! render styles on a generated world without re-running the ~5 s
//! generate pass.

use std::str::FromStr;

use mapgen_core::WorldData;
use mapgen_render::style::Style;
use mapgen_world::GenerateParams;
use wasm_bindgen::prelude::*;

/// Opaque handle to a fully-generated world. JS keeps this around and
/// passes it to `render()` whenever the user picks a new style. The
/// handle owns its `WorldData`; dropping it on the JS side frees the
/// underlying allocation.
#[wasm_bindgen]
pub struct WorldHandle {
    inner: WorldData,
}

/// Run the full geography → climate → biomes pipeline. Slow (~5 s at
/// 15 k cells). Returns an opaque handle the caller passes to `render`.
#[wasm_bindgen]
pub fn generate(seed: u64, cells: usize, nations: usize) -> WorldHandle {
    let world = mapgen_world::generate_full(GenerateParams {
        seed,
        cell_count: cells,
        nation_count: nations,
        ..Default::default()
    });
    WorldHandle { inner: world }
}

#[wasm_bindgen]
impl WorldHandle {
    /// Render the world to an SVG string. `style` is one of
    /// `"greyscale"`, `"biomes"`, `"cultures"`, or `"ornate"`
    /// (aliases accepted — see `Style::from_str`). Cheap (~1 s);
    /// call repeatedly to switch styles without regenerating.
    #[wasm_bindgen(js_name = render)]
    pub fn render(&self, style: &str) -> Result<String, JsError> {
        let style = Style::from_str(style).map_err(|e| JsError::new(&e))?;
        mapgen_render::render(&self.inner, style).map_err(|e| JsError::new(&e))
    }
}

/// Deprecated single-shot wrapper kept for one release so external
/// probes don't break. Prefer `generate` + `WorldHandle::render`.
/// Not marked `#[deprecated]` because wasm-bindgen's generated wrapper
/// trips the lint, and CI runs `-Dwarnings`.
#[wasm_bindgen]
pub fn generate_and_render(seed: u64, cells: usize, nations: usize) -> String {
    let world = mapgen_world::generate_full(GenerateParams {
        seed,
        cell_count: cells,
        nation_count: nations,
        ..Default::default()
    });
    mapgen_render::render(&world, Style::Greyscale).expect("greyscale style is implemented")
}

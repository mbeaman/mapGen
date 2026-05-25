//! Browser-facing façade. Does NOT depend on `mapgen-lore` — Claude API
//! traffic flows through a native sidecar to keep the API key off the wire.
//!
//! Two generate paths share one pipeline implementation:
//!
//! - [`generate`] — one-shot full pipeline, returns a [`WorldHandle`].
//! - [`Generation`] — resumable: the caller steps stage by stage,
//!   rendering the partial world between stages for a live build-up.
//!
//! Both bottom out in `mapgen_world`'s `Pipeline`, so they are identical
//! byte-for-byte. `WorldHandle::render` stays cheap (style-only) so the
//! frontend can restyle a finished world without regenerating.

use std::str::FromStr;

use mapgen_core::WorldData;
use mapgen_render::style::Style;
use mapgen_world::{
    scale::{refine_sector as refine, RefineParams, Sector},
    GenerateParams, Pipeline, PipelineStage,
};
use serde::Serialize;
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
/// For a progress UI, prefer [`Generation`].
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
    /// Refine a sub-sector of *this* world at finer resolution (Phase 7
    /// multi-scale). Call on the whole-world handle — its society is projected
    /// onto the sector, so the same towns/borders appear. `(level, sx, sy)`
    /// address the `2^level × 2^level` quadtree. Returns a new [`WorldHandle`]
    /// rendered exactly like a whole world (its SVG carries the sector viewBox).
    #[wasm_bindgen(js_name = refineSector)]
    pub fn refine_sector(
        &self,
        level: u32,
        sx: u32,
        sy: u32,
        cells: usize,
    ) -> Result<WorldHandle, JsError> {
        let sector = Sector { level, sx, sy };
        if !sector.is_valid() {
            return Err(JsError::new(&format!(
                "sector ({sx},{sy}) out of range for level {level} (valid 0..{})",
                sector.span()
            )));
        }
        Ok(WorldHandle {
            inner: refine(
                &self.inner,
                sector,
                RefineParams {
                    target_cells: cells,
                    ..Default::default()
                },
            ),
        })
    }

    /// Render the world to an SVG string. `style` is one of
    /// `"greyscale"`, `"biomes"`, `"cultures"`, or `"ornate"`
    /// (aliases accepted — see `Style::from_str`). Cheap (~1 s);
    /// call repeatedly to switch styles without regenerating.
    #[wasm_bindgen(js_name = render)]
    pub fn render(&self, style: &str) -> Result<String, JsError> {
        let style = Style::from_str(style).map_err(|e| JsError::new(&e))?;
        mapgen_render::render(&self.inner, style).map_err(|e| JsError::new(&e))
    }

    /// Serialize the world to JSON — the body the narration sidecar
    /// (`mapgen serve`) deserializes for `POST /narrate`. Same shape as the CLI's
    /// `.json.gz`, just uncompressed.
    #[wasm_bindgen(js_name = worldJson)]
    pub fn world_json(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.inner).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Render the map as its borders stood at the end of `year` (the
    /// time-slider). Polity territory is reconstructed from the recorded
    /// territorial timeline; everything else (terrain, settlements, labels) is
    /// the present state. Cheap — call repeatedly while scrubbing.
    #[wasm_bindgen(js_name = renderAtYear)]
    pub fn render_at_year(&mut self, style: &str, year: i32) -> Result<String, JsError> {
        let style = Style::from_str(style).map_err(|e| JsError::new(&e))?;
        let past = self.inner.control_at_year(year);
        let saved = std::mem::replace(&mut self.inner.society.control, past);
        let svg = mapgen_render::render(&self.inner, style).map_err(|e| JsError::new(&e));
        self.inner.society.control = saved; // restore the present
        svg
    }

    /// `[start, end]` years for the time-slider — `start` at the founding map,
    /// `end` at the present — or an empty array if borders never moved (so the
    /// frontend can hide the slider).
    #[wasm_bindgen(js_name = historyYears)]
    pub fn history_years(&self) -> Vec<i32> {
        match self.inner.border_change_year_span() {
            Some((_, last)) => vec![0, last],
            None => Vec::new(),
        }
    }
}

/// Per-step progress descriptor handed back to JS. A plain serializable
/// struct (not a `#[wasm_bindgen]` class) so it survives a worker→main
/// `postMessage` structured clone.
#[derive(Serialize)]
struct StageInfo {
    /// 0-based stage index in canonical order.
    index: usize,
    /// Total stage count.
    total: usize,
    /// Cumulative weighted completion fraction, 0.0..=1.0.
    progress: f64,
    /// Human-readable stage label, e.g. "Carving rivers".
    label: String,
    /// Stable lowercase stage id, e.g. "hydrology" (drives style choice).
    stage: String,
    /// 1-based sub-step within the stage (erosion animates; else 1).
    sub: u32,
    /// Total sub-steps in this stage (1 for everything but erosion).
    sub_total: u32,
}

/// Resumable generation. Construct, then call `step()` until it returns
/// `undefined`, rendering `render(style)` between steps for a live
/// build-up. Call `finish()` to hand the finished world to a cheap-restyle
/// [`WorldHandle`].
#[wasm_bindgen]
pub struct Generation {
    inner: Pipeline,
}

#[wasm_bindgen]
impl Generation {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u64, cells: usize, nations: usize) -> Generation {
        let params = GenerateParams {
            seed,
            cell_count: cells,
            nation_count: nations,
            ..Default::default()
        };
        Generation {
            inner: Pipeline::new(params),
        }
    }

    /// Advance one fine step (erosion reports per-iteration so it
    /// animates). Returns a `StageInfo` object, or `undefined` when done.
    pub fn step(&mut self) -> Result<JsValue, JsError> {
        match self.inner.step_fine() {
            Some(fs) => {
                let info = StageInfo {
                    index: fs.stage.index(),
                    total: PipelineStage::total(),
                    progress: self.inner.progress(),
                    label: fs.stage.label().to_string(),
                    stage: fs.stage.id().to_string(),
                    sub: fs.sub,
                    sub_total: fs.sub_total,
                };
                serde_wasm_bindgen::to_value(&info).map_err(|e| JsError::new(&e.to_string()))
            }
            None => Ok(JsValue::UNDEFINED),
        }
    }

    /// Render the partial world built so far. All styles tolerate a
    /// partially-populated world.
    pub fn render(&self, style: &str) -> Result<String, JsError> {
        let style = Style::from_str(style).map_err(|e| JsError::new(&e))?;
        mapgen_render::render(self.inner.world(), style).map_err(|e| JsError::new(&e))
    }

    /// Consume the generation and return a cheap-restyle handle. The
    /// pipeline should be finished (all steps run) before calling this.
    pub fn finish(self) -> WorldHandle {
        WorldHandle {
            inner: self.inner.into_world(),
        }
    }
}

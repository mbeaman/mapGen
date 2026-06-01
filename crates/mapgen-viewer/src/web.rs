//! Web entry point (wasm32-only).
//!
//! Mirrors `src/bin/native.rs` but for the browser: builds the shared
//! [`App`](crate::App) with canvas-backed `WindowAttributes`, and drives
//! it via `event_loop.spawn_app(app)` (returns immediately because we
//! can't block the JS thread). All event handling lives in
//! [`crate::app`]; the renderer init is async on web (see
//! `App::resumed`).
//!
//! JS surface:
//! ```js
//! import init, { start_web } from "./pkg-viewer/mapgen_viewer.js";
//! await init();
//! const canvas = document.querySelector("canvas");
//! start_web(canvas, 42n, 4000);
//! ```

use wasm_bindgen::prelude::*;
use winit::event_loop::EventLoop;
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::Window;

use crate::app::{App, AppArgs};

/// Auto-runs when the wasm module loads: routes panics through the
/// browser console (otherwise they're swallowed) and wires up `log` to
/// `console.log` / `console.warn` / etc. Idempotent.
#[wasm_bindgen(start)]
pub fn _init_logging() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
}

/// Mount the live 3D viewer on `canvas`, generate `(seed, cells)` and
/// kick off the event loop. Returns immediately — rendering happens
/// asynchronously after wgpu's adapter/device handshake completes.
#[wasm_bindgen]
pub fn start_web(
    canvas: web_sys::HtmlCanvasElement,
    seed: u64,
    cells: usize,
) -> Result<(), JsValue> {
    let attrs = Window::default_attributes()
        .with_canvas(Some(canvas))
        // Let mouse drags scroll-wheel on the canvas reach winit
        // instead of triggering browser scroll / context menu.
        .with_prevent_default(true);

    let event_loop =
        EventLoop::new().map_err(|e| JsValue::from_str(&format!("event loop: {e}")))?;
    let app = App::new(AppArgs { seed, cells }, attrs);
    event_loop.spawn_app(app);
    Ok(())
}

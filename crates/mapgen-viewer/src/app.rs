//! Shared winit `ApplicationHandler` driving the viewer on both
//! native and web targets.
//!
//! Native and web differ only in:
//! - **`WindowAttributes`**: web uses `with_canvas(Some(canvas))`; the
//!   App takes them by value and forwards to `event_loop.create_window`.
//! - **Renderer init**: [`Renderer::new`] is async. Native blocks with
//!   `pollster::block_on`; web spawns a `wasm_bindgen_futures` task
//!   that parks the finished renderer in a slot ([`Self::pending`]) for
//!   the next `window_event` tick to pick up.
//! - **Loop driver**: native calls `event_loop.run_app(&mut app)`; web
//!   calls `event_loop.spawn_app(app)` (returns immediately because the
//!   web platform can't block the JS thread).
//!
//! Everything else — input mapping, redraw, resize, exit — is shared.

use std::sync::Arc;

use mapgen_world::{generate_full, GenerateParams};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::Renderer;

/// Pixel-to-line conversion for trackpad-style pixel scroll deltas.
/// Most desktops report ~50px per "line"; matching that keeps a single
/// scroll-notch on a wheel mouse feel comparable to a trackpad swipe.
const PIXELS_PER_LINE: f32 = 50.0;

#[derive(Clone, Copy, Debug)]
pub struct AppArgs {
    pub seed: u64,
    pub cells: usize,
}

#[derive(Default)]
struct Input {
    last_cursor: Option<PhysicalPosition<f64>>,
    left_held: bool,
    right_held: bool,
}

pub struct App {
    args: AppArgs,
    window_attrs: WindowAttributes,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    input: Input,

    /// Async renderer init parks the finished renderer here on web;
    /// each `window_event` tick promotes it to `self.renderer` once
    /// ready. Unused on native (renderer is built synchronously).
    #[cfg(target_arch = "wasm32")]
    pending: Option<std::rc::Rc<std::cell::RefCell<Option<Renderer>>>>,
}

impl App {
    pub fn new(args: AppArgs, window_attrs: WindowAttributes) -> Self {
        Self {
            args,
            window_attrs,
            window: None,
            renderer: None,
            input: Input::default(),
            #[cfg(target_arch = "wasm32")]
            pending: None,
        }
    }

    /// On wasm, move a finished renderer out of the pending slot into
    /// `self.renderer`. No-op once promoted. No-op entirely on native.
    #[cfg(target_arch = "wasm32")]
    fn promote_pending(&mut self) {
        if self.renderer.is_some() {
            return;
        }
        let Some(slot) = self.pending.as_ref() else {
            return;
        };
        if let Some(r) = slot.borrow_mut().take() {
            self.renderer = Some(r);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(self.window_attrs.clone())
                .expect("window create"),
        );

        log::info!(
            "generating world (seed = {}, cells = {})...",
            self.args.seed,
            self.args.cells
        );
        let world = generate_full(GenerateParams {
            seed: self.args.seed,
            cell_count: self.args.cells,
            ..GenerateParams::default()
        });
        log::info!(
            "world ready: {} × {} world units, {} cells",
            world.mesh.width,
            world.mesh.height,
            world.mesh.sites.len()
        );

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut renderer =
                pollster::block_on(Renderer::new(window.clone())).expect("renderer init");
            renderer.load_world(&world);
            self.renderer = Some(renderer);
        }

        #[cfg(target_arch = "wasm32")]
        {
            use std::cell::RefCell;
            use std::rc::Rc;
            let slot = Rc::new(RefCell::new(None));
            let slot_for_task = slot.clone();
            let window_for_task = window.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Renderer::new(window_for_task.clone()).await {
                    Ok(mut r) => {
                        r.load_world(&world);
                        *slot_for_task.borrow_mut() = Some(r);
                        // Kick a redraw so the freshly-promoted
                        // renderer paints something without needing
                        // the user to wiggle the mouse first.
                        window_for_task.request_redraw();
                        log::info!("renderer ready");
                    }
                    Err(e) => {
                        log::error!("renderer init failed: {e:?}");
                    }
                }
            });
            self.pending = Some(slot);
        }

        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(target_arch = "wasm32")]
        self.promote_pending();

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some(window) = self.window.as_ref() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match code {
                KeyCode::Escape => event_loop.exit(),
                KeyCode::KeyR => renderer.reset_camera(),
                _ => {}
            },
            WindowEvent::Resized(size) => {
                renderer.resize(size);
                window.request_redraw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => self.input.left_held = pressed,
                    MouseButton::Right => self.input.right_held = pressed,
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(last) = self.input.last_cursor {
                    let dx = (position.x - last.x) as f32;
                    let dy = (position.y - last.y) as f32;
                    if self.input.left_held {
                        renderer.pan(dx, dy);
                    }
                    if self.input.right_held {
                        renderer.orbit(dx, dy);
                    }
                }
                self.input.last_cursor = Some(position);
            }
            WindowEvent::CursorLeft { .. } => {
                // Drop the anchor so re-entry doesn't synthesise a huge
                // delta from the off-window position.
                self.input.last_cursor = None;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y as f32) / PIXELS_PER_LINE,
                };
                renderer.zoom(lines);
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = renderer.render() {
                    log::error!("render error: {e:?}");
                }
                window.request_redraw();
            }
            _ => {}
        }
    }
}

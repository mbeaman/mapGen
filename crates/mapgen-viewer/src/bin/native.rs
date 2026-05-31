//! Native entry point for the live 3D explorer.
//!
//! Parses `--seed/--cells`, generates a world via the existing pipeline,
//! opens a winit window, hands the world + window to [`Renderer`], and
//! pumps the event loop. Stage 0b will introduce the web entry point
//! (lib.rs, behind `cfg(target_arch = "wasm32")`) over the same
//! [`Renderer`] type.
//!
//! Input mapping (Stage 0c.2):
//! - Left-drag: pan the camera target across the ground plane.
//! - Right-drag: orbit the camera (yaw + pitch).
//! - Scroll wheel: zoom in / out.
//! - `R`: reset the camera to its startup framing.
//! - `Esc`: exit.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use mapgen_viewer::Renderer;
use mapgen_world::{generate_full, GenerateParams};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Pixel-to-line conversion for trackpad-style pixel scroll deltas.
/// Most desktops report ~50px per "line"; matching that keeps a single
/// scroll-notch on a wheel mouse feel comparable to a trackpad swipe.
const PIXELS_PER_LINE: f32 = 50.0;

#[derive(Parser, Debug, Clone)]
#[command(about = "Live 3D explorer for generated worlds")]
struct Args {
    /// World seed (any u64).
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Cell count. Smaller = faster generation; 4000 is the dev-loop default.
    #[arg(long, default_value_t = 4000)]
    cells: usize,
}

#[derive(Default)]
struct Input {
    last_cursor: Option<PhysicalPosition<f64>>,
    left_held: bool,
    right_held: bool,
}

struct App {
    args: Args,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    input: Input,
}

impl App {
    fn new(args: Args) -> Self {
        Self {
            args,
            window: None,
            renderer: None,
            input: Input::default(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = format!(
            "mapgen-viewer — seed {} · {} cells",
            self.args.seed, self.args.cells
        );
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(title)
                        .with_inner_size(LogicalSize::new(1280, 800)),
                )
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

        let mut renderer =
            pollster::block_on(Renderer::new(window.clone())).expect("renderer init");
        renderer.load_world(&world);

        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"),
    )
    .init();
    let args = Args::parse();
    let event_loop = EventLoop::new()?;
    let mut app = App::new(args);
    event_loop.run_app(&mut app)?;
    Ok(())
}

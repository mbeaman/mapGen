//! Native entry point for the live 3D explorer (Stage 0c.1).
//!
//! Parses `--seed/--cells`, generates a world via the existing pipeline,
//! opens a winit window, hands the world + window to [`Renderer`], and
//! pumps the event loop. Stage 0b will introduce the web entry point
//! (lib.rs, behind `cfg(target_arch = "wasm32")`) over the same
//! [`Renderer`] type. Stage 0c.2 wires mouse/keyboard input through to
//! an orbit camera.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use mapgen_viewer::Renderer;
use mapgen_world::{generate_full, GenerateParams};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[derive(Parser, Debug, Clone)]
#[command(about = "Live 3D explorer for generated worlds (Stage 0c.1)")]
struct Args {
    /// World seed (any u64).
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Cell count. Smaller = faster generation; 4000 is the dev-loop default.
    #[arg(long, default_value_t = 4000)]
    cells: usize,
}

struct App {
    args: Args,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

impl App {
    fn new(args: Args) -> Self {
        Self {
            args,
            window: None,
            renderer: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = format!(
            "mapgen-viewer (Stage 0c.1) — seed {} · {} cells",
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
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        ..
                    },
                ..
            } => event_loop.exit(),
            WindowEvent::Resized(size) => {
                renderer.resize(size);
                window.request_redraw();
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

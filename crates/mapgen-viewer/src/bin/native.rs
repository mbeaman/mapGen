//! Native entry point for the live 3D explorer (Stage 0a).
//!
//! Opens a winit window, hands it to [`Renderer`], and pumps the event loop
//! with clear-on-redraw + resize handling + Esc-to-quit. The web entry
//! (Stage 0b) will live in `lib.rs` behind a `cfg(target_arch = "wasm32")`
//! gate and exercise the same [`Renderer`] type.

use std::sync::Arc;

use anyhow::Result;
use mapgen_viewer::Renderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("mapgen-viewer (Stage 0a)")
                        .with_inner_size(LogicalSize::new(1280, 800)),
                )
                .expect("window create"),
        );
        let renderer = pollster::block_on(Renderer::new(window.clone())).expect("renderer init");
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
    let event_loop = EventLoop::new()?;
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}

//! Native entry point for the live 3D explorer.
//!
//! Thin wrapper: parses `--seed/--cells`, builds the shared `App` with
//! desktop-style window attributes, and drives it via
//! `event_loop.run_app(&mut app)`. All the event handling lives in
//! [`mapgen_viewer::app`]; the web entry uses the same `App`.
//!
//! On wasm32 this binary compiles to an empty `main` — the
//! `clap`/`pollster`/`env_logger` deps are gated out in `Cargo.toml`,
//! and the web entry point lives in `mapgen_viewer::start_web` (lib).

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    native::run()
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use anyhow::Result;
    use clap::Parser;
    use mapgen_viewer::{App, AppArgs};
    use winit::dpi::LogicalSize;
    use winit::event_loop::EventLoop;
    use winit::window::Window;

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

    pub fn run() -> Result<()> {
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"),
        )
        .init();
        let args = Args::parse();
        let title = format!("mapgen-viewer — seed {} · {} cells", args.seed, args.cells);
        let window_attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(1280, 800));

        let event_loop = EventLoop::new()?;
        let mut app = App::new(
            AppArgs {
                seed: args.seed,
                cells: args.cells,
            },
            window_attrs,
        );
        event_loop.run_app(&mut app)?;
        Ok(())
    }
}

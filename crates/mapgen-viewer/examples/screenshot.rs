//! Render a world headlessly from a handful of camera poses and save
//! PNGs. Useful for README hero images and for sanity-checking the
//! wgpu pipeline without a display.
//!
//! Usage: `cargo run -p mapgen-viewer --example screenshot -- \
//!         [seed] [cells] [out_prefix] [width] [height]`
//!
//! Writes `<prefix>-default.png`, `<prefix>-topdown.png`, and
//! `<prefix>-lowangle.png` next to one another.
//!
//! Native-only: pollster/env_logger/image aren't reachable on wasm32.

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    run()
}

#[cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use mapgen_viewer::{screenshot_with, Pose};
#[cfg(not(target_arch = "wasm32"))]
use mapgen_world::{generate_full, GenerateParams};

#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();

    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().as_deref().unwrap_or("42").parse()?;
    let cells: usize = args.next().as_deref().unwrap_or("4000").parse()?;
    let prefix = args.next().unwrap_or_else(|| "shot".to_string());
    let width: u32 = args.next().as_deref().unwrap_or("1280").parse()?;
    let height: u32 = args.next().as_deref().unwrap_or("800").parse()?;

    log::info!("generating world (seed = {seed}, cells = {cells})...");
    let world = generate_full(GenerateParams {
        seed,
        cell_count: cells,
        ..GenerateParams::default()
    });

    let shots: &[(&str, Pose)] = &[
        // Tighter frame than the OrbitCamera default — the 1.4× margin
        // it uses for interactive panning is too generous for a still.
        (
            "default",
            Pose {
                yaw: 0.0,
                pitch: 1.05,
                distance_scale: 0.75,
            },
        ),
        // Near-vertical: lets the user compare against the existing
        // SVG export (mapgen-render uses a top-down projection).
        (
            "topdown",
            Pose {
                yaw: 0.0,
                pitch: 1.50,
                distance_scale: 0.85,
            },
        ),
        // Low angle from the south-east, framed close. Stage 1's
        // elevation lift will make this view dramatic; for 0c.2 it
        // just shows the camera can swing freely.
        (
            "lowangle",
            Pose {
                yaw: -0.6,
                pitch: 0.45,
                distance_scale: 0.7,
            },
        ),
    ];

    for (name, pose) in shots {
        log::info!("rendering {name} at {width}×{height}...");
        let rgba = pollster::block_on(screenshot_with(&world, width, height, *pose))?;
        let path = format!("{prefix}-{name}.png");
        image::RgbaImage::from_raw(width, height, rgba)
            .expect("rgba size mismatch")
            .save(&path)?;
        log::info!("saved {path}");
    }

    Ok(())
}

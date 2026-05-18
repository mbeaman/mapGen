//! `mapgen` — native developer CLI.

mod sweep;

use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use mapgen_core::WorldData;
use mapgen_render::style::Style;
use mapgen_world::GenerateParams;

#[derive(Parser)]
#[command(name = "mapgen", version, about = "Fantasy map generator.")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a world from a seed and persist it.
    Generate {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value_t = 15_000)]
        cells: usize,
        #[arg(long, default_value_t = 14)]
        plates: usize,
        #[arg(long, default_value_t = 8)]
        nations: usize,
        #[arg(long, default_value = "worlds/world.json.gz")]
        out: PathBuf,
    },
    /// Render a previously generated world to SVG.
    Render {
        #[arg(long, value_name = "WORLD")]
        r#in: PathBuf,
        #[arg(long, default_value = "greyscale")]
        style: String,
        #[arg(long, default_value = "maps/world.svg")]
        out: PathBuf,
    },
    /// Sweep one parameter knob across a range and render every step
    /// plus an `index.html` grid for eyeball selection.
    Sweep {
        #[arg(long)]
        seed: u64,
        /// Knob to vary. One of: erosion_rate, base_precip, lapse_rate, axial_tilt.
        #[arg(
            long,
            long_help = "Knob to vary. Supported (default · typical range · units):\n  \
                         erosion_rate (0.04 · 0.01..0.10 · unitless k)\n  \
                         base_precip  (0.7  · 0.4..1.0   · saturated-ocean fraction)\n  \
                         lapse_rate   (0.45 · 0.30..0.60 · normalized °C per unit elev)\n  \
                         axial_tilt   (0.41 · 0.20..0.50 · RADIANS — 0.41 ≈ 23.5°)"
        )]
        knob: String,
        /// `lo..hi` (e.g. `0.01..0.10`). Inclusive form `lo..=hi` accepted.
        #[arg(long)]
        range: String,
        #[arg(long, default_value_t = 8)]
        steps: usize,
        /// Cell count per generated world. Lower = faster sweep,
        /// fewer details per map. 4000 is a good ratio for tuning.
        #[arg(long, default_value_t = 4_000)]
        cells: usize,
        #[arg(long, default_value = "biomes")]
        style: String,
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Generate {
            seed,
            cells,
            plates,
            nations,
            out,
        } => {
            let params = GenerateParams {
                seed,
                cell_count: cells,
                plate_count: plates,
                nation_count: nations,
                ..Default::default()
            };
            let world = mapgen_world::generate_full(params);
            write_world(&out, &world)?;
            eprintln!("wrote world: {}", out.display());
        }
        Cmd::Render { r#in, style, out } => {
            let world = read_world(&r#in)?;
            let style: Style = style.parse().map_err(anyhow::Error::msg)?;
            let svg = mapgen_render::render(&world, style).map_err(anyhow::Error::msg)?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&out, svg).with_context(|| format!("writing {}", out.display()))?;
            eprintln!("wrote svg: {}", out.display());
        }
        Cmd::Sweep {
            seed,
            knob,
            range,
            steps,
            cells,
            style,
            out,
        } => {
            sweep::run(seed, &knob, &range, steps, cells, &style, &out)?;
        }
    }
    Ok(())
}

fn write_world(path: &Path, world: &WorldData) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut gz = GzEncoder::new(BufWriter::new(file), Compression::default());
    serde_json::to_writer(&mut gz, world)?;
    gz.finish()?.flush()?;
    Ok(())
}

fn read_world(path: &Path) -> Result<WorldData> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut gz = GzDecoder::new(BufReader::new(file));
    let mut buf = String::new();
    gz.read_to_string(&mut buf)?;
    let world: WorldData = serde_json::from_str(&buf)?;
    Ok(world)
}

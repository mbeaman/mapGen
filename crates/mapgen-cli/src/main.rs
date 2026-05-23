//! `mapgen` — native developer CLI.

mod sweep;

use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use mapgen_core::WorldData;
use mapgen_render::style::Style;
use mapgen_world::{GenerateParams, Pipeline, PipelineStage};

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
        /// Print a live per-stage progress line (auto-on when stderr is a TTY).
        #[arg(long)]
        progress: bool,
        /// Print a per-stage wall-clock timing table after generation.
        #[arg(long)]
        timings: bool,
        /// Write an SVG snapshot of the world after each stage into this
        /// directory (`NN-<stage>.svg`), for pipeline debugging.
        #[arg(long, value_name = "DIR")]
        dump_stages: Option<PathBuf>,
        /// Fixed style for `--dump-stages`. Defaults to a per-stage
        /// progression (greyscale → biomes → cultures).
        #[arg(long, value_name = "STYLE")]
        dump_style: Option<String>,
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
            progress,
            timings,
            dump_stages,
            dump_style,
        } => {
            let params = GenerateParams {
                seed,
                cell_count: cells,
                plate_count: plates,
                nation_count: nations,
                ..Default::default()
            };
            let dump_style = match dump_style {
                Some(s) => Some(s.parse::<Style>().map_err(anyhow::Error::msg)?),
                None => None,
            };
            // Drive the pipeline directly only when the caller wants
            // observability; otherwise take the fast one-shot path. Both
            // are the same pipeline, so the persisted world is identical.
            let world = if progress || timings || dump_stages.is_some() {
                drive_generate(
                    params,
                    progress || std::io::stderr().is_terminal(),
                    timings,
                    dump_stages.as_deref(),
                    dump_style,
                )?
            } else {
                mapgen_world::generate_full(params)
            };
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

/// Drive the generation pipeline one stage at a time, optionally printing
/// a live progress line, recording per-stage timings, and dumping a
/// per-stage SVG. Returns the same world `generate_full` would.
fn drive_generate(
    params: GenerateParams,
    show_progress: bool,
    timings: bool,
    dump_dir: Option<&Path>,
    dump_style: Option<Style>,
) -> Result<WorldData> {
    if let Some(dir) = dump_dir {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }

    let total = PipelineStage::total();
    let mut pipeline = Pipeline::new(params);
    let mut times: Vec<(&'static str, Duration)> = Vec::with_capacity(total);

    for (i, stage) in PipelineStage::ORDER.iter().enumerate() {
        if show_progress {
            eprint!("\r[{:>2}/{}] {:<22}", i + 1, total, stage.label());
            std::io::stderr().flush().ok();
        }

        let t0 = Instant::now();
        let ran = pipeline
            .step()
            .expect("stage available while iterating ORDER");
        let dt = t0.elapsed();
        debug_assert_eq!(ran, *stage, "pipeline diverged from PipelineStage::ORDER");

        if timings {
            times.push((stage.label(), dt));
        }
        if let Some(dir) = dump_dir {
            let style = dump_style.unwrap_or_else(|| progressive_style(*stage));
            let svg = mapgen_render::render(pipeline.world(), style).map_err(anyhow::Error::msg)?;
            let path = dir.join(format!("{:02}-{}.svg", i + 1, stage.id()));
            fs::write(&path, svg).with_context(|| format!("writing {}", path.display()))?;
        }
    }

    if show_progress {
        eprintln!("\r[{0:>2}/{0}] done.{1:<18}", total, "");
    }
    if timings {
        let total_t: Duration = times.iter().map(|(_, d)| *d).sum();
        eprintln!("stage timings:");
        for (label, d) in &times {
            eprintln!("  {label:<22} {:>8.3}s", d.as_secs_f64());
        }
        eprintln!("  {:<22} {:>8.3}s", "total", total_t.as_secs_f64());
    }

    Ok(pipeline.into_world())
}

/// Per-stage render style for `--dump-stages`: show the world in the
/// richest style whose inputs exist by that stage.
fn progressive_style(stage: PipelineStage) -> Style {
    use PipelineStage::*;
    match stage {
        Terrain | Erosion => Style::Greyscale,
        Hydrology | Ocean | Climate | Biomes => Style::Biomes,
        Cultures | Religions | Polities | Naming => Style::Cultures,
    }
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

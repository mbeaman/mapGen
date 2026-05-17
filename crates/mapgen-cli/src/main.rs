//! `mapgen` — native developer CLI.

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
            let world = mapgen_world::generate(params);
            write_world(&out, &world)?;
            eprintln!("wrote world: {}", out.display());
        }
        Cmd::Render { r#in, style, out } => {
            let world = read_world(&r#in)?;
            let style: Style = style.parse().map_err(anyhow::Error::msg)?;
            let svg = mapgen_render::render(&world, style);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&out, svg).with_context(|| format!("writing {}", out.display()))?;
            eprintln!("wrote svg: {}", out.display());
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

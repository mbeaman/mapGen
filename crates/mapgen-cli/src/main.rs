//! `mapgen` — native developer CLI.

#[cfg(feature = "lore")]
mod serve;
mod sweep;

use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use mapgen_core::WorldData;
use mapgen_render::style::Style;
use mapgen_world::{
    scale::{refine_sector, RefineParams, Sector},
    GenerateParams, Pipeline, PipelineStage,
};

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
    /// Refine a sub-sector of a seed's world at finer resolution (Phase 7
    /// multi-scale) and render it. No parent file needed — the base field is
    /// recomputed from the seed, so a sector is a pure function of
    /// `(seed, plates, level, sx, sy)`. `(sx, sy)` index the 2^level × 2^level
    /// quadtree grid at `level` (e.g. level 2 → sx,sy in 0..4).
    Refine {
        #[arg(long)]
        seed: u64,
        /// Quadtree depth (0 = whole world). Each level is 2× finer per axis.
        #[arg(long, default_value_t = 2)]
        level: u32,
        /// Column in the 2^level grid.
        #[arg(long, default_value_t = 0)]
        sx: u32,
        /// Row in the 2^level grid.
        #[arg(long, default_value_t = 0)]
        sy: u32,
        /// Plate count — must match the world this sector belongs to.
        #[arg(long, default_value_t = 14)]
        plates: usize,
        /// Target cell count inside the sector (finer than the parent world).
        #[arg(long, default_value_t = 4_000)]
        cells: usize,
        #[arg(long, default_value = "ornate_antique")]
        style: String,
        #[arg(long, default_value = "maps/sector.svg")]
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
    /// Summarize a generated world's history: mythic ages, major events, and
    /// the narrative arcs woven from them.
    Events {
        #[arg(long, value_name = "WORLD")]
        r#in: PathBuf,
        /// Only show events at or above this salience (0.8 = the "major" bar).
        #[arg(long, default_value_t = 0.8)]
        min_salience: f32,
        /// Cap on events printed (highest-salience first).
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
    /// Narrate a major event into an in-world chronicle. Offline by default
    /// (deterministic template narrator). With the `lore` feature +
    /// ANTHROPIC_API_KEY it calls Claude (Phase 5e); otherwise it stays offline.
    Lore {
        #[arg(long, value_name = "WORLD")]
        r#in: PathBuf,
        /// Event to narrate: a numeric id or "auto-major-war".
        #[arg(long, default_value = "auto-major-war")]
        event: String,
        /// Register: saga | monastic-chronicle | hymn | courtly-letter | peasant-rumor.
        #[arg(long, default_value = "monastic-chronicle")]
        voice: String,
        /// Optionally write the world back with the new chronicle persisted.
        #[arg(long, value_name = "WORLD")]
        out: Option<PathBuf>,
    },
    /// Run a local narration sidecar (`POST /narrate`) so the browser frontend
    /// can request chronicles without the API key leaving this process. Only
    /// available with `--features lore`.
    #[cfg(feature = "lore")]
    Serve {
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// World to narrate when a request omits its own `world`.
        #[arg(long, value_name = "WORLD")]
        r#in: Option<PathBuf>,
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
        Cmd::Refine {
            seed,
            level,
            sx,
            sy,
            plates,
            cells,
            style,
            out,
        } => {
            let sector = Sector { level, sx, sy };
            if !sector.is_valid() {
                bail!(
                    "sector ({sx},{sy}) out of range for level {level} (valid 0..{})",
                    sector.span()
                );
            }
            // Build the parent world, then project its society onto the sector.
            // (The physical base field only needs seed/dims/plates, but towns &
            // borders are carried from the parent — so we generate it.)
            let params = GenerateParams {
                seed,
                plate_count: plates,
                ..Default::default()
            };
            let parent = mapgen_world::generate_full(params);
            let refine = RefineParams {
                target_cells: cells,
                ..Default::default()
            };
            let world = refine_sector(&parent, sector, refine);
            let style: Style = style.parse().map_err(anyhow::Error::msg)?;
            let svg = mapgen_render::render(&world, style).map_err(anyhow::Error::msg)?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&out, svg).with_context(|| format!("writing {}", out.display()))?;
            eprintln!(
                "wrote sector L{level} ({sx},{sy}) [{} cells]: {}",
                world.mesh.cell_count(),
                out.display()
            );
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
        Cmd::Events {
            r#in,
            min_salience,
            limit,
        } => {
            let world = read_world(&r#in)?;
            let h = &world.history;
            if !h.ages.is_empty() {
                println!("Mythic ages:");
                for age in &h.ages {
                    println!("  {:>4}–{:<4}  {}", age.start_year, age.end_year, age.name);
                }
                println!();
            }

            let mut major: Vec<_> = world
                .events
                .events
                .iter()
                .filter(|e| e.salience >= min_salience)
                .collect();
            major.sort_by(|a, b| {
                b.salience
                    .partial_cmp(&a.salience)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.year.cmp(&b.year))
            });
            println!(
                "{} major events (salience ≥ {:.2}; {} total in the log):",
                major.len(),
                min_salience,
                world.events.events.len()
            );
            for e in major.iter().take(limit) {
                println!(
                    "  y{:>4}  s{:.2}  {}",
                    e.year, e.salience, e.summary_canonical
                );
            }

            if !h.arcs.is_empty() {
                println!("\n{} narrative arcs:", h.arcs.len());
                let mut arcs: Vec<_> = h.arcs.iter().collect();
                arcs.sort_by_key(|a| std::cmp::Reverse(a.member_events.len()));
                for a in arcs.iter().take(12) {
                    println!(
                        "  [{:?}] {} — {} events",
                        a.kind,
                        a.title,
                        a.member_events.len()
                    );
                }
            }
        }
        Cmd::Lore {
            r#in,
            event,
            voice,
            out,
        } => {
            use mapgen_lore::{narrate, select_focal, Register, VoiceCard};

            let mut world = read_world(&r#in)?;
            let register = Register::parse(&voice).ok_or_else(|| {
                let opts: Vec<&str> = Register::ALL.iter().map(|r| r.as_str()).collect();
                anyhow::anyhow!("unknown voice '{voice}' (options: {})", opts.join(", "))
            })?;
            let card = VoiceCard::for_register(register);
            let focal = select_focal(&world, &event)?;
            let focal_summary = world
                .events
                .events
                .get(focal.0 as usize)
                .map(|e| e.summary_canonical.clone())
                .unwrap_or_default();

            // Offline by default. With `--features lore` and a key set, narrate
            // through Claude; otherwise (no feature, or no key) fall back to the
            // deterministic template narrator.
            #[cfg(feature = "lore")]
            let client = mapgen_lore::AnthropicClient::from_env();
            #[cfg(feature = "lore")]
            let narrator: Option<&dyn mapgen_lore::LlmClient> = match &client {
                Some(c) => {
                    eprintln!("narrating via Claude ({})", c.model());
                    Some(c)
                }
                None => {
                    eprintln!("ANTHROPIC_API_KEY not set — narrating offline (template)");
                    None
                }
            };
            #[cfg(not(feature = "lore"))]
            let narrator: Option<&dyn mapgen_lore::LlmClient> = None;

            let work = narrate(&mut world, focal, &card, narrator)?;

            println!("# {}", work.title);
            println!(
                "*{}, in the {} year*\n",
                work.in_world_author, work.written_year
            );
            println!("{}\n", work.body);
            if !work.lacunae.is_empty() {
                println!("Lacunae: {}\n", work.lacunae.join("; "));
            }
            println!(
                "(narrated event [{}] \"{}\" — citing {} events, voice: {})",
                focal.0,
                focal_summary,
                work.references.len(),
                register.as_str()
            );

            if let Some(path) = out {
                write_world(&path, &world)?;
                eprintln!(
                    "wrote {} ({} chronicle(s) now persisted)",
                    path.display(),
                    world.works.len()
                );
            }
        }
        #[cfg(feature = "lore")]
        Cmd::Serve { port, r#in } => {
            let startup = match r#in {
                Some(p) => Some(read_world(&p)?),
                None => None,
            };
            serve::run(port, startup)?;
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
        Cultures | Religions | Polities | Naming | History => Style::Cultures,
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

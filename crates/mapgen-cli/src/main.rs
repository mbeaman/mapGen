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
        /// Persist the planet-scale, multi-continent root (`GenerateParams::planet`)
        /// instead of a default continental world. `--cells`/`--plates`/`--nations`
        /// are ignored — the planet preset sets its own (matching `refine --planet`
        /// and the `planet` command), so the saved world round-trips identically to
        /// the globe those commands generate (and grows the inter-continental sea
        /// lanes a continental world never has).
        #[arg(long)]
        planet: bool,
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
        /// Plate count — must match the world this sector belongs to (ignored
        /// with --planet, which uses the planet preset's own plate count).
        #[arg(long, default_value_t = 14)]
        plates: usize,
        /// Drill into the planet-scale root (`GenerateParams::planet`) rather
        /// than a default continental world — so `planet` + `refine --planet`
        /// share one consistent globe.
        #[arg(long)]
        planet: bool,
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
    /// Render the world under every curated preset into one shareable HTML
    /// atlas (a multi-page "world bible"). Each page bakes a layer state into
    /// the ornate render, so the file is self-contained — no scripts, no
    /// external assets — and prints to PDF cleanly (one preset per page).
    Atlas {
        #[arg(long)]
        seed: u64,
        /// Cells per map. Kept modest by default: the atlas embeds the SVG
        /// once per preset, so file size scales with this × the preset count.
        #[arg(long, default_value_t = 6_000)]
        cells: usize,
        #[arg(long, default_value_t = 14)]
        plates: usize,
        #[arg(long, default_value_t = 8)]
        nations: usize,
        #[arg(long, default_value = "atlas.html")]
        out: PathBuf,
    },
    /// Generate a planet-scale, multi-continent world and render the zoomed-out
    /// planisphere overview (`Style::Planet`). The planet is the root level;
    /// drill into any continent with `refine` for the ornate continental view.
    Planet {
        #[arg(long)]
        seed: u64,
        /// Target cells across the whole planet (default planet preset: 18k).
        #[arg(long, default_value_t = 18_000)]
        cells: usize,
        /// Tectonic plates — more plates means more, smaller continents.
        #[arg(long, default_value_t = 32)]
        plates: usize,
        #[arg(long, default_value = "maps/planet.svg")]
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
        /// Event to narrate: a numeric id, "auto-major-war" (the most salient
        /// war), "auto-contact" (the most salient sea-trade contact — weaves the
        /// first-contact / Sundered-Lane arc), "auto-faith" (a faith's first
        /// crossing to a far shore, named from the event's far_shore), or
        /// "auto-shore" (the multi-strand chronicle of the most-reached far shore —
        /// faith, colony, and conquest woven together, naming the landmass).
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
            planet,
            out,
            progress,
            timings,
            dump_stages,
            dump_style,
        } => {
            // `--planet` uses the preset wholesale (its own cells/plates/nations),
            // so a persisted planet is byte-identical to `planet` / `refine --planet`
            // and to the `GenerateParams::planet` fixtures the tests pin.
            let params = if planet {
                GenerateParams::planet(seed)
            } else {
                GenerateParams {
                    seed,
                    cell_count: cells,
                    plate_count: plates,
                    nation_count: nations,
                    ..Default::default()
                }
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
            planet,
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
            // borders are carried from the parent — so we generate it.) The
            // parent params must match exactly how the parent was generated, so
            // a sector is the same window of the same world — hence --planet
            // mirrors the `planet` command's preset.
            let params = if planet {
                GenerateParams::planet(seed)
            } else {
                GenerateParams {
                    seed,
                    plate_count: plates,
                    ..Default::default()
                }
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
        Cmd::Atlas {
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
            // Render the ornate base once; each atlas page bakes a preset's
            // layer state into a copy, so the simulation runs a single time.
            let base =
                mapgen_render::render(&world, Style::OrnateAntique).map_err(anyhow::Error::msg)?;
            let html = build_atlas_html(&world, &base);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&out, &html).with_context(|| format!("writing {}", out.display()))?;
            eprintln!(
                "wrote atlas: {} ({} plates, {:.1} MB)",
                out.display(),
                mapgen_render::layers::PRESETS.len(),
                html.len() as f64 / 1.0e6,
            );
        }
        Cmd::Planet {
            seed,
            cells,
            plates,
            out,
        } => {
            let mut params = GenerateParams::planet(seed);
            params.cell_count = cells;
            params.plate_count = plates;
            let world = mapgen_world::generate_full(params);
            let svg = mapgen_render::render(&world, Style::Planet).map_err(anyhow::Error::msg)?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&out, svg).with_context(|| format!("writing {}", out.display()))?;
            eprintln!(
                "wrote planet: {} ({} cells, {} plates)",
                out.display(),
                world.mesh.cell_count(),
                plates,
            );
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
            use mapgen_lore::{narrate, narrate_shore, select_focal, Register, VoiceCard};

            let mut world = read_world(&r#in)?;
            let register = Register::parse(&voice).ok_or_else(|| {
                let opts: Vec<&str> = Register::ALL.iter().map(|r| r.as_str()).collect();
                anyhow::anyhow!("unknown voice '{voice}' (options: {})", opts.join(", "))
            })?;
            let card = VoiceCard::for_register(register);

            // `auto-shore` is the multi-strand FAR-SHORE chronicle: it picks the
            // most strand-diverse landmass and weaves every carrier strand that
            // reached it (faith / colony / conquest), naming the shore from the
            // `far_shore` tags. It has no single focal event and is always woven by
            // the deterministic template (not the LLM), so it routes separately.
            if event == "auto-shore" {
                let work = narrate_shore(&mut world, &card)?;
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
                    "(wove the far-shore chronicle — citing {} events, voice: {})",
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
                return Ok(());
            }

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
        Hydrology | Ocean | Climate | Biomes | SeaLanes => Style::Biomes,
        Cultures | Religions | Polities | Naming | History => Style::Cultures,
    }
}

/// Assemble the self-contained HTML atlas: a parchment-themed page per preset,
/// each embedding the ornate render with that preset's layer state baked in.
fn build_atlas_html(world: &WorldData, base_svg: &str) -> String {
    use mapgen_render::layers::{bake_layer_state_pruned, PRESETS};

    let seed = world.meta.seed;
    let cells = world.mesh.cell_count();
    let nations = world.society.nations.len();
    let plates = world.terrain.plates.len();

    let mut html = String::with_capacity(base_svg.len() * PRESETS.len() + 4_096);
    html.push_str(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>An Atlas of the Known World</title>
<style>
*{box-sizing:border-box}
:root{--ink:#2a2418;--soft:#5c513a;--parch:#f4e9cf;--accent:#8a3324;--line:#c8b89a}
body{margin:0;background:#2a2418;color:var(--ink);font-family:"EB Garamond",Georgia,serif}
.brand{text-align:center;padding:2.6rem 1rem 1.2rem;color:var(--parch)}
.brand h1{font-family:Georgia,serif;font-weight:700;letter-spacing:.12em;text-transform:uppercase;font-size:2.2rem;margin:0}
.brand .sub{color:#c9bd9c;font-style:italic;margin:.45rem 0 0}
.plate{max-width:1100px;margin:1.6rem auto;background:var(--parch);border:1px solid #000;border-radius:4px;box-shadow:0 6px 24px rgba(0,0,0,.45);overflow:hidden}
.plate-head{display:flex;align-items:baseline;gap:.6rem;padding:1rem 1.4rem .2rem}
.plate-head .num{font-family:Georgia,serif;color:var(--accent);font-size:1.4rem;font-weight:700}
.plate-head h2{font-family:Georgia,serif;letter-spacing:.14em;text-transform:uppercase;font-size:1.1rem;margin:0;color:var(--ink)}
.map{padding:.3rem 1rem}
.map svg{width:100%;height:auto;display:block;border:1px solid var(--line)}
.caption{padding:.1rem 1.5rem 1.3rem;color:var(--soft);font-style:italic}
footer{text-align:center;color:#8c8169;font-size:.85rem;padding:1.5rem 1rem 3rem;font-style:italic}
@media print{body{background:#fff}.brand{color:var(--ink)}.plate{box-shadow:none;border:none;max-width:none;margin:0;page-break-after:always}}
</style></head><body>
"##,
    );
    html.push_str(&format!(
        r#"<header class="brand"><h1>An Atlas of the Known World</h1><p class="sub">Seed {seed} · {nations} realms · {plates} tectonic plates · {cells} cells</p></header>"#,
    ));

    for (i, p) in PRESETS.iter().enumerate() {
        // Prune the hidden groups so each page carries only what it shows.
        let svg = bake_layer_state_pruned(base_svg, p.enabled);
        html.push_str(&format!(
            r#"<section class="plate"><div class="plate-head"><span class="num">{n}</span><h2>{label}</h2></div><div class="map">{svg}</div><p class="caption">{caption}</p></section>"#,
            n = i + 1,
            label = p.label,
            caption = p.caption,
        ));
    }

    html.push_str(
        r#"<footer>Generated by mapgen — the same world, six ways. Print to PDF for a bound atlas.</footer></body></html>"#,
    );
    html
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

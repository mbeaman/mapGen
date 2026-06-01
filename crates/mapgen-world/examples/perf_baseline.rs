//! Per-component perf baseline harness.
//!
//! Default — print measurements only:
//!     cargo run --release -p mapgen-world --example perf_baseline
//!
//! Regression check — also assert each median ≤ baseline × 1.5, exit non-zero
//! on any violation:
//!     cargo run --release -p mapgen-world --example perf_baseline -- --check
//!
//! User-facing components, three samples each:
//! - `generate_full` — the geography→society→history pipeline (4k/15k/30k).
//! - `render` — ornate SVG build over a generated world (4k/15k).
//! - `render` planet — the zoomed-out planisphere (`Style::Planet`) over an
//!   18k-cell planet; most cells, least clutter, so it is the cheapest render.
//! - `refine_sector` — one on-demand zoom-in tile (Phase 7); the navigation ADR
//!   flags sector latency as user-facing, so it gets its own budget.
//!
//! Numbers + budgets live in `docs/perf_baseline.md`; the constants here must
//! stay in sync. Never invoked by `cargo test`.

use std::hint::black_box;
use std::time::{Duration, Instant};

use mapgen_core::WorldData;
use mapgen_render::{render, style::Style};
use mapgen_world::{
    generate_full,
    scale::{refine_sector, RefineParams, Sector},
    GenerateParams,
};

const SEEDS: &[u64] = &[1, 2, 3];

/// (target cell count, baseline median ms, 1.5× budget ms).
///
/// Re-anchored 2026-05-24 (Phase 4j) to the current dev box — ~1.6× slower than
/// the original Ryzen 9 5950X anchor (now gone) and including the full Phase-4
/// history sim. Prior Ryzen anchor was 17/66/133 ms.
const GEN_BASELINES: &[(usize, u64, u64)] =
    &[(4_000, 26, 39), (15_000, 111, 166), (30_000, 259, 388)];

/// `render` (ornate SVG string build) at two sizes. Anchored 2026-05-25 to this
/// dev box; render is nearly as costly as generation (forests, ripples, glyphs).
const RENDER_BASELINES: &[(usize, u64, u64)] = &[(4_000, 22, 33), (15_000, 78, 117)];

/// `refine_sector`: a level-2 tile (target 4k cells) from a 15k-cell parent.
/// `(parent cells, baseline ms, budget ms)`. Anchored 2026-05-25 to this box.
const REFINE_BASELINE: (usize, u64, u64) = (15_000, 68, 102);

/// `render` with `Style::Planet` (the zoomed-out planisphere) over an 18k-cell
/// planet world — the heaviest render path by cell count, but it drops the
/// per-cell ornate clutter (forests, ripples, glyphs), so it is far cheaper than
/// the ornate render. `(cells, baseline ms, budget ms)`. Anchored 2026-06-01.
const PLANET_RENDER_BASELINE: (usize, u64, u64) = (18_000, 13, 20);

fn gen_params(cells: usize, seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        cell_count: cells,
        ..Default::default()
    }
}

fn measure_generate(cells: usize, seed: u64) -> Duration {
    let params = gen_params(cells, seed);
    let start = Instant::now();
    // `black_box` both ends: input opaque so the call can't be const-folded,
    // output kept live so the pipeline can't be elided.
    let world = black_box(generate_full(black_box(params)));
    let dt = start.elapsed();
    drop(world);
    dt
}

fn measure_render(world: &WorldData, style: Style) -> Duration {
    let start = Instant::now();
    let svg = black_box(render(black_box(world), style).expect("render"));
    let dt = start.elapsed();
    drop(svg);
    dt
}

fn measure_refine(parent: &WorldData, sector: Sector) -> Duration {
    let start = Instant::now();
    let world = black_box(refine_sector(
        black_box(parent),
        sector,
        RefineParams::default(),
    ));
    let dt = start.elapsed();
    drop(world);
    dt
}

fn main() {
    let check = std::env::args().any(|a| a == "--check");

    // One untimed warmup to prime caches / allocators.
    let _ = measure_generate(GEN_BASELINES[0].0, SEEDS[0]);

    println!("# Perf baseline — per component");
    if check {
        println!("\nMode: --check (budget = baseline × 1.5; non-zero exit on violation)");
    }

    let mut failures: Vec<String> = Vec::new();

    // ---- generate_full -----------------------------------------------------
    header("generate_full — full pipeline");
    for &(cells, base, budget) in GEN_BASELINES {
        let samples: Vec<Duration> = SEEDS.iter().map(|&s| measure_generate(cells, s)).collect();
        row(
            &format!("{cells} cells"),
            samples,
            base,
            budget,
            &mut failures,
        );
    }

    // ---- render (over a pre-generated world, generation untimed) -----------
    header("render — ornate SVG build");
    for &(cells, base, budget) in RENDER_BASELINES {
        let samples: Vec<Duration> = SEEDS
            .iter()
            .map(|&s| measure_render(&generate_full(gen_params(cells, s)), Style::OrnateAntique))
            .collect();
        row(
            &format!("{cells} cells"),
            samples,
            base,
            budget,
            &mut failures,
        );
    }

    // ---- planet render (planisphere; planet generation untimed) ------------
    header("render — planisphere (Style::Planet)");
    {
        let (cells, base, budget) = PLANET_RENDER_BASELINE;
        let samples: Vec<Duration> = SEEDS
            .iter()
            .map(|&s| {
                let mut p = GenerateParams::planet(s);
                p.cell_count = cells;
                measure_render(&generate_full(p), Style::Planet)
            })
            .collect();
        row(
            &format!("{cells} cells"),
            samples,
            base,
            budget,
            &mut failures,
        );
    }

    // ---- refine_sector (parent generation untimed) -------------------------
    header("refine_sector — one zoom-in tile (level 2, target 4k)");
    {
        let (parent_cells, base, budget) = REFINE_BASELINE;
        // Three distinct sectors of one parent, so the samples vary.
        let sectors = [
            Sector {
                level: 2,
                sx: 1,
                sy: 1,
            },
            Sector {
                level: 2,
                sx: 2,
                sy: 1,
            },
            Sector {
                level: 2,
                sx: 1,
                sy: 2,
            },
        ];
        let parent = generate_full(gen_params(parent_cells, SEEDS[0]));
        let samples: Vec<Duration> = sectors
            .iter()
            .map(|&s| measure_refine(&parent, s))
            .collect();
        row(
            &format!("{parent_cells}→4k"),
            samples,
            base,
            budget,
            &mut failures,
        );
    }

    if check && !failures.is_empty() {
        eprintln!("\nREGRESSION — {} budget(s) exceeded:", failures.len());
        for f in &failures {
            eprintln!("  * {f}");
        }
        eprintln!(
            "\nIf the slowdown is intrinsic to a new feature, re-anchor by updating the \
             BASELINES constants in this file and the matching numbers in \
             docs/perf_baseline.md. Otherwise, profile and fix."
        );
        std::process::exit(1);
    }
}

fn header(title: &str) {
    println!("\n## {title}\n");
    println!("| case | s1 | s2 | s3 | min | median | mean | baseline | budget | status |");
    println!("|------|----|----|----|-----|--------|------|----------|--------|--------|");
}

fn row(
    label: &str,
    mut samples: Vec<Duration>,
    base_ms: u64,
    budget_ms: u64,
    failures: &mut Vec<String>,
) {
    let cols: Vec<String> = samples.iter().map(fmt_ms).collect();
    samples.sort();
    let min = samples[0];
    let median = samples[samples.len() / 2];
    let mean: Duration = samples.iter().sum::<Duration>() / samples.len() as u32;
    let median_ms = median.as_secs_f64() * 1000.0;

    let status = if median_ms <= budget_ms as f64 {
        "ok"
    } else {
        failures.push(format!(
            "{label}: median {median_ms:.0}ms > budget {budget_ms}ms (baseline {base_ms}ms × 1.5)"
        ));
        "OVER"
    };
    println!(
        "| {label} | {} | {} | {} | {} | {} | {} | {base_ms}ms | {budget_ms}ms | {status} |",
        cols[0],
        cols[1],
        cols[2],
        fmt_ms(&min),
        fmt_ms(&median),
        fmt_ms(&mean),
    );
}

fn fmt_ms(d: &Duration) -> String {
    format!("{:.0}ms", d.as_secs_f64() * 1000.0)
}

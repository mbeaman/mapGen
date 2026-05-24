//! Perf baseline harness for `generate_full`.
//!
//! Default — print measurements only:
//!     cargo run --release -p mapgen-world --example perf_baseline
//!
//! Regression check — also assert median ≤ baseline × 1.5, exit non-zero
//! on any violation:
//!     cargo run --release -p mapgen-world --example perf_baseline -- --check
//!
//! Times the full geography pipeline at 4k / 15k / 30k target cell counts,
//! three seeds per size. Prints a markdown-friendly table to stdout.
//! Numbers + budget live in `docs/perf_baseline.md`; the constants in
//! this file must stay in sync. Never invoked by `cargo test`.

use std::time::{Duration, Instant};

use mapgen_world::{generate_full, GenerateParams};

/// (target cell count, baseline median ms, 1.5× budget ms).
/// Updated when `docs/perf_baseline.md` is re-anchored.
///
/// Re-anchored 2026-05-24 (Phase 4j) to the current dev box — ~1.6× slower than
/// the original Ryzen 9 5950X anchor (now gone) and now including the full
/// Phase-4 history sim. Prior Ryzen anchor was 17/66/133 ms.
const BASELINES: &[(usize, u64, u64)] = &[(4_000, 26, 39), (15_000, 111, 166), (30_000, 259, 388)];

const SEEDS: &[u64] = &[1, 2, 3];

fn measure(cell_count: usize, seed: u64) -> Duration {
    let params = GenerateParams {
        seed,
        cell_count,
        ..Default::default()
    };
    let start = Instant::now();
    // `black_box` both ends within the timed region: input opaque so the
    // call can't be const-folded across loop unrolling, output kept live
    // so the pipeline can't be elided. Standard criterion / divan bench
    // idiom — survives future LTO or aggressive-inlining changes that
    // today's cross-crate boundaries already happen to block.
    let _world = std::hint::black_box(generate_full(std::hint::black_box(params)));
    start.elapsed()
}

fn main() {
    let check_mode = std::env::args().any(|a| a == "--check");

    // One untimed warmup at the smallest size to prime caches / allocators.
    let _ = measure(BASELINES[0].0, SEEDS[0]);

    println!("# Perf baseline — generate_full");
    if check_mode {
        println!();
        println!("Mode: --check (budget = baseline × 1.5; non-zero exit on violation)");
    }
    println!();
    println!("| target cells | seed=1 | seed=2 | seed=3 | min | median | mean | baseline | budget | status |");
    println!("|--------------|--------|--------|--------|-----|--------|------|----------|--------|--------|");

    let mut failures: Vec<String> = Vec::new();

    for &(cells, baseline_ms, budget_ms) in BASELINES {
        let mut samples: Vec<Duration> = SEEDS.iter().map(|&s| measure(cells, s)).collect();
        let row: Vec<String> = samples.iter().map(fmt_ms).collect();
        samples.sort();
        let min = samples[0];
        let median = samples[samples.len() / 2];
        let mean: Duration = samples.iter().sum::<Duration>() / samples.len() as u32;
        let median_ms = median.as_secs_f64() * 1000.0;

        let status = if median_ms <= budget_ms as f64 {
            "ok"
        } else {
            failures.push(format!(
                "{cells} cells: median {median_ms:.0}ms > budget {budget_ms}ms \
                 (baseline {baseline_ms}ms × 1.5)"
            ));
            "OVER"
        };

        println!(
            "| {:>12} | {:>6} | {:>6} | {:>6} | {:>5} | {:>6} | {:>4} | {:>8} | {:>6} | {:>6} |",
            cells,
            row[0],
            row[1],
            row[2],
            fmt_ms(&min),
            fmt_ms(&median),
            fmt_ms(&mean),
            format!("{baseline_ms}ms"),
            format!("{budget_ms}ms"),
            status,
        );
    }

    if check_mode && !failures.is_empty() {
        eprintln!();
        eprintln!("REGRESSION — {} size(s) exceeded budget:", failures.len());
        for f in &failures {
            eprintln!("  * {f}");
        }
        eprintln!();
        eprintln!(
            "If the slowdown is intrinsic to a new feature, re-anchor the \
             baseline by updating the BASELINES table in this file and the \
             matching numbers in docs/perf_baseline.md. Otherwise, profile \
             and fix."
        );
        std::process::exit(1);
    }
}

fn fmt_ms(d: &Duration) -> String {
    format!("{:.0}ms", d.as_secs_f64() * 1000.0)
}

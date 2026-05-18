//! Perf baseline harness for `generate_full`.
//!
//! Run with:
//!     cargo run --release -p mapgen-world --example perf_baseline
//!
//! Times the full geography pipeline at 4k / 15k / 30k target cell counts,
//! three seeds per size. Prints a markdown-friendly table to stdout. Numbers
//! are copy/pasted into `docs/perf_baseline.md` and serve as the regression
//! budget reference. Never invoked by `cargo test`.

use std::time::{Duration, Instant};

use mapgen_world::{generate_full, GenerateParams};

const SIZES: &[usize] = &[4_000, 15_000, 30_000];
const SEEDS: &[u64] = &[1, 2, 3];

fn measure(cell_count: usize, seed: u64) -> Duration {
    let params = GenerateParams {
        seed,
        cell_count,
        ..Default::default()
    };
    let start = Instant::now();
    let world = generate_full(params);
    let elapsed = start.elapsed();
    // Force-use the result so the optimizer can't elide the pipeline.
    std::hint::black_box(world.mesh.cell_count());
    elapsed
}

fn main() {
    // One untimed warmup at the smallest size to prime caches / allocators.
    let _ = measure(SIZES[0], SEEDS[0]);

    println!("# Perf baseline — generate_full");
    println!();
    println!("| target cells | seed=1 | seed=2 | seed=3 | min | median | mean |");
    println!("|--------------|--------|--------|--------|-----|--------|------|");

    for &cells in SIZES {
        let mut samples: Vec<Duration> = SEEDS.iter().map(|&s| measure(cells, s)).collect();
        let row: Vec<String> = samples.iter().map(fmt_ms).collect();
        samples.sort();
        let min = samples[0];
        let median = samples[samples.len() / 2];
        let mean: Duration = samples.iter().sum::<Duration>() / samples.len() as u32;

        println!(
            "| {:>12} | {:>6} | {:>6} | {:>6} | {:>5} | {:>6} | {:>4} |",
            cells,
            row[0],
            row[1],
            row[2],
            fmt_ms(&min),
            fmt_ms(&median),
            fmt_ms(&mean),
        );
    }
}

fn fmt_ms(d: &Duration) -> String {
    format!("{:.0}ms", d.as_secs_f64() * 1000.0)
}

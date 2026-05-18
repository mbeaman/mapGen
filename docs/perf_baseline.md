# Performance baseline — `generate_full`

Pre-Phase-3 reference point for the full geography pipeline (mesh → plates →
noise → erosion → hydrology → ocean → seasonal climate → biomes). Phase 3
will grow the pipeline (cultures, religions, polities, naming, ornate
render); these numbers exist so the cost of each new stage is visible
instead of hidden in compounding latency.

## Numbers

Captured 2026-05-17 on `claude/fantasy-map-generator-1du5B` at commit
`eacba95`. Release build, three seeds (1/2/3) per size, one untimed warmup
at the smallest size. Values below are median across three full runs of
the harness (so 9 worlds per size); seed-to-seed variance was ≤5% within a
run, run-to-run variance ≤5%.

| target cells | median | scaling vs. 4k |
|--------------|-------:|----------------|
|        4,000 |  17 ms | 1.00×          |
|       15,000 |  66 ms | 3.88×          |
|       30,000 | 133 ms | 7.82×          |

Roughly linear in cell count. 30k vs. 15k is 2.02× time for 2× cells; the
slight super-linearity at small sizes is the fixed cost of mesh
construction + Lloyd relaxation amortizing in.

### Environment

- CPU: AMD Ryzen 9 5950X (16 cores, 32 threads)
- OS: Linux 6.17.0-29-generic, x86_64
- Rust: 1.95.0
- Profile: `release` (workspace default)

## Regression budget

**A `generate_full` measurement is a regression if it exceeds 1.5× the
baseline median on the same hardware class.** Budget table:

| target cells | baseline | budget (1.5×) |
|--------------|---------:|--------------:|
|        4,000 |    17 ms |         26 ms |
|       15,000 |    66 ms |         99 ms |
|       30,000 |   133 ms |        200 ms |

1.5× was chosen to absorb normal CPU/run noise (~5% × normal hardware
spread × measurement count) while still flagging any single stage that
doubles or worse. Tighten to 1.3× once Phase 3 lands and the pipeline is
stable again.

This budget is **not** enforced by CI — the harness is run manually after
work that touches the science pipeline. Wiring it into a release-mode
`#[ignore]`d test or a Criterion bench is in `docs/BACKLOG.md` if the
budget starts getting violated quietly.

## Re-running

```sh
cargo run --release -p mapgen-world --example perf_baseline
```

Prints a markdown table to stdout. Compare against the baseline above. If
any size exceeds budget, identify which stage grew (`hyperfine` /
`cargo flamegraph` on the example binary), and either:

1. Optimize the offending stage back under budget, or
2. Update this baseline with a justification in the commit message
   explaining why the new cost is intrinsic to the feature (e.g., Phase 3a
   cultures adds a Voronoi-assignment pass — fine, but document it).

The harness lives at `crates/mapgen-world/examples/perf_baseline.rs`; it
imports nothing CI-relevant and never runs during `cargo test`.

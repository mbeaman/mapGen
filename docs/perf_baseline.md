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

The budget is **not** in CI; the harness is run manually after work that
touches the science pipeline. It is, however, self-checking — see
"Re-running" below — so a stray `cargo run … -- --check` in a local
gate or developer workflow is enough to catch a regression.

## Re-running

```sh
# Print measurements only:
cargo run --release -p mapgen-world --example perf_baseline

# Self-check: also assert median ≤ budget, exit non-zero on violation:
cargo run --release -p mapgen-world --example perf_baseline -- --check
```

Both modes print a markdown table to stdout with a `status` column
showing `ok` / `OVER` per size. `--check` adds a stderr summary and exits
1 on any violation. The baseline + budget constants live in
`BASELINES` at the top of
[`crates/mapgen-world/examples/perf_baseline.rs`](../crates/mapgen-world/examples/perf_baseline.rs)
and must stay in sync with the table above.

If any size exceeds budget, identify which stage grew (`hyperfine` /
`cargo flamegraph` on the example binary), and either:

1. Optimize the offending stage back under budget, or
2. Update both the `BASELINES` table in the harness *and* the numbers
   in this doc, with a commit message explaining why the new cost is
   intrinsic to the feature (e.g., Phase 3a cultures adds a Voronoi-
   assignment pass — fine, but document it).

The harness imports nothing CI-relevant and never runs during `cargo
test`.

## WASM bundle size

Browser distribution is the constraint here — `mapgen-wasm` ships as a
single `.wasm` blob plus a JS shim. ARCHITECTURE.md set a 3 MB budget;
this is the pre-Phase-3 baseline. Phase 3a adds a CSV plus new enums,
which is the first time growth is structurally expected; tracking it now
makes that growth visible instead of cumulative-and-silent.

| artifact                                    | size      | budget | status |
|---------------------------------------------|----------:|-------:|:------:|
| `target/.../release/mapgen_wasm.wasm` (raw) | 152 KB    | 3 MB   | ok     |

Captured 2026-05-17 on commit `43d4e4a`, target `wasm32-unknown-unknown`,
profile `release` (workspace default — `opt-level = 3`, no `lto`, no
`strip`). No `wasm-opt`, no `wasm-strip`, no `--no-default-features` pass
applied; this is the raw `cargo build` artifact a CI pipeline would
produce. We are at ~5 % of budget pre-Phase-3.

### Re-measuring

```sh
cargo build -p mapgen-wasm --target wasm32-unknown-unknown --release
ls -la target/wasm32-unknown-unknown/release/mapgen_wasm.wasm
```

If the artifact crosses 1 MB, set up `twiggy` to attribute bytes by
function / crate, and consider enabling `lto = "fat"` + `wasm-opt -Oz`
before re-anchoring the budget. The `twiggy` integration is tracked in
session state as "WASM bundle size profiling" — not on BACKLOG yet
because the trigger (size > 1 MB) hasn't fired.

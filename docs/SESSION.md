# Session state

**Resume entry point.** Read this first to understand where the
project is right now, then read `AGENTS.md` (if you're an agent) or
`CONTRIBUTING.md` (if you're a human) for process. This file changes
fast; the others are stable.

---

## Latest state

| Field | Value |
|---|---|
| Branch | `claude/fantasy-map-generator-1du5B` |
| Latest commit | `bfcdb5b feat: live stage-by-stage generation build-up via resumable Pipeline` |
| Tree | clean, synced to origin |
| Tests | 129 across 31 test binaries, 0 failures |
| Gate | fmt + clippy clean, wasm release builds |
| Schema | v6 |
| Architecture | LOCKED 2026-05-17 (§5.5 + Phase 2.5 require explicit user approval + trigger) |

---

## Phase status

| Phase | Status | Anchor commits |
|---|---|---|
| 2 (geography pipeline) | done | through `eacba95` |
| 2.5 (perf + sweep CLI) | done | `cad82b7`, `0d44aa3`, `eacba95`, `d4caefa` |
| 3a cultures | done | `98c7422` (skel) → `0080cf0` (spec) → `5ac66af` + `1451d5f` + `aca2fed` |
| 3b religions | done | `bdfeeb7` (skel) → `b0ff362` (spec) → `c94e856` |
| 3c polities | done | `e4d4c35` (skel) → `59e84bf` (spec) → `48957b8` |
| 3d naming | done | `9aedddb` (skel) → `6ba0ff6` (spec) → `c74089f` |
| 3e ornate render | **done per architecture spec** | MVP `342f606` + labels `8125c41` + glyph dispatch `dc6db51` + typography `90567ff` + compass/cartouche/edge-burn `65ab468` + roughr coastlines `b1815b2`. Only `docs/target_aesthetic.svg` deferred (BACKLOG, revival trigger: starting a new style variant). |
| 3e backlog polish | open | 11 ideas catalogued in `docs/BACKLOG.md` Render section, each with revival trigger. None required to call 3e "done." |
| web frontend | **shipped (MVP)** | wasm split `8916303` + Vite/TS scaffold `98ba3de` + worker/pan-zoom/theme/export `103be12` + live stage build-up `bfcdb5b`. Setup: install Node 18+/npm, then `just web-setup` (handles wasm-pack + deps + first build), `just web-dev` to run. `scripts/bootstrap.sh` is Rust-core only. See `web/README.md`. |
| 4 history sim | not started | — |

---

## Recently shipped (most recent first)

```
bfcdb5b feat: live stage-by-stage generation build-up via resumable Pipeline
103be12 feat(web): worker + pan/zoom + parchment theme + SVG/PNG/permalink export
98ba3de feat(web): scaffold Vite + TypeScript frontend driving the wasm-pack output
8916303 feat(wasm): split generate/render with WorldHandle so style switching is cheap
0f50dc7 docs(backlog): defer repository doc top-tier polish pass
8e340da docs: promote resume context to first-class tracked docs
b1815b2 feat(render): roughr pen-jitter coastlines + 4th ripple
8efb276 docs(backlog): catalog Phase 3e polish ideas surfaced this session
```

Regenerate this list when stale:

```sh
git log -8 --oneline
```

---

## Currently in flight

Nothing. Phase 3e (per architecture spec) and the web-frontend MVP are
both complete. Next direction is the user's call:

- **Phase 4 (history sim).** Six causal loops (Turchin secular cycles
  + Khaldun dynasty decline + Mearsheimer offensive realism +
  Succession crises + Schism splits + Hero events). 500-year
  deterministic agent-based sim. Reads cultures + religions +
  polities; writes to `WorldData.events` (already in schema, empty).
  ARCHITECTURE.md estimates ~2 weeks of focused work.

- **3e backlog polish.** Pick from `docs/BACKLOG.md` Render section.
  ~6–8 days total if shipped end-to-end.

- **Web frontend hardening.** The MVP shipped (generate/render/style/
  pan-zoom/export/permalinks). Setup is now codified in `just web-setup`
  / `web-build` / `web-dev`, but it still has no automated tests and no
  CI step. Candidate consolidation work if the frontend becomes
  load-bearing.

- **Other.** Bug fixes, dep bumps, or anything else.

---

## Known gotchas

Specific traps that have bitten work before. None are bugs to fix
(yet); each is a "watch out" with the rationale.

- **Float-determinism.** Route every transcendental through
  `mapgen_core::fmath::*`. Direct `f32::sin` etc. breaks the
  native ↔ wasm32 byte-identical contract.

- **Schema bump → golden hash re-anchor required.** Done 4× during
  Phase 3 (v2→3→4→5→6). Skip the re-anchor and
  `phase2_spec::golden_hash_native_matches_committed_value` fails.
  See `CONTRIBUTING.md` § Schema changes.

- **ARCHITECTURE.md's pinned dep versions can be stale.** Pinned
  `resvg 0.43` (we're on 0.47), `roughr 0.6` (we're on 0.12). Verify
  upstream before adopting; bump the architecture pin if you find
  a current version that works.

- **Halfling polity drops on seed 42.** Their cells exist (104 of
  them) but no individual cell clears the 0.4 `CAPITAL_FITNESS_FLOOR`
  for the archetype, so polities skips creating a polity. Surfaces
  as "4 polities + 4 polity labels on seed 42 even though there are
  5 cultures." Stage interaction, not a bug. Workaround would be
  lower the floor or widen Halfling's habitat preference.

- **`mapgen_render::render` returns `Result<String, String>`.** All
  implemented styles return `Ok`; the type is retained so future
  un-implemented variants can return `Err`. CLI propagates with
  `.map_err(anyhow::Error::msg)?`.

- **`world.cultures.cultures[i].name` is the CSV exonym** (e.g.,
  "Riverfolk"). Phase 3d's naming stage doesn't rewrite it — it
  generates new names for settlements / polities / religions but
  keeps the culture name. Intentional. If you want endonyms,
  that's a separate pass.

- **roughr 0.12 has a Move-as-L bug.** `OpType::Move` is serialized
  as `L` (lineto) instead of `M` (moveto). Worked around in
  `ornate_antique::roughr_path_with_move_fix`. Pinned by
  `svg_invariants::ornate_antique_coastline_paths_have_explicit_moveto`.

- **roughr on wasm32 needs `getrandom = { features = ["js"] }`** in
  `mapgen-wasm`. Otherwise `cargo build --target wasm32-unknown-unknown`
  fails on the upstream `wasm*-unknown-unknown` compile_error in
  getrandom 0.2.

---

## File entry points

Where to look when working in a given area:

- `crates/mapgen-world/src/lib.rs` — `generate_full_with` pipeline
  (the full mesh → naming sequence).
- `crates/mapgen-world/src/{cultures,religions,polities,naming}.rs` —
  per-substage implementations.
- `crates/mapgen-world/tests/*_spec.rs` — contract pins per phase.
- `crates/mapgen-render/src/style/ornate_antique.rs` — the marquee
  render.
- `crates/mapgen-render/tests/svg_invariants.rs` — render contract.
- `docs/tuning_log.md` — knob values per stage.
- `docs/perf_baseline.md` — 17/72/149 ms perf budget (4k/15k/30k).

---

## Common commands

```sh
just check          # full validation gate (pre-push)
just test           # workspace tests only
just render-42      # generate + render canonical seed-42 ornate SVG
just render-42-png  # same render via sweep CLI's PNG path
just perf           # perf regression check
```

Inspect a generated world:

```sh
zcat /tmp/mapgen/w42.json.gz | jq -r '"Languages: " + ([.languages[].name] | join(", "))'
zcat /tmp/mapgen/w42.json.gz | jq -r '"Polities: " + ([.society.nations[].name] | join(", "))'
zcat /tmp/mapgen/w42.json.gz | jq -r '"Religions: " + ([.religions.religions[].name] | join(", "))'
```

---

## Updating this file

Update after every push that lands new commits, or whenever phase
status / gotchas / current focus shifts. The "Latest state",
"Recently shipped", and "Currently in flight" sections are the
fastest-moving — keep them accurate or the file loses its job.

Stable sections (gotchas, file entry points, common commands) move
slower; promote items to `CONTRIBUTING.md` or `AGENTS.md` if they
become part of the project's standing process rather than ephemeral
state.

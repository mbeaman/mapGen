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
| Latest commit | Phase 7 complete — refinement framework + drill-in nav + scale-dependent render fidelity (forests→cities) + seam-pinning + projected rivers + hardening. Run `git log -1` for the head. |
| Tree | clean |
| Tests | full workspace suite green (adds `scale_spec`, `soils_spec`, `rivers_spec`, `refine_render`, sector-seed + `refineSector` wasm tests) + `--features lore` suite + web `tsc --noEmit`/vite build clean. **`wasm-pack test --node` (native↔wasm golden + refineSector) green** — in CI + `just test-wasm`. |
| Gate | `just check` green (fmt + clippy -D warnings + tests + wasm release). `just perf` **green** (26/111/259 ms medians vs 39/166/388 budgets). |
| Schema | v15 (Phase 6: v14 6.1.4 `ClimateData::soil` + `biomes::WETLAND`, v15 6.1.5 `HydrologyData::strahler` + `River::{strahler,regime}`). `MeshData::region` (Phase 7) is elided when `None` → no schema bump, level-0 byte-identical. |
| Architecture | LOCKED 2026-05-17 (§5.5 + Phase 2.5 require explicit user approval + trigger). §2/§4 **amended 2026-05-24** to record the six-loop Phase-4 scope (trigger: the "time is not a factor" + "uplevel" directive) |

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
| 3e backlog polish | **done this session** | 9 polish items shipped: town-size scaling, mountain depth shadow, edge-burn stains, ocean hatching, polity borders, trunk/branch roads, per-pantheon sacred sites, river/lake names + Imhof SA labels, and mountain-range clustering + labels (schema v8). Only `target_aesthetic.svg` stays deferred (hand-drawn taste reference; user-deferred). |
| web frontend | **shipped (MVP)** | wasm split `8916303` + Vite/TS scaffold `98ba3de` + worker/pan-zoom/theme/export `103be12` + live stage build-up `bfcdb5b`. Setup: install Node 18+/npm, then `just web-setup` (handles wasm-pack + deps + first build), `just web-dev` to run. `scripts/bootstrap.sh` is Rust-core only. See `web/README.md`. |
| 4 history sim | **done (+ reviewed & upleveled, 4k)** | Full six-loop scope + uplevel (amends locked MVP; trigger 2026-05-24). Option-B wiring (History is a `PipelineStage`). A deep multi-agent review after 4j (verdict: substrate sound; presentation needed work) drove the **4k** pass — see "Currently in flight". `4a` foundation `87333ac` → `4b` Turchin demographic `…` → `4c` agents → `4d` Turchin fiscal + Khaldun `52c709f` → `4e` Mearsheimer wars `28de2c0` → `4f` succession `4c2348e` → `4g` schism `858e9d4` → `4h` hero/megabeast `26df4a3` → pre-4i hardening `ae5c90e` → `4i.1`–`4i.4` `66b7f17`/`ab119b0`/`980f461`/`532444c` → `4j` closer → `4k` review uplevel + polish. See `docs/TASKS.md` `## Phase 4`. |
| 6 realism + cross-cutting | **done** | 6.0 backlog housekeeping; 6.1.1 precip variance; 6.1.3 wider biomes; **6.1.4 USDA soils + WETLAND biome** (schema v14); **6.1.5 Strahler order + seasonal river regime, ephemeral rivers dashed** (schema v15); **6.2 cross-platform native↔wasm golden** — caught + fixed 2 real determinism bugs (usize `gen_range` in poisson, std `exp` in `band_precip`), fmath purity guard now workspace-wide, runs in CI; 6.3 navigation ADR (`docs/adr/0001`). |
| 7 multi-scale atlas | **done (2026-05-25)** | The keystone: `mapgen-world/src/scale.rs` (`Sector` quadtree + `refine_sector` — deterministic on-demand sector refinement; base field recomputed from the ROOT seed, detail + stateful stages from a per-sector seed; coarsening contract tested at 94–98% coastline agreement). **Society + river network projected** from the parent (towns/borders/roads + global drainage, seam-consistent). **Seam-pinning** (`pin_edges_to_shared`): neighbours agree on elevation/coast/rivers at shared edges (tested MAD < 0.04). **Scale-dependent render fidelity** (ornate LOD, gated on zoom so level-0 is byte-identical): forests→layered trees, ridged mountains, screen-consistent coastlines, settlements→town clusters→full city plans (walls/streets/quarters/market/harbour/wards). `mapgen-wasm::refineSector` (method) + `mapgen refine` CLI + browser drill-in (click-to-zoom, coarse-first focus, breadcrumb). Hardened: robustness sweep across all sectors + wasm error path. Open follow-ups (BACKLOG): planet view (zoom-out) + generalisation, prefetch. |
| 5 lore engine | **done (5a–5f) + reviewed & remediated; opt-in** | The Claude narration engine, offline-first. `5a` lore types + template narrator `9c88606` → `5b` prompt assembly `060b3ed` → `5c` NER validator + `narrate()` + Work persistence `0453cfc` → `5d` `mapgen lore` CLI `2df3915` → `5e` real Anthropic client behind `--features lore` `6979e38` → polish (title + NER stoplist) `8d0ce02` → `5f` `mapgen serve` sidecar `3a8e003` + frontend narrate button `e1e1d69`. Opt-in: a stock build makes no paid call; `mapgen lore` runs the offline template narrator, Claude lights up only with `--features lore` + `ANTHROPIC_API_KEY`. **Live API call + browser click are unexercised in-sandbox** (no network/browser) — verify with a key; the sidecar was curl-verified and the frontend typechecks. A 4-agent deep review then drove remediation (`d42bb07`…`32e90ff`): NER lexicon blocker (mountain ranges) + validator bypasses (title/hyphen/digit/curly) fixed; request timeouts added; engine-populated lacunae + `max_calls_per_world` cap; CORS scoped to localhost + body cap; serve/register/serde tests. |

---

## Recently shipped (most recent first)

```
(HEAD) P5 review — hardening + test coverage (CORS, body cap, stop_reason, tests)
87711bd P5 review — engine-populated lacunae + per-world budget cap (§7)
0c3742c P5 review — request timeouts (Anthropic client + frontend fetch)
d42bb07 P5 review — NER soundness (lexicon blocker + validator bypasses)
e1e1d69 5f — frontend "narrate" button → sidecar (completes Phase 5)
3a8e003 5f — `mapgen serve` narration sidecar (POST /narrate), opt-in
8d0ce02 polish(lore) — cleaner template title + broadened NER stoplist
6979e38 5e — real Anthropic client behind the opt-in `lore` feature
```

Regenerate this list when stale:

```sh
git log -8 --oneline
```

---

## Currently in flight

**Nothing mid-flight.** Phases 5 (lore), 6 (realism + cross-cutting), and 7
(multi-scale atlas — refinement, drill-in nav, projected society + rivers,
seam-pinning, scale-dependent render fidelity, hardening) are all complete and
pushed. Remaining follow-ups, documented in `docs/adr/0001-multiscale-navigation.md`
("Implementation status") and the BACKLOG: **planet / multi-continent view above
level 0** (zoom-out — also the prerequisite that makes cartographic
generalisation meaningful: Töpfer/Visvalingam + scale-rank label declutter);
rank-driven background prefetch; per-sector local history; the deferred Phase-8
scale-consumer bands (urban interiors, planet). None is mid-flight.

The full narration round-trip, built offline-first in `mapgen-lore`:
voice/register types, a tolerant `ChronicleDraft` parser, prompt assembly (WORLD
BIBLE + ENTITY CONTEXT + EVENT SLICE + VOICE + SCHEMA, split at the prompt-cache
boundary), the NER validator, `narrate()` (LLM draft → validate → one retry →
deterministic template fallback → persist a `Work`), the `mapgen lore` CLI, the
real `AnthropicClient` (blocking `ureq`, `cache_control: ephemeral`, behind
`--features lore`), the `mapgen serve` sidecar (`POST /narrate`, CORS), and a
frontend narrate button (worker serializes the world via `WorldHandle::worldJson`
and POSTs to the sidecar). Consumes the 4i.4/4k boundary API.

**Opt-in at both layers:** a stock `just check` build pulls in no network deps
and can make no paid call; `mapgen lore` / `mapgen serve` narrate via the offline
template, and Claude lights up only with `--features lore` + `ANTHROPIC_API_KEY`
(graceful downgrade otherwise).

**Verified here:** default `just check` green (198 tests); `cargo
{build,clippy,test} -p mapgen-{lore,cli} --features lore` green (ureq 2.12.1,
tiny_http 0.12); sidecar curl-verified (/health, POST /narrate, CORS preflight);
frontend `tsc --noEmit` clean with the regenerated `worldJson` binding.
**Unexercised in-sandbox** (no network/browser): the live Anthropic call and the
browser button click. To run for real: `export ANTHROPIC_API_KEY=…` then
`cargo run -p mapgen-cli --features lore -- lore --in world.json.gz` (or
`… serve` + the web button). The NER stoplist (`mapgen-lore/src/ner.rs`) is broad
but may want tuning against real model output — a false positive only triggers
the safe template fallback.

**Next major arc:** none defined past Phase 5. Candidates live in `docs/BACKLOG.md`
(multi-scale generation; cataclysm clock; alternative render styles; the deferred
cross-platform native↔wasm golden pin).

**Phase 4 (history sim) is complete** (4j closer + 4k review uplevel + polish).
Its boundary API (`ner_lexicon` / `entity_brief` / `arc_event_closure` in
`lore_api.rs`) is what Phase 5 reads.

What Phase 4 delivered (plan of record: `.claude/plans/cosmic-dreaming-comet.md`;
task list: `docs/TASKS.md` `## Phase 4`): a deterministic 500-year history sim,
wired as `PipelineStage::History` after `Naming` (Option B), running a
system-dynamics backbone (per-polity `SimState`) under six causal loops
(Turchin demographic + fiscal, Khaldun asabiyyah, Mearsheimer wars, succession,
schism, hero) over an agent layer (Characters/Houses/Dynasties, lineage, blood
feuds). Output: a causally-chained `EventLog` (~808 events on seed 42 across 24
kinds) + `HistoryData` (mythic ages + classified narrative arcs); wars mutate the
political map the renderer reads. Inspect via `mapgen events`.

- **4j (the closer):** `mapgen events` subcommand; doc roll-ups; perf re-anchored
  to this box (now green); the first salience recalibration (`STAKES_REF` 300 →
  2000).
- **Deep review + 4k uplevel (post-4j):** four independent fresh-lens agents
  audited Phase 4 across correctness/determinism/coherence/Phase-5-readiness.
  Verdict: the **substrate is sound** (all invariants hold across 7 seeds;
  determinism, fmath, schema back-compat, wasm all clean) — the gaps were in the
  *presentation* layer + one Phase-5 handoff. The **4k** pass (commits
  `669f174`…`d1a6197`) fixed them, keeping loop dynamics intact:
  - Phase-5 NER blocker: megabeasts are now named `Entity::Megabeast` (v12) so
    they enter `ner_lexicon`; a new acid-test scans every summary's proper nouns.
  - Salience: a novelty discount (`refine_salience`) demotes verbatim-repeated
    beats — seed-42 major reel 96 → 39, varied.
  - Arcs: hero cassettes now weave into HeroSaga arcs; HolyWar requires a real
    schism (4 → 1 on seed 42); new `ArcKind::Conquest` (v13); titles deduped with
    numerals; tighter bar. seed 42: 52 → 20 well-classified arcs.
  - Ages: classified relative to the timeline's average — motifs vary across
    seeds and "Golden" is reachable.
  - Hardening: `transfer_border_cells` panic-fix; `extract.rs` + closure unit tests.
  - Polish (follow-up commit `55b48b5`): hero-event phrasing variety; ~30% of
    beasts are unslayable "great wyrms" (standing menaces, not a 100% kill rate);
    per-motif age epithets (a grim seed reads "Founding / Shadow / Ruin / Woe").

Phase 3e (spec + polish), the web-frontend MVP, and the multi-scale BACKLOG
track are all complete. Deferred render item: a hand-authored
`target_aesthetic.svg` (taste reference; user-deferred).

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

- **`just perf` was re-anchored to this box at 4j (2026-05-24) and is
  green again.** The Ryzen 9 5950X anchor (17/66/133 ms) is retired; the
  current box is ~1.6× slower and the pipeline now includes the 500-year
  history sim, so the baseline is 26/111/259 ms (4k/15k/30k) with a 1.5×
  budget (39/166/388 ms). It is **no longer advisory** — treat an OVER as
  a real regression. Numbers live in `perf_baseline.rs` `BASELINES` +
  `docs/perf_baseline.md` (keep them in sync). `just check` is still the
  CI-enforced gate; `just perf` is run manually after science-pipeline work.

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
  (the full mesh → naming → history sequence).
- `crates/mapgen-world/src/{cultures,religions,polities,naming}.rs` —
  per-substage implementations.
- `crates/mapgen-history/src/lib.rs` — history `run` driver + `SimState`
  + sub-seeding; `loops/{turchin,khaldun,mearsheimer,succession,schism,
  hero}.rs` the six loops; `agent.rs` dynasties/lineage; `emit.rs` the
  `Emit` event builder; `extract.rs` arc/age extraction; `lore_api.rs`
  the Phase-5 boundary API.
- `crates/mapgen-world/tests/*_spec.rs` — contract pins per phase
  (`history_spec.rs` is the Phase-4 contract; `pipeline_spec.rs` holds
  the `seed42_full` golden).
- `crates/mapgen-cli/src/main.rs` — CLI; `events` subcommand reads the log.
- `crates/mapgen-render/src/style/ornate_antique.rs` — the marquee
  render.
- `crates/mapgen-render/tests/svg_invariants.rs` — render contract.
- `docs/tuning_log.md` — knob values per stage (incl. Phase-4 loops + salience).
- `docs/perf_baseline.md` — 26/111/259 ms baseline, 39/166/388 budget (4k/15k/30k).

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

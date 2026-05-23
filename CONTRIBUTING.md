# Contributing

How this project works as a development effort: setup, the validation
gate, commit conventions, test discipline, and the architectural
guardrails that keep changes coherent.

For AI assistants and agent collaborators, also read `AGENTS.md` —
it's the persona + interaction layer on top of this file.

---

## First-time setup

One command from a fresh clone (macOS / Linux):

```sh
./scripts/bootstrap.sh
```

Installs rustup (if missing) → installs `just` via cargo → adds the
wasm32 target → runs the full validation gate. Idempotent.

By hand, or on Windows:

1. Install [rustup](https://rustup.rs) — `rust-toolchain.toml` pins
   the channel; the right Rust version auto-installs on first
   `cargo` invocation.
2. Install [`just`](https://github.com/casey/just):
   `cargo install just`.
3. Bootstrap: `just setup`.
4. Verify: `just check`.

CI runs the same `just check` on every push and PR. If it passes
locally, it should pass on CI.

---

## The validation gate

Run before every push. Non-negotiable.

```sh
just check
```

Which runs:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p mapgen-wasm --target wasm32-unknown-unknown --release
```

If any step fails, fix the root cause; don't skip the check with
`--no-verify` or comment out the failing assertion. `cargo fmt` was
missed on the very first push of this project, and that single lapse
required a follow-up cleanup commit. Don't repeat the lesson.

---

## TDD discipline

Every feature begins with a failing test. First commit of a phase
or substage is the spec; the last commit makes it green.

### Substage shape (new generation stage)

The full pattern, applied 4× during Phase 3 (cultures → religions
→ polities → naming):

1. **Commit A — Skeleton.** Types in `mapgen-core::entities` (new
   struct + enum if any) + new module file `mapgen-world/src/<phase>.rs`
   with `pub fn <op>(...) { todo!("...see <phase>_spec.rs") }` +
   `WorldData` field with `#[serde(default)]` (if any) +
   `SCHEMA_VERSION` bump + golden hash re-anchor +
   `pub mod <phase>;` in `mapgen-world/src/lib.rs`. Compiles, all
   existing tests still pass.

   **Don't put the spec file in the test tree before the module stub
   exists — `cargo test` won't compile.**

2. **Commit B — RED spec.** New file `crates/mapgen-world/tests/<phase>_spec.rs`
   with N populate-based tests (panic from `todo!()`) + one
   synthetic-world test (green today). Both use shared
   `check_*(world)` helpers so the assertion code runs in *some* test
   today and in *all* tests once the impl lands. Helper order:
   structural invariants first (vec lengths, valid indices), then
   ARCHITECTURE.md-quoted behavioral invariants, then a determinism
   pin.

3. **Commit C — Impl + wire.** Fills the function body, wires into
   `generate_full_with` in `mapgen-world/src/lib.rs` on its own
   `Stage::<X>` RNG sub-stream. Greens all RED spec tests. May
   re-anchor the golden hash again if pipeline output changes
   deterministically.

4. **Optional Commit D — docs roll-up.** README pipeline diagram,
   TASKS.md check-offs, tuning_log.md new section. Bundle with C if
   scope stays small.

### Validation-gaps audit (run after every skeleton)

Ask "how much have we truly validated this phase?" Common gaps:

- Assertion bodies inside RED spec tests never run (they panic
  upstream of the assertion). Fix by refactoring `#[test]` bodies
  into `check_*(&world)` free functions plus a
  `synthetic_world_satisfies_<phase>_contract` test.
- Serde round-trip untested → add a `#[cfg(test)]` test in
  mapgen-core that constructs a fully-populated instance, round-trips
  through JSON, asserts equality.
- Pre-version deserialization untested → unit test that deserializes
  a hand-written older-version JSON, asserts the new field defaults
  empty.
- `Stage::<X>` independence untested → extend
  `rng::tests::stages_are_independent` to include the new variant.

### Test assertions cite their source

When asserting physics or domain rules ("east coast warmer at
mid-latitude," "Christaller hexagons spaced by k"), put the source in
a doc comment. Lesson from the inverted-ocean-current bug: the test
passed *because the test had the wrong sign,* and the wrong sign
matched the bug. A test without a citation can co-evolve with a
broken implementation.

---

## Schema changes

Every `WorldData` shape change — even adding an empty
`#[serde(default)]` field — changes CBOR serialization and breaks
`phase2_spec::golden_hash_native_matches_committed_value`.

The fix is mechanical:

1. Bump `SCHEMA_VERSION` in `crates/mapgen-core/src/world_data.rs`.
2. Run the failing golden-hash test. It prints `left: "<new hash>"`.
3. Copy that hash into
   `crates/mapgen-world/tests/golden/seed42_phase2.blake3.txt`.
4. Always bump SCHEMA_VERSION in the same commit as the shape change.

Done 4× during Phase 3 (v2→3 cultures, v3→4 religions, v4→5
settlements, v5→6 languages).

---

## Commit conventions

### Two-commit pattern when concerns differ

Test infra and feature shipment land as separate commits when
bisect-friendliness matters. Bundle only when artifacts are
conceptually paired — e.g., the substage skeleton ships types +
module + schema bump + golden re-anchor together because they're a
single concern.

### Validate before drafting the commit message

Before composing any commit message:

1. Run the specific new/changed test(s) standalone and confirm green.
2. Run the full gate without pipe-truncating output.
   `just check 2>&1 | grep -E "FAILED|warning:|error\[|test result:"`
   surfaces problems without flooding context.
3. If the change is visible (UI, render, dev verb), run it
   end-to-end and confirm the artifact looks right (`just
   render-42-png` for ornate renders).
4. State the verification explicitly in the commit message body:
   what tests passed, what was visually verified, what's
   intentionally not tested.

### Commit message body

For non-trivial commits, the body should answer:

- What changed (succinct).
- Why this approach (decisions worth recording).
- What was verified locally (tests, gate, visual).
- What's NOT verified and why (out of scope, requires external system, etc.).

The PR description is for the merge audience; the commit body is for
git-bisect-future-you.

---

## Architectural discipline

### LOCKED status

`docs/ARCHITECTURE.md` has a `Status: LOCKED 2026-05-17` banner.
§5.5 (race-archetype utility table) and Phase 2.5 (sweep CLI +
property tests) require explicit user approval + a triggering signal
before edit. The lock exists because earlier proposals to expand
scope were caught and reversed during a multi-reviewer DA process.

### Backlog discipline

Deferred work goes to `docs/BACKLOG.md` with three required fields:

- Why deferred — the case for not doing this now.
- Trigger for revival — the concrete signal that would move it back
  into the active plan.
- Cost — rough order of magnitude.

If you can't name a trigger, the item probably shouldn't be on the
backlog at all. Cut it.

### Devil's advocate for major designs

When proposing a foundational decision (new pipeline stage,
significant dependency, schema change beyond field-add), also surface
the steelman against it. The Refinery SA-tuning loop was killed by a
three-reviewer DA process; that pattern remains the default for
substantial scope additions.

---

## Where to find further docs

- `README.md` — project overview, quick start, what works today.
- `docs/ARCHITECTURE.md` — locked strategic plan, phase definitions.
- `docs/TASKS.md` — active tactical work.
- `docs/BACKLOG.md` — deferred items with revival triggers.
- `docs/SESSION.md` — current state snapshot (resume entry point).
- `docs/tuning_log.md` — calibrated knob values per stage.
- `docs/perf_baseline.md` — perf budget and baseline.
- `AGENTS.md` — persona + interaction layer for AI collaborators.

## When to update this file

After any change to the validation gate, commit conventions, schema
process, or architectural-discipline rules. This file is the social
contract; keep it accurate but resist adding fine-grained
project-state — that's `docs/SESSION.md`'s job.

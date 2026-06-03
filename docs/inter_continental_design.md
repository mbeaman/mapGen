# Inter-continental civilization — design: "The Sundered Lanes"

> **Status: DESIGN (not yet built), 2026-06-01.** Produced by a multi-round design
> campaign (two workflows): 5 competing architectures × a 3-lens judge panel →
> a synthesis → a 3-person devil's-advocate panel + completeness critic →
> advisor reconciliation. This document is the convergent, DA-hardened design and
> the decision set for the user. It is NOT locked; it is the plan to build from.

> ## ✅ RESOLVED (2026-06-02) — premise fixed at the polity level; arc UNBLOCKED.
>
> The CRITICAL FINDING below (society spanned every continent at gen time) was
> **fixed**: the user chose to rework the foundation, and `cultures::populate` now
> **instances each culture per landmass** (schema v19, commit on branch). Probe
> confirms: polities spanning >1 landmass **3+ → 0** on planet seeds 11/19/7/4;
> sizable-body land controlled 92–100% (earned-sparse, NOT gutted); cultures
> 5 → 21–32, polities ~4 → 13–22 per planet. seed42 (single landmass) is a proven
> byte-identical no-op (goldens re-anchored for the version byte alone).
> **Polity/culture spanning — the carrier blocker — is dead.** So 3a's sea lanes
> are now load-bearing, and the arc can proceed to the carriers.
>
> **Honest residue (deferred, not a bug):** RELIGION still spreads globally at gen
> time (`religions::found` is a global spreader) — instancing confines polities,
> not faith. "A religion provably crosses water" is the Phase-2 Diffusion
> milestone, NOT this change; the later religion increment must reconcile gen-time
> confinement vs Diffusion's global-then-spread.
>
> ## ✅ CARRIER SHIPPED (2026-06-02) — earned cross-water conquest works.
>
> The **beachhead carrier** is built (`loops/mearsheimer.rs`): a war can now cross
> a sea lane the aggressor's culture has the `naval` tech to sail (`SimState.naval`
> per polity = max coastal-culture naval tech; lane gated by `min_naval`). On a win
> it seizes the loser's far-shore anchor cell as a beachhead, recorded as a
> `BorderChange` so it replays for free in the time-slider and on the planisphere.
> Proven on planet seeds: earned overseas holdings on 11(4)/19(3)/7(2)/4(1), and
> the "both-directions" property of the lanes (sundered on 23/42). seed42 goldens
> hold byte-identical (integer-only path = no-op there); the earned-crossing
> claim is now pinned in BOTH directions, mutation-verified, by
> `sundered_lanes_claims.rs` (`cross_water_conquest_fires_on_every_crossing_seed`
> + `no_cross_water_conquest_on_any_sundered_seed`). See `docs/CLAIMS.md`.
>
> **Deferred (named, not vague):**
> 1. **Exclave legibility / surfacing — THE gating work for a *visible* Phase 1.**
>    `polity_color` wraps **mod-5** over ~22 polities, so an overseas exclave is the
>    same color as some native same-color realm — the conquest is in the DATA and
>    replays in the slider, but it is NOT yet visually distinguishable on the
>    planisphere. The data replays for free; the legibility does not. Until a
>    polity has a stable, exclave-distinct rendering, do NOT claim "you can see the
>    sundering" — only "it animates in the slider." This is the next high-value item.
> 2. ~~**Colonization carrier (`from:None` far-shore claim).**~~ **DONE** —
>    `loops/colonization.rs` (`LoopId::Colonization`): a polity settles an
>    unclaimed lane far-anchor (`from:None` overseas `BorderChange`), the sibling
>    of the beachhead's `from:Some` conquest. A re-probe of the post-foundation
>    world found it fires on seeds 2/5/9/11/18 (persistently-unclaimed far
>    anchors), not just 11. Pinned both-directions + mutation-verified
>    (`colonization_settles_unclaimed_far_shores_and_nowhere_else`). Own RNG
>    stream → seed42 no-op (goldens hold). **Phase 1's carriers are complete.**
> 3. **Land-predicate unification.** cultures (`>=0.0`) vs sea_lanes (`>0.0`)
>    divergence is pinned by a tripwire (`no_culture_instance_spans_a_sea_lanes_body`,
>    passes trivially today — no cell sits at exactly 0.0). Unify on one canonical
>    body primitive before it bites.
>
> ---
>
> ### The original finding (kept for the record):
>
> After shipping Phase 1 Step 3a (the isotropic sea-lane substrate), a probe of
> the *consumer* (which the design campaign and 3a never checked) found that
> **society already spanned every continent at gen time — there was nothing for the
> lanes to gate.**
>
> - **Root cause:** `crates/mapgen-world/src/polities.rs:145-148` stamps
>   `control[cell] = polity_id` for *every cell whose `culture_id` matches the
>   polity's founding culture*. Cultures are assigned **globally by habitat**
>   (a culture occupies matching-habitat cells on *all* landmasses), so each
>   polity controls all of its culture's cells across every continent. Land wars
>   cannot cross water, so the spanning is purely gen-time.
> - **Probe (planet seeds, `connected_bodies` landmasses):** seed 11 → 3 of 4
>   polities span ALL 8 landmasses (`[8,8,8,3]`); seed 19 → `[6,6,6,1]`; seed 4 →
>   `[4,4,3,4,0]`. And every inter-body lane anchor is owned at every sampled year
>   (`bothNone=0, oneOwned=0`) — far shores are never unclaimed, so even the
>   `from:None` colonization carrier has nothing to claim.
> - **Consequence:** the "earned / sundered" meaning the whole arc is built on does
>   not exist. The carriers, diffusion, first-contact, and plague all read as
>   *nothing* when every polity is already everywhere. 3a's substrate is correct
>   but **consumer-less**; the design's own "The problem" section below (which
>   claims interactions "stop at `elevation < 0`") is **factually wrong** for the
>   polity-control assignment.
> - **The real prerequisite (the multi-week change the design flagged but
>   mis-scoped):** society must be **landmass-distinct** at the FOUNDATION —
>   per-landmass capitals/cultures so each continent has its own polities — for any
>   "earned crossing" to mean anything. A surgical "confine control to the
>   capital's landmass" is a trap: ~4–5 capitals sit on a few landmasses, leaving
>   most of the planet uncontrolled; a *populated* per-landmass world requires
>   editing `pick_capitals` (the cultures/capitals foundation), which re-anchors
>   every golden and reshapes the whole society layer.
> - **DECISION (user's):** (A) commit to landmass-distinct society — makes the arc
>   real, multi-week, all goldens move, 3a becomes load-bearing; or (B) shelve the
>   arc, keep the sea lanes as pure geography / a future trade-overlay (society
>   "by habitat, globally" is a legitimate worldgen choice), and redirect.

## The problem

A planet is **one** `generate_full` world over a single ~18k-cell mesh, and its
society + history already run planet-wide — but **every interaction is
land-adjacency-only** (`mesh.neighbors`). Cultures (Voronoi over land), polities
(BFS over land), religions (attach to cultures), and history (polity adjacency
from `mesh.neighbors` → Mearsheimer wars) all stop at `elevation < 0`. So the
ocean is a hard wall and **continents are isolated civilization pools inside one
world.** Goal: make the planet a *genuinely interconnected, earned* civilization
web a curious player can trace — without breaking determinism or the time-slider.

## Code-verified facts (the load-bearing reality — established by the campaign)

1. **The 0-cell-war trap.** `transfer_border_cells` (mearsheimer.rs:392) builds
   its frontier from {loser cells bordering a *winner*-controlled mesh neighbor};
   across an ocean that set is empty, so a naive sea-edge war moves **zero cells**.
2. **`naval` is never mutated during the 500-year sim** (history reads only
   `tech.agriculture`/`military`). So "naval grows yearly until a lane opens" is
   *invented machinery* — capability-over-time must be **spatial**: a capable navy
   reaches more lane targets and settles/fights its way to a far coast.
3. **`ocean.rs:76` computes the gyre current sign then discards it;
   `climate::wind_vector(lat_norm)` exists** (climate.rs:200, `pub(crate)`). A
   wind+current sea-lane cost is *persisting already-derived physics* (modest
   derivation — the gyre value is a scalar sense, not a 2D vector; reconstructing a
   tangential direction is real but small work). `fmath::atan2` exists, but
   dot-products are cheaper and avoid the transcendental.
4. **`TradeRouteOpened` + `EmbargoImposed` EventKinds already exist but are
   unused** (event.rs:44-45).
5. **The payoff has no surface today.** The narrator's `select_focal` is
   *war-first categorical* (narrates a contact event only if zero wars exist — i.e.
   never); the time-slider renders **control only**, no event markers; the
   arc-weaver `extract_arcs` drops components with < 3 salient events. A dated
   contact event therefore reaches no player surface as-is.
6. **`refine_sector` (the drill — the *primary* interaction) projects end-state
   `control` but NOT `history.border_changes` or events.** So the time-slider /
   chronicle payoff is planisphere-only unless drill projection is added.
7. **Max achievable `naval` is 40** (the 5-archetype roster: Riverfolk 40,
   Halfling 25, Elf 20, Orc 15, Dwarf 5; ceiling read by nothing today). `naval`
   lives on `Culture`, not per-polity.
8. **No `is_planet` flag.** `planet()` and the default continental world share the
   identical pipeline; a new stage in `PipelineStage::ORDER` runs for *every* world.

## Core architecture

One immutable **sea-lane substrate**, computed once at gen time (right after
Ocean), queried at two capability thresholds, feeding cross-water reach into the
*existing* `border_changes` channel so the time-slider replays it for free.

### The substrate (geography, no society)
- A new `Stage::SeaLanes` (rng append-only) + `PipelineStage` after Ocean.
- Revive the discarded gyre sign + `wind_vector` into a per-sea-cell flow field
  (a 0.6 current / 0.4 wind blend, normalized; **not persisted** — see Perf).
- Deterministically downsample the coast into **anchors** (greedy keep one per
  ~`r_anchor` geodesic radius, ascending cell id — the one hand-set knob, never
  auto-tuned). Anchors precede Cultures/Polities, so they're pure geometry.
- Anisotropic Dijkstra over the ~15k-node *sea* subgraph: stepping `u→v` costs
  `dist(u,v) · (1 − K·dot(unit(v−u), flow[u]))` clamped — downwind/with-current is
  cheap, beating upwind is dear. PQ keyed `(cost.to_bits(), cell_id)`; `fmath`
  throughout; BTreeSet/lowest-id tiebreaks. → a sparse `Vec<SeaLane{a,b,cost}>`
  (a<b, canonical-sorted), each tagged with a **relative** `min_naval` (see below).
- **Dual-filter** dissolves the chicken-egg with no iteration: `passable(lane, q)
  = q ≥ lane.min_naval`. Gen-time diffusion queries at a low Neolithic-strait
  threshold; history queries at the polity's `naval` (derived as the **max over
  its controlled coastal cultures**, not just the capital, so a conqueror inherits
  a seafaring coast's reach).

### The solvability fix (the blocking DA finding — relative, not absolute)
The danger: `naval ≤ 40` but absolute `quantize(cost)` could put every
inter-continental lane at `min_naval ~90` → **no crossing is ever possible**; the
arc builds and never fires. **Fix: derive `min_naval` from the *achievable naval
distribution*, not an absolute scale** — calibrate so the cheapest tier of
inter-body crossings is reachable by the *top* of the naval range and the dearest
tier by no one. Then a **non-empty crossable set AND a non-empty wall set exist by
construction, on every seed.** Step 0 of the build is a throwaway calibration
diagnostic that prints, per canonical seed, every inter-`connected_bodies` lane's
cost vs the achievable naval range — proving both sets non-empty *before* any
carrier code. A rare **seafarer archetype** (naval ~75) is worth adding for
*soul* (the unusual maritime culture is "who earns the crossing"), but solvability
must not depend on it.

**Step-0 result (measured 2026-06-01, de-risks the arc).** A throwaway probe of
narrowest inter-`connected_bodies` coastal gaps on canonical planet seeds (W=2048):
seed 4 → `[32, 219, 237, 341, 419, 991]`; seed 7 → `[31, 216, 420]`; seed 11 →
`[54 … 1116]`; seed 19 → `[19 … 1521]`; seed 42 → `[479]`; seed 23 → `[138, 382,
1189]`. Clear straits (~20–70) **and** clear open-ocean walls (~400–1500) coexist,
so the relative calibration has a non-empty crossable set AND wall set by
construction. **Canonical both-directions seeds = 11 and 19** (six continents,
guaranteed both), with 4 and 7 as secondary (a strait + a wall); **42 and 23 are
legitimately sundered** (no clear strait) — the seed-contingency, not a bug.
Calibration target: map sea-path cost → `min_naval` so gaps ≲ ~100 world-units are
reachable by the top of the naval range (Riverfolk 40) and gaps ≳ ~400 are
reachable by none; a seafarer archetype (naval ~75) extends the reachable band
(crosses what others can't). NOTE these are straight-line proxies; the real lanes
use anisotropic sea-path cost (≥ straight-line), so the spread only widens.

### The two carriers (cross-water reach = BorderChanges)
- **The adjacency fold.** Mearsheimer's war-target enumeration also includes any
  polity reachable over a `passable` lane (attacker controls a cell near `lane.a`,
  target near `lane.b`). This is capability-as-spatial. Dynastic claims + wars of
  religion cross water for free (they already iterate adjacency).
- **Beachhead-first conquest** (beats the 0-cell trap): on a won cross-ocean war,
  seize the loser-side anchor cell *first* as an explicit `BorderChange`, creating
  an exclave that borders loser land, *then* run the unmodified
  `transfer_border_cells`. The colony grows by normal land BFS in later wars.
- **`from:None` colonization** (the second carrier): a polity with spare capacity
  + naval reach to an *unclaimed* far anchor flips it `BorderChange{from:None,
  to:founder}` and writes the founder's `culture_id`/`religion_id` there —
  carrying culture + faith to a distant shore, replayed by `control_at_year` free.

### Diffusion + dated events (the watchable soul)
- A new `LoopId::Diffusion` (append-only; discriminant = the enum's actual next
  value) walks `passable` lanes each year: religion conversion along lanes; trade
  via the **dormant `TradeRouteOpened`** feeding `state.capacity` (so trade makes
  realms grow through the existing Turchin loop); `EmbargoImposed` on hostiles.
- **Dated first contact**: the first year a lane bridges two never-linked
  *regions* (a `connected_bodies` component with ≥1 controlled cell) emits a
  high-salience dated event — "the year the worlds met."
- **First-contact plague**: severity scales with isolation depth; **recontact
  immunity** (fires once per pair) — the Columbian shock as an *earned* consequence
  of long isolation, not a random event.

### Surfacing (the second blocking finding — "emit" ≠ "see")
The contact payoff is invisible at all three surfaces, so **surfacing is part of
the work, not a free byte**: slider **tick-marks** at event years (driven off the
log); a `select_focal` contact path + click-a-marker-to-narrate (replacing the
hardcoded `auto-major-war`); **`cause_ids`** linking contact→plague→colonization so
`extract_arcs` clusters a visible "First Contact" arc (≥3 events); the **lane layer
auto-ON** from the first cross-water event; and **cross-water battle prose** ("the
host crossed the Sundering Sea") so an overseas campaign reads as story, not noise.

## Honest truths to state, not bury
- **Maritime history is seed-contingent.** Because `naval` is static and a
  crossing needs a capable culture near a crossable lane, *some planets get rich
  contact/colonization and some stay sundered* — which is **earned** (real
  continents stayed isolated for millennia). The both-directions property test
  runs on **chosen canonical seeds** known to exhibit both, never random ones.
- **The both-directions test (anti-Goodhart, keeps §5.5 cut).** Assert on chosen
  seeds: ≥1 **earned** crossing (an actor that reached the lane via a prior land
  war or settlement, not gen-time capital adjacency) AND ≥1 **missed** connection
  (a capable polity on a shore that still can't cross in 500y). Auto-tuning the
  geometry knobs toward a crossing count is the exact Refinery-Goodhart trap that
  was rightly cut — **§5.5 stays cut**, justified by "auto-tuning is Goodhart," not
  by "the test passes."

## Determinism + perf
- Schema bump per phase + re-anchor the 3 goldens (`seed42_full` moves for
  content; `phase2`/`sector` for the version byte). Extend the rng
  collision-guard arrays to the new stages. f32-in-heap keyed on `to_bits()`.
- **Do not persist the flow field** (~144KB/world shipped to the browser): it's a
  pure function consumed only by lane construction — compute and drop, or
  `#[serde(skip)]` + recompute. Add `perf_baseline` rows for the SeaLanes stage +
  the Diffusion loop (the project gates perf manually).

## Phasing (each independently testable; first *user-facing* milestone is visible)
- **Phase 1 — visible maritime politics** = the substrate (P0) + both carriers +
  the *minimum surfacing* (lane auto-ON on first crossing, cross-water prose, a
  slider marker). Ship-gate: an earned far-shore exclave/colony you can **see** on
  the planisphere + scrub to. (P0 alone is NOT a release — a consumer-less
  substrate is the project's own displacement-activity pattern.)
- **Phase 2 — the watchable web** = the Diffusion loop + dated first contact +
  first-contact plague + trade→Turchin + the full narrator/arc surfacing.
  Ship-gate: a player reaches the contact narration in ≤2 clicks; a "First
  Contact" arc forms; a religion/plague provably crosses water.
- **Phase 3 — shared ethnicity (DEFERRED, heavy)** = two-pass culture placement so
  a culture spans water at *gen* time (the only piece needing the substrate before
  Cultures; ripples through Religions/Polities/Naming/History). AND-gated trigger
  in BACKLOG: P1+P2 shipped AND the both-directions test passes land-only AND a
  concrete logged user request.

## Decisions for the user (with recommendations)
1. **Scope depth** — Phase 1 only (visible colonies/exclaves), through Phase 2
   (watchable contact/plague/diffusion — the full soul), or include Phase 3?
   *Rec: commit to Phase 1, then Phase 2; keep Phase 3 deferred.*
2. **Payoff surface** — project `border_changes`/events into `refine_sector` so
   the **drill** view replays contact (more work), or **planisphere-only** first?
   *Rec: planisphere-only first — the cheaper honest cut; add drill projection if
   it proves worth it.*
3. **Seafarer archetype** — add a 6th, rare, high-`naval` archetype (data/CSV row,
   not a §5.5 edit)? *Rec: yes, for flavor — but calibrate solvability relatively
   so it doesn't depend on the archetype.*
4. **`is_planet` gating** — needed regardless (else every continental world gains
   maritime history). Add an `is_planet`/scale flag, or gate on "≥2 sizable
   `connected_bodies`"? *Rec: an explicit scale flag; P0 may run for all worlds.*

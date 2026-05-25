# Design influences & comparative positioning

*Snapshot at Phase 4k (2026-05-24). A reference for where the history pipeline
sits relative to prior art, and the scope choices that put it there. Revisit when
the simulation grows or when Phase 5 (LLM narration) changes what the log is for.*

## Purpose

This is not a spec; it's a map. When someone asks "is this like Dwarf Fortress?"
or "why don't we track individual figures the way DF does?", the answer is here.
It records both the lineage we drew on and the *deliberate* places we stopped
short, so a future contributor can tell a scope choice from an oversight.

## TL;DR

Phase 4 is a **hybrid system-dynamics + agent-based historical generator with an
LLM-handoff distillation layer**. That combination is unusual:

- **Dwarf Fortress** is the depth gold-standard — vastly more granular and
  content-rich than us, but its worldgen is mostly **agent + event-rule** driven,
  not macro-social-dynamics driven.
- **RimWorld** is a *different paradigm* and the weakest analog: its "narrative"
  is a **runtime incident scheduler**, not a pre-generated chronicle.
- The closest real cousins are **Crusader Kings** (our agent/dynastic layer) and
  **Caves of Qud** (its procedural mythic-history generator).

One-sentence placement: **a small, deterministic, social-science-grounded
chronicle generator built to feed an LLM — CK's dynastic logic and Qud's
mythic-history distillation, run on a Turchin/Khaldun/Mearsheimer engine, at a
fraction of Dwarf Fortress's depth.**

## What we actually built (the baseline this compares against)

- A deterministic 500-year sim wired as a `PipelineStage` after naming.
- **System-dynamics backbone:** per-polity state vectors — population, carrying
  capacity (biome-derived), aridity, elites, fiscal health, instability,
  asabiyyah, relative power.
- **Six causal loops:** Turchin (demographic + fiscal secular cycles), Ibn
  Khaldun (asabiyyah rise/decay), Mearsheimer (power-transition wars),
  succession, religious schism, hero/megabeast.
- **Agent layer:** dynasties, houses, characters (rulers, consorts, heirs,
  champions), lineage, adult-first + contested succession, blood feuds, titles,
  dynastic claims as casus belli.
- **Uplevel / distillation layer:** a causal DAG (`cause_ids` with a validated
  effect→cause grammar), a salience model with a verbatim-repeat novelty
  discount, narrative-arc extraction (union-find → classified HolyWar / HeroSaga /
  DynasticConflict / Conquest / Chronicle, titled and disambiguated), mythic ages
  classified relative to the timeline, and a Phase-5 boundary API (closed NER
  lexicon, `entity_brief`, transitive arc closure).
- **Scale on the canonical seed:** ~24 event kinds, ~800 events, 4 polities,
  ~20 narrative arcs, 4 mythic ages.
- **Determinism contract:** per-stage seeded ChaCha8 with hierarchical
  sub-seeding (adding a loop can't perturb another's stream); all transcendentals
  routed through a shared `fmath` so the sim is byte-identical native↔wasm;
  pinned by golden hashes.

## Dwarf Fortress (legends mode)

The obvious comparison, and the one where we're clearly the smaller system.

**What DF does that we don't:**

- **Individual-figure granularity.** DF tracks thousands of named historical
  figures — not just rulers but warriors, poets, criminals, necromancers — each
  with kills, relationships, careers, worship. We mint only the *load-bearing*
  cast (rulers, consorts, heirs, champions): hundreds of entities, not thousands.
- **Hundreds of event types** and deeply nested content: artifacts with engraved
  images and full theft/ownership provenance, procedurally-described forgotten
  beasts, vampire bloodlines, werecurses, site-level sacking, written works. Our
  `Artifact` is literally `{ name }`; DF's is a paragraph.
- **Emergent, rule-driven causation.** DF wars erupt from ethics/values clashes
  and grudges; megabeasts wander and rampage on the actual map.

**Where we differ in kind, not just scale:** DF does not model secular
demographic cycles, carrying capacity, elite overproduction, or asabiyyah. Our
wars and collapses fall out of a Turchin/Khaldun **social-science backbone**. DF
is bookkeeping-of-emergence; we are theory-as-engine.

**Verdict:** DF is orders of magnitude deeper and more surprising. We are
smaller, more curated, and more explicitly modeled.

## RimWorld

The weakest analog, worth reframing. RimWorld's celebrated "storyteller"
(Cassandra / Phoebe / Randy) is a **real-time incident pacer** — it spends a
points budget to schedule raids, disease, and weather *during play*, tuned to a
difficulty/adaptation curve. Its world generation is comparatively shallow on
history: factions, settlements, biomes, and per-pawn **backstories**, but no
multi-century simulated chronicle.

So RimWorld's relevant lessons are about **pacing and salience**, not history
structure:

- Its storyteller throttles event frequency and escalates stakes so the *reel*
  feels shaped — the same problem our salience model + novelty discount solves,
  just as a post-hoc pass over a fixed deterministic log rather than a live budget.
- Its pawn backstories are the analog of our character entities — but ours are
  connected into lineages and feuds rather than standalone flavor.

If you took RimWorld's storyteller and ran it unattended for 500 years emitting a
readable chronicle, you'd get something closer to us — but that is explicitly not
what it does.

## The closer cousins

- **Crusader Kings (CK2/CK3)** — best analog for our **agent layer**. Dynasties,
  houses, lineage, succession crises with multiple claimants, dynastic claims as
  casus belli, wars of religion, inherited grudges. Our `agent.rs` +
  `succession.rs` (adult-first heirs, contested succession, blood feuds, `Claim`
  → `DynasticClaim`) is a stripped-down CK succession/claim model — run
  autonomously and deterministically rather than interactively.
- **Caves of Qud's history generator** — closest analog to our **uplevel layer**.
  Qud generates a branching mythic history of sultans via a generative grammar
  and renders it as lore text. That is spiritually what our arc/age extraction
  does: turn a raw event graph into named, classified threads ("The Saga of
  Ekep", "The War of Faith of Elinera") and epochs ("an Age of Ruin").
- **Academic lineage** (for deeper reading): James Ryan's *Talk of the Town* /
  *Bad News* (simulationist social-history generation); Peter Turchin's
  *cliodynamics* / structural-demographic theory, which we implement more
  literally than any game we know of; Ibn Khaldun's asabiyyah; Mearsheimer's
  offensive realism.

## What's distinctive about ours

Three things none of DF / RimWorld combine:

1. **A theory-grounded SD backbone.** State vectors driving macro transitions,
   with the six loops mapped to named social-science models. Closer to an
   academic cliodynamics toy than to a game's worldgen.
2. **Determinism as a hard contract.** Byte-identical native↔wasm, golden-hash
   pinned. DF is seed-reproducible but doesn't engineer cross-platform
   byte-identity — which matters because our world runs in a browser.
3. **An LLM-handoff distillation layer.** The causal DAG, salience-ranked
   highlight reel, classified arcs, mythic ages, and a closed NER lexicon +
   transitive arc closure so Phase 5 can narrate without hallucinating proper
   nouns. DF's legends mode is built for *human* browsing; it doesn't pre-distill
   arcs/ages/salience for a generator to consume. This is the modern, distinctive
   bet.

## The core tension (open question for Phase 5)

The natural way to close the gap with DF is **content breadth** — more event
kinds, individual non-ruler figures, artifact provenance chains, site-level
history. But each of those trades away the **legibility** that makes the log easy
to narrate: a closed proper-noun set, a small kind taxonomy, and a salience model
that surfaces a handful of threads.

**Depth vs. narratability is the whole design question for Phase 5.** This
document exists so that when we revisit it, the trade is a deliberate decision and
not a drift.

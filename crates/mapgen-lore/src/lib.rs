//! Claude integration — the lore engine that narrates the history sim's event
//! log into grounded, in-world chronicles. Build with `cargo` on a native target
//! only. On wasm32 the crate is intentionally empty so the WASM binary cannot
//! leak API credentials.
//!
//! **Opt-in.** Narration is never part of `generate_full`; it is an explicit
//! step (`mapgen lore`). The deterministic [`template`] narrator runs offline
//! with no key. The real Anthropic client (Phase 5e) is gated behind the `lore`
//! Cargo feature *and* a runtime API key, so a stock build can make no paid call.
//!
//! Pipeline: [`select_focal`] picks the event → [`prompt::build`] assembles the
//! grounded prompt → an [`LlmClient`] (if any) drafts → [`ner::validate`] checks
//! it → one retry on violation → the [`template`] fallback if that fails → the
//! accepted chronicle is persisted as a [`mapgen_core::Work`].

#![cfg(not(target_arch = "wasm32"))]

#[cfg(feature = "lore")]
pub mod anthropic;
pub mod bible;
pub mod client;
pub mod context;
pub mod ner;
pub mod prompt;
pub mod schema;
pub mod template;
pub mod voice;

#[cfg(feature = "lore")]
pub use anthropic::AnthropicClient;
pub use client::LlmClient;
pub use prompt::Prompt;
pub use schema::{ChronicleDraft, SCHEMA_HINT};
pub use voice::{Register, VoiceCard};

use std::cmp::Ordering;

use mapgen_core::{Event, EventId, EventKind, Work, WorldData};

/// Resolve a `--event` selector to a concrete event id. Accepts a numeric id or
/// one of three `auto-*` modes, each of which prefers a class of event but falls
/// back to the most salient event overall when that class is empty (ties broken
/// to the earliest):
///   - `auto-major-war` / `auto`: the most salient war event
///     (`WarDeclared` | `BattleFought` | `Siege`);
///   - `auto-contact`: the most salient inter-continental *contact* event
///     (`TradeRouteOpened` | `EmbargoImposed`) — the clearest cross-water markers,
///     so the narrator can weave the Sundered Lanes story instead of only wars.
pub fn select_focal(world: &WorldData, spec: &str) -> anyhow::Result<EventId> {
    if let Ok(id) = spec.parse::<u32>() {
        return Ok(EventId(id));
    }
    if !matches!(spec, "auto-major-war" | "auto" | "auto-contact") {
        anyhow::bail!(
            "unknown event selector '{spec}' \
             (use a numeric id or 'auto-major-war' / 'auto-contact')"
        );
    }
    let is_war = |e: &&Event| {
        matches!(
            e.kind,
            EventKind::WarDeclared | EventKind::BattleFought | EventKind::Siege
        )
    };
    // Inter-continental contact: a sea-trade route opening or its severance by
    // embargo — the two events that provably cross water.
    let is_contact = |e: &&Event| {
        matches!(
            e.kind,
            EventKind::TradeRouteOpened | EventKind::EmbargoImposed
        )
    };
    let preferred: &dyn Fn(&&Event) -> bool = if spec == "auto-contact" {
        &is_contact
    } else {
        &is_war
    };
    let most_salient = |a: &Event, b: &Event| match a
        .salience
        .partial_cmp(&b.salience)
        .unwrap_or(Ordering::Equal)
    {
        Ordering::Equal => b.id.0.cmp(&a.id.0), // tie → earliest id wins the max
        ord => ord,
    };
    let pick = world
        .events
        .events
        .iter()
        .filter(preferred)
        .max_by(|a, b| most_salient(a, b))
        .or_else(|| world.events.events.iter().max_by(|a, b| most_salient(a, b)));
    pick.map(|e| e.id)
        .ok_or_else(|| anyhow::anyhow!("the world has no events to narrate"))
}

/// Per-world ceiling on paid (Claude) narrations, per ARCHITECTURE §7
/// (`max_calls_per_world`). Counted against the works already persisted on the
/// world; the free offline template narrator is never capped.
pub const MAX_CALLS_PER_WORLD: usize = 20;

/// Cap on engine-derived lacunae attached to a chronicle (so a world's standing
/// mysteries don't flood the list).
const MAX_LACUNAE: usize = 3;

/// Narrate `focal` (and its causal lead-up) in `voice`, persist the result as a
/// `Work` on `world`, and return it. With `client = None` (or on any client
/// failure / repeated NER violation) the deterministic template narrator is
/// used, so this never fails for lack of a working model — except when the paid
/// per-world budget is exhausted (a deliberate cost guard, not a model failure).
pub fn narrate(
    world: &mut WorldData,
    focal: EventId,
    voice: &VoiceCard,
    client: Option<&dyn LlmClient>,
) -> anyhow::Result<Work> {
    // Cost guard: cap paid calls per world. Offline (template) narration is free
    // and uncapped.
    if client.is_some() && world.works.len() >= MAX_CALLS_PER_WORLD {
        anyhow::bail!(
            "per-world narration budget reached ({MAX_CALLS_PER_WORLD} chronicles); \
             narrate offline (drop --features lore or unset ANTHROPIC_API_KEY) or start a fresh world"
        );
    }

    let focal_event = world
        .events
        .events
        .get(focal.0 as usize)
        .ok_or_else(|| anyhow::anyhow!("focal event {} is out of range", focal.0))?
        .clone();
    let slice = context::event_closure(world, focal);

    let draft = match client {
        Some(c) => draft_with_client(world, focal, voice, &slice, c).unwrap_or_else(|_| {
            let slice_events = resolve(world, &slice);
            template::template_draft(&focal_event, &slice_events, voice)
        }),
        None => {
            let slice_events = resolve(world, &slice);
            template::template_draft(&focal_event, &slice_events, voice)
        }
    };

    let written_year = world
        .events
        .events
        .iter()
        .map(|e| e.year)
        .max()
        .unwrap_or(0);
    // Engine-derived lacunae (the age's unresolved threads) merged with any the
    // model self-reported — so gaps are recorded even on the template path.
    let mut lacunae = draft.lacunae;
    for l in world_lacunae(world) {
        if !lacunae.contains(&l) {
            lacunae.push(l);
        }
    }
    let work = Work {
        title: draft.title,
        body: draft.body,
        in_world_author: voice.author.clone(),
        references: draft.references.iter().map(|&r| EventId(r)).collect(),
        lacunae,
        written_year,
    };
    world.works.push(work.clone());
    Ok(work)
}

/// Resolve event ids to event references in the world.
fn resolve<'a>(world: &'a WorldData, ids: &[EventId]) -> Vec<&'a Event> {
    ids.iter()
        .filter_map(|id| world.events.events.get(id.0 as usize))
        .collect()
}

/// The age's unresolved threads, derived from the persisted log: prophecies that
/// were uttered but no deed ever fulfilled. The history sim leaves these
/// dangling deliberately (`SimState.pending_prophecies`), so they are genuine
/// `[lacuna]`s a chronicler would note. Capped so they don't flood the list.
fn world_lacunae(world: &WorldData) -> Vec<String> {
    let fulfilled: std::collections::BTreeSet<u32> = world
        .events
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::ProphecyFulfilled))
        .flat_map(|e| e.cause_ids.iter().map(|c| c.0))
        .collect();
    world
        .events
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::ProphecyUttered) && !fulfilled.contains(&e.id.0))
        .map(|e| format!("A prophecy left unfulfilled: \"{}\"", e.summary_canonical))
        .take(MAX_LACUNAE)
        .collect()
}

/// One LLM attempt with a single NER-violation retry. `Err` on parse failure,
/// client failure, or a second violation — the caller then falls back to the
/// template narrator.
fn draft_with_client(
    world: &WorldData,
    focal: EventId,
    voice: &VoiceCard,
    slice: &[EventId],
    client: &dyn LlmClient,
) -> anyhow::Result<ChronicleDraft> {
    let prompt = prompt::build(world, focal, voice);
    let draft = ChronicleDraft::from_response(&client.complete(&prompt)?)?;
    match ner::validate(&draft, world, slice) {
        Ok(()) => Ok(draft),
        Err(violation) => {
            // Retry once, reporting the violation in the (volatile) focal block.
            let mut retry_prompt = prompt.clone();
            retry_prompt.focal.push_str(&format!(
                "\n\n# CORRECTION\nYour previous response was rejected: {violation}. \
                 Use only names from the WORLD BIBLE / ENTITY CONTEXT and ids from the \
                 SUPPLIED EVENTS.",
            ));
            let retry = ChronicleDraft::from_response(&client.complete(&retry_prompt)?)?;
            ner::validate(&retry, world, slice).map_err(|e| anyhow::anyhow!(e))?;
            Ok(retry)
        }
    }
}

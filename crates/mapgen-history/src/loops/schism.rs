//! Religious schism. A faith fractures into a heterodox sect that inherits its
//! pantheon and founding culture but drifts in alignment — the doctrinal-split
//! layer of religious history.
//!
//! Phase 4g: schisms mint a splinter `Entity::Religion` (the sect) and a
//! `Schism` event referencing the parent faith. Per-cell adherence / sect
//! *spread* is deferred; the schism is recorded as an entity + event for the
//! chronicle, and religious difference between polities drives `ReligiousSchism`
//! wars in the Mearsheimer loop.

use mapgen_core::ids::CellId;
use mapgen_core::{
    generate_name, Entity, EntityId, Event, EventId, EventKind, Religion, WorldData,
};
use smallvec::SmallVec;

use crate::loops::{CausalLoop, LoopId, TickCtx};
use crate::unit_f32;

/// Per-religion yearly chance of schism — low, so a faith fractures a handful
/// of times across 500 years, not constantly.
const SCHISM_PROB: f32 = 0.003;
/// How far the sect's alignment drifts from the parent on the chosen axis.
const ALIGN_DRIFT: f32 = 0.3;

/// Religious-schism loop.
pub struct Schism;

impl CausalLoop for Schism {
    fn id(&self) -> LoopId {
        LoopId::Schism
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        let n_rel = ctx.world.religions.religions.len();
        for ri in 0..n_rel {
            if unit_f32(ctx.rng) < SCHISM_PROB {
                schism(ctx, ri, ctx.year);
            }
        }
    }
}

fn schism(ctx: &mut TickCtx, ri: usize, year: i32) {
    let parent = ctx.world.religions.religions[ri].clone();
    let parent_id = ensure_religion_entity(ctx, ri);

    // The sect inherits pantheon + founding culture but drifts on one alignment
    // axis (a doctrinal divergence).
    let mut alignment = parent.alignment;
    let sign = if unit_f32(ctx.rng) < 0.5 { -1.0 } else { 1.0 };
    if unit_f32(ctx.rng) < 0.5 {
        alignment.law_chaos = (alignment.law_chaos + sign * ALIGN_DRIFT).clamp(-1.0, 1.0);
    } else {
        alignment.good_evil = (alignment.good_evil + sign * ALIGN_DRIFT).clamp(-1.0, 1.0);
    }
    let name = religion_name(ctx.world, parent.founder_culture_id, ctx.rng);
    let splinter = Religion {
        name: name.clone(),
        pantheon: parent.pantheon,
        founder_culture_id: parent.founder_culture_id,
        alignment,
        sacred_sites: Vec::new(),
    };
    let splinter_id = ctx.world.entities.insert(Entity::Religion(splinter));

    let cell = parent.sacred_sites.first().copied().unwrap_or(0);
    emit(
        ctx.world,
        year,
        cell,
        0.6,
        &[splinter_id],
        &[parent_id],
        format!(
            "The {} faith was riven by schism; the {name} sect broke away.",
            parent.name
        ),
    );
}

/// Mint (once) an `Entity::Religion` mirror of the original religion at index
/// `ri`, so `Schism` events can reference the parent faith as an entity.
fn ensure_religion_entity(ctx: &mut TickCtx, ri: usize) -> EntityId {
    if let Some(id) = ctx.state.religion_entities[ri] {
        return id;
    }
    let r = ctx.world.religions.religions[ri].clone();
    let id = ctx.world.entities.insert(Entity::Religion(r));
    ctx.state.religion_entities[ri] = Some(id);
    id
}

/// A name in the founding culture's language (fallback to the first language).
fn religion_name(
    world: &WorldData,
    founder_culture_id: u16,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> String {
    let lang_id = world
        .cultures
        .cultures
        .get(founder_culture_id as usize)
        .map(|c| c.language_id as usize)
        .unwrap_or(0);
    match world
        .languages
        .get(lang_id)
        .or_else(|| world.languages.first())
    {
        Some(lang) => generate_name(lang, rng),
        None => "Sect".to_string(),
    }
}

fn emit(
    world: &mut WorldData,
    year: i32,
    cell: u32,
    salience: f32,
    actors: &[EntityId],
    patients: &[EntityId],
    summary: String,
) -> EventId {
    let mut a: SmallVec<[EntityId; 4]> = SmallVec::new();
    a.extend(actors.iter().copied());
    let mut p: SmallVec<[EntityId; 4]> = SmallVec::new();
    p.extend(patients.iter().copied());
    world.events.push(Event {
        id: EventId(0),
        year,
        kind: EventKind::Schism,
        actors: a,
        patients: p,
        location: Some(CellId(cell)),
        cause_ids: SmallVec::new(),
        salience,
        casus_belli: None,
        summary_canonical: summary,
    })
}

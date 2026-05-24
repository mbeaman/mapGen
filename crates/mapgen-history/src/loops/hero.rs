//! Heroic / megabeast loop — the legendary layer. Rare, high-salience events
//! that form a recognizable saga: a prophecy is uttered, a megabeast rises to
//! ravage a realm, a champion arises to slay it and forge an artifact from the
//! spoils, and the prophecy is fulfilled.
//!
//! Phase 4h. These are world-scale (not per-polity) and deliberately rare. The
//! Megabeast→Slain and Prophecy→Fulfilled pairs set `cause_ids` now (a head
//! start on 4i's causal chaining). Mythic-age framing lands in 4i with the
//! `HistoryData` side-table.

use mapgen_core::{generate_name, Artifact, Character, Entity, EventKind, Sex, WorldData};
use rand_chacha::{rand_core::RngCore, ChaCha8Rng};

use crate::emit::Emit;
use crate::loops::{CausalLoop, LoopId, TickCtx};
use crate::unit_f32;

/// Yearly chance a seer utters a prophecy.
const PROPHECY_PROB: f32 = 0.020;
/// Yearly chance a megabeast rises somewhere in the world.
const MEGABEAST_PROB: f32 = 0.025;
/// Per-beast yearly chance a champion arises and slays it.
const SLAY_PROB: f32 = 0.18;
/// Assumed age of a newly-arisen champion (so `born_year` predates the deed).
const HERO_AGE: i32 = 25;

/// Hero / megabeast loop.
pub struct Hero;

impl CausalLoop for Hero {
    fn id(&self) -> LoopId {
        LoopId::Hero
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        let year = ctx.year;

        // 1. A prophecy is foretold (queued until a deed fulfils it).
        if unit_f32(ctx.rng) < PROPHECY_PROB {
            if let Some(cell) = random_capital(ctx) {
                let ev = Emit::new(
                    year,
                    EventKind::ProphecyUttered,
                    cell,
                    0.5,
                    "A seer foretold that a great evil would rise, and a champion to end it."
                        .to_string(),
                )
                .push(ctx.world);
                ctx.state.pending_prophecies.push(ev);
            }
        }

        // 2. A megabeast rises.
        if unit_f32(ctx.rng) < MEGABEAST_PROB {
            if let Some(cell) = random_capital(ctx) {
                let name = legendary_name(ctx.world, ctx.rng);
                let ev = Emit::new(
                    year,
                    EventKind::MegabeastRise,
                    cell,
                    0.80,
                    format!("{name}, a monstrous beast, rose to ravage the land."),
                )
                .push(ctx.world);
                ctx.state.active_megabeasts.push((cell, ev, name));
            }
        }

        // 3. Champions slay active beasts.
        let mut i = 0;
        while i < ctx.state.active_megabeasts.len() {
            if unit_f32(ctx.rng) >= SLAY_PROB {
                i += 1;
                continue;
            }
            let (cell, rise_ev, beast) = ctx.state.active_megabeasts.remove(i);
            let hero_name = legendary_name(ctx.world, ctx.rng);
            let sex = if ctx.rng.next_u32() & 1 == 0 {
                Sex::Female
            } else {
                Sex::Male
            };
            let hero = ctx.world.entities.insert(Entity::Character(Character {
                name: hero_name.clone(),
                born_year: year - HERO_AGE,
                died_year: None,
                house: None,
                culture_id: None,
                sex,
                relationships: Vec::new(),
                titles: Vec::new(),
                birth_event: None,
                death_event: None,
            }));
            Emit::new(
                year,
                EventKind::Ascension,
                cell,
                0.82,
                format!("{hero_name} arose as a champion of the age."),
            )
            .actors(&[hero])
            .push(ctx.world);
            Emit::new(
                year,
                EventKind::MegabeastSlain,
                cell,
                0.88,
                format!("{hero_name} slew the beast {beast}."),
            )
            .actors(&[hero])
            .causes(&[rise_ev])
            .push(ctx.world);
            let artifact_name = legendary_name(ctx.world, ctx.rng);
            let artifact = ctx.world.entities.insert(Entity::Artifact(Artifact {
                name: artifact_name.clone(),
            }));
            Emit::new(
                year,
                EventKind::ArtifactForged,
                cell,
                0.6,
                format!("{hero_name} forged {artifact_name} from the beast's remains."),
            )
            .actors(&[hero, artifact])
            .push(ctx.world);
            if let Some(uttered) = ctx.state.pending_prophecies.pop() {
                Emit::new(
                    year,
                    EventKind::ProphecyFulfilled,
                    cell,
                    0.86,
                    format!("The old prophecy was fulfilled in {hero_name}'s triumph."),
                )
                .actors(&[hero])
                .causes(&[uttered])
                .push(ctx.world);
            }
        }
    }
}

/// A deterministically chosen *living* polity's capital cell (beasts menace
/// settled realms). `None` if there are no surviving polities.
fn random_capital(ctx: &mut TickCtx) -> Option<u32> {
    let living: Vec<usize> = (0..ctx.world.society.nations.len())
        .filter(|&pid| !ctx.state.dissolved.get(pid).copied().unwrap_or(false))
        .collect();
    if living.is_empty() {
        return None;
    }
    let pid = living[(ctx.rng.next_u32() as usize) % living.len()];
    Some(ctx.world.society.nations[pid].capital_cell)
}

/// A legendary name in the first available language (heroes / beasts / relics
/// belong to no single culture). Falls back to a placeholder.
fn legendary_name(world: &WorldData, rng: &mut ChaCha8Rng) -> String {
    match world.languages.first() {
        Some(lang) => generate_name(lang, rng),
        None => "the Nameless".to_string(),
    }
}

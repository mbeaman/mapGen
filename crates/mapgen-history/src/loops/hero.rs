//! Heroic / megabeast loop — the legendary layer. Rare, high-salience events
//! that form a recognizable saga: a prophecy is uttered, a megabeast rises to
//! ravage a realm, a champion arises to slay it and forge an artifact from the
//! spoils, and the prophecy is fulfilled.
//!
//! Phase 4h. These are world-scale (not per-polity) and deliberately rare. The
//! Megabeast→Slain and Prophecy→Fulfilled pairs set `cause_ids` now (a head
//! start on 4i's causal chaining). Mythic-age framing lands in 4i with the
//! `HistoryData` side-table.

use mapgen_core::{
    generate_name, Artifact, Character, Entity, EventKind, Megabeast, Sex, WorldData,
};
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
/// Chance a risen beast is a "great wyrm" — too mighty to be slain. It is
/// recorded but never enters the slay queue, so it haunts the age as a standing
/// danger (its `MegabeastRise` is left without a `MegabeastSlain` — a Phase-5
/// lacuna / lingering menace rather than a tidy 100%-kill cassette).
const GREAT_BEAST_PROB: f32 = 0.30;
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
                let line = match ctx.rng.next_u32() % 3 {
                    0 => "A seer foretold that a great evil would rise, and a champion to end it.",
                    1 => {
                        "A prophet spoke of a coming darkness, and one who would stand against it."
                    }
                    _ => "A dire omen was read in the heavens: a scourge would rise, and a savior.",
                };
                let ev = Emit::new(
                    year,
                    EventKind::ProphecyUttered,
                    cell,
                    0.5,
                    line.to_string(),
                )
                .push(ctx.world);
                ctx.state.pending_prophecies.push(ev);
            }
        }

        // 2. A megabeast rises.
        if unit_f32(ctx.rng) < MEGABEAST_PROB {
            if let Some(cell) = random_capital(ctx) {
                let name = legendary_name(ctx.world, ctx.rng);
                let beast = ctx
                    .world
                    .entities
                    .insert(Entity::Megabeast(Megabeast { name: name.clone() }));
                let line = match ctx.rng.next_u32() % 3 {
                    0 => format!("{name}, a monstrous beast, rose to ravage the land."),
                    1 => format!("{name} emerged from the wastes to terrorize the realm."),
                    _ => format!("{name}, a dread wyrm, awoke and laid the country waste."),
                };
                let ev = Emit::new(year, EventKind::MegabeastRise, cell, 0.80, line)
                    .actors(&[beast])
                    .push(ctx.world);
                // Most beasts can be slain; a "great wyrm" is too mighty and
                // never enters the slay queue — it endures as a standing menace.
                if unit_f32(ctx.rng) >= GREAT_BEAST_PROB {
                    ctx.state.active_megabeasts.push((cell, ev, beast, name));
                }
            }
        }

        // 3. Champions slay active beasts.
        let mut i = 0;
        while i < ctx.state.active_megabeasts.len() {
            if unit_f32(ctx.rng) >= SLAY_PROB {
                i += 1;
                continue;
            }
            let (cell, rise_ev, beast_id, beast) = ctx.state.active_megabeasts.remove(i);
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
            // The cassette is causally chained so it weaves into one HeroSaga
            // arc: the beast's rise summons the champion (ascension), who slays
            // it, forges a relic from the spoils, and fulfils the old prophecy.
            let ascension_line = match ctx.rng.next_u32() % 3 {
                0 => format!("{hero_name} arose as a champion of the age."),
                1 => format!("{hero_name} took up the sword against the terror."),
                _ => format!("{hero_name} was hailed as the realm's champion."),
            };
            Emit::new(year, EventKind::Ascension, cell, 0.82, ascension_line)
                .actors(&[hero])
                .causes(&[rise_ev])
                .push(ctx.world);
            let slain_line = match ctx.rng.next_u32() % 3 {
                0 => format!("{hero_name} slew the beast {beast}."),
                1 => format!("{hero_name} cut down {beast} in single combat."),
                _ => format!("{hero_name} brought the beast {beast} to its end."),
            };
            let slain_ev = Emit::new(year, EventKind::MegabeastSlain, cell, 0.88, slain_line)
                .actors(&[hero])
                .patients(&[beast_id])
                .causes(&[rise_ev])
                .push(ctx.world);
            let artifact_name = legendary_name(ctx.world, ctx.rng);
            let artifact = ctx.world.entities.insert(Entity::Artifact(Artifact {
                name: artifact_name.clone(),
            }));
            let artifact_line = match ctx.rng.next_u32() % 3 {
                0 => format!("{hero_name} forged {artifact_name} from the beast's remains."),
                1 => format!("{hero_name} wrought {artifact_name} from the slain beast's hoard."),
                _ => format!("{hero_name} fashioned {artifact_name} from the beast's spoils."),
            };
            Emit::new(year, EventKind::ArtifactForged, cell, 0.6, artifact_line)
                .actors(&[hero, artifact])
                .causes(&[slain_ev])
                .push(ctx.world);
            if let Some(uttered) = ctx.state.pending_prophecies.pop() {
                let fulfilled_line = match ctx.rng.next_u32() % 3 {
                    0 => format!("The old prophecy was fulfilled in {hero_name}'s triumph."),
                    1 => format!("{hero_name}'s victory bore out the ancient prophecy."),
                    _ => format!("The prophecy of old came to pass with {hero_name}'s deed."),
                };
                Emit::new(
                    year,
                    EventKind::ProphecyFulfilled,
                    cell,
                    0.86,
                    fulfilled_line,
                )
                .actors(&[hero])
                .causes(&[uttered, slain_ev])
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

//! Mearsheimer offensive realism + Allison's Thucydides trap. Polities eye
//! their neighbours; war ignites when a strong (or rising, near-parity) power
//! sees advantage, especially expansionist ones or those holding a dynastic
//! claim. Wars resolve in-year: the stronger side (with luck) wins a battle and
//! takes border territory, then peace is signed.
//!
//! Phase 4e: the "first wars". Actors are the reigning rulers (Characters from
//! 4c); claims are `Claim` entities targeting a polity's throne `Title`.
//! Territory changes hands by reassigning `society.control` cells (total
//! controlled-cell count is conserved — cells move, none vanish). A war only
//! ignites between polities that currently `share_border`, so it always can
//! change the map; a realm conquered to 0 cells is marked dissolved (4h.5
//! hardening) and stops warring / being warred.

use mapgen_core::{CasusBelli, Claim, DiplomaticPattern, Entity, EventKind, WorldData};

use crate::emit::Emit;
use crate::loops::{CausalLoop, LoopId, TickCtx};
use crate::{polity_capacity, polity_cell_count, polity_military, unit_f32, SimState};

/// Per-polity yearly chance to press a dynastic claim on a neighbour.
const CLAIM_PROB: f32 = 0.012;
/// Years a dynastic claim stays live before lapsing (~two generations) — so an
/// ancient claim doesn't relabel every war on the dyad as dynastic forever.
const CLAIM_EXPIRY: i32 = 50;
/// Base war chance per (aggressor, neighbour) per year, scaled by power
/// pressure and the aggressor's diplomatic posture.
const WAR_BASE: f32 = 0.020;
/// Years a polity must wait after starting a war before starting another —
/// stops the strongest realm warring every few years (rich-get-richer runaway).
const WAR_COOLDOWN: i32 = 12;
/// Diplomatic multipliers on war propensity.
const EXPANSIONIST_MULT: f32 = 2.5;
const ISOLATIONIST_MULT: f32 = 0.3;
const HONORBOUND_MULT: f32 = 1.4;
/// Border cells the victor seizes from the loser.
const TRANSFER_CELLS: usize = 6;
/// Combined-power reference for battle salience: only wars whose stakes
/// approach this read as "major" (salience ≥ 0.8).
const STAKES_REF: f32 = 300.0;
const EPS: f32 = 1e-3;

/// Mearsheimer power-balance loop.
pub struct Mearsheimer;

impl CausalLoop for Mearsheimer {
    fn id(&self) -> LoopId {
        LoopId::Mearsheimer
    }

    fn tick(&mut self, ctx: &mut TickCtx) {
        let year = ctx.year;
        let n = ctx.state.polity_count;

        // Expire stale claims so casus belli stays truthful.
        ctx.state
            .claims
            .retain(|&(_, _, asserted, _)| year - asserted < CLAIM_EXPIRY);

        // 1. Dynastic claims — occasional pressure on a neighbour's throne.
        for a in 0..n {
            if ctx.state.dissolved[a] {
                continue;
            }
            let neighbors = ctx.state.adjacency.get(a).cloned().unwrap_or_default();
            if neighbors.is_empty() {
                continue;
            }
            if unit_f32(ctx.rng) < CLAIM_PROB {
                if let Some(&b) = neighbors.iter().find(|&&b| {
                    !ctx.state.dissolved[b]
                        && !ctx
                            .state
                            .claims
                            .iter()
                            .any(|&(x, y, _, _)| x == a as u16 && y == b as u16)
                }) {
                    assert_claim(ctx, a, b, year);
                }
            }
        }

        // 2. Wars — each polity may initiate at most one per year.
        for a in 0..n {
            if ctx.state.dissolved[a] {
                continue;
            }
            if ctx.state.courts.get(a).and_then(|c| c.ruler).is_none() {
                continue;
            }
            if year.saturating_sub(ctx.state.last_war[a]) < WAR_COOLDOWN {
                continue;
            }
            let neighbors = ctx.state.adjacency.get(a).cloned().unwrap_or_default();
            for b in neighbors {
                if ctx.state.dissolved[b] {
                    continue;
                }
                // A war only happens if it can change the map — the two polities
                // must currently share a border (frozen adjacency over-counts
                // once a frontier has been fully absorbed). Kills repeated
                // 0-transfer wars. `share_border` (O(cells)) is checked only when
                // the cheap ignition roll passes.
                if war_ignites(ctx, a, b) && share_border(ctx.world, a, b) {
                    ctx.state.last_war[a] = year;
                    resolve_war(ctx, a, b, year);
                    break;
                }
            }
        }
    }
}

fn war_power(world: &WorldData, state: &SimState, pid: usize) -> f32 {
    state.population[pid] * (0.5 + polity_military(world, pid) as f32 / 100.0)
}

/// The polity's faith (the religion at its capital cell), if any.
fn polity_religion(world: &WorldData, pid: usize) -> Option<u16> {
    let cap = world.society.nations[pid].capital_cell as usize;
    world.religions.religion_id.get(cap).copied().flatten()
}

/// Whether polities `a` and `b` currently share a controlled border (a cell of
/// one adjacent to a cell of the other). O(cells); only called when a war's
/// ignition roll has already passed.
fn share_border(world: &WorldData, a: usize, b: usize) -> bool {
    let owns =
        |c: usize, pid: usize| world.society.control.get(c).copied().flatten() == Some(pid as u32);
    (0..world.mesh.cell_count()).any(|c| {
        owns(c, a)
            && world
                .mesh
                .neighbors
                .get(c)
                .map(|nbrs| nbrs.iter().any(|&nb| owns(nb as usize, b)))
                .unwrap_or(false)
    })
}

fn expansion_mult(world: &WorldData, pid: usize) -> f32 {
    let cap = world.society.nations[pid].capital_cell as usize;
    let dip = world
        .cultures
        .culture_id
        .get(cap)
        .copied()
        .flatten()
        .and_then(|cid| world.cultures.cultures.get(cid as usize))
        .map(|c| c.diplomatic);
    match dip {
        Some(DiplomaticPattern::Expansionist) => EXPANSIONIST_MULT,
        Some(DiplomaticPattern::Isolationist) => ISOLATIONIST_MULT,
        Some(DiplomaticPattern::HonorBound) => HONORBOUND_MULT,
        _ => 1.0,
    }
}

fn war_ignites(ctx: &mut TickCtx, a: usize, b: usize) -> bool {
    let pa = war_power(ctx.world, ctx.state, a);
    let pb = war_power(ctx.world, ctx.state, b);
    if pb <= EPS {
        return false;
    }
    let ratio = pa / pb;
    // Opportunism when stronger; a smaller pull near/below parity (rising
    // challenger). Below ~0.6 the aggressor is too weak to bother.
    let pressure = if ratio >= 1.0 {
        (ratio - 0.8).min(2.0)
    } else {
        (ratio - 0.6).max(0.0)
    };
    if pressure <= 0.0 {
        return false;
    }
    unit_f32(ctx.rng) < WAR_BASE * pressure * expansion_mult(ctx.world, a)
}

fn assert_claim(ctx: &mut TickCtx, a: usize, b: usize, year: i32) {
    let (Some(ruler_a), Some(title_b)) = (ctx.state.courts[a].ruler, ctx.state.courts[b].title)
    else {
        return;
    };
    let cell = ctx.world.society.nations[a].capital_cell;
    let name_a = ctx.world.society.nations[a].name.clone();
    let name_b = ctx.world.society.nations[b].name.clone();
    let claim_ev = Emit::new(
        year,
        EventKind::ClaimAsserted,
        cell,
        0.4,
        format!("{name_a} pressed a claim upon the throne of {name_b}."),
    )
    .actors(&[ruler_a])
    .push(ctx.world);
    ctx.world.entities.insert(Entity::Claim(Claim {
        claimant: ruler_a,
        target: title_b,
        asserted_year: year,
        dormant: false,
    }));
    ctx.state.claims.push((a as u16, b as u16, year, claim_ev));
}

fn resolve_war(ctx: &mut TickCtx, a: usize, b: usize, year: i32) {
    let (Some(ruler_a), Some(ruler_b)) = (ctx.state.courts[a].ruler, ctx.state.courts[b].ruler)
    else {
        return;
    };
    let pa = war_power(ctx.world, ctx.state, a);
    let pb = war_power(ctx.world, ctx.state, b);
    let cell = ctx.world.society.nations[a].capital_cell;
    let name_a = ctx.world.society.nations[a].name.clone();
    let name_b = ctx.world.society.nations[b].name.clone();
    // A claim (if any) both sets the casus belli and is cited as the war's cause.
    let claim_ev = ctx
        .state
        .claims
        .iter()
        .find(|&&(x, y, _, _)| x == a as u16 && y == b as u16)
        .map(|&(_, _, _, ev)| ev);
    let (ra, rb) = (polity_religion(ctx.world, a), polity_religion(ctx.world, b));
    let casus = if claim_ev.is_some() {
        CasusBelli::DynasticClaim
    } else if ra.is_some() && rb.is_some() && ra != rb {
        // Different faiths — a war of religion (schisms deepen these divides).
        CasusBelli::ReligiousSchism
    } else {
        CasusBelli::FrontierIncident
    };

    let mut war = Emit::new(
        year,
        EventKind::WarDeclared,
        cell,
        0.72,
        format!("{name_a} declared war upon {name_b}."),
    )
    .actors(&[ruler_a, ruler_b])
    .casus(casus);
    if let Some(ce) = claim_ev {
        war = war.causes(&[ce]);
    }
    let war_ev = war.push(ctx.world);

    // Battle: stronger side, perturbed by fortune, prevails.
    let la = pa * (0.7 + 0.6 * unit_f32(ctx.rng));
    let lb = pb * (0.7 + 0.6 * unit_f32(ctx.rng));
    let a_wins = la >= lb;
    let (w_pid, l_pid, w_ruler, l_ruler, w_name, l_name) = if a_wins {
        (a, b, ruler_a, ruler_b, name_a.clone(), name_b.clone())
    } else {
        (b, a, ruler_b, ruler_a, name_b.clone(), name_a.clone())
    };
    // Salience scales with the stakes (combined power) against a reference, so
    // only the largest wars read as "major" (≥ 0.8).
    let stakes = ((pa + pb) / STAKES_REF).clamp(0.0, 1.0);
    let battle_sal = (0.58 + 0.4 * stakes).clamp(0.0, 1.0);
    let battle_ev = Emit::new(
        year,
        EventKind::BattleFought,
        cell,
        battle_sal,
        format!("{w_name} defeated {l_name} in the field."),
    )
    .actors(&[w_ruler])
    .patients(&[l_ruler])
    .causes(&[war_ev])
    .push(ctx.world);

    let moved = transfer_border_cells(ctx.world, w_pid, l_pid, TRANSFER_CELLS);
    if moved > 0 {
        let siege_ev = Emit::new(
            year,
            EventKind::Siege,
            cell,
            (battle_sal * 0.9).clamp(0.0, 1.0),
            format!("{w_name} wrested {moved} settlements from {l_name}."),
        )
        .actors(&[w_ruler])
        .patients(&[l_ruler])
        .causes(&[battle_ev])
        .push(ctx.world);
        // Borders moved — Turchin's capacity must track the new territory.
        ctx.state.capacity[w_pid] = polity_capacity(ctx.world, w_pid);
        ctx.state.capacity[l_pid] = polity_capacity(ctx.world, l_pid);

        // If that conquest took the loser's last land, the realm is no more.
        if !ctx.state.dissolved[l_pid] && polity_cell_count(ctx.world, l_pid) == 0 {
            ctx.state.dissolved[l_pid] = true;
            Emit::new(
                year,
                EventKind::CityAbandoned,
                cell,
                0.82,
                format!(
                    "The realm of {l_name} was extinguished; {w_name} seized its last holdings."
                ),
            )
            .actors(&[w_ruler])
            .patients(&[l_ruler])
            .causes(&[siege_ev])
            .push(ctx.world);
        }
    }

    Emit::new(
        year,
        EventKind::TreatySigned,
        cell,
        0.42,
        format!("{w_name} and {l_name} made peace."),
    )
    .actors(&[w_ruler, l_ruler])
    .causes(&[war_ev])
    .push(ctx.world);
}

/// Reassign up to `k` of the loser's cells that border the winner's territory
/// to the winner. Returns how many moved. Deterministic (ascending cell index);
/// total controlled-cell count is unchanged (cells change owner, none vanish).
fn transfer_border_cells(world: &mut WorldData, winner: usize, loser: usize, k: usize) -> usize {
    let n = world.mesh.cell_count();
    let frontier: Vec<usize> = (0..n)
        .filter(|&c| world.society.control.get(c).copied().flatten() == Some(loser as u32))
        .filter(|&c| {
            world.mesh.neighbors[c].iter().any(|&nb| {
                world.society.control.get(nb as usize).copied().flatten() == Some(winner as u32)
            })
        })
        .take(k)
        .collect();
    for c in &frontier {
        world.society.control[*c] = Some(winner as u32);
    }
    frontier.len()
}

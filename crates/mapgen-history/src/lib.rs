//! Deterministic agent-based history simulation. Produces a typed, append-only
//! event log (and named entities) that the rendering and lore layers consume.
//!
//! # Determinism
//! All randomness is derived hierarchically from one base seed via
//! [`mapgen_core::splitmix64`]:
//! `sim_seed → splitmix64(sim_seed, year) → splitmix64(year_seed, loop_id)`.
//! Each (year, loop) seed is *derived*, never drawn from a shared advancing
//! stream, so re-rolling or adding one loop cannot perturb another. The base
//! seed comes from the pipeline's `Stage::History` sub-stream. See
//! `tests` below and `mapgen-world/tests/history_spec.rs`.

pub mod agent;
pub(crate) mod emit;
pub mod extract;
pub mod loops;
pub mod lore_api;

use mapgen_core::{splitmix64, EntityId, EventId, WorldData};
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaCha8Rng,
};

use crate::loops::{default_loops, CausalLoop, LoopId, TickCtx};

/// Tunables for a history run.
#[derive(Clone, Debug)]
pub struct HistoryParams {
    /// Number of years to simulate. MVP target is 500.
    pub years: i32,
}

impl Default for HistoryParams {
    fn default() -> Self {
        Self { years: 500 }
    }
}

/// Cross-loop scratch state for one sim run. Holds the system-dynamics
/// aggregates the loops read and write each year, all indexed by polity id
/// (position in `world.society.nations`). **Never serialized** — only the
/// resulting events / entities persist (the persistence boundary from the
/// Phase-4 plan). 4b adds the demographic backbone; later loops add fiscal /
/// asabiyyah / power vectors here.
#[derive(Clone, Debug, Default)]
pub struct SimState {
    /// Number of polities (length of every per-polity vector below).
    pub polity_count: usize,
    /// Per-polity population in relative units (no real-world scale). Written
    /// by the Turchin loop each year; read by power / dynasty loops later.
    pub population: Vec<f32>,
    /// Per-polity carrying capacity in the same units. Computed once at sim
    /// start from territory biomes × the founding culture's agriculture tech.
    pub capacity: Vec<f32>,
    /// Per-polity aridity in `[0, 1]` (drought propensity), from mean
    /// territory precipitation. Computed once at start.
    pub aridity: Vec<f32>,
    /// Per-polity ruling court (current ruler, dynasty, heirs). Advanced by the
    /// agent layer each year; read by succession / power loops later.
    pub courts: Vec<agent::Court>,
    /// Per-polity elite cohort size (normalized). Turchin: grows with
    /// prosperity, overproduction drives instability.
    pub elites: Vec<f32>,
    /// Per-polity treasury (normalized; may go negative = fiscal crisis).
    pub fiscal: Vec<f32>,
    /// Per-polity sociopolitical instability (≥ 0). Crosses a threshold → a
    /// secular crisis that vents it.
    pub instability: Vec<f32>,
    /// Per-polity asabiyyah / group cohesion in `[0, 1]` (Khaldun). High for a
    /// young dynasty, decays with age; low cohesion amplifies instability.
    pub asabiyyah: Vec<f32>,
    /// The dynasty id observed last year, so Khaldun can detect a dynastic
    /// change and reset asabiyyah.
    pub last_dynasty: Vec<Option<EntityId>>,
    /// Polity adjacency (who borders whom), computed once from the initial
    /// borders. Wars ignite between neighbors; the 4e loop reads this.
    pub adjacency: Vec<Vec<usize>>,
    /// Active dynastic claims as `(claimant_polity, target_polity, asserted_year,
    /// claim_event)` — a claim supplies a `DynasticClaim` casus belli until it
    /// expires, and the war it justifies cites `claim_event` as its cause.
    pub claims: Vec<(u16, u16, i32, EventId)>,
    /// Year each polity last initiated a war, for the war cooldown (init
    /// `i32::MIN` = never).
    pub last_war: Vec<i32>,
    /// Entity id for each original religion (index into
    /// `world.religions.religions`), minted lazily by the schism loop so
    /// `Schism` events can reference the parent faith. `None` until minted.
    pub religion_entities: Vec<Option<EntityId>>,
    /// Megabeasts currently ravaging the world: `(cell, rise_event, beast_entity,
    /// name)`. A `MegabeastSlain` cites the stored rise event (foreshadow→payoff)
    /// and names the beast entity as its patient.
    pub active_megabeasts: Vec<(u32, EventId, EntityId, String)>,
    /// `ProphecyUttered` event ids awaiting fulfillment; a heroic deed fulfils
    /// the oldest, and any still pending at sim end are Phase-5 lacunae.
    pub pending_prophecies: Vec<EventId>,
    /// Per-polity: dissolved (conquered to 0 cells). A dissolved polity's court
    /// freezes and it stops warring / being a war target — no more "throne of a
    /// realm that owns nothing".
    pub dissolved: Vec<bool>,
    /// Schisms that have happened: `(sect_religion_index, parent_religion_index,
    /// schism_event)`. A war between a polity following the sect and one following
    /// its parent faith is a genuine *war of religion* and cites the schism as its
    /// cause — distinct from an ordinary border war between unrelated faiths.
    pub schism_parent: Vec<(u16, u16, EventId)>,
    /// Territorial changes recorded chronologically as wars are won (the
    /// time-slider). Moved into `world.history.border_changes` after the sim.
    pub border_changes: Vec<mapgen_core::history::BorderChange>,
}

/// Initial population as a fraction of carrying capacity — low enough that the
/// logistic growth phase plays out before Malthusian crises begin.
const INITIAL_FILL: f32 = 0.35;
/// Precipitation reference for the aridity scale: territory at or above this
/// mean precip is treated as non-arid (aridity 0). Land precip in this model
/// runs ~0.04–0.07, so this differentiates dry from wet realms.
const ARID_REF_PRECIP: f32 = 0.08;

impl SimState {
    fn new(world: &WorldData) -> Self {
        let n_pol = world.society.nations.len();
        let n_cells = world.mesh.cell_count();
        let mut capacity = vec![0.0f32; n_pol];
        let mut precip_sum = vec![0.0f32; n_pol];
        let mut cells = vec![0u32; n_pol];

        // Aggregate each polity's controlled cells into a carrying capacity
        // and a precipitation tally (the cells → per-polity SD rollup, done
        // once; per the perf note, loops then run over O(polities), not cells).
        for cell in 0..n_cells {
            let Some(pid) = world.society.control.get(cell).copied().flatten() else {
                continue;
            };
            let pid = pid as usize;
            if pid >= n_pol {
                continue;
            }
            let biome = world.climate.biome.get(cell).copied().unwrap_or(0);
            capacity[pid] += cell_capacity(biome);
            precip_sum[pid] += world
                .climate
                .precipitation
                .get(cell)
                .copied()
                .unwrap_or(0.0);
            cells[pid] += 1;
        }

        for (pid, cap) in capacity.iter_mut().enumerate() {
            *cap *= agriculture_factor(world, pid);
        }
        let aridity = (0..n_pol)
            .map(|pid| {
                if cells[pid] == 0 {
                    return 0.0;
                }
                let mean = precip_sum[pid] / cells[pid] as f32;
                // Floor at 0.15 so even wet realms see the occasional drought
                // year (no perfectly drought-proof territory).
                (1.0 - (mean / ARID_REF_PRECIP).min(1.0)).clamp(0.15, 1.0)
            })
            .collect();
        let population = capacity.iter().map(|&k| INITIAL_FILL * k).collect();

        // Polity adjacency from the initial borders: two polities are neighbors
        // if any cell of one borders a cell of the other.
        let mut adj: Vec<std::collections::BTreeSet<usize>> =
            vec![std::collections::BTreeSet::new(); n_pol];
        for cell in 0..n_cells {
            let Some(a) = world.society.control.get(cell).copied().flatten() else {
                continue;
            };
            let a = a as usize;
            if a >= n_pol {
                continue;
            }
            let Some(nbrs) = world.mesh.neighbors.get(cell) else {
                continue;
            };
            for &nb in nbrs {
                if let Some(b) = world.society.control.get(nb as usize).copied().flatten() {
                    let b = b as usize;
                    if b < n_pol && b != a {
                        adj[a].insert(b);
                        adj[b].insert(a);
                    }
                }
            }
        }
        let adjacency = adj.into_iter().map(|s| s.into_iter().collect()).collect();

        Self {
            polity_count: n_pol,
            population,
            capacity,
            aridity,
            courts: vec![agent::Court::default(); n_pol],
            elites: vec![0.0; n_pol],
            fiscal: vec![0.0; n_pol],
            instability: vec![0.0; n_pol],
            // Reset to ASAB_HIGH by Khaldun on the first tick (dynasty founded).
            asabiyyah: vec![1.0; n_pol],
            last_dynasty: vec![None; n_pol],
            adjacency,
            claims: Vec::new(),
            last_war: vec![i32::MIN; n_pol],
            religion_entities: vec![None; world.religions.religions.len()],
            active_megabeasts: Vec::new(),
            pending_prophecies: Vec::new(),
            dissolved: vec![false; n_pol],
            schism_parent: Vec::new(),
            border_changes: Vec::new(),
        }
    }
}

/// Number of cells a polity currently controls (used to detect dissolution
/// after a conquest).
pub(crate) fn polity_cell_count(world: &WorldData, pid: usize) -> usize {
    world
        .society
        .control
        .iter()
        .filter(|c| **c == Some(pid as u32))
        .count()
}

/// Agronomic carrying weight per biome, in relative units. Biome ids are
/// frozen schema (source of truth: `mapgen_world::biomes`); grassland is the
/// breadbasket, riparian / temperate forest fertile, deserts / ice barren, sea
/// uninhabited. History interprets biomes agronomically just as the renderer
/// interprets them visually.
fn cell_capacity(biome: u8) -> f32 {
    match biome {
        4 => 1.00,          // TEMPERATE_GRASSLAND
        14 => 0.95,         // RIPARIAN
        3 => 0.80,          // TEMPERATE_FOREST
        5 => 0.70,          // TEMPERATE_RAINFOREST
        8 => 0.60,          // TROPICAL_RAINFOREST
        9 => 0.55,          // TROPICAL_DRY_FOREST
        7 => 0.50,          // SAVANNA
        2 => 0.35,          // TAIGA
        10 => 0.25,         // SHRUBLAND
        1 => 0.10,          // TUNDRA
        0 | 6 | 11 => 0.05, // SNOW, DESERT, ALPINE
        _ => 0.0,           // SEA_SHALLOW(12), SEA_DEEP(13), UNASSIGNED
    }
}

/// Multiplier on raw territory capacity from the founding culture's
/// agriculture skill (0..100 → 0.6..1.4). The founder is the culture owning
/// the polity's capital cell.
fn agriculture_factor(world: &WorldData, pid: usize) -> f32 {
    let cap_cell = world.society.nations[pid].capital_cell as usize;
    let agri = world
        .cultures
        .culture_id
        .get(cap_cell)
        .copied()
        .flatten()
        .and_then(|cid| world.cultures.cultures.get(cid as usize))
        .map(|c| c.tech.agriculture)
        .unwrap_or(40);
    0.6 + 0.008 * agri as f32
}

/// Recompute a polity's carrying capacity from its *current* controlled cells —
/// called after conquest shifts borders, so Turchin's dynamics track the new
/// territory. Mirrors the rollup in `SimState::new`.
pub(crate) fn polity_capacity(world: &WorldData, pid: usize) -> f32 {
    let raw: f32 = (0..world.mesh.cell_count())
        .filter(|&c| world.society.control.get(c).copied().flatten() == Some(pid as u32))
        .map(|c| cell_capacity(world.climate.biome.get(c).copied().unwrap_or(0)))
        .sum();
    raw * agriculture_factor(world, pid)
}

/// The founding culture's military skill (0..100; 40 if unknown) — the martial
/// multiplier on a polity's war power.
pub(crate) fn polity_military(world: &WorldData, pid: usize) -> u8 {
    let cap = world.society.nations[pid].capital_cell as usize;
    world
        .cultures
        .culture_id
        .get(cap)
        .copied()
        .flatten()
        .and_then(|cid| world.cultures.cultures.get(cid as usize))
        .map(|c| c.tech.military)
        .unwrap_or(40)
}

/// A deterministic uniform draw in `[0, 1)` from a loop RNG. Avoids relying on
/// `rand::Rng::gen`'s float algorithm so the value is explicit and portable
/// (top 24 bits of a `u64`, 2⁻²⁴ resolution). No transcendental.
pub(crate) fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    ((rng.next_u64() >> 40) as f32) / ((1u64 << 24) as f32)
}

/// Derive the RNG seed for one loop in one year. Pure: depends only on the
/// base seed, the year, and the loop's stable [`LoopId`] — never on which other
/// loops exist or the order they run. That purity is the determinism property
/// the tests pin.
pub fn loop_seed(sim_seed: u64, year: i32, loop_id: LoopId) -> u64 {
    let year_seed = splitmix64(sim_seed, year as u64);
    splitmix64(year_seed, loop_id as u64)
}

/// Stream key for the agent layer's per-year RNG. Distinct from every
/// [`LoopId`] (which start at 1) so the agent stream is independent of the
/// causal loops — adding or removing a loop cannot perturb the dynasties.
const AGENT_STREAM: u64 = 64;

/// Run `params.years` of deterministic history against `world` in place: each
/// year the agent layer advances the ruling dynasties, then the six causal
/// loops run in canonical order. `rng` is the `Stage::History` sub-stream from
/// the pipeline; its first draw fixes the sim's base seed, from which every
/// per-year / per-loop / agent seed is derived.
pub fn run(world: &mut WorldData, params: HistoryParams, rng: &mut ChaCha8Rng) {
    let sim_seed = rng.next_u64();
    let mut state = SimState::new(world);
    let mut loops = default_loops();
    for year in 0..params.years {
        let year_seed = splitmix64(sim_seed, year as u64);
        // Agent layer first, so the loops see the current rulers.
        let mut arng = ChaCha8Rng::seed_from_u64(splitmix64(year_seed, AGENT_STREAM));
        agent::advance(world, &mut state, year, &mut arng);
        for lp in loops.iter_mut() {
            let mut lrng = ChaCha8Rng::seed_from_u64(splitmix64(year_seed, lp.id() as u64));
            let mut ctx = TickCtx {
                world,
                state: &mut state,
                year,
                rng: &mut lrng,
            };
            lp.tick(&mut ctx);
        }
    }

    // Post-sim: a beat repeated verbatim is less newsworthy each time, so the
    // major-event reel isn't dominated by one rivalry's serial battles.
    refine_salience(world);

    // Weave the (refined) causal event graph into narrative arcs + mythic ages
    // (this rebuilds `world.history`), then attach the territorial timeline the
    // sim accumulated so the map can be reconstructed at any past year.
    world.history = extract::build(world);
    world.history.border_changes = std::mem::take(&mut state.border_changes);
}

/// Salience multiplier applied per *prior* verbatim recurrence of an event's
/// canonical summary. 0.6 ⇒ a second identical line reads at 60% (dropping a
/// major beat out of the ≥0.8 reel), a third at 36%, etc. Keying on the summary
/// text — not actor entities — is deliberate: a hegemon's serial battles span
/// many successive rulers but render as the *same* sentence, while genuinely
/// distinct events (other polities, distinct hero sagas) keep unique text and
/// are untouched.
const REPEAT_DECAY: f32 = 0.6;

/// Discount the salience of verbatim-repeated event summaries. Deterministic:
/// events are processed in id order, so the *first* occurrence keeps full
/// salience and later identical ones recede.
fn refine_salience(world: &mut WorldData) {
    let mut seen: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for e in world.events.events.iter_mut() {
        let prior = seen.entry(e.summary_canonical.clone()).or_insert(0);
        if *prior > 0 {
            e.salience = (e.salience * REPEAT_DECAY.powi(*prior as i32)).clamp(0.0, 1.0);
        }
        *prior += 1;
    }
}

/// [`run`] with an injectable loop set — the seam tests use to count ticks and
/// verify ordering / stream-independence without the real (no-op in 4a) loops.
pub fn run_with_loops(
    world: &mut WorldData,
    params: HistoryParams,
    rng: &mut ChaCha8Rng,
    mut loops: Vec<Box<dyn CausalLoop>>,
) {
    let sim_seed = rng.next_u64();
    let mut state = SimState::new(world);
    for year in 0..params.years {
        for lp in loops.iter_mut() {
            let mut lrng = ChaCha8Rng::seed_from_u64(loop_seed(sim_seed, year, lp.id()));
            let mut ctx = TickCtx {
                world,
                state: &mut state,
                year,
                rng: &mut lrng,
            };
            lp.tick(&mut ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A loop that records `(id, year, first-draw)` per tick into a shared log —
    /// lets the tests observe execution count, ordering, and the exact RNG seed
    /// each loop received.
    struct Recorder {
        id: LoopId,
        log: Rc<RefCell<Vec<(LoopId, i32, u64)>>>,
    }

    impl CausalLoop for Recorder {
        fn id(&self) -> LoopId {
            self.id
        }
        fn tick(&mut self, ctx: &mut TickCtx) {
            self.log
                .borrow_mut()
                .push((self.id, ctx.year, ctx.rng.next_u64()));
        }
    }

    /// Drive `ids` for `years` years from a fixed base seed and return the log.
    fn drive_and_record(ids: &[LoopId], years: i32) -> Vec<(LoopId, i32, u64)> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let loops: Vec<Box<dyn CausalLoop>> = ids
            .iter()
            .map(|&id| {
                Box::new(Recorder {
                    id,
                    log: log.clone(),
                }) as Box<dyn CausalLoop>
            })
            .collect();
        let mut world = WorldData::default();
        let mut rng = ChaCha8Rng::seed_from_u64(0xC0FF_EE00);
        run_with_loops(&mut world, HistoryParams { years }, &mut rng, loops);
        Rc::try_unwrap(log).unwrap().into_inner()
    }

    #[test]
    fn driver_ticks_each_loop_once_per_year_in_order() {
        let log = drive_and_record(&[LoopId::Turchin, LoopId::Mearsheimer], 10);
        assert_eq!(log.len(), 20, "2 loops x 10 years = 20 ticks");
        assert_eq!(
            log.iter()
                .filter(|(id, _, _)| *id == LoopId::Turchin)
                .count(),
            10
        );
        // Turchin (first in the set) ticks years 0..10 in order.
        let years: Vec<i32> = log
            .iter()
            .filter(|(id, _, _)| *id == LoopId::Turchin)
            .map(|(_, y, _)| *y)
            .collect();
        assert_eq!(years, (0..10).collect::<Vec<_>>());
        // Within a year, Turchin runs before Mearsheimer (ORDER preserved).
        assert_eq!(log[0].0, LoopId::Turchin);
        assert_eq!(log[1].0, LoopId::Mearsheimer);
    }

    #[test]
    fn loop_seed_is_pure_distinct_per_loop_and_year() {
        let sim = 0xDEAD_BEEF_u64;
        assert_eq!(
            loop_seed(sim, 5, LoopId::Turchin),
            loop_seed(sim, 5, LoopId::Turchin),
            "same inputs must give the same seed"
        );
        assert_ne!(
            loop_seed(sim, 5, LoopId::Turchin),
            loop_seed(sim, 5, LoopId::Mearsheimer),
            "distinct loops must get distinct seeds in the same year"
        );
        assert_ne!(
            loop_seed(sim, 5, LoopId::Turchin),
            loop_seed(sim, 6, LoopId::Turchin),
            "the same loop must get distinct seeds across years"
        );
    }

    #[test]
    fn a_loops_stream_is_unaffected_by_adding_other_loops() {
        // The core determinism property: Mearsheimer's per-year seed is
        // identical whether it runs alone or alongside other loops, because the
        // seed is derived from its own id, not from a shared advancing stream.
        let alone = drive_and_record(&[LoopId::Mearsheimer], 5);
        let crowded = drive_and_record(&[LoopId::Turchin, LoopId::Mearsheimer, LoopId::Hero], 5);
        let pick = |log: &[(LoopId, i32, u64)]| -> Vec<u64> {
            log.iter()
                .filter(|(id, _, _)| *id == LoopId::Mearsheimer)
                .map(|(_, _, s)| *s)
                .collect()
        };
        assert_eq!(
            pick(&alone),
            pick(&crowded),
            "adding loops around Mearsheimer perturbed its RNG stream"
        );
    }

    #[test]
    fn order_constant_matches_default_loops() {
        let ids: Vec<LoopId> = default_loops().iter().map(|l| l.id()).collect();
        assert_eq!(ids, loops::ORDER.to_vec());
    }

    // --- 4b: Turchin demographic backbone -----------------------------------

    use mapgen_core::{EventKind, Nation};

    /// A minimal world with one polity controlling `n_cells` grassland cells
    /// at low precipitation — fertile enough to grow, arid enough to see
    /// droughts. Enough for the demographic loop; no cultures (agriculture
    /// defaults), no mesh geometry beyond the cell count.
    fn synthetic_world(n_cells: usize) -> WorldData {
        let mut w = WorldData::default();
        w.mesh.sites = vec![[0.0, 0.0]; n_cells];
        w.climate.biome = vec![4u8; n_cells]; // TEMPERATE_GRASSLAND
        w.climate.precipitation = vec![0.03f32; n_cells]; // arid-ish
        w.society.nations = vec![Nation {
            name: "Testria".to_string(),
            capital_cell: 0,
            color: [0, 0, 0],
        }];
        w.society.control = vec![Some(0u32); n_cells];
        w
    }

    fn run_synth(years: i32, seed: u64) -> WorldData {
        // Loops-only (no agent layer) so these stay focused on the demographic
        // backbone — the agent layer's dynastic events are covered separately.
        let mut w = synthetic_world(200);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        run_with_loops(&mut w, HistoryParams { years }, &mut rng, default_loops());
        w
    }

    #[test]
    fn demographic_events_accumulate_with_years() {
        let short = run_synth(40, 99).events.len();
        let long = run_synth(400, 99).events.len();
        assert!(
            long > short,
            "more simulated years should yield more events: {short} (40y) vs {long} (400y)"
        );
    }

    #[test]
    fn turchin_fires_both_famine_and_drought_over_a_long_run() {
        let w = run_synth(500, 99);
        assert!(!w.events.is_empty(), "expected demographic events");
        assert!(
            w.events
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Famine)),
            "a 500-year arid grassland run should suffer at least one famine"
        );
        assert!(
            w.events
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Drought)),
            "an arid polity should suffer at least one drought"
        );
    }

    #[test]
    fn turchin_output_is_deterministic() {
        let fingerprint = |w: &WorldData| -> Vec<(i32, String, Option<u32>, u32)> {
            w.events
                .events
                .iter()
                .map(|e| {
                    (
                        e.year,
                        format!("{:?}", e.kind),
                        e.location.map(|c| c.0),
                        e.salience.to_bits(),
                    )
                })
                .collect()
        };
        assert_eq!(
            fingerprint(&run_synth(300, 7)),
            fingerprint(&run_synth(300, 7)),
            "same seed must produce a byte-identical event log"
        );
    }

    #[test]
    fn demographic_events_have_valid_fields() {
        let w = run_synth(300, 7);
        for (i, e) in w.events.events.iter().enumerate() {
            assert_eq!(e.id.0, i as u32, "event ids sequential from 0");
            assert!((0..300).contains(&e.year));
            assert!((0.0..=1.0).contains(&e.salience));
            assert_eq!(
                e.location,
                Some(mapgen_core::CellId(0)),
                "located at the capital cell"
            );
        }
    }

    // --- 4d: Khaldun asabiyyah dynamics -------------------------------------

    #[test]
    fn khaldun_renews_then_decays_asabiyyah_monotonically() {
        use crate::agent::Court;
        use crate::loops::khaldun::Khaldun;
        use crate::loops::{CausalLoop, TickCtx};

        let mut st = SimState {
            asabiyyah: vec![1.0],
            last_dynasty: vec![None],
            courts: vec![Court {
                dynasty: Some(EntityId(5)),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut world = WorldData::default();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut khaldun = Khaldun;

        // Year 0: a dynasty appears (None → Some) → cohesion is renewed high.
        {
            let mut ctx = TickCtx {
                world: &mut world,
                state: &mut st,
                year: 0,
                rng: &mut rng,
            };
            khaldun.tick(&mut ctx);
        }
        let high = st.asabiyyah[0];
        assert!(
            (0.85..=0.95).contains(&high),
            "renewed asabiyyah ~0.9, got {high}"
        );

        // Same dynasty thereafter: non-increasing and bounded in [0, 1].
        let mut prev = high;
        for year in 1..300 {
            let mut ctx = TickCtx {
                world: &mut world,
                state: &mut st,
                year,
                rng: &mut rng,
            };
            khaldun.tick(&mut ctx);
            let a = st.asabiyyah[0];
            assert!(
                a <= prev,
                "asabiyyah rose within a stable dynasty: {a} > {prev}"
            );
            assert!((0.0..=1.0).contains(&a), "asabiyyah out of [0,1]: {a}");
            prev = a;
        }
        assert!(
            prev < high,
            "asabiyyah should decay over 300 years of one dynasty"
        );
    }
}

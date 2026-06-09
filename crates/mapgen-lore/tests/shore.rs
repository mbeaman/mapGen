//! The multi-strand FAR-SHORE chronicle — `narrate_shore` / `select_shore`. The
//! Sundered Lanes' richest payoff: one landmass reached by faith AND settlement
//! AND the sword, woven into a single chronicle that NAMES the shore from the
//! `Event::far_shore` tags the gen carriers stamped.
//!
//! SYNTHETIC FIXTURE (Phase 5 — periodic planet). Before the periodic flip, seed
//! 9 was the canonical three-strand world (faith+colony+sword on one continent).
//! Periodicity merged the seam-straddling continents, and a scan of seeds 0..120
//! found NO world where all three carriers converge on a single shore (the colony
//! carrier settles unclaimed fringes; conquest seizes owned anchors; periodicity
//! stops them landing on the same landmass). The three-strand chronicle therefore
//! no longer occurs naturally — so the strand classifier and the 3-beat weave are
//! pinned here against a hand-built world that tags one continent with one event
//! of each strand. This tests the *logic* (group-by-`far_shore`, classify by
//! kind, weave one beat per strand) directly and robustly. The 2-strand reality
//! and the laneless guard ride real worlds below; the CLI round-trip rides a real
//! 2-strand seed in `mapgen-cli/tests/lore_cli.rs`.

use mapgen_core::{Continent, Event, EventId, EventKind, WorldData};
use mapgen_lore::voice::{Register, VoiceCard};
use mapgen_lore::{narrate_shore, select_shore};
use mapgen_testsupport::{planet_params, SUNDERED_SEEDS};
use mapgen_world::generate_full;

/// The reached shore's name in the synthetic fixture. Deliberately a token that
/// appears in NO strand summary, so any naming in the weave is tag-sourced.
const SHORE_NAME: &str = "Faloria";

/// A strand event tagged to continent 0 — generic summary that never names the
/// shore (the anti-false-green precondition of the weave test).
fn strand_event(id: u32, year: i32, kind: EventKind, summary: &str) -> Event {
    Event {
        id: EventId(id),
        year,
        kind,
        actors: Default::default(),
        patients: Default::default(),
        location: None,
        cause_ids: Default::default(),
        salience: 0.9,
        casus_belli: None,
        summary_canonical: summary.to_string(),
        far_shore: Some(0),
    }
}

/// A hand-built world whose continent 0 ("Faloria") is reached by all three
/// strands — one `FaithCrossed`, one `CityFounded`, one `Siege`, each tagged
/// `far_shore = Some(0)`. `select_shore`/`narrate_shore` read only `events`,
/// `continents[idx].name`, and the voice, so a defaulted world + these two
/// fields is a complete fixture. (Reassign-on-default is clearest here: the two
/// fields we set are a tiny corner of `WorldData`, and `events` is nested.)
#[allow(clippy::field_reassign_with_default)]
fn three_strand_world() -> WorldData {
    let mut w = WorldData::default();
    w.continents = vec![Continent {
        name: SHORE_NAME.to_string(),
        centroid: [0.0, 0.0],
        cell_count: 256,
    }];
    w.events.events = vec![
        strand_event(
            0,
            110,
            EventKind::FaithCrossed,
            "a faith was carried over the water",
        ),
        strand_event(
            1,
            150,
            EventKind::CityFounded,
            "a colony was planted across the sea",
        ),
        strand_event(
            2,
            190,
            EventKind::Siege,
            "a beachhead was stormed beyond the water",
        ),
    ];
    w
}

#[test]
fn select_shore_picks_a_three_strand_far_shore_on_the_diverse_seed() {
    let w = three_strand_world();
    let pick = select_shore(&w).expect("the synthetic world reaches a far shore");

    // The picked shore is reached by ALL THREE strands — the richest story. If the
    // classifier stopped routing a kind to its strand this fails LOUDLY here, not
    // silently in the weave. (Selector: max distinct strands.)
    assert_eq!(
        pick.distinct_strands(),
        3,
        "the pick must be reached by faith+colony+sword; got faith={} colony={} sword={}",
        pick.faith.len(),
        pick.colony.len(),
        pick.sword.len()
    );

    // Every member event actually carries the tag for THIS shore — the structured
    // place tag, not a coincidence of presence. (Each strand is gathered ONLY from
    // events whose `far_shore == pick.continent`.)
    for &id in pick.faith.iter().chain(&pick.colony).chain(&pick.sword) {
        assert_eq!(
            w.events.events[id.0 as usize].far_shore,
            Some(pick.continent),
            "every strand event must be tagged with the picked shore (event {})",
            id.0
        );
    }
}

#[test]
fn the_far_shore_chronicle_weaves_all_three_strands_each_naming_the_shore() {
    // End-to-end MULTI-STRAND consumer of `Event::far_shore`. `narrate_shore` picks
    // the three-strand shore and must weave ONE beat per strand, EACH naming the
    // shore. The anti-false-green discriminator: the shore name is in NONE of the
    // strand events' summaries (the FaithCrossed / CityFounded / Siege summaries all
    // say only "...the water" / carry no place), so each beat's "...upon the shore of
    // {shore}" could ONLY come from the weaver reading the tags. Each beat is a
    // distinct phrase gated on its strand, so dropping ANY of the three tags (faith
    // in diffusion.rs, colony in colonization.rs, sword in mearsheimer.rs) removes
    // exactly that beat → trips.
    let mut w = three_strand_world();
    let pick = select_shore(&w).expect("the synthetic world reaches a far shore");
    assert_eq!(
        pick.distinct_strands(),
        3,
        "fixture precondition: three strands"
    );
    let shore = w.continents[pick.continent as usize].name.clone();
    assert!(!shore.is_empty(), "the reached continent must be named");

    // Precondition: NO strand event summary names the shore — so any naming in the
    // chronicle is tag-sourced, never echoed from a summary.
    for &id in pick.faith.iter().chain(&pick.colony).chain(&pick.sword) {
        let s = &w.events.events[id.0 as usize].summary_canonical;
        assert!(
            !s.contains(&shore),
            "precondition: a strand summary must not already name the shore '{shore}': {s}"
        );
    }

    let voice = VoiceCard::for_register(Register::MonasticChronicle);
    let work = narrate_shore(&mut w, &voice).unwrap();

    // All three strand beats, EACH naming the shore — the woven payoff.
    let faith_beat = format!("faith first took root upon the shore of {shore}");
    let colony_beat = format!("first colony was planted upon the shore of {shore}");
    let sword_beat = format!("sword first won a foothold upon the shore of {shore}");
    assert!(
        work.body.contains(&faith_beat),
        "missing the faith beat naming {shore}; got: {}",
        work.body
    );
    assert!(
        work.body.contains(&colony_beat),
        "missing the colony beat naming {shore}; got: {}",
        work.body
    );
    assert!(
        work.body.contains(&sword_beat),
        "missing the sword beat naming {shore}; got: {}",
        work.body
    );

    // Titled on the shore, and cites every strand event it wove.
    assert!(
        work.title.contains(&shore),
        "the chronicle must be titled on the shore; got: {}",
        work.title
    );
    for &id in pick.faith.iter().chain(&pick.colony).chain(&pick.sword) {
        assert!(
            work.references.contains(&id),
            "the chronicle must cite every strand event it wove (missing {})",
            id.0
        );
    }
}

#[test]
fn a_laneless_world_has_no_far_shore_to_chronicle() {
    // The both-directions guard: a SUNDERED seed has no crossable lane — no carrier
    // crosses water, so nothing is tagged, `select_shore` is None, and
    // `narrate_shore` ERRS rather than inventing a shore. A weave that fired
    // unconditionally would pass the positive tests but fail here. (Rides a real
    // world, unlike the synthetic positive fixture: the negative must prove the
    // generator really produces NO far_shore tag on a sundered world. Periodic flip
    // note: the old reference seed 42 became a crossing seed, so this uses a
    // re-derived SUNDERED_SEED.)
    let mut w = generate_full(planet_params(SUNDERED_SEEDS[0]));
    assert!(
        select_shore(&w).is_none(),
        "a sundered world reaches no far shore"
    );
    let voice = VoiceCard::for_register(Register::MonasticChronicle);
    assert!(
        narrate_shore(&mut w, &voice).is_err(),
        "narrate_shore must err on a world with no far shore"
    );
}

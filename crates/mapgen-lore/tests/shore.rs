//! The multi-strand FAR-SHORE chronicle — `narrate_shore` / `select_shore`. The
//! Sundered Lanes' richest payoff: one landmass reached by faith AND settlement
//! AND the sword, woven into a single chronicle that NAMES the shore from the
//! `Event::far_shore` tags the gen carriers stamped. seed 9 (planet scale) is the
//! canonical fixture — its continent is reached by all three strands (confirmed
//! by the per-kind probe that gated this increment), the only thing on the
//! canonical map that exercises the full "by trade, by faith, by sword" weave.

use mapgen_lore::voice::{Register, VoiceCard};
use mapgen_lore::{narrate_shore, select_shore};
use mapgen_testsupport::{planet_params, REFERENCE_SEED};
use mapgen_world::generate_full;

/// The strand-diverse fixture: seed 9 reaches one shore by faith, colony, AND
/// conquest — the only canonical seed with a full three-strand shore.
const THREE_STRAND_SEED: u64 = 9;

#[test]
fn select_shore_picks_a_three_strand_far_shore_on_the_diverse_seed() {
    let w = generate_full(planet_params(THREE_STRAND_SEED));
    let pick = select_shore(&w).expect("seed 9 reaches a far shore");

    // The picked shore is reached by ALL THREE strands — the richest story. If a
    // future change shifts seed 9's strand mix this fails LOUDLY here, not silently
    // in the weave. (Selector: max distinct strands, so a 3-strand shore always
    // outranks the faith+sword and faith-only shores the probe also found.)
    assert_eq!(
        pick.distinct_strands(),
        3,
        "seed 9's pick must be reached by faith+colony+sword; got faith={} colony={} sword={}",
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
    // say only "a far shore" / carry no place), so each beat's "...upon the shore of
    // {shore}" could ONLY come from the weaver reading the tags. Each beat is a
    // distinct phrase gated on its strand, so dropping ANY of the three tags (faith
    // in diffusion.rs, colony in colonization.rs, sword in mearsheimer.rs) removes
    // exactly that beat → trips. (Mutation-verified for the colony + sword tags.)
    let mut w = generate_full(planet_params(THREE_STRAND_SEED));
    let pick = select_shore(&w).expect("seed 9 reaches a far shore");
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
    // The both-directions guard: seed 42 (a sundered seed) is laneless — no carrier
    // crosses water, so nothing is tagged, `select_shore` is None, and
    // `narrate_shore` ERRS rather than inventing a shore. A weave that fired
    // unconditionally would pass the positive test but fail here.
    let mut w = generate_full(planet_params(REFERENCE_SEED));
    assert!(
        select_shore(&w).is_none(),
        "a laneless world reaches no far shore"
    );
    let voice = VoiceCard::for_register(Register::MonasticChronicle);
    assert!(
        narrate_shore(&mut w, &voice).is_err(),
        "narrate_shore must err on a world with no far shore"
    );
}

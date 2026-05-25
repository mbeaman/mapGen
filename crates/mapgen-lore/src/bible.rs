//! The WORLD BIBLE: a stable, factual digest of the world the chronicler may
//! draw proper nouns from — peoples, faiths, realms, named heights, and mythic
//! ages. Sits (prompt-cached) at the top of every narration for a given world.
//! Every name here is part of the NER-allowed context.

use mapgen_core::{PantheonPattern, WorldData};

/// Render the world bible as a markdown-ish text block.
pub fn world_bible(world: &WorldData) -> String {
    let mut s = String::from("# WORLD BIBLE\n");

    s.push_str("\n## The Land\n");
    let ranges: Vec<&str> = world
        .mountain_ranges
        .iter()
        .map(|m| m.name.as_str())
        .filter(|n| !n.is_empty())
        .collect();
    if ranges.is_empty() {
        s.push_str("A single continent of mountains, rivers, and coasts.\n");
    } else {
        s.push_str(&format!(
            "A single continent. Its named heights: {}.\n",
            ranges.join(", ")
        ));
    }

    if !world.cultures.cultures.is_empty() {
        s.push_str("\n## Peoples\n");
        for c in &world.cultures.cultures {
            s.push_str(&format!("- {}\n", c.name));
        }
    }

    if !world.religions.religions.is_empty() {
        s.push_str("\n## Faiths\n");
        for r in &world.religions.religions {
            s.push_str(&format!(
                "- {}, a {} faith\n",
                r.name,
                pantheon_word(r.pantheon)
            ));
        }
    }

    if !world.society.nations.is_empty() {
        s.push_str("\n## Realms\n");
        for n in &world.society.nations {
            s.push_str(&format!("- {}\n", n.name));
        }
    }

    if !world.history.ages.is_empty() {
        s.push_str("\n## Ages\n");
        for a in &world.history.ages {
            s.push_str(&format!("- {} ({}–{})\n", a.name, a.start_year, a.end_year));
        }
    }

    s
}

fn pantheon_word(p: PantheonPattern) -> &'static str {
    match p {
        PantheonPattern::Mono => "monotheistic",
        PantheonPattern::Poly => "polytheistic",
        PantheonPattern::Dual => "dualist",
        PantheonPattern::Animism => "animist",
        PantheonPattern::Ancestor => "ancestor-venerating",
        PantheonPattern::CosmicOrder => "cosmic-order",
    }
}

//! Assembles the full narration prompt: a cached system block of chronicler
//! rules, and a user block of WORLD BIBLE + ENTITY CONTEXT + SUPPLIED EVENTS +
//! VOICE + SCHEMA. Pure function of the world, the focal event, and the voice.

use mapgen_core::{EventId, WorldData};

use crate::bible::world_bible;
use crate::context::{entity_context, event_closure, event_slice_text};
use crate::schema::SCHEMA_HINT;
use crate::voice::VoiceCard;

/// The chronicler's standing rules — stable, so the API can prompt-cache it.
pub const SYSTEM_RULES: &str = "\
You are an in-world chronicler. Narrate ONLY from the SUPPLIED EVENTS. Refer ONLY \
to entities named in the WORLD BIBLE or ENTITY CONTEXT — invent no new persons, \
places, gods, artifacts, or dates. Where the events do not tell you something, \
write \"[lacuna]\" rather than guessing. Stay in the requested VOICE. Respond with \
valid JSON exactly per the SCHEMA, and nothing else.";

/// A narration prompt, split at the prompt-cache boundary: `system` (standing
/// rules) and `world_bible` are stable per world and marked cacheable by the
/// real client; `focal` (entity context + events + voice + schema) is volatile
/// per call.
#[derive(Clone, Debug)]
pub struct Prompt {
    pub system: String,
    pub world_bible: String,
    pub focal: String,
}

impl Prompt {
    /// The full user-message text (bible + focal), for clients/tests that don't
    /// distinguish the cache boundary.
    pub fn user_text(&self) -> String {
        format!("{}\n{}", self.world_bible, self.focal)
    }
}

/// Build the prompt to narrate `focal` (and its causal lead-up) in `voice`.
pub fn build(world: &WorldData, focal: EventId, voice: &VoiceCard) -> Prompt {
    let slice = event_closure(world, focal);

    let mut focal_block = String::from("# ENTITY CONTEXT\n");
    focal_block.push_str(&entity_context(world, &slice));
    focal_block.push_str("\n# SUPPLIED EVENTS\n");
    focal_block.push_str(&event_slice_text(world, &slice));
    focal_block.push_str(&format!(
        "\n# VOICE\nWrite as {}. {}\n",
        voice.author,
        voice.register.style_hint()
    ));
    focal_block.push_str(&format!("\n# SCHEMA\n{SCHEMA_HINT}\n"));

    Prompt {
        system: SYSTEM_RULES.to_string(),
        world_bible: world_bible(world),
        focal: focal_block,
    }
}

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

/// A system/user prompt pair ready for an [`crate::client::LlmClient`].
#[derive(Clone, Debug)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

/// Build the prompt to narrate `focal` (and its causal lead-up) in `voice`.
pub fn build(world: &WorldData, focal: EventId, voice: &VoiceCard) -> Prompt {
    let slice = event_closure(world, focal);

    let mut user = String::new();
    user.push_str(&world_bible(world));
    user.push_str("\n# ENTITY CONTEXT\n");
    user.push_str(&entity_context(world, &slice));
    user.push_str("\n# SUPPLIED EVENTS\n");
    user.push_str(&event_slice_text(world, &slice));
    user.push_str(&format!(
        "\n# VOICE\nWrite as {}. {}\n",
        voice.author,
        voice.register.style_hint()
    ));
    user.push_str(&format!("\n# SCHEMA\n{SCHEMA_HINT}\n"));

    Prompt {
        system: SYSTEM_RULES.to_string(),
        user,
    }
}

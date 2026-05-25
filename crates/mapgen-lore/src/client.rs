//! The narrator backend abstraction. The real Anthropic client (Phase 5e, behind
//! the `lore` feature) implements [`LlmClient`]; tests inject fakes. When no
//! client is configured, the engine narrates with the deterministic template
//! narrator ([`crate::template`]) instead — so narration always works offline.

use crate::prompt::Prompt;

/// A backend that turns a [`Prompt`] into a raw text response (expected to
/// contain the JSON of a [`crate::schema::ChronicleDraft`]).
pub trait LlmClient {
    fn complete(&self, prompt: &Prompt) -> anyhow::Result<String>;
}

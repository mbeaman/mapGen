//! The real Anthropic Messages-API client. Compiled **only** with the `lore`
//! feature, so a stock build pulls in no network deps and cannot make a paid
//! call. A blocking `ureq` POST keeps the engine synchronous (no async runtime).
//!
//! Prompt caching: the standing rules and the world bible are sent as
//! `cache_control: ephemeral` blocks, so across the (≤20) calls for one world the
//! large stable prefix is reused — only the per-event focal block is fresh.

use std::time::Duration;

use crate::client::LlmClient;
use crate::prompt::Prompt;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
/// Architecture cap (`max_tokens_per_call`).
const MAX_TOKENS: u32 = 4000;
/// Overall per-call deadline. ureq's default read timeout is *none* (a stalled
/// response would hang the single-threaded sidecar forever); this bounds it
/// while comfortably covering a full 4000-token generation.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// A Claude narration backend, keyed by `ANTHROPIC_API_KEY`.
pub struct AnthropicClient {
    api_key: String,
    model: String,
    agent: ureq::Agent,
}

impl AnthropicClient {
    /// Build from the environment: `ANTHROPIC_API_KEY` (required) and
    /// `MAPGEN_LORE_MODEL` (optional override). Returns `None` when no key is
    /// set — the caller then narrates offline with the template narrator, so a
    /// missing key is a graceful downgrade, not an error.
    pub fn from_env() -> Option<AnthropicClient> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())?;
        let model = std::env::var("MAPGEN_LORE_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        Some(AnthropicClient {
            api_key,
            model,
            agent,
        })
    }

    /// The model this client will call (for logging).
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl LlmClient for AnthropicClient {
    fn complete(&self, prompt: &Prompt) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": [
                { "type": "text", "text": prompt.system, "cache_control": { "type": "ephemeral" } }
            ],
            "messages": [
                { "role": "user", "content": [
                    { "type": "text", "text": prompt.world_bible, "cache_control": { "type": "ephemeral" } },
                    { "type": "text", "text": prompt.focal }
                ]}
            ]
        });

        let resp = match self
            .agent
            .post(API_URL)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", API_VERSION)
            .set("content-type", "application/json")
            .send_json(body)
        {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                let detail = r.into_string().unwrap_or_default();
                anyhow::bail!("Anthropic API returned {code}: {detail}");
            }
            Err(e) => anyhow::bail!("Anthropic request failed: {e}"),
        };

        let v: serde_json::Value = resp.into_json()?;
        // `content` is an array of blocks; concatenate the text of each.
        let text: String = v["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if text.trim().is_empty() {
            anyhow::bail!("Anthropic response contained no text content: {v}");
        }
        Ok(text)
    }
}

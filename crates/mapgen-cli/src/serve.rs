//! `mapgen serve` — a native narration sidecar wrapping `mapgen_lore::narrate`
//! behind `POST /narrate`, so the browser frontend can request a chronicle
//! without the API key ever leaving this process. Compiled only with the `lore`
//! feature. Blocking, single-threaded — fine for a single local user.

use std::io::{Cursor, Read};

use anyhow::{Context, Result};
use mapgen_core::WorldData;
use mapgen_lore::{narrate, select_focal, AnthropicClient, LlmClient, Register, VoiceCard};
use serde::Deserialize;
use tiny_http::{Header, Method, Response, Server, StatusCode};

type Resp = Response<Cursor<Vec<u8>>>;

#[derive(Deserialize)]
struct NarrateReq {
    /// The world to narrate. If omitted, the server's `--in` world is used.
    #[serde(default)]
    world: Option<WorldData>,
    #[serde(default = "default_event")]
    event: String,
    #[serde(default = "default_voice")]
    voice: String,
}

fn default_event() -> String {
    "auto-major-war".to_string()
}
fn default_voice() -> String {
    "monastic-chronicle".to_string()
}

/// Serve until interrupted. `startup` is the optional `--in` world used when a
/// request omits its own `world`.
pub fn run(port: u16, startup: Option<WorldData>) -> Result<()> {
    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("could not bind 127.0.0.1:{port}: {e}"))?;
    eprintln!("mapgen serve → http://127.0.0.1:{port}  (POST /narrate, GET /health)");
    eprintln!(
        "  narration backend: {}",
        if AnthropicClient::from_env().is_some() {
            "Claude (ANTHROPIC_API_KEY set)"
        } else {
            "offline template (no ANTHROPIC_API_KEY)"
        }
    );

    for mut req in server.incoming_requests() {
        let response = route(&mut req, startup.as_ref());
        let _ = req.respond(response);
    }
    Ok(())
}

/// Reject request bodies larger than this (a serialized world is a few MB).
const MAX_BODY: u64 = 64 * 1024 * 1024;

fn route(req: &mut tiny_http::Request, startup: Option<&WorldData>) -> Resp {
    // Capture the Origin before consuming the body, so CORS can be scoped to it.
    let origin = req
        .headers()
        .iter()
        .find(|h| h.field.to_string().eq_ignore_ascii_case("origin"))
        .map(|h| h.value.as_str().to_string());

    let base = match (req.method().clone(), strip_query(req.url())) {
        (Method::Options, _) => empty(204), // CORS preflight
        (Method::Get, "/health") => json(200, r#"{"status":"ok"}"#),
        (Method::Post, "/narrate") => {
            let mut body = String::new();
            // Bound the read so an oversized body can't exhaust memory.
            if req
                .as_reader()
                .take(MAX_BODY)
                .read_to_string(&mut body)
                .is_err()
            {
                error(
                    400,
                    "could not read request body (or it exceeded the size limit)",
                )
            } else {
                match narrate_request(&body, startup) {
                    Ok(work_json) => json(200, &work_json),
                    Err(e) => error(400, &e.to_string()),
                }
            }
        }
        _ => error(404, "not found"),
    };
    cors(base, origin.as_deref())
}

fn strip_query(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

fn narrate_request(body: &str, startup: Option<&WorldData>) -> Result<String> {
    let req: NarrateReq = serde_json::from_str(body).context("invalid JSON request body")?;
    let mut world = req
        .world
        .or_else(|| startup.cloned())
        .ok_or_else(|| anyhow::anyhow!("no world supplied (POST a `world`, or start with --in)"))?;
    let register = Register::parse(&req.voice)
        .ok_or_else(|| anyhow::anyhow!("unknown voice '{}'", req.voice))?;
    let card = VoiceCard::for_register(register);
    let focal = select_focal(&world, &req.event)?;

    let client = AnthropicClient::from_env();
    let narrator: Option<&dyn LlmClient> = client.as_ref().map(|c| c as &dyn LlmClient);
    let work = narrate(&mut world, focal, &card, narrator)?;
    Ok(serde_json::to_string(&work)?)
}

// --- response helpers -------------------------------------------------------

fn json(status: u16, body: &str) -> Resp {
    Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", "application/json"))
}

fn error(status: u16, message: &str) -> Resp {
    let body = serde_json::json!({ "error": message }).to_string();
    json(status, &body)
}

fn empty(status: u16) -> Resp {
    Response::from_string("").with_status_code(StatusCode(status))
}

/// Allow only a localhost browser origin (the dev frontend) to call the sidecar
/// — not any random site the user happens to be visiting (which could otherwise
/// spend their API credits and read their generated worlds). Non-localhost
/// origins get `null`, which browsers reject. (curl and other non-browser
/// clients don't enforce CORS, so the offline/manual paths are unaffected.)
fn cors(resp: Resp, origin: Option<&str>) -> Resp {
    let allow = match origin {
        Some(o) if is_localhost(o) => o,
        _ => "null",
    };
    resp.with_header(header("Access-Control-Allow-Origin", allow))
        .with_header(header("Access-Control-Allow-Methods", "POST, GET, OPTIONS"))
        .with_header(header("Access-Control-Allow-Headers", "Content-Type"))
}

fn is_localhost(origin: &str) -> bool {
    ["http://localhost", "http://127.0.0.1"]
        .iter()
        .any(|p| origin == *p || origin.starts_with(&format!("{p}:")))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrate_request_rejects_malformed_and_unknown_voice() {
        // Both fail before any client/network call — deterministic, no key needed.
        assert!(narrate_request("not json", None).is_err());

        let w = mapgen_world::generate_full(mapgen_world::GenerateParams {
            seed: 42,
            cell_count: 4_000,
            ..Default::default()
        });
        let body = format!(
            r#"{{"world":{},"event":"auto-major-war","voice":"bogus-voice"}}"#,
            serde_json::to_string(&w).unwrap()
        );
        assert!(narrate_request(&body, None).is_err()); // unknown voice → Err
    }

    #[test]
    fn is_localhost_only_allows_loopback_origins() {
        assert!(is_localhost("http://localhost:5173"));
        assert!(is_localhost("http://127.0.0.1:8080"));
        assert!(!is_localhost("https://evil.example.com"));
        assert!(!is_localhost("http://localhost.evil.com"));
    }
}

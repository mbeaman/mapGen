//! `mapgen serve` — a native narration sidecar wrapping `mapgen_lore::narrate`
//! behind `POST /narrate`, so the browser frontend can request a chronicle
//! without the API key ever leaving this process. Compiled only with the `lore`
//! feature. Blocking, single-threaded — fine for a single local user.

use std::io::Cursor;

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

fn route(req: &mut tiny_http::Request, startup: Option<&WorldData>) -> Resp {
    match (req.method().clone(), req.url().to_string().as_str()) {
        (Method::Options, _) => cors(empty(204)), // CORS preflight
        (Method::Get, "/health") => cors(json(200, r#"{"status":"ok"}"#)),
        (Method::Post, "/narrate") => {
            let mut body = String::new();
            if req.as_reader().read_to_string(&mut body).is_err() {
                return cors(error(400, "could not read request body"));
            }
            match narrate_request(&body, startup) {
                Ok(work_json) => cors(json(200, &work_json)),
                Err(e) => cors(error(400, &e.to_string())),
            }
        }
        _ => cors(error(404, "not found")),
    }
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

/// Allow the browser frontend (a different origin) to call the sidecar.
fn cors(resp: Resp) -> Resp {
    resp.with_header(header("Access-Control-Allow-Origin", "*"))
        .with_header(header("Access-Control-Allow-Methods", "POST, GET, OPTIONS"))
        .with_header(header("Access-Control-Allow-Headers", "Content-Type"))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header is valid")
}

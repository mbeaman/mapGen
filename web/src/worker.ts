/// WASM lives here, off the main thread, so generate (~5 s) never freezes
/// the UI. The `WorldHandle`/`Generation` are opaque WASM objects that
/// cannot cross the worker boundary, so they stay here: the main thread
/// sends commands and gets back progress + SVG strings.
import init, { Generation, type WorldHandle } from "../pkg/mapgen_wasm";

let ready: Promise<unknown> | null = null;
let world: WorldHandle | null = null;

/// Mirrors the Rust `StageInfo` struct (a plain serializable object).
export interface StageInfo {
  index: number;
  total: number;
  progress: number;
  label: string;
  stage: string;
  sub: number;
  sub_total: number;
}

/// A persisted, NER-validated chronicle — mirrors the Rust `Work`.
export interface Work {
  title: string;
  body: string;
  in_world_author: string;
  references: number[];
  lacunae: string[];
  written_year: number;
}

export type WorkerRequest =
  | { type: "generate"; seed: string; cells: number; nations: number; style: string }
  | { type: "render"; style: string }
  | { type: "narrate"; event: string; voice: string; sidecar: string };

export type WorkerResponse =
  | { type: "ready" }
  | { type: "progress"; info: StageInfo }
  | { type: "frame"; svg: string; info: StageInfo }
  | { type: "generated"; svg: string; genMs: number; totalMs: number; frameCount: number }
  | { type: "rendered"; svg: string; ms: number }
  | { type: "chronicle"; work: Work }
  | { type: "error"; message: string };

const post = (msg: WorkerResponse) => (self as DedicatedWorkerGlobalScope).postMessage(msg);

const ensureReady = async () => {
  if (!ready) {
    ready = init().then(() => post({ type: "ready" }));
  }
  await ready;
};

/// Progressive richness: render the partial world in the richest style
/// whose inputs exist by this stage. Ornate is never rendered mid-build
/// (it is 2–3× costlier) — the final frame handles the chosen style.
const styleForStage = (stage: string): string => {
  switch (stage) {
    case "terrain":
    case "erosion":
      return "greyscale";
    case "hydrology":
    case "ocean":
    case "climate":
    case "biomes":
      return "biomes";
    default:
      return "cultures";
  }
};

self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  try {
    await ensureReady();
    const msg = e.data;

    if (msg.type === "generate") {
      const t0 = performance.now();
      world?.free();
      world = null;

      const gen = new Generation(BigInt(msg.seed), msg.cells, msg.nations);
      let frameCount = 0;
      for (;;) {
        const info = gen.step() as StageInfo | undefined;
        if (!info) break;
        post({ type: "progress", info });
        const svg = gen.render(styleForStage(info.stage));
        post({ type: "frame", svg, info });
        frameCount++;
      }
      const genMs = performance.now() - t0;

      world = gen.finish();
      const svg = world.render(msg.style);
      const totalMs = performance.now() - t0;
      post({ type: "generated", svg, genMs, totalMs, frameCount });
    } else if (msg.type === "render") {
      if (!world) {
        post({ type: "error", message: "no world generated yet" });
        return;
      }
      const t0 = performance.now();
      const svg = world.render(msg.style);
      post({ type: "rendered", svg, ms: performance.now() - t0 });
    } else if (msg.type === "narrate") {
      if (!world) {
        post({ type: "error", message: "no world generated yet" });
        return;
      }
      // Serialize the world here (it can't cross the worker boundary as a WASM
      // object) and POST it to the native sidecar, which holds the API key.
      // Concatenate the body string so the multi-MB world JSON isn't parsed and
      // re-stringified.
      const worldJson = world.worldJson();
      const body = `{"world":${worldJson},"event":${JSON.stringify(msg.event)},"voice":${JSON.stringify(msg.voice)}}`;
      let resp: Response;
      try {
        resp = await fetch(`${msg.sidecar}/narrate`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body,
        });
      } catch {
        post({
          type: "error",
          message: `could not reach the narration sidecar at ${msg.sidecar} — start it with: cargo run -p mapgen-cli --features lore -- serve`,
        });
        return;
      }
      if (!resp.ok) {
        const detail = await resp.text().catch(() => "");
        post({ type: "error", message: `narration failed (${resp.status}): ${detail}` });
        return;
      }
      const work = (await resp.json()) as Work;
      post({ type: "chronicle", work });
    }
  } catch (err) {
    post({ type: "error", message: err instanceof Error ? err.message : String(err) });
  }
};

// Kick off WASM init eagerly so the module is warm before the first generate.
void ensureReady();

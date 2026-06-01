/// WASM lives here, off the main thread, so generate (~5 s) never freezes
/// the UI. The `WorldHandle`/`Generation` are opaque WASM objects that
/// cannot cross the worker boundary, so they stay here: the main thread
/// sends commands and gets back progress + SVG strings.
import init, { Generation, type WorldHandle } from "../pkg/mapgen_wasm";
import { styleForStage } from "./sector";

let ready: Promise<unknown> | null = null;
// The whole-world (level-0) handle and the current refined sector (Phase 7).
// `sector === null` means we're viewing the root world. The root is kept so
// drilling back up never regenerates; sectors are cheap to re-refine on demand,
// and `refineSector` is a method on the root (its society is projected in).
let root: WorldHandle | null = null;
let sector: WorldHandle | null = null;
/// Cells per refined sector — finer than the parent yet quick (~100 ms).
const SECTOR_CELLS = 4_000;
const view = (): WorldHandle | null => sector ?? root;

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

export type Scale = "continent" | "planet";

export type WorkerRequest =
  | { type: "generate"; seed: string; cells: number; nations: number; style: string; scale: Scale }
  | { type: "render"; style: string }
  | { type: "refine"; level: number; sx: number; sy: number; style: string }
  | { type: "renderYear"; style: string; year: number }
  | { type: "narrate"; event: string; voice: string; sidecar: string };

export type WorkerResponse =
  | { type: "ready" }
  | { type: "progress"; info: StageInfo }
  | { type: "frame"; svg: string; info: StageInfo }
  | {
      type: "generated";
      svg: string;
      genMs: number;
      totalMs: number;
      frameCount: number;
      historyYears: number[];
    }
  | { type: "rendered"; svg: string; ms: number }
  | { type: "refined"; svg: string; level: number; sx: number; sy: number; ms: number }
  | { type: "yearFrame"; svg: string; year: number }
  | { type: "chronicle"; work: Work }
  | { type: "error"; message: string };

const post = (msg: WorkerResponse) => (self as DedicatedWorkerGlobalScope).postMessage(msg);

const ensureReady = async () => {
  if (!ready) {
    ready = init().then(() => post({ type: "ready" }));
  }
  await ready;
};

self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  try {
    await ensureReady();
    const msg = e.data;

    if (msg.type === "generate") {
      const t0 = performance.now();
      root?.free();
      sector?.free();
      root = null;
      sector = null;

      // Planet scale runs the identical pipeline on the planet preset (wide 2:1
      // globe, many plates → many continents); the worker stays style-agnostic,
      // so the planisphere-vs-continental choice rides in on `msg.style` (the
      // main thread resolves it via `navStyle`).
      const gen =
        msg.scale === "planet"
          ? Generation.planet(BigInt(msg.seed), msg.cells, msg.nations)
          : new Generation(BigInt(msg.seed), msg.cells, msg.nations);
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

      root = gen.finish();
      const svg = root.render(msg.style);
      const totalMs = performance.now() - t0;
      const historyYears = Array.from(root.historyYears());
      post({ type: "generated", svg, genMs, totalMs, frameCount, historyYears });
    } else if (msg.type === "refine") {
      if (!root) {
        post({ type: "error", message: "no world generated yet" });
        return;
      }
      const t0 = performance.now();
      // Level 0 = back to the whole world; otherwise (re-)refine the sector.
      // Sectors are stateless, so navigating up just re-refines the parent.
      const prev = sector;
      sector =
        msg.level === 0 ? null : root.refineSector(msg.level, msg.sx, msg.sy, SECTOR_CELLS);
      prev?.free();
      const svg = view()!.render(msg.style);
      post({
        type: "refined",
        svg,
        level: msg.level,
        sx: msg.sx,
        sy: msg.sy,
        ms: performance.now() - t0,
      });
    } else if (msg.type === "render") {
      const v = view();
      if (!v) {
        post({ type: "error", message: "no world generated yet" });
        return;
      }
      const t0 = performance.now();
      const svg = v.render(msg.style);
      post({ type: "rendered", svg, ms: performance.now() - t0 });
    } else if (msg.type === "renderYear") {
      // Time-slider applies to the whole world (history is world-scale).
      if (!root) {
        post({ type: "error", message: "no world generated yet" });
        return;
      }
      const svg = root.renderAtYear(msg.style, msg.year);
      post({ type: "yearFrame", svg, year: msg.year });
    } else if (msg.type === "narrate") {
      // Narration always targets the whole world (it owns the history/events);
      // a refined sector has no chronicle of its own.
      const world = root;
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
      // Bound the wait so a stalled sidecar/model can't leave the UI spinning.
      const ctrl = new AbortController();
      const timer = setTimeout(() => ctrl.abort(), 125_000);
      let resp: Response;
      try {
        resp = await fetch(`${msg.sidecar}/narrate`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body,
          signal: ctrl.signal,
        });
      } catch (err) {
        const timedOut = err instanceof DOMException && err.name === "AbortError";
        post({
          type: "error",
          message: timedOut
            ? "narration timed out — the sidecar or model took too long"
            : `could not reach the narration sidecar at ${msg.sidecar} — start it with: cargo run -p mapgen-cli --features lore -- serve`,
        });
        return;
      } finally {
        clearTimeout(timer);
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

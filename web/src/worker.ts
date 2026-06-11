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

export type Scale = "continent" | "planet" | "globe";

/// Mirrors the Rust `ContinentInfo` — the landmass under a clicked point.
export interface ContinentInfo {
  cx: number;
  cy: number;
  cell_count: number;
  total_cells: number;
  /** The grounded landmass name — the drilled-region billboard (1b-ii). */
  name: string;
}

export type WorkerRequest =
  | { type: "generate"; seed: string; cells: number; nations: number; style: string; scale: Scale }
  | { type: "render"; style: string }
  | { type: "refine"; level: number; sx: number; sy: number; style: string }
  | {
      type: "refineTile";
      level: number;
      sx: number;
      sy: number;
      style: string;
      // Off-main-thread rasterize (Phase A): the worker renders the tile to RGBA
      // at this exact size, with `lens` (e.g. "on-faith", "" = political baseline)
      // injected on the SVG root so the lens is baked in before rasterizing.
      w: number;
      h: number;
      lens: string;
      // Relief grid dims (addendum §5): the seam-banded heightfield sampled per
      // tile — FIXED constants on main (RELIEF_GW/GH), decoupled from the patch
      // tessellation. gridW=0 is the failStage=relief fault-injection (a real
      // wasm-side reject, like w=0 for the raster).
      gridW: number;
      gridH: number;
    }
  | { type: "continentAt"; x: number; y: number }
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
  | {
      // Off-main-thread rasterize (Phase A): RGBA pixels, not an SVG string. The
      // `rgba` ArrayBuffer is TRANSFERRED (zero-copy), so it is neutered in the
      // worker after the post. `lens` is echoed for the main thread's
      // verify-before-apply (drop a tile rasterized under a now-stale lens).
      type: "tile";
      rgba: ArrayBuffer;
      // Relief (R2): the SAME seam-banded grid that shaded the raster, shipped
      // for vertex displacement — bit-identical edges across neighbours, so
      // displaced patches meet exactly. Transferred alongside rgba.
      heights: ArrayBuffer;
      gridW: number;
      gridH: number;
      w: number;
      h: number;
      level: number;
      sx: number;
      sy: number;
      style: string;
      lens: string;
    }
  // A SINGLE tile's refine/rasterize failed — sector + lens echoed so the main
  // thread can release exactly that budget reservation (and ignore a stale-lens
  // echo, mirroring the `tile` handler's verify-before-apply). NEVER a sector-less
  // `error` for tile work: that arm can't free the slot, so it would wedge
  // 1/STREAM_BUDGET of streaming throughput.
  | { type: "tileFailed"; level: number; sx: number; sy: number; lens: string }
  | { type: "continentInfo"; info: ContinentInfo | null; x: number; y: number }
  | { type: "yearFrame"; svg: string; year: number }
  | { type: "chronicle"; work: Work }
  | { type: "error"; message: string };

const post = (msg: WorkerResponse, transfer: Transferable[] = []) =>
  (self as DedicatedWorkerGlobalScope).postMessage(msg, transfer);

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
      // main thread resolves it via `navStyle`). The 3D Globe view textures a
      // sphere with the SAME planet world, so it generates identically to planet.
      const gen =
        msg.scale === "planet" || msg.scale === "globe"
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
    } else if (msg.type === "refineTile") {
      // Streaming patch refine (ST-1): refine + RASTERIZE ONE tile WITHOUT touching
      // the persistent `sector` slot (the legacy drill path owns that). Free the temp
      // handle so the cache holds zero wasm handles. Same `refineSector` call the
      // cross-platform golden pins, just a different (raster-only) consumer.
      //
      // Phase A: rasterize to RGBA HERE (off the main thread) and transfer the bytes,
      // instead of shipping the ~200 KB SVG string for the main thread to drawImage
      // (~300 ms on the paint thread). `renderRgba` returns a Uint8Array copied out
      // of wasm memory, so its `.buffer` is a private ArrayBuffer safe to transfer.
      //
      // EVERY failure of tile work — refineSector, renderRgba, even the can't-happen
      // !root — must answer with the sector-carrying `tileFailed`, never the generic
      // sector-less `error`: the main thread's `error` arm can't free this tile's
      // pendingTiles reservation, and with STREAM_BUDGET slots an unfreed key is
      // 1/BUDGET of streaming throughput wedged until the next level change.
      let tile: WorldHandle | undefined;
      try {
        if (!root) throw new Error("no world generated yet");
        tile = root.refineSector(msg.level, msg.sx, msg.sy, SECTOR_CELLS);
        // Relief (addendum §5): one seam-banded heightfield per tile, computed
        // ONCE — it shades the raster here (renderRgbaShaded multiplies the
        // baked hillshade into RGB; alpha untouched) and, from R2, the same
        // buffer ships for vertex displacement, so shade and geometry cannot
        // drift. Both wasm calls sit inside this ONE try: any failure (the
        // ?failTiles w=0 raster reject, the failStage=relief gw=0 grid reject)
        // still answers exactly one sector-carrying tileFailed.
        const grid = root.reliefGrid(tile, msg.level, msg.sx, msg.sy, msg.gridW, msg.gridH);
        const rgba = tile.renderRgbaShaded(
          msg.style,
          msg.lens,
          msg.w,
          msg.h,
          grid,
          msg.gridW,
          msg.gridH,
        );
        const buf = rgba.buffer as ArrayBuffer;
        const hbuf = grid.buffer as ArrayBuffer; // Float32Array copied out of wasm — private, transferable
        post(
          {
            type: "tile",
            rgba: buf,
            heights: hbuf,
            gridW: msg.gridW,
            gridH: msg.gridH,
            w: msg.w,
            h: msg.h,
            level: msg.level,
            sx: msg.sx,
            sy: msg.sy,
            style: msg.style,
            lens: msg.lens,
          },
          [buf, hbuf], // transfer both (zero-copy); neutered in the worker after this
        );
      } catch {
        post({ type: "tileFailed", level: msg.level, sx: msg.sx, sy: msg.sy, lens: msg.lens });
      } finally {
        // free even on throw — keep the "cache holds zero wasm handles" invariant
        // (`tile` is undefined when refineSector itself threw; nothing to free).
        tile?.free();
      }
    } else if (msg.type === "continentAt") {
      // Point→landmass for the continent-aware drill. Always against the root
      // world (drill snapping only fires at the root). Cheap; no busy state.
      const info = root ? (root.continentAt(msg.x, msg.y) as ContinentInfo | null) : null;
      post({ type: "continentInfo", info, x: msg.x, y: msg.y });
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

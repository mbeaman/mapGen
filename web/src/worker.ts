/// WASM lives here, off the main thread, so generate (~5 s) never freezes
/// the UI. The `WorldHandle` is an opaque WASM object that cannot cross the
/// worker boundary, so it stays here: the main thread sends commands and
/// gets back SVG strings.
import init, { generate, type WorldHandle } from "../pkg/mapgen_wasm";

let ready: Promise<unknown> | null = null;
let world: WorldHandle | null = null;

export type WorkerRequest =
  | { type: "generate"; seed: string; cells: number; nations: number; style: string }
  | { type: "render"; style: string };

export type WorkerResponse =
  | { type: "ready" }
  | { type: "generated"; svg: string; genMs: number; totalMs: number }
  | { type: "rendered"; svg: string; ms: number }
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
      world?.free();
      world = generate(BigInt(msg.seed), msg.cells, msg.nations);
      const genMs = performance.now() - t0;
      const svg = world.render(msg.style);
      const totalMs = performance.now() - t0;
      post({ type: "generated", svg, genMs, totalMs });
    } else if (msg.type === "render") {
      if (!world) {
        post({ type: "error", message: "no world generated yet" });
        return;
      }
      const t0 = performance.now();
      const svg = world.render(msg.style);
      post({ type: "rendered", svg, ms: performance.now() - t0 });
    }
  } catch (err) {
    post({ type: "error", message: err instanceof Error ? err.message : String(err) });
  }
};

// Kick off WASM init eagerly so the module is warm before the first generate.
void ensureReady();

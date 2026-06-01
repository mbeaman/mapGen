// Entry point for /3d.html — boots the wgpu viewer wasm and wires the
// "Generate" button to its `start_web(canvas, seed, cells)` export.
//
// We re-instantiate the wasm module per generation (calling start_web
// again would just stack a second event loop on top of the first). That
// keeps the code dead-simple at the cost of re-running the adapter
// handshake each time; if the wait ever feels noticeable, the path
// forward is to expose a `load_world` JS method on a held handle.

import init, { start_web } from "../pkg-viewer/mapgen_viewer.js";

const form = document.getElementById("form") as HTMLFormElement;
const seedEl = document.getElementById("seed") as HTMLInputElement;
const cellsEl = document.getElementById("cells") as HTMLInputElement;
const goEl = document.getElementById("go") as HTMLButtonElement;
const statusEl = document.getElementById("status") as HTMLDivElement;
const canvas = document.getElementById("viewer") as HTMLCanvasElement;

function setStatus(text: string, isError = false): void {
  statusEl.textContent = text;
  statusEl.classList.toggle("err", isError);
}

function fail(text: string, e?: unknown): void {
  setStatus(text + (e ? ` — ${String(e)}` : ""), true);
  goEl.disabled = false;
}

async function generate(): Promise<void> {
  const seedRaw = seedEl.value.trim();
  const cells = Number.parseInt(cellsEl.value, 10);
  if (!/^\d+$/.test(seedRaw)) {
    fail("seed must be a non-negative integer");
    return;
  }
  if (!Number.isFinite(cells) || cells < 500) {
    fail("cells must be ≥ 500");
    return;
  }

  goEl.disabled = true;
  setStatus(`generating seed ${seedRaw} (${cells} cells)…`);

  // Fresh init each time — see header comment.
  try {
    await init();
  } catch (e) {
    fail("wasm init failed", e);
    return;
  }

  try {
    start_web(canvas, BigInt(seedRaw), cells);
    setStatus("ready — drag to pan, right-drag to orbit, scroll to zoom, R to reset");
  } catch (e) {
    fail("start_web failed", e);
    return;
  }

  // Re-enable so the user can regenerate with a different seed. The
  // first run still owns the canvas; subsequent runs append a second
  // event loop. The page-refresh story is documented in the README.
  goEl.disabled = false;
}

form.addEventListener("submit", (e: SubmitEvent) => {
  e.preventDefault();
  void generate();
});

// Eagerly initialise so the first click feels instant; if the user
// never clicks Generate, this is the only init cost.
init()
  .then(() => setStatus("ready — click Generate"))
  .catch((e) => fail("wasm init failed", e));

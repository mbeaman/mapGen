import init, {
  generate,
  type WorldHandle,
} from "../pkg/mapgen_wasm";

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el as T;
};

const seedInput = $<HTMLInputElement>("seed");
const cellsInput = $<HTMLInputElement>("cells");
const nationsInput = $<HTMLInputElement>("nations");
const styleSelect = $<HTMLSelectElement>("style");
const generateBtn = $<HTMLButtonElement>("generate");
const rerenderBtn = $<HTMLButtonElement>("rerender");
const statusEl = $<HTMLParagraphElement>("status");
const mapEl = $<HTMLElement>("map");

type Status = "idle" | "busy" | "ok" | "error";
const setStatus = (text: string, kind: Status = "idle") => {
  statusEl.textContent = text;
  statusEl.classList.remove("busy", "ok", "error");
  if (kind !== "idle") statusEl.classList.add(kind);
};

let world: WorldHandle | null = null;

const renderCurrent = () => {
  if (!world) return;
  const style = styleSelect.value;
  setStatus(`Rendering ${style}…`, "busy");
  // Yield to the browser so the status paints before the heavy call.
  requestAnimationFrame(() => {
    try {
      const t0 = performance.now();
      const svg = world!.render(style);
      const dt = (performance.now() - t0) / 1000;
      mapEl.innerHTML = svg;
      setStatus(`Rendered ${style} in ${dt.toFixed(2)} s`, "ok");
    } catch (err) {
      setStatus(`Render failed: ${err}`, "error");
    }
  });
};

const doGenerate = () => {
  const seed = BigInt(seedInput.value || "0");
  const cells = Number(cellsInput.value);
  const nations = Number(nationsInput.value);

  if (!Number.isFinite(cells) || cells < 100) {
    setStatus("Cells must be ≥ 100", "error");
    return;
  }
  if (!Number.isFinite(nations) || nations < 1) {
    setStatus("Nations must be ≥ 1", "error");
    return;
  }

  generateBtn.disabled = true;
  rerenderBtn.disabled = true;
  setStatus(`Generating world (seed=${seed}, cells=${cells})…`, "busy");

  requestAnimationFrame(() => {
    try {
      const t0 = performance.now();
      world?.free();
      world = generate(seed, cells, nations);
      const dt = (performance.now() - t0) / 1000;
      setStatus(`Generated in ${dt.toFixed(2)} s. Rendering…`, "busy");
      const svg = world.render(styleSelect.value);
      mapEl.innerHTML = svg;
      const total = (performance.now() - t0) / 1000;
      setStatus(`Done in ${total.toFixed(2)} s (generate ${dt.toFixed(2)} s)`, "ok");
      rerenderBtn.disabled = false;
    } catch (err) {
      setStatus(`Generate failed: ${err}`, "error");
    } finally {
      generateBtn.disabled = false;
    }
  });
};

(async () => {
  try {
    await init();
    setStatus("Ready. Pick a seed and click Generate.", "idle");
    generateBtn.addEventListener("click", doGenerate);
    rerenderBtn.addEventListener("click", renderCurrent);
    styleSelect.addEventListener("change", () => {
      if (world) renderCurrent();
    });
  } catch (err) {
    setStatus(`Failed to load WASM: ${err}`, "error");
  }
})();

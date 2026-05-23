import "./style.css";
import { PanZoom } from "./panzoom";
import type { WorkerRequest, WorkerResponse } from "./worker";

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
};

const seedInput = $<HTMLInputElement>("seed");
const diceBtn = $<HTMLButtonElement>("dice");
const cellsInput = $<HTMLInputElement>("cells");
const cellsLabel = $<HTMLSpanElement>("cells-label");
const nationsInput = $<HTMLInputElement>("nations");
const nationsLabel = $<HTMLSpanElement>("nations-label");
const styleSelect = $<HTMLSelectElement>("style");
const generateBtn = $<HTMLButtonElement>("generate");
const dlSvgBtn = $<HTMLButtonElement>("dl-svg");
const dlPngBtn = $<HTMLButtonElement>("dl-png");
const shareBtn = $<HTMLButtonElement>("share");
const statusEl = $<HTMLParagraphElement>("status");
const mapEl = $<HTMLElement>("map");
const contentEl = $<HTMLDivElement>("map-content");
const placeholderEl = $<HTMLParagraphElement>("placeholder");
const overlayEl = $<HTMLDivElement>("overlay");
const overlayText = $<HTMLParagraphElement>("overlay-text");

type Status = "idle" | "busy" | "ok" | "error";
const setStatus = (text: string, kind: Status = "idle") => {
  statusEl.textContent = text;
  statusEl.classList.toggle("busy", kind === "busy");
  statusEl.classList.toggle("ok", kind === "ok");
  statusEl.classList.toggle("error", kind === "error");
};

const panzoom = new PanZoom(mapEl, contentEl);
let lastSvg = "";
let lastDims = { w: 0, h: 0 };
let exportReady = false;

// ---- URL permalink state ----
interface MapState {
  seed: string;
  cells: number;
  nations: number;
  style: string;
}

const readState = (): MapState => {
  const p = new URLSearchParams(location.search);
  return {
    seed: p.get("seed") ?? "42",
    cells: Number(p.get("cells") ?? cellsInput.value),
    nations: Number(p.get("nations") ?? nationsInput.value),
    style: p.get("style") ?? styleSelect.value,
  };
};

const writeState = (s: MapState) => {
  const p = new URLSearchParams({
    seed: s.seed,
    cells: String(s.cells),
    nations: String(s.nations),
    style: s.style,
  });
  history.replaceState(null, "", `?${p}`);
};

const currentState = (): MapState => ({
  seed: seedInput.value.trim() || "0",
  cells: Number(cellsInput.value),
  nations: Number(nationsInput.value),
  style: styleSelect.value,
});

const applyStateToControls = (s: MapState) => {
  seedInput.value = s.seed;
  if (Number.isFinite(s.cells)) cellsInput.value = String(s.cells);
  if (Number.isFinite(s.nations)) nationsInput.value = String(s.nations);
  const known = ["ornate", "biomes", "cultures", "greyscale"];
  if (known.includes(s.style)) styleSelect.value = s.style;
  syncLabels();
};

const syncLabels = () => {
  cellsLabel.textContent = `${Number(cellsInput.value).toLocaleString()} cells`;
  nationsLabel.textContent = nationsInput.value;
};

// ---- SVG injection + sizing ----
const parseDims = (svg: SVGSVGElement): { w: number; h: number } => {
  const vb = svg.viewBox.baseVal;
  if (vb && vb.width && vb.height) return { w: vb.width, h: vb.height };
  const w = parseFloat(svg.getAttribute("width") ?? "0");
  const h = parseFloat(svg.getAttribute("height") ?? "0");
  return { w: w || 1000, h: h || 1000 };
};

const showSvg = (svg: string) => {
  lastSvg = svg;
  contentEl.innerHTML = svg;
  const el = contentEl.querySelector("svg");
  if (!el) return;
  // Let the wrapper transform drive size; pin SVG to its natural box.
  const dims = parseDims(el);
  lastDims = dims;
  el.removeAttribute("width");
  el.removeAttribute("height");
  el.style.width = `${dims.w}px`;
  el.style.height = `${dims.h}px`;
  panzoom.setContentSize(dims.w, dims.h);
  panzoom.fit();
  placeholderEl.classList.add("hidden");
  setExportEnabled(true);
};

const setExportEnabled = (on: boolean) => {
  exportReady = on;
  dlSvgBtn.disabled = !on;
  dlPngBtn.disabled = !on;
  shareBtn.disabled = !on;
};

// ---- Generating overlay with elapsed timer ----
let timerId: number | null = null;
const showOverlay = (label: string) => {
  const t0 = performance.now();
  overlayText.textContent = label;
  overlayEl.classList.remove("hidden");
  timerId = window.setInterval(() => {
    overlayText.textContent = `${label} ${((performance.now() - t0) / 1000).toFixed(1)}s`;
  }, 100);
};
const hideOverlay = () => {
  if (timerId !== null) {
    clearInterval(timerId);
    timerId = null;
  }
  overlayEl.classList.add("hidden");
};

// ---- Worker ----
const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
const send = (req: WorkerRequest) => worker.postMessage(req);

let busy = false;
const setBusy = (on: boolean) => {
  busy = on;
  generateBtn.disabled = on;
};

worker.onmessage = (e: MessageEvent<WorkerResponse>) => {
  const msg = e.data;
  switch (msg.type) {
    case "ready":
      generateBtn.disabled = false;
      setStatus("Ready.", "idle");
      doGenerate(); // autoload a map so the first paint is a finished world
      break;
    case "generated":
      hideOverlay();
      setBusy(false);
      showSvg(msg.svg);
      setStatus(
        `Generated in ${(msg.genMs / 1000).toFixed(2)}s · rendered in ${((msg.totalMs - msg.genMs) / 1000).toFixed(2)}s`,
        "ok",
      );
      break;
    case "rendered":
      hideOverlay();
      setBusy(false);
      showSvg(msg.svg);
      setStatus(`Re-styled in ${(msg.ms / 1000).toFixed(2)}s`, "ok");
      break;
    case "error":
      hideOverlay();
      setBusy(false);
      setStatus(`Error: ${msg.message}`, "error");
      break;
  }
};

const doGenerate = () => {
  if (busy) return;
  const s = currentState();
  if (!/^\d+$/.test(s.seed)) {
    setStatus("Seed must be a non-negative integer.", "error");
    return;
  }
  writeState(s);
  setBusy(true);
  setStatus(`Generating seed ${s.seed}…`, "busy");
  showOverlay("Generating");
  send({ type: "generate", ...s });
};

const doRestyle = () => {
  if (busy) return;
  if (!lastSvg) {
    doGenerate();
    return;
  }
  const s = currentState();
  writeState(s);
  setBusy(true);
  setStatus(`Re-styling as ${s.style}…`, "busy");
  showOverlay("Rendering");
  send({ type: "render", style: s.style });
};

// ---- Export ----
const triggerDownload = (blob: Blob, filename: string) => {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
};

const exportName = (ext: string) => {
  const s = currentState();
  return `mapgen-${s.seed}-${s.style}.${ext}`;
};

const downloadSvg = () => {
  if (!exportReady) return;
  triggerDownload(new Blob([lastSvg], { type: "image/svg+xml;charset=utf-8" }), exportName("svg"));
};

const downloadPng = async () => {
  if (!exportReady) return;
  setStatus("Rasterising PNG…", "busy");
  try {
    const scale = 2;
    const blob = new Blob([lastSvg], { type: "image/svg+xml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.src = url;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(lastDims.w * scale);
    canvas.height = Math.round(lastDims.h * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 2d context unavailable");
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    URL.revokeObjectURL(url);
    canvas.toBlob((png) => {
      if (png) triggerDownload(png, exportName("png"));
      setStatus("PNG saved.", "ok");
    }, "image/png");
  } catch (err) {
    setStatus(`PNG export failed: ${err instanceof Error ? err.message : err}`, "error");
  }
};

const copyLink = async () => {
  writeState(currentState());
  try {
    await navigator.clipboard.writeText(location.href);
    setStatus("Link copied to clipboard.", "ok");
  } catch {
    setStatus(location.href, "idle");
  }
};

// ---- Wire up ----
cellsInput.addEventListener("input", syncLabels);
nationsInput.addEventListener("input", syncLabels);
diceBtn.addEventListener("click", () => {
  seedInput.value = String(Math.floor(Math.random() * 1_000_000));
});
generateBtn.addEventListener("click", doGenerate);
styleSelect.addEventListener("change", doRestyle);
dlSvgBtn.addEventListener("click", downloadSvg);
dlPngBtn.addEventListener("click", () => void downloadPng());
shareBtn.addEventListener("click", () => void copyLink());

$<HTMLButtonElement>("zoom-in").addEventListener("click", () => panzoom.zoomIn());
$<HTMLButtonElement>("zoom-out").addEventListener("click", () => panzoom.zoomOut());
$<HTMLButtonElement>("zoom-fit").addEventListener("click", () => panzoom.fit());
window.addEventListener("resize", () => panzoom.fit());

applyStateToControls(readState());

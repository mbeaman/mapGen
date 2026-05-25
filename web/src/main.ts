import "./style.css";
import { PanZoom } from "./panzoom";
import type { WorkerRequest, WorkerResponse, StageInfo, Work } from "./worker";

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
const barFill = $<HTMLDivElement>("bar-fill");
const stageListEl = $<HTMLOListElement>("stage-list");
const scrubberEl = $<HTMLDivElement>("scrubber");
const scrubInput = $<HTMLInputElement>("scrub");
const scrubLabel = $<HTMLSpanElement>("scrub-label");
const replayBtn = $<HTMLButtonElement>("replay");
const voiceSelect = $<HTMLSelectElement>("voice");
const narrateBtn = $<HTMLButtonElement>("narrate");
const chronicleEl = $<HTMLDivElement>("chronicle");

/// The native narration sidecar (`mapgen serve`). The browser POSTs the world
/// here so the API key never enters page JS.
const SIDECAR = "http://127.0.0.1:7878";

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

// Build-up frames cached for the scrubber/replay (JS-side only; no Rust
// snapshots). Each entry is one rendered stage frame.
interface Frame {
  svg: string;
  label: string;
}
let frames: Frame[] = [];
let replayId: number | null = null;

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

// Swap the displayed SVG. Refits the view only on the first paint or when
// the content dimensions change, so pan/zoom set mid-build (or while
// scrubbing) survives the next frame.
const showSvg = (svg: string) => {
  lastSvg = svg;
  contentEl.innerHTML = svg;
  const el = contentEl.querySelector("svg");
  if (!el) return;
  // Let the wrapper transform drive size; pin SVG to its natural box.
  const dims = parseDims(el);
  const dimsChanged = dims.w !== lastDims.w || dims.h !== lastDims.h;
  lastDims = dims;
  el.removeAttribute("width");
  el.removeAttribute("height");
  el.style.width = `${dims.w}px`;
  el.style.height = `${dims.h}px`;
  panzoom.setContentSize(dims.w, dims.h);
  if (dimsChanged) panzoom.fit();
  placeholderEl.classList.add("hidden");
};

const setExportEnabled = (on: boolean) => {
  exportReady = on;
  dlSvgBtn.disabled = !on;
  dlPngBtn.disabled = !on;
  shareBtn.disabled = !on;
};

// ---- Generating overlay: determinate bar + stage checklist ----
const showOverlay = (label: string) => {
  overlayText.textContent = label;
  barFill.style.width = "0%";
  stageListEl.innerHTML = "";
  overlayEl.classList.remove("hidden");
};
const hideOverlay = () => overlayEl.classList.add("hidden");

// Build the checklist rows lazily (count known from the first progress
// event), then fill labels in as each stage is reached — so JS never
// hard-codes the stage list; it tracks whatever the Rust pipeline runs.
const ensureStageRows = (total: number) => {
  if (stageListEl.children.length === total) return;
  stageListEl.innerHTML = "";
  for (let i = 0; i < total; i++) {
    const li = document.createElement("li");
    li.dataset.state = "pending";
    li.innerHTML = `<span class="s-dot"></span><span class="s-label">…</span>`;
    stageListEl.appendChild(li);
  }
};

const updateProgress = (info: StageInfo) => {
  ensureStageRows(info.total);
  const pct = Math.round(info.progress * 100);
  barFill.style.width = `${pct}%`;
  const sub = info.sub_total > 1 ? ` ${info.sub}/${info.sub_total}` : "";
  overlayText.textContent = `${info.label}${sub} · ${pct}%`;
  const rows = stageListEl.children;
  const active = rows[info.index] as HTMLLIElement | undefined;
  const lbl = active?.querySelector(".s-label");
  if (lbl) lbl.textContent = info.label;
  for (let i = 0; i < rows.length; i++) {
    (rows[i] as HTMLLIElement).dataset.state =
      i < info.index ? "done" : i === info.index ? "active" : "pending";
  }
};

const markAllDone = () => {
  const rows = stageListEl.children;
  for (let i = 0; i < rows.length; i++) (rows[i] as HTMLLIElement).dataset.state = "done";
  barFill.style.width = "100%";
};

// ---- Scrubber / replay over cached build-up frames ----
const stopReplay = () => {
  if (replayId !== null) {
    clearInterval(replayId);
    replayId = null;
  }
  replayBtn.textContent = "▶";
};

const showFrame = (i: number) => {
  const f = frames[i];
  if (!f) return;
  scrubInput.value = String(i);
  scrubLabel.textContent = f.label;
  showSvg(f.svg);
};

const setupScrubber = () => {
  if (frames.length === 0) {
    scrubberEl.classList.add("hidden");
    return;
  }
  scrubInput.min = "0";
  scrubInput.max = String(frames.length - 1);
  scrubInput.value = String(frames.length - 1);
  scrubLabel.textContent = frames[frames.length - 1].label;
  scrubberEl.classList.remove("hidden");
};

const startReplay = () => {
  if (frames.length === 0) return;
  stopReplay();
  let i = 0;
  showFrame(0);
  replayBtn.textContent = "⏸";
  replayId = window.setInterval(() => {
    i += 1;
    if (i >= frames.length) {
      stopReplay();
      return;
    }
    showFrame(i);
  }, 220);
};

// ---- Worker ----
const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
const send = (req: WorkerRequest) => worker.postMessage(req);

let busy = false;
let hasWorld = false;
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
    case "progress":
      updateProgress(msg.info);
      break;
    case "frame":
      frames.push({ svg: msg.svg, label: msg.info.label });
      showSvg(msg.svg);
      break;
    case "generated":
      markAllDone();
      hideOverlay();
      setBusy(false);
      frames.push({ svg: msg.svg, label: "Finished" });
      showSvg(msg.svg);
      setExportEnabled(true);
      setupScrubber();
      hasWorld = true;
      narrateBtn.disabled = false;
      setStatus(
        `Generated in ${(msg.genMs / 1000).toFixed(2)}s · ${msg.frameCount} frames · rendered in ${((msg.totalMs - msg.genMs) / 1000).toFixed(2)}s`,
        "ok",
      );
      break;
    case "rendered":
      hideOverlay();
      setBusy(false);
      showSvg(msg.svg);
      setExportEnabled(true);
      setStatus(`Re-styled in ${(msg.ms / 1000).toFixed(2)}s`, "ok");
      break;
    case "chronicle":
      narrateBtn.disabled = false;
      renderChronicle(msg.work);
      setStatus(`Chronicle set down by ${msg.work.in_world_author}.`, "ok");
      break;
    case "error":
      hideOverlay();
      setBusy(false);
      narrateBtn.disabled = !hasWorld;
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
  setExportEnabled(false);
  hasWorld = false;
  narrateBtn.disabled = true;
  chronicleEl.classList.add("hidden");
  stopReplay();
  frames = [];
  scrubberEl.classList.add("hidden");
  setStatus(`Generating seed ${s.seed}…`, "busy");
  showOverlay("Generating");
  send({ type: "generate", ...s });
};

// ---- Narration (Phase 5, via the native sidecar) ----
const renderChronicle = (work: Work) => {
  chronicleEl.replaceChildren();
  const title = document.createElement("h3");
  title.textContent = work.title;
  const byline = document.createElement("p");
  byline.className = "byline";
  byline.textContent = `${work.in_world_author} · year ${work.written_year}`;
  const body = document.createElement("p");
  body.className = "chronicle-body";
  body.textContent = work.body; // textContent, never innerHTML — the body is model output
  chronicleEl.append(title, byline, body);
  if (work.lacunae.length > 0) {
    const lac = document.createElement("p");
    lac.className = "lacunae";
    lac.textContent = `Lacunae: ${work.lacunae.join("; ")}`;
    chronicleEl.append(lac);
  }
  chronicleEl.classList.remove("hidden");
};

const doNarrate = () => {
  if (!hasWorld) return;
  narrateBtn.disabled = true;
  setStatus("Composing the chronicle…", "busy");
  send({ type: "narrate", event: "auto-major-war", voice: voiceSelect.value, sidecar: SIDECAR });
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
narrateBtn.addEventListener("click", doNarrate);
styleSelect.addEventListener("change", doRestyle);
dlSvgBtn.addEventListener("click", downloadSvg);
dlPngBtn.addEventListener("click", () => void downloadPng());
shareBtn.addEventListener("click", () => void copyLink());

$<HTMLButtonElement>("zoom-in").addEventListener("click", () => panzoom.zoomIn());
$<HTMLButtonElement>("zoom-out").addEventListener("click", () => panzoom.zoomOut());
$<HTMLButtonElement>("zoom-fit").addEventListener("click", () => panzoom.fit());
window.addEventListener("resize", () => panzoom.fit());

scrubInput.addEventListener("input", () => {
  stopReplay();
  showFrame(Number(scrubInput.value));
});
replayBtn.addEventListener("click", () => {
  if (replayId !== null) stopReplay();
  else startReplay();
});

applyStateToControls(readState());

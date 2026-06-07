import "./style.css";
import { applyLayers, defaultLayerState, LAYERS, PRESETS, presetState, toggleLayer } from "./layers";
import { PanZoom } from "./panzoom";
import {
  ancestors,
  childSectorAt,
  continentDrillLevel,
  crumbLabel,
  mollweideUnproject,
  navStyle,
  projectedBounds,
  ROOT,
  sectorAt,
  sectorRect,
  uvToWorld,
  type Sector,
} from "./sector";
import type { Scale, WorkerRequest, WorkerResponse, StageInfo, Work } from "./worker";

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
const scaleSelect = $<HTMLSelectElement>("scale");
const generateBtn = $<HTMLButtonElement>("generate");
const dlSvgBtn = $<HTMLButtonElement>("dl-svg");
const dlPngBtn = $<HTMLButtonElement>("dl-png");
const shareBtn = $<HTMLButtonElement>("share");
const statusEl = $<HTMLParagraphElement>("status");
const mapEl = $<HTMLElement>("map");
const contentEl = $<HTMLDivElement>("map-content");
const placeholderEl = $<HTMLParagraphElement>("placeholder");
const globeCanvas = $<HTMLCanvasElement>("globe-canvas");
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
const breadcrumbEl = $<HTMLDivElement>("breadcrumb");
const layersEl = $<HTMLDivElement>("layers");
const timesliderEl = $<HTMLDivElement>("timeslider");
const timescrubInput = $<HTMLInputElement>("timescrub");
const timeyearEl = $<HTMLSpanElement>("timeyear");

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

// ---- Multi-scale navigation (Phase 7) ----
// Geometry lives in ./sector (pure + unit-tested); this module wires it to the
// DOM, worker, and pan/zoom.
let nav: Sector = { ...ROOT };
// World extent in SVG/world units, captured from the root render (the default
// GenerateParams dims). Sector rectangles are computed against it.
let worldW = 2048;
let worldH = 1280;
// Scale of the *currently generated* world. In planet scale the root (level 0)
// is the planisphere overview; drilling refines a continental sector. Set at
// generate time, so it always matches the world the worker holds.
let planetScale = false;
// Globe scale: the root (level 0) is the 3D sphere instead of an SVG. It
// generates the same planet world as planet scale; drilling (a later increment)
// hands off to the 2D SVG sector view.
let globeScale = false;

// ---- 3D globe (Scale: Globe) — three.js, lazy-loaded on first entry ----
// `import("./globe")` is the code-split point: three lands in its own chunk so
// the default SVG page stays tiny (guarded by scripts/check-bundle.mjs).
type GlobeHandle = import("./globe").GlobeHandle;
let globe: GlobeHandle | null = null;
const ensureGlobe = async (): Promise<GlobeHandle> => {
  if (!globe) {
    const mod = await import("./globe");
    globe = mod.mountGlobe(globeCanvas);
    // A click on the sphere drills: surface UV → world (x,y) → the SAME
    // continentAt query the 2D planet uses. The input is already world space
    // (the globe is equirectangular), so it bypasses the Mollweide unproject
    // that the SVG planet click applies.
    globe.onPick((u, v) => {
      if (busy || !hasWorld || !globeScale || nav.level !== 0) return;
      const { x, y } = uvToWorld(u, v, worldW, worldH);
      send({ type: "continentAt", x, y });
    });
  }
  return globe;
};
// Rasterize an equirectangular `globe` SVG and wrap it onto the sphere. Shared by
// the initial texture (enterGlobeView) and the per-year re-texture the time-slider
// drives (the `yearFrame` handler), so empires animate ON the globe.
const textureGlobe = async (g: GlobeHandle, svg: string): Promise<void> => {
  try {
    // Planet world is 2048×1024; a 2:1 texture wraps the sphere's UVs cleanly.
    g.setTexture(await rasterizeSvg(svg, 2048, 1024));
  } catch {
    // Rasterization failed (unlikely) — keep the current texture.
  }
};

// Show the sphere over the (hidden) SVG layer, optionally texturing it from an
// equirectangular `globe` render of the world (parchment + political wash + coast
// + rivers). The texture is applied BEFORE show() so the placeholder graticule
// never flashes.
const enterGlobeView = async (textureSvg?: string) => {
  contentEl.style.display = "none";
  placeholderEl.classList.add("hidden");
  mapEl.classList.remove("navigable"); // the globe rotates; it isn't zoom-in
  styleSelect.disabled = true; // the sphere uses the fixed `globe` texture (see doRestyle)
  let g: GlobeHandle;
  try {
    g = await ensureGlobe();
  } catch (err) {
    // The globe chunk (three.js) failed to load — almost always a missing/stale
    // dependency install (run `npm install` / `just web-setup`) or an offline
    // first load. The caller fires this with `void` and then reports "Globe
    // ready", so without this catch the failure is a SILENT blank canvas behind
    // a misleading status. Surface it and restore the 2D layer instead.
    exitGlobeView();
    setStatus(
      `3D globe failed to load (${
        err instanceof Error ? err.message : String(err)
      }). Run \`npm install\` (or \`just web-setup\`) and reload.`,
      "error",
    );
    return;
  }
  if (textureSvg) await textureGlobe(g, textureSvg);
  g.show();
};
// Return to the SVG layer (used when leaving globe scale, and on drill into a
// 2D sector — where the style control applies again).
const exitGlobeView = () => {
  globe?.hide();
  contentEl.style.display = "";
  styleSelect.disabled = false;
};

// The style to send for the current nav level: the planisphere at the planet
// root, the user's chosen style everywhere else (see ./sector navStyle).
const effectiveStyle = (): string => navStyle(nav.level, planetScale, currentState().style);

// ---- History time-slider (Phase 7+) ----
// `[start, end]` years the map changed across, or empty if borders never moved.
let historyYears: number[] = [];
// Coalesce rapid scrubs: render one year at a time, remembering the latest.
let yearBusy = false;
let pendingYear: number | null = null;

// ---- Layer toggles ---- (which layers are enabled; applied to every SVG swap)
const layerState = defaultLayerState();

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
  scale: Scale;
}

const asScale = (v: string | null): Scale =>
  v === "planet" ? "planet" : v === "globe" ? "globe" : "continent";

const readState = (): MapState => {
  const p = new URLSearchParams(location.search);
  return {
    seed: p.get("seed") ?? "42",
    cells: Number(p.get("cells") ?? cellsInput.value),
    nations: Number(p.get("nations") ?? nationsInput.value),
    style: p.get("style") ?? styleSelect.value,
    scale: asScale(p.get("scale") ?? scaleSelect.value),
  };
};

const writeState = (s: MapState) => {
  const p = new URLSearchParams({
    seed: s.seed,
    cells: String(s.cells),
    nations: String(s.nations),
    style: s.style,
    scale: s.scale,
  });
  history.replaceState(null, "", `?${p}`);
};

const currentState = (): MapState => ({
  seed: seedInput.value.trim() || "0",
  cells: Number(cellsInput.value),
  nations: Number(nationsInput.value),
  style: styleSelect.value,
  scale: asScale(scaleSelect.value),
});

const applyStateToControls = (s: MapState) => {
  seedInput.value = s.seed;
  if (Number.isFinite(s.cells)) cellsInput.value = String(s.cells);
  if (Number.isFinite(s.nations)) nationsInput.value = String(s.nations);
  const known = ["ornate", "biomes", "cultures", "greyscale"];
  if (known.includes(s.style)) styleSelect.value = s.style;
  scaleSelect.value = s.scale;
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
  applyLayers(el, layerState); // re-apply toggles to the freshly-injected SVG
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
      // Globe mode hides the SVG layer and has no scrubber, so skip the live
      // build-up frames entirely (the overlay still shows stage progress).
      if (globeScale) break;
      frames.push({ svg: msg.svg, label: msg.info.label });
      showSvg(msg.svg);
      break;
    case "generated":
      markAllDone();
      hideOverlay();
      setBusy(false);
      hasWorld = true;
      nav = { ...ROOT };
      if (globeScale) {
        // Globe view: texture the sphere with the returned SVG (we asked the
        // worker for the equirectangular `globe` render at generate time, which is
        // exactly the lon/lat whole-world map the sphere UVs want) and show it
        // instead of injecting the SVG into the DOM. worldW/worldH were already
        // pinned to planet dims.
        void enterGlobeView(msg.svg);
        historyYears = msg.historyYears;
        updateBreadcrumb();
        narrateBtn.disabled = false;
        setStatus(`Globe ready · generated in ${(msg.genMs / 1000).toFixed(2)}s`, "ok");
        break;
      }
      frames.push({ svg: msg.svg, label: "Finished" });
      showSvg(msg.svg);
      setExportEnabled(true);
      setupScrubber();
      // The root render establishes the world extent for sector geometry.
      worldW = lastDims.w || worldW;
      worldH = lastDims.h || worldH;
      historyYears = msg.historyYears;
      mapEl.classList.add("navigable");
      updateBreadcrumb(); // also reconciles the time-slider
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
    case "refined":
      setBusy(false);
      showSvg(msg.svg);
      setExportEnabled(true);
      narrateBtn.disabled = msg.level !== 0;
      updateBreadcrumb();
      setStatus(
        msg.level === 0
          ? planetScale
            ? "Planet."
            : "Whole world."
          : `Sector L${msg.level} (${msg.sx},${msg.sy}) · refined in ${(msg.ms / 1000).toFixed(2)}s`,
        "ok",
      );
      break;
    case "continentInfo": {
      // The async answer to a root click. Re-center the drill on the clicked
      // continent's mass and size its depth; ignore if the nav moved or a refine
      // started meanwhile. Over sea / a speck (info null), grid-drill the click.
      if (busy || !hasWorld || nav.level !== 0) break;
      if (msg.info) {
        const level = continentDrillLevel(msg.info.cell_count, msg.info.total_cells, MAX_LEVEL);
        navTo(sectorAt(msg.info.cx, msg.info.cy, level, worldW, worldH));
      } else {
        const child = childSectorAt(msg.x, msg.y, nav.level, worldW, worldH, MAX_LEVEL);
        if (child) navTo(child);
      }
      break;
    }
    case "yearFrame":
      // A time-slider frame. On the globe, re-texture the sphere (empires animate
      // ON the globe); on a flat map, swap the SVG. Either way, no refit/status
      // churn while scrubbing. Release the coalescer once the frame is applied,
      // then send the latest pending year if the user moved on. The globe path is
      // async (rasterize→upload), so it flushes in `.finally`.
      if (globeScale && globe) {
        void textureGlobe(globe, msg.svg).finally(() => {
          yearBusy = false;
          flushPendingYear();
        });
      } else {
        showSvg(msg.svg);
        yearBusy = false;
        flushPendingYear();
      }
      break;
    case "chronicle":
      setBusy(false);
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
  nav = { ...ROOT };
  planetScale = s.scale === "planet";
  globeScale = s.scale === "globe";
  if (!globeScale) exitGlobeView(); // leaving globe → restore the SVG layer
  if (globeScale) {
    // The globe textures the SAME planet world (2048×1024). Pin the world extent
    // to those dims NOW — the texture path bypasses showSvg, which is what
    // normally sets worldW/worldH from the SVG viewBox, so a later drill must not
    // inherit the stale 2048×1280 continent default.
    worldW = 2048;
    worldH = 1024;
  }
  historyYears = [];
  updateBreadcrumb();
  narrateBtn.disabled = true;
  chronicleEl.classList.add("hidden");
  stopReplay();
  frames = [];
  scrubberEl.classList.add("hidden");
  setStatus(`Generating ${globeScale ? "globe" : planetScale ? "planet" : "world"} seed ${s.seed}…`, "busy");
  showOverlay("Generating");
  // Final render at the root uses the effective style ("planet" at a planet
  // root); the per-stage build-up frames pick their own style in the worker. The
  // globe ignores the returned SVG (it textures a sphere), so ask for the cheap
  // fontless equirectangular `globe` texture (parchment + political wash + coast
  // + rivers) rather than a slow ornate planisphere.
  send({ type: "generate", ...s, style: globeScale ? "globe" : effectiveStyle() });
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
  if (!hasWorld || busy) return;
  narrateBtn.disabled = true;
  setBusy(true); // also blocks a regenerate landing mid-narration
  setStatus("Composing the chronicle…", "busy");
  send({ type: "narrate", event: "auto-major-war", voice: voiceSelect.value, sidecar: SIDECAR });
};

const doRestyle = () => {
  if (busy) return;
  // At the globe root the sphere wears the fixed equirectangular `globe` texture
  // (a font-free skin — the labelled ornate styles rasterize unreliably as an
  // <img>), so the style control doesn't apply — the select is disabled there, and
  // this guards the path defensively. In a drilled 2D sector it works normally.
  if (globeScale && nav.level === 0) return;
  if (!lastSvg) {
    doGenerate();
    return;
  }
  const s = currentState();
  writeState(s);
  setBusy(true);
  setStatus(`Re-styling as ${s.style}…`, "busy");
  showOverlay("Rendering");
  send({ type: "render", style: effectiveStyle() });
};

// ---- Drill-in navigation (Phase 7) ----
const MAX_LEVEL = 6; // 2^6 sectors/axis — a level-6 sector is ~1/64 of the world

const updateBreadcrumb = () => {
  breadcrumbEl.replaceChildren();
  for (const a of ancestors(nav)) {
    if (a.level > 0) {
      const sep = document.createElement("span");
      sep.className = "crumb-sep";
      sep.textContent = "›";
      breadcrumbEl.append(sep);
    }
    const crumb = document.createElement("button");
    crumb.className = "crumb";
    crumb.textContent = globeScale && a.level === 0 ? "Globe" : crumbLabel(a, planetScale);
    if (a.level === nav.level) {
      crumb.classList.add("current");
      crumb.disabled = true;
    } else {
      crumb.addEventListener("click", () => navTo(a));
    }
    breadcrumbEl.append(crumb);
  }
  if (hasWorld && nav.level < MAX_LEVEL) {
    const hint = document.createElement("span");
    hint.className = "crumb-hint";
    hint.textContent =
      globeScale && nav.level === 0 ? "· click the globe to zoom in" : "· click the map to zoom in";
    breadcrumbEl.append(hint);
  }
  refreshTimeslider(); // nav changed → show at world scale, hide in a sector
};

const requestRefine = () => {
  setBusy(true);
  narrateBtn.disabled = nav.level !== 0; // a sector has no chronicle of its own
  setStatus(
    nav.level === 0
      ? planetScale
        ? "Returning to the planet…"
        : "Returning to the whole world…"
      : `Refining sector L${nav.level} (${nav.sx},${nav.sy})…`,
    "busy",
  );
  updateBreadcrumb();
  send({ type: "refine", level: nav.level, sx: nav.sx, sy: nav.sy, style: effectiveStyle() });
};

// Navigate to an explicit sector (breadcrumb / up). Re-refines from the seed —
// sectors are stateless, so this never needs the parent to be cached.
const navTo = (target: Sector) => {
  if (busy || !hasWorld) return;
  // Globe mode: the root (level 0) is the 3D sphere, which has no SVG parent
  // pixels for the coarse-first zoom. So when crossing the globe boundary, hand
  // off directly instead of animating a focusContentRect.
  if (globeScale && (nav.level === 0 || target.level === 0)) {
    nav = target;
    updateBreadcrumb();
    if (target.level === 0) {
      // Back to the globe — re-show the (already-textured) sphere, no regenerate.
      void enterGlobeView();
      setStatus("Globe.", "ok");
    } else {
      // Drill in from the globe → leave the sphere, refine the 2D SVG sector.
      exitGlobeView();
      requestRefine();
    }
    return;
  }
  // Was the current view the projected planet oval? Then its pixels are
  // Mollweide-projected, so frame the target's PROJECTED box, not its world box.
  const fromPlanetOval = planetScale && nav.level === 0;
  const parent = sectorRect(nav, worldW, worldH);
  nav = target;
  const child = sectorRect(nav, worldW, worldH);
  // Coarse-first: frame the target rectangle with the current (parent) pixels
  // so the zoom feels instant, then the refined (equirectangular) sector swaps
  // in over it and `fit()` reframes exactly.
  if (fromPlanetOval) {
    const b = projectedBounds(child.x0, child.y0, child.w, child.h, worldW, worldH);
    panzoom.focusContentRect(b.x0, b.y0, b.w, b.h);
  } else {
    panzoom.focusContentRect(child.x0 - parent.x0, child.y0 - parent.y0, child.w, child.h);
  }
  requestRefine();
};

// Drill beneath a content-space point. At the root, snap to the clicked
// *continent* — the worker answers `continentAt` async, and `continentInfo`
// re-centers + sizes the drill on its mass (so clicking a continent's edge no
// longer lands you in a half-ocean quadrant). Deeper in, grid-drill the child.
const drillAt = (wx: number, wy: number) => {
  if (busy || !hasWorld) return;
  if (nav.level === 0) {
    // The planet root renders as a Mollweide oval, so the clicked content coords
    // are projected — invert to world space before the continent query. A click
    // in the bare oval corners unprojects to null → inert (no drill).
    let qx = wx;
    let qy = wy;
    if (planetScale) {
      const w = mollweideUnproject(wx, wy, worldW, worldH);
      if (!w) return;
      qx = w.x;
      qy = w.y;
    }
    send({ type: "continentAt", x: qx, y: qy });
    return;
  }
  const child = childSectorAt(wx, wy, nav.level, worldW, worldH, MAX_LEVEL);
  if (child) navTo(child);
};

// Treat a near-stationary pointer press as a click (drill in); a drag pans.
let downPt: { x: number; y: number } | null = null;
mapEl.addEventListener("pointerdown", (e) => {
  // Only a press that STARTS on the map surface is a potential drill. The
  // breadcrumb, zoom buttons, slider, etc. live inside #map, so their clicks
  // also bubble here — arming a drill on them would fire a spurious drill on the
  // bubbled pointerup (e.g. clicking a breadcrumb to navigate up would instead
  // drill down, setting `busy` and swallowing the navigation).
  if ((e.target as HTMLElement).closest(".breadcrumb, .zoom, .timeslider, .scrubber, .overlay")) {
    downPt = null;
    return;
  }
  downPt = { x: e.clientX, y: e.clientY };
});
mapEl.addEventListener("pointerup", (e) => {
  if (!downPt) return;
  const moved = Math.hypot(e.clientX - downPt.x, e.clientY - downPt.y);
  downPt = null;
  if (moved > 6 || busy || !hasWorld) return;
  // At the globe ROOT the 3D canvas owns interaction (its own raycaster drill);
  // the SVG drill path is inert. But once drilled into a 2D sector (level ≥ 1,
  // the canvas hidden), SVG clicks drill deeper as usual.
  if (globeScale && nav.level === 0) return;
  const c = panzoom.clientToContent(e.clientX, e.clientY);
  const cur = sectorRect(nav, worldW, worldH);
  drillAt(cur.x0 + c.x, cur.y0 + c.y);
});

// ---- History time-slider ----
// Shown only at the world scale (history is world-wide) and only when borders
// actually moved. Drilling into a sector hides it (sectors have no timeline).
const refreshTimeslider = () => {
  // Shown at the root of every scale — the planisphere and the globe both wash in
  // political control, so scrubbing animates empires rise + fall on either (the
  // globe re-textures the sphere per year via `renderAtYear("globe", y)`). Hidden
  // inside a drilled sector, which has no timeline of its own.
  const show = hasWorld && nav.level === 0 && historyYears.length === 2;
  timesliderEl.classList.toggle("hidden", !show);
  if (!show) return;
  const [start, end] = historyYears;
  timescrubInput.min = String(start);
  timescrubInput.max = String(end);
  timescrubInput.value = String(end); // start at the present
  timeyearEl.textContent = "present";
};

const labelYear = (y: number) => {
  const end = historyYears[1] ?? 0;
  timeyearEl.textContent = y >= end ? "present" : `year ${y}`;
};

// Coalesce scrubs: one render in flight, remember only the latest target.
const requestYear = (y: number) => {
  if (yearBusy) {
    pendingYear = y;
    return;
  }
  yearBusy = true;
  // On the globe the texture is the equirectangular `globe` render; elsewhere the
  // current view's style. Both go through `renderAtYear`, which swaps control to
  // the year before rendering.
  send({ type: "renderYear", style: globeScale ? "globe" : effectiveStyle(), year: y });
};

// Send the latest year the user scrubbed to while a render was in flight.
const flushPendingYear = () => {
  if (pendingYear !== null) {
    const p = pendingYear;
    pendingYear = null;
    requestYear(p);
  }
};

timescrubInput.addEventListener("input", () => {
  if (busy) return;
  const y = Number(timescrubInput.value);
  labelYear(y);
  requestYear(y);
});

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

// Rasterize an SVG string to an off-screen canvas at a given pixel size. Shared
// by the PNG export and the 3D globe texture. The render SVG carries its fonts
// and styles inline (base64 @font-face / literal fills), so it loads as a
// same-origin <img> and the canvas stays untainted (no SecurityError on read).
const rasterizeSvg = async (svg: string, w: number, h: number): Promise<HTMLCanvasElement> => {
  const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml;charset=utf-8" }));
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 2d context unavailable");
    ctx.drawImage(img, 0, 0, w, h);
    return canvas;
  } finally {
    URL.revokeObjectURL(url);
  }
};

const downloadPng = async () => {
  if (!exportReady) return;
  setStatus("Rasterising PNG…", "busy");
  try {
    const canvas = await rasterizeSvg(lastSvg, Math.round(lastDims.w * 2), Math.round(lastDims.h * 2));
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

// ---- Layer panel ----
const layerChecks = new Map<string, HTMLInputElement>();

// Replace the whole enabled set, then re-sync the checkboxes + the live SVG.
// (`toggleLayer` can turn other overlays off, so every checkbox is reconciled.)
const setLayerState = (next: Set<string>) => {
  layerState.clear();
  for (const n of next) layerState.add(n);
  for (const [name, cb] of layerChecks) cb.checked = layerState.has(name);
  const svg = contentEl.querySelector("svg");
  if (svg) applyLayers(svg, layerState);
};

const buildLayerPanel = () => {
  layersEl.replaceChildren();
  layerChecks.clear();

  // Preset "lenses": one click swaps the whole enabled set to a curated view.
  const presets = document.createElement("div");
  presets.className = "layer-presets";
  for (const p of PRESETS) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "layer-preset";
    btn.textContent = p.label;
    btn.addEventListener("click", () => setLayerState(presetState(p)));
    presets.append(btn);
  }
  layersEl.append(presets);

  // Per-layer checkboxes (fine-grained control on top of the presets).
  for (const l of LAYERS) {
    const row = document.createElement("label");
    row.className = l.overlay ? "layer-row is-overlay" : "layer-row";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = layerState.has(l.name);
    cb.addEventListener("change", () => setLayerState(toggleLayer(layerState, l.name, cb.checked)));
    layerChecks.set(l.name, cb);
    row.append(cb, document.createTextNode(` ${l.label}`));
    layersEl.append(row);
  }
};

// ---- Wire up ----
buildLayerPanel();
cellsInput.addEventListener("input", syncLabels);
nationsInput.addEventListener("input", syncLabels);
diceBtn.addEventListener("click", () => {
  seedInput.value = String(Math.floor(Math.random() * 1_000_000));
});
generateBtn.addEventListener("click", doGenerate);
narrateBtn.addEventListener("click", doNarrate);
styleSelect.addEventListener("change", doRestyle);
// Scale changes the generation params (continent vs planet), so it regenerates.
// If a generation is already in flight, doGenerate would silently no-op (its
// `busy` guard) while the native <select> has already moved — leaving the
// control disagreeing with the rendered world and poisoning the permalink
// (currentState reads the select). Revert the select to the in-flight scale so
// it never diverges.
scaleSelect.addEventListener("change", () => {
  if (busy) {
    scaleSelect.value = globeScale ? "globe" : planetScale ? "planet" : "continent";
    return;
  }
  doGenerate();
});
dlSvgBtn.addEventListener("click", downloadSvg);
dlPngBtn.addEventListener("click", () => void downloadPng());
shareBtn.addEventListener("click", () => void copyLink());

$<HTMLButtonElement>("zoom-in").addEventListener("click", () => panzoom.zoomIn());
$<HTMLButtonElement>("zoom-out").addEventListener("click", () => panzoom.zoomOut());
$<HTMLButtonElement>("zoom-fit").addEventListener("click", () => panzoom.fit());
window.addEventListener("resize", () => {
  if (globeScale) globe?.resize();
  else panzoom.fit();
});
// Layout changes that don't fire a window resize (a collapsing panel, the
// breadcrumb wrapping) still need the globe's camera aspect + drawing buffer
// re-fit. The SVG path uses CSS, so it only matters for the WebGL canvas.
new ResizeObserver(() => {
  if (globeScale && globe) globe.resize();
}).observe(mapEl);

scrubInput.addEventListener("input", () => {
  stopReplay();
  showFrame(Number(scrubInput.value));
});
replayBtn.addEventListener("click", () => {
  if (replayId !== null) stopReplay();
  else startReplay();
});

applyStateToControls(readState());

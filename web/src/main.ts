import "./style.css";
import {
  applyLayers,
  defaultLayerState,
  LAYERS,
  PRESETS,
  presetState,
  svgLayerClasses,
  toggleLayer,
} from "./layers";
import { PanZoom } from "./panzoom";
import {
  ancestors,
  childSectorAt,
  continentDrillLevel,
  crumbLabel,
  GLOBE_FIRST_DRILL_LEVEL,
  globeDrillTarget,
  mollweideUnproject,
  navStyle,
  projectedBounds,
  ROOT,
  sectorAt,
  sectorRect,
  uvToWorld,
  type Sector,
} from "./sector";
import { sectorPatchParams, type Vec3, worldToLonLat } from "./camera";
import { desiredSectors, predictedAhead } from "./lod";
import { displacedPatchArrays, GH as RELIEF_GH, GW as RELIEF_GW, patchGpuBytes } from "./relief";
import {
  ESTIMATED_PATCH_BYTES,
  MAX_LIVE_PATCHES,
  MAX_TEXTURE_BYTES,
  patchKey,
  reconcile,
} from "./patchcache";
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
// The latest equirectangular `globe` SVG (present render or a scrubbed year). Kept
// so a lens toggle can re-texture the sphere by re-rasterizing it with a new layer
// class — no worker round-trip.
let lastGlobeSvg = "";
// The grounded name of the drilled landmass (from continentAt), for the globe
// region billboard (1b-ii). Set on a continent drill, reused across intermediate
// breadcrumb hops within that continent, cleared on a sea/speck grid-drill.
let regionName = "";
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
    // Deeper drill (1c): a click while drilled → the clicked surface point (base
    // sphere uv → uvToWorld) → the child sector one level finer → navTo (stays on
    // the globe). navTo's globe guard handles L>0→L>0 (from the 1a fix).
    globe.onPatchPick((u, v) => {
      if (busy || !hasWorld || !globeScale || nav.level === 0) return;
      const { x, y } = uvToWorld(u, v, worldW, worldH);
      const child = childSectorAt(x, y, nav.level, worldW, worldH, MAX_LEVEL);
      if (child) navTo(child, { x, y }); // centre on the click, not the child sector centre
    });
    // Streaming (ST-1): every time the flight camera settles (after a drill / pan /
    // zoom), reconcile the in-view patch set — load what entered view, evict what
    // left, within the caps. THIS is "scroll around and keep the detail".
    globe.onSettle(() => streamReconcile(true)); // pump/settle: sample the camera velocity
  }
  return globe;
};

// ---- Continuous-LOD streaming: the in-view patch set + during-motion follow ----
// window 3 = a 7×7 candidate set sized to cover the L3 visible cap (patchcache.ts
// caps note). Phase B (off-main-thread raster unlock): detail now fills DURING
// motion, not only on settle — globe.ts pumps `streamReconcile` every ~PUMP_MS
// while the camera moves. Two disciplines keep the SERIAL, non-preemptible worker
// from thrashing on a fast pan: an in-flight BUDGET (only STREAM_BUDGET tiles
// outstanding, nearest-first) and discard-stale on arrival (a tile the camera
// already passed is dropped, not uploaded-then-LRU-evicted). A velocity-predictive
// ring extends the desired set one tick AHEAD of the camera so the next sectors are
// warm on arrival — naturally idle-priority (the prediction sits AFTER the in-view
// set in nearest-first order, so the budget feeds in-view first).
const STREAM_CFG = { maxPatches: MAX_LIVE_PATCHES, window: 3 };
const CACHE_CFG = { maxPatches: MAX_LIVE_PATCHES, maxBytes: MAX_TEXTURE_BYTES, estBytes: ESTIMATED_PATCH_BYTES };
// At most this many refineTile requests outstanding in the serial worker at once.
// Bounds a fast pan's wasted work: the worker can't be preempted, so a deep queue
// of already-passed sectors would delay the one under the camera. Re-prioritised
// nearest-first every pump; stale RESULTS are also discarded on arrival.
const STREAM_BUDGET = 3;
const pendingTiles = new Set<string>(); // sector keys with a refineTile in flight
// Discard-stale + predictive-prefetch state (all JS-ephemeral — no dataset, no
// session, no WorldHandle; a drill/return is a discontinuity, so prevPosUnit resets
// on a level change, never extrapolating a camera jump into garbage prefetches).
let currentDesiredKeys = new Set<string>(); // in-view ∪ predicted, this pump
let prevPosUnit: Vec3 | null = null; // finite-difference camera-velocity source
let reconcileLevel = -1; // detect the level discontinuity that resets the velocity
let tilesDiscarded = 0; // results dropped as stale (camera moved past) — e2e signal
let tilesFailed = 0; // worker tile failures gracefully released (tileFailed) — e2e signal
// Persistent-failure memo: `tileFailed` releases the budget slot and the refill
// re-requests the sector — right for a TRANSIENT fault, but a sector that fails
// deterministically (a recurring wasm fault on one tile) would otherwise ping-pong
// main↔worker forever (~10 attempts/s), pegging the serial worker and pinning the
// status at "Loading detail…". After MAX_TILE_RETRIES failures a key is BENCHED:
// it stays coarse (no worse than never-loaded) until the next discontinuity —
// level change / lens refresh / regenerate — clears the memo with pendingTiles.
const MAX_TILE_RETRIES = 3;
const failedTiles = new Map<string, number>();
// Sticky worker-degraded flag (see onWorkerFailure at the worker creation site):
// once the worker has fallen silent, reconcile must not re-reserve into the void
// and the stream-progress status must not clobber the failure banner.
let workerDegraded = false;
// FAULT-INJECTION test hook (?failTiles=N): the first N DISTINCT sectors requested
// fail PERSISTENTLY — every send for those keys carries w=0, which the wasm
// rasterizer genuinely rejects (`Pixmap::new(0,h)` → JsError "invalid raster
// dimensions"). A REAL end-to-end failure, not a mock, and persistent (not
// one-shot) so the e2e pins BOTH halves of the recovery contract: a failure
// RELEASES its budget slot (the fill completes around the failing sectors) AND the
// retry memo BOUNDS the loop (`data-tiles-failed` stabilises at N × MAX_TILE_RETRIES
// instead of climbing forever). Inert without the param.
const failTilesN = Number(new URLSearchParams(location.search).get("failTiles") ?? 0);
const failTileKeys = new Set<string>();
// `&failStage=relief` aims the injected failure at the OTHER wasm call in the
// tile try: gridW=0 -> relief_grid rejects (vs w=0 -> Pixmap::new rejects).
// Pins that BOTH stages route through the same single-tileFailed recovery.
const failStageRelief = new URLSearchParams(location.search).get("failStage") === "relief";
// Relief grid dims live in relief.ts (GW/GH) — imported above as RELIEF_GW/GH.

// Tile raster size: long edge 1024, the short edge scaled by the sector's aspect
// so the cartography isn't anisotropically squashed on the curved patch. Computed
// here (worldW/H live on the main thread) and sent to the worker, which rasterizes
// to exactly this size — the SVG never crosses the boundary.
const tileDims = (sec: Sector): { w: number; h: number } => {
  const rect = sectorRect(sec, worldW, worldH);
  const aspect = rect.w / rect.h;
  const long = 1024;
  return aspect >= 1
    ? { w: long, h: Math.round(long / aspect) }
    : { w: Math.round(long * aspect), h: long };
};

// Reconcile the live patch set against what the camera now sees (+ where it is
// heading): the pure selector (lod.ts) + cache policy (patchcache.ts) decide; here
// we drive the worker + cache. Fired on every flight pump (during motion) and on
// settle — cheap now the raster is off-thread (no main-thread work but the bounded
// selector + cache diff + ≤ BUDGET message posts).
// `sampleVelocity` advances the finite-difference window (prevPosUnit ← now). ONLY
// the pump/settle (line ~151) passes true, so the camera-velocity is sampled at the
// fixed ~PUMP_MS cadence. The tile-completion REFILLS (and the lens refresh) call
// this with the default false: they fire at the worker's irregular, faster cadence,
// and re-sampling prevPosUnit there would shrink the (pos − prev) delta toward zero
// — collapsing the prefetch lead. Refills still reconcile + feed the budget; they
// just don't disturb the velocity window.
const streamReconcile = (sampleVelocity = false): void => {
  if (workerDegraded || !globe || !globeScale || nav.level === 0) return;
  const cam = globe.getCameraState();
  // A level change (drill / deeper-drill / re-drill) is a camera DISCONTINUITY, not
  // motion — drop the velocity so the predictor doesn't extrapolate the jump.
  if (nav.level !== reconcileLevel) {
    reconcileLevel = nav.level;
    prevPosUnit = null;
  }
  const inView = desiredSectors(cam, nav.level, worldW, worldH, STREAM_CFG);
  const inViewKeys = inView.map((s) => patchKey(s, "globe"));
  // Velocity-predictive prefetch: extend the desired set one tick AHEAD of the camera
  // (constant-velocity extrapolation; EMPTY when stationary). The ahead sectors sit
  // AFTER the in-view set, so the nearest-first budget feeds in-view first (idle-
  // priority prefetch). The extrapolation lives in the pure `predictedAhead` so it is
  // unit-tested off-GPU (`lod.test.ts`), not just exercised.
  const ahead = prevPosUnit
    ? predictedAhead(cam, prevPosUnit, inView, nav.level, worldW, worldH, STREAM_CFG)
    : [];
  const desired = [...inView, ...ahead];
  if (sampleVelocity) prevPosUnit = cam.posUnit; // advance the window ONLY on the pump cadence
  currentDesiredKeys = new Set(desired.map((s) => patchKey(s, "globe"))); // discard-stale set
  globe.markSeen(inViewKeys); // protect the IN-VIEW set (not the prediction) as freshest
  const { toLoad, toEvict } = reconcile(globe.liveEntries(), desired, "globe", CACHE_CFG);
  for (const key of toEvict) {
    globe.evictPatch(key);
    pendingTiles.delete(key);
  }
  // In-flight budget: feed the serial worker a bounded NEAREST-first head. toLoad is
  // already nearest-first (desiredSectors ranks by closeness), so stopping at BUDGET
  // gives don't-start-if-stale for free — a queued-but-unsent far sector is simply
  // re-evaluated next pump against the live camera.
  for (const sec of toLoad) {
    if (pendingTiles.size >= STREAM_BUDGET) break;
    const key = patchKey(sec, "globe");
    if (pendingTiles.has(key)) continue; // already refining
    if ((failedTiles.get(key) ?? 0) >= MAX_TILE_RETRIES) continue; // benched (see memo)
    pendingTiles.add(key);
    const { w, h } = tileDims(sec);
    if (failTileKeys.size < failTilesN) failTileKeys.add(key); // ?failTiles hook (see decl)
    const injectFail = failTileKeys.has(key);
    // Bake the active lens + the relief hillshade into the worker raster
    // (off-main-thread); see renderRgbaShaded. failStage=relief aims the
    // injected failure at the relief_grid call instead of the raster.
    send({
      type: "refineTile",
      level: sec.level,
      sx: sec.sx,
      sy: sec.sy,
      style: "globe",
      w: injectFail && !failStageRelief ? 0 : w,
      h,
      lens: activeLensClass(),
      gridW: injectFail && failStageRelief ? 0 : RELIEF_GW,
      gridH: RELIEF_GH,
    });
  }
  updateStreamProgress();
};

// #10 (quality hunt): the drill's detail fill is a SERIAL worker queue (up to
// 7×7 = 49 tiles × ~100ms+) — without an affordance the user sees a coarse base,
// a status claiming readiness, and detail that "eventually happens". Surface the
// queue: a live "Loading detail… N tiles" status while tiles are outstanding,
// the region prompt once the in-view set is filled, and `data-pending-tiles` so
// the e2e can pin the affordance (appears > 0, then drains to 0).
let lastPendingShown = -1;
const updateStreamProgress = (): void => {
  // Degraded: the failure banner owns the status — don't clobber it with
  // "Loading detail…" / the region prompt.
  if (workerDegraded || !globeScale || nav.level === 0) return;
  // Debounce: the ~PUMP_MS during-motion pump (Phase B) calls this many times a
  // second — only rewrite the dataset + status when the count actually changes, so
  // a pan doesn't thrash the DOM / the e2e signal.
  if (pendingTiles.size === lastPendingShown) return;
  lastPendingShown = pendingTiles.size;
  globeCanvas.dataset.pendingTiles = String(pendingTiles.size);
  if (pendingTiles.size > 0) {
    setStatus(`Loading detail… ${pendingTiles.size} tile${pendingTiles.size === 1 ? "" : "s"}`, "busy");
  } else {
    setStatus(`Region L${nav.level} — drag to pan, scroll to zoom; Globe to return.`, "ok");
  }
};

// 1e — lens toggle while DRILLED: the streamed patches dominate the drilled surface,
// and each patch's raster bakes in the active lens IN THE WORKER (render_rgba →
// with_root_class; the `tile` handler verifies `msg.lens === activeLensClass()` before
// applying). So a lens change must FORCE the in-view set to re-stream:
// evict the live patches + drop the in-flight reservations, then reconcile re-requests
// the same sectors, which re-rasterize under the new lens. (The patchKey style stays
// "globe" — a lens isn't a key dimension yet; this is the brute refresh, briefly
// flashing the coarse base until the new patches land — the accepted 1e tradeoff.)
const refreshGlobePatches = (): void => {
  if (!globe || !globeScale || nav.level === 0) return;
  for (const e of globe.liveEntries()) globe.evictPatch(e.key);
  pendingTiles.clear();
  failedTiles.clear(); // new lens = new raster input; benched sectors get a fresh shot
  streamReconcile();
};
// Inject the active layer classes onto the SVG root, so a rasterized `<img>` of it
// applies the embedded lens CSS (`svg.on-faith .planet-faith{display:inline}` …) —
// the same swap the 2D map does live, but baked in before rasterizing. Only the
// overlay `on-*` classes affect the globe texture (it carries no `off-*` feature
// CSS); the rest are harmless no-ops.
// The active lens class string (e.g. "on-faith"), or "" for the political
// baseline. The streaming tile path passes THIS to the worker, which bakes it
// into the SVG root before rasterizing (`render_rgba` → `with_root_class`), so
// the off-main-thread raster carries the same lens the live 2D map shows.
const activeLensClass = (): string => svgLayerClasses(layerState).join(" ");
// Inject those classes onto an SVG root before a MAIN-THREAD rasterize (the base
// sphere + PNG export still rasterize here). The globe tile path no longer uses
// this — it passes `activeLensClass()` to the worker instead.
const withLayerClasses = (svg: string): string => {
  const classes = activeLensClass();
  return classes ? svg.replace("<svg ", `<svg class="${classes}" `) : svg;
};

// Rasterize an equirectangular `globe` SVG and wrap it onto the sphere. Shared by
// the initial texture (enterGlobeView), the per-year re-texture the time-slider
// drives (the `yearFrame` handler), and lens toggles (`setLayerState`).
const textureGlobe = async (g: GlobeHandle, svg: string): Promise<void> => {
  try {
    // Planet world is 2048×1024; a 2:1 texture wraps the sphere's UVs cleanly.
    g.setTexture(await rasterizeSvg(withLayerClasses(svg), 2048, 1024));
  } catch {
    // Rasterization failed (unlikely) — keep the current texture.
  }
};

// Re-texture the sphere from the cached `globe` SVG with the current lens applied —
// used when a lens toggles while the globe is shown (no regenerate / worker hop).
const retextureGlobe = (): void => {
  if (globe && lastGlobeSvg) {
    // The re-rasterize blocks the main thread ~300ms+ (2048×1024 SVG drawImage) —
    // say so instead of letting the UI silently freeze on a lens click.
    setStatus("Applying lens…", "busy");
    void textureGlobe(globe, lastGlobeSvg).then(() => setStatus("Lens applied.", "ok"));
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
  if (textureSvg) lastGlobeSvg = textureSvg;
  if (lastGlobeSvg) await textureGlobe(g, lastGlobeSvg);
  // Every globe (re-)entry lands at the OVERVIEW. enterRegion never hides the
  // sphere, and a regenerate-while-drilled (or scale-away-and-back) keeps the loop
  // alive, so without this a stale `flight` would leave the fresh globe wedged in
  // the previous region with OrbitControls dead. Reset before showing.
  g.exitRegion();
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
// The year the base view is scrubbed to (null = present). Drilling SNAPS this to
// present: refined tiles always render the present, so a drill from a past year
// would otherwise composite present detail over a past base — a silent temporal
// lie (and the slider label lied again on return). Tracked here so navTo can snap.
let scrubbedYear: number | null = null;

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
// Worker-death backstop — DEFENSIVE, and honest about its reach: the worker wraps
// its whole onmessage in try/catch and answers tile work with a typed `tileFailed`,
// so no in-handler failure can ever reach this `error` event. What CAN: a module
// load/parse failure (which fires before any world exists, so there is nothing to
// release) — and any future code that escapes the worker's try/catch. Kept because
// it is the only handler for "the worker fell silent", and if that ever becomes
// reachable mid-stream the alternative is a permanently wedged budget + a UI stuck
// busy. STICKY (`workerDegraded`): streamReconcile must not re-reserve into a dead
// worker, and updateStreamProgress must not clobber this banner with "Loading
// detail…". Also unwedges the GENERATE path (overlay + busy) — a death
// mid-generate would otherwise leave the whole app frozen behind the overlay.
const onWorkerFailure = (): void => {
  workerDegraded = true;
  pendingTiles.clear();
  failedTiles.clear();
  hideOverlay();
  setBusy(false);
  globeCanvas.dataset.pendingTiles = "0";
  setStatus("Map worker crashed — reload the page to recover.", "error");
};
worker.onerror = onWorkerFailure;
// A single unreadable MESSAGE (worker still alive — realistically unreachable for
// these plain-object/ArrayBuffer messages): release the reservations so nothing
// wedges and reconcile re-requests; late duplicate answers are harmless (showPatch
// replaces idempotently, discard-stale drops). NOT degraded — the worker works.
worker.onmessageerror = () => {
  pendingTiles.clear();
  streamReconcile();
};

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
        // "Globe ready" only AFTER the texture actually lands — enterGlobeView's
        // rasterize+upload takes ~300ms+, and announcing readiness first created a
        // false-ready window where clicks were accepted over the placeholder.
        setStatus("Texturing globe…", "busy");
        void enterGlobeView(msg.svg).then(() => {
          setStatus(`Globe ready · generated in ${(msg.genMs / 1000).toFixed(2)}s`, "ok");
        });
        historyYears = msg.historyYears;
        updateBreadcrumb();
        narrateBtn.disabled = false;
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
    case "tile": {
      // ST-1 streaming: a refined patch tile arrived as RGBA (rasterized in the
      // worker — Phase A). Build the texture source and add it to the cache,
      // SYNCHRONOUSLY (no main-thread rasterize). Drop it if we've changed level /
      // left the globe since the request (stale), or if a deeper drill superseded
      // this level.
      const sec: Sector = { level: msg.level, sx: msg.sx, sy: msg.sy };
      const key = patchKey(sec, "globe");
      // Stale (changed level / left the globe since the request): drop AND release the
      // key, then refill the freed budget slot for the current view.
      if (!globe || !globeScale || nav.level !== msg.level) {
        pendingTiles.delete(key);
        streamReconcile();
        break;
      }
      // Lens-echo (verify-before-apply): the lens is now baked in the worker raster,
      // so the main thread can't re-apply the current lens. A tile rasterized under a
      // now-stale lens (a toggle landed mid-flight) must be dropped, not welded onto a
      // base re-textured under the new lens — the 1e refresh race. Do NOT delete the
      // key: a lens toggle ran `refreshGlobePatches` (clear + re-stream under the new
      // lens), so this same key is now reserving a LIVE new-lens request — deleting it
      // would un-reserve a real in-flight tile and let the budget overshoot. The
      // new-lens tile's show/discard path releases it. (VALUE-compare, not request
      // generations: an A→B→A double toggle can mis-match one echo — bounded to one
      // duplicate raster, self-healed by the idempotent show/discard path.)
      //
      // While the old-lens echoes drain (≤ BUDGET of them) the budget reads full and
      // this branch issues no refill — deliberately NOT a stall: the worker is SERIAL
      // and non-preemptible, so it must chew through those already-queued old-lens
      // jobs regardless; queueing new requests deeper behind them would not land the
      // first new-lens tile one ms sooner. Throughput is worker-bound, not queue-
      // depth-bound; the first new-lens completion resumes the refill cadence.
      if (msg.lens !== activeLensClass()) {
        break;
      }
      // Discard-stale (Phase B): the camera moved past this sector since it was
      // requested — it's no longer in the in-view ∪ predicted set. Drop it rather
      // than upload-then-immediately-LRU-evict (the wasted-upload stutter Phase B
      // exists to kill), then refill the freed slot with what IS now in view.
      if (!currentDesiredKeys.has(key)) {
        pendingTiles.delete(key);
        globeCanvas.dataset.tilesDiscarded = String((tilesDiscarded += 1));
        streamReconcile();
        break;
      }
      // Arrival bookkeeping FIRST: the reservation is spent whether or not the blit
      // below succeeds — a putImageData/showPatch throw must not strand the slot
      // (the uncaught error still surfaces; the next reconcile just re-requests).
      pendingTiles.delete(key);
      const params = sectorPatchParams(sec, worldW, worldH);
      // Relief (R2): the worker's seam-banded heightfield → displaced geometry.
      // Pure math (relief.ts), built INSIDE the data-last-blit-ms span below so
      // the <50 ms gate covers it with zero new plumbing.
      const heights = new Float32Array(msg.heights);
      // The ONLY synchronous main-thread cost now: a putImageData blit + the patch-mesh
      // build (geometry/material/scene.add). The GPU `texImage2D` is DEFERRED by three.js
      // to the next `renderer.render()` and is NOT in this span. The ~300 ms SVG drawImage
      // moved into the worker; `data-last-blit-ms` proves the synchronous paint-thread cost
      // is now single-digit (vs ~300 ms before).
      const t0 = performance.now();
      const cv = document.createElement("canvas");
      cv.width = msg.w;
      cv.height = msg.h;
      cv.getContext("2d")!.putImageData(
        new ImageData(new Uint8ClampedArray(msg.rgba), msg.w, msg.h),
        0,
        0,
      );
      const displaced = displacedPatchArrays(params, heights, msg.gridW, msg.gridH);
      globe.showPatch(sec, cv, params, patchGpuBytes(msg.w, msg.h, msg.gridW, msg.gridH), displaced);
      globeCanvas.dataset.lastBlitMs = (performance.now() - t0).toFixed(1);
      // Refill the budget slot this completion freed — drains the full in-view set at
      // STREAM_BUDGET-outstanding even on a STATIC drill (the pump dies after settle, so
      // without this only ~BUDGET tiles ever load and the rest stay coarse forever).
      streamReconcile();
      break;
    }
    case "tileFailed": {
      // A single tile's refine/rasterize failed in the worker — release its budget
      // slot (so it doesn't wedge 1/STREAM_BUDGET of throughput), remember the
      // failure (the memo bounds re-requests at MAX_TILE_RETRIES), and refill. No UI
      // banner: one dropped tile leaves the coarse base showing there, no worse than
      // not-yet-loaded. `data-tiles-failed` is the e2e recovery-contract witness.
      const key = patchKey({ level: msg.level, sx: msg.sx, sy: msg.sy }, "globe");
      // Stale (changed level / left the globe): release + refill but do NOT count —
      // mirror of the tile arm's first guard; an invalidated failure isn't a
      // recovery event, and counting it would make `data-tiles-failed` lie.
      if (!globe || !globeScale || nav.level !== msg.level) {
        pendingTiles.delete(key);
        streamReconcile();
        break;
      }
      // Lens-echo guard, mirroring the `tile` handler: a failure rasterized under a
      // now-stale lens must NOT delete the key — a lens toggle ran refreshGlobePatches
      // (clear + re-stream), so this same key is reserving a LIVE new-lens request;
      // deleting it would un-reserve that request and overshoot the budget. (Like
      // the tile arm this compares lens VALUES, not request generations, so an
      // A→B→A double toggle can mis-match one echo — bounded: one duplicate raster,
      // self-healed by the idempotent show/discard path.)
      if (msg.lens !== activeLensClass()) {
        break;
      }
      failedTiles.set(key, (failedTiles.get(key) ?? 0) + 1);
      pendingTiles.delete(key);
      globeCanvas.dataset.tilesFailed = String((tilesFailed += 1));
      streamReconcile();
      break;
    }
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
        regionName = msg.info.name; // the billboard reads this in enterRegion (1b-ii)
        if (globeScale) {
          // The 3D globe zooms TOWARD THE CLICK at a fixed sane level — not the
          // continent centroid + size (which landed every drill on the continent's
          // middle and, for a big continent, made a ~quarter-globe distorted patch).
          // The click point is the camera focus too (the sector centre snap put the
          // camera up to half a sector from the click). Deeper clicks (1c) go +1.
          navTo(globeDrillTarget(msg.x, msg.y, worldW, worldH), { x: msg.x, y: msg.y });
        } else {
          const level = continentDrillLevel(msg.info.cell_count, msg.info.total_cells, MAX_LEVEL);
          navTo(sectorAt(msg.info.cx, msg.info.cy, level, worldW, worldH));
        }
      } else {
        regionName = ""; // sea / speck grid-drill — no grounded landmass name
        const child = childSectorAt(msg.x, msg.y, nav.level, worldW, worldH, MAX_LEVEL);
        if (child) navTo(child, { x: msg.x, y: msg.y });
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
        lastGlobeSvg = msg.svg; // cache so a lens toggle re-applies to this year
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
  pendingTiles.clear(); // a regenerate invalidates any in-flight streaming tiles
  failedTiles.clear(); // …and the failure memo with them (new world, new tiles)
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
    // On the globe a click lands at GLOBE_FIRST_DRILL_LEVEL (L3) in one hop — the
    // shallower L1/L2 ancestors are quadtree path stops you JUMPED, never a distinct
    // globe view (L1/L2 patches are too curved to show). Mute them so the breadcrumb
    // reads "Globe › ⟨zoom path⟩ › L3", not four equal steps you stepped through. They
    // stay clickable (up-nav intact) — only the styling changes.
    if (globeScale && a.level > 0 && a.level < GLOBE_FIRST_DRILL_LEVEL) {
      crumb.classList.add("via");
      crumb.title = "Zoom out to here";
    }
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
  // On the globe, every level wears the fontless equirect `globe` render (the
  // user's labelled style rasterizes unreliably as an <img>, and the patch needs
  // the sector's lon/lat viewBox) — force it here (1b).
  send({ type: "refine", level: nav.level, sx: nav.sx, sy: nav.sy, style: globeScale ? "globe" : effectiveStyle() });
};

// Navigate to an explicit sector (breadcrumb / up). Re-refines from the seed —
// sectors are stateless, so this never needs the parent to be cached.
// `focus` is the WORLD-SPACE point the user actually clicked (when the nav came
// from a click): the globe camera centres on IT, not the containing sector's
// geometric centre. Without it (breadcrumb hops, programmatic nav) the sector
// centre is the only sensible target. THE CONTRACT: click = zoom toward exactly
// that point — a sector-centre snap at L3 put the camera up to ~25° (half a
// sector) from the click, marooning the view over the wrong content.
const navTo = (target: Sector, focus?: { x: number; y: number }) => {
  if (busy || !hasWorld) return;
  // Globe mode: EVERY navigation stays on the sphere (1a) — there is no globe→2D
  // path. The guard must catch all globe-mode hops, not just those touching level 0:
  // an intermediate breadcrumb hop between two non-zero levels (e.g. L3→L1) would
  // otherwise fall through to the 2D refine path and render an invisible sector
  // behind the still-shown globe. So branch purely on the target level here.
  if (globeScale) {
    // A level change (drill / crumb hop / return) invalidates every in-flight tile
    // request for the old level. Clear the pending set so a refineTile that never
    // answers with a typed message (a worker killed mid-flight, an unreadable
    // answer — every in-handler failure DOES answer `tileFailed` now) can't leave
    // a key stuck forever, permanently blocking that sector's detail. The failure
    // memo resets with it: a benched sector at the old level deserves a fresh shot
    // after the discontinuity. The new level re-streams on settle.
    pendingTiles.clear();
    failedTiles.clear();
    nav = target;
    updateBreadcrumb();
    if (target.level === 0) {
      // Back to the whole globe — clear the region + pull the camera back to the
      // overview. The sphere was never hidden on drill (1a stays in 3D), so no
      // re-show / re-texture / regenerate.
      globe?.exitRegion();
      // A return is a camera DISCONTINUITY: drop the velocity source so the next drill
      // doesn't extrapolate a stale jump. The `reconcileLevel` guard alone misses this —
      // streamReconcile early-returns at level 0, so a return→re-drill to the SAME depth
      // would skip the level-change reset and predict from the pre-return position.
      prevPosUnit = null;
      reconcileLevel = -1;
      narrateBtn.disabled = false; // the globe root has a chronicle again (1b drill disabled it)
      setStatus("Globe.", "ok");
    } else {
      // Drill STAYS on the globe: fly into a free-fly camera over the region on the
      // same sphere — no 2D handoff. The camera centres on the CLICK POINT when the
      // nav came from one (focus), falling back to the sector centre for clickless
      // navs (crumb hops). The sector still defines the refine level, breadcrumb,
      // and framing altitude — but where you LOOK is where you POINTED. Streaming
      // doesn't care: desiredSectors windows around whatever sub-point the camera
      // has. Altitude frames the sector by its angular (longitude) span: deeper
      // sectors → closer.
      // TEMPORAL SNAP: refined tiles always render the PRESENT (refineTile has no
      // year), so a drill from a scrubbed past year would paint present detail
      // over a past base — mixed-era content with no warning. Snap the base back
      // to the present before drilling (the e2e pins the re-texture).
      if (scrubbedYear !== null) {
        scrubbedYear = null;
        requestYear(historyYears[1] ?? 0);
      }
      const rc = sectorRect(target, worldW, worldH);
      const fx = focus?.x ?? rc.x0 + rc.w / 2;
      const fy = focus?.y ?? rc.y0 + rc.h / 2;
      const { lon, lat } = worldToLonLat(fx, fy, worldW, worldH);
      // Frame the sector to fill the view, getting CLOSER each level so the globe
      // visibly GROWS as you zoom in (~1.4× the sector's angular span; the previous
      // formula clamped flat at shallow levels so zooming barely changed distance).
      const altitude = Math.max(0.12, Math.min(2.2, 1.4 * ((2 * Math.PI) / 2 ** target.level)));
      globe?.enterRegion(lon, lat, altitude, regionName);
      narrateBtn.disabled = true; // a drilled sector has no chronicle of its own
      setStatus(`Region L${target.level} — drag to pan, scroll to zoom; Globe to return.`, "ok");
      // ST-1: enterRegion marks the camera changed → on settle, streamReconcile
      // loads the in-view patch set (and pan/zoom re-stream). No single-patch refine.
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
  if (child) navTo(child, { x: wx, y: wy }); // centre the camera on the click, not the sector
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
  // In globe mode the 3D canvas owns ALL interaction at EVERY level (1a: a globe
  // drill stays on the sphere via its own raycaster — there is no globe→2D handoff,
  // the canvas is never hidden). #globe-canvas is a child of #map, so its clicks
  // bubble here; without this guard a deeper globe drill fires TWICE (the patch-pick
  // raycaster AND this SVG path), jumping two levels to a bogus sector. So the SVG
  // drill path is inert whenever globeScale — not only at the root.
  if (globeScale) return;
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
  scrubbedYear = null;
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
  scrubbedYear = y >= (historyYears[1] ?? 0) ? null : y;
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
  // On the globe the lens lives in the TEXTURE at EVERY level, not the DOM: re-
  // rasterize the cached `globe` SVG (the base sphere) with the new class — and when
  // DRILLED, also refresh the streamed patches (1e), which cover the surface you're
  // looking at and bake the lens in per-patch. Without the patch refresh a drilled
  // lens toggle silently no-ops on the region in view (a stale `nav.level === 0`
  // guard — the same bug class as the #map drill double-fire). On a flat map, toggle
  // the live SVG's classes (CSS does the rest).
  if (globeScale) {
    retextureGlobe();
    if (nav.level !== 0) refreshGlobePatches();
    return;
  }
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

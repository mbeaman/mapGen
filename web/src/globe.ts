/// 3D globe view (Scale: Globe). A three.js sphere textured with the rendered
/// world map, rotatable (drag) and zoomable (scroll). This module owns ALL
/// three.js — it is loaded on demand via a dynamic `import("./globe")` the first
/// time the globe is opened, so three lands in its own lazy chunk and the default
/// SVG page never pays for it (guarded by `scripts/check-bundle.mjs`).
///
/// The sphere is unlit (`MeshBasicMaterial`) — the flat antique map should read
/// like a paper globe, not a shaded modern Earth. The drill/picking is added in
/// a later increment; this module's job is to mount, texture, rotate, and zoom.
///
/// Lifecycle: one `WebGLRenderer` is created on mount and REUSED across
/// show/hide (creating a GL context per entry exhausts the browser's context
/// pool and flakes headless tests). `dispose()` is the full teardown.

import {
  BufferAttribute,
  BufferGeometry,
  Mesh,
  MeshBasicMaterial,
  PerspectiveCamera,
  Raycaster,
  Scene,
  SphereGeometry,
  Texture,
  CanvasTexture,
  Vector2,
  Vector3,
  WebGLRenderer,
  SRGBColorSpace,
  RepeatWrapping,
  ClampToEdgeWrapping,
  LinearFilter,
} from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import {
  type CamPose,
  type GlobeCamState,
  globeCamPose,
  LAT_MAX,
  lerpPose,
  lonLatToUnit,
  panSubPoint,
  unitToUv,
  type PatchParams,
} from "./camera";
import type { CamState } from "./lod";
import { maxPitch, nearFor } from "./camera";
import type { DisplacedPatch } from "./relief";
import { patchKey } from "./patchcache";
import type { Sector } from "./sector";

export interface GlobeHandle {
  /** Reveal the canvas and start the render loop. */
  show(): void;
  /** Hide the canvas and stop the render loop (renderer is kept for re-show). */
  hide(): void;
  /** Replace the sphere's map texture with a (canvas) source. Disposes the old. */
  setTexture(source: HTMLCanvasElement): void;
  /** Re-read the container size (call on resize / on show). */
  resize(): void;
  /** Register a click handler: fired with the surface UV (u,v ∈ [0,1]) of a
   *  near-stationary click on the sphere (a drag rotates instead). */
  onPick(cb: (u: number, v: number) => void): void;
  /** Register the DEEPER-drill handler (increment 1c): fired with the patch's
   *  surface uv on a click while drilled (a patch is shown). The caller maps it
   *  via `patchUvToWorld` → `childSectorAt` → `navTo` to drill one level deeper. */
  onPatchPick(cb: (u: number, v: number) => void): void;
  /** Drill stays on the globe (increment 1a): enter free-fly mode framing the
   *  drilled region (sub-point `subLon`/`subLat` in radians, `altitude` above the
   *  unit sphere). The camera is then driven from this state — drag pans across
   *  the surface, wheel zooms — and the sphere is NOT hidden (no 2D handoff).
   *  Sets `data-region="1"` and the per-frame `data-sub-lon/lat`/`data-altitude`
   *  the e2e reads. (Deeper 3D drilling + the detail patch land in later increments.) */
  enterRegion(subLon: number, subLat: number, altitude: number, name: string): void;
  /** Add (or replace) ONE patch in the streaming cache (ST-1): the rasterized
   *  refined-sector render on a curved partial-sphere segment over the base globe,
   *  occupying `sector`'s lon/lat span (`params` from `sectorPatchParams`). Keyed by
   *  sector; bumps `data-patch`/`data-patch-textures`/`data-refines`/`data-live-patches`. */
  showPatch(sector: Sector, source: HTMLCanvasElement, params: PatchParams, gpuBytes: number, displaced: DisplacedPatch): void;
  /** Evict ONE patch (by `patchKey`) and free its GPU resources (ST-1). */
  evictPatch(key: string): void;
  /** The live patch cache state for the pure `reconcile` policy (ST-1). */
  liveEntries(): { key: string; lastSeen: number; texBytes: number }[];
  /** Mark these patch keys as just-seen (in-view) so LRU eviction spares them (ST-1). */
  markSeen(keys: string[]): void;
  /** The camera state the LOD selector consumes (ST-1) — packs the camera in the
   *  unit-sphere frame. The 1a-deferred streaming hook, now with its consumer. */
  getCameraState(): CamState;
  /** Register the settle handler (ST-1): fired once the flight camera has been
   *  still briefly after a drill / pan / zoom — main.ts reconciles the patch set. */
  onSettle(cb: () => void): void;
  /** Return to the whole-globe overview: drop flight mode, dispose all patches, pull
   *  the camera back to the overview framing, re-arm the pick, clear region signals. */
  exitRegion(): void;
  /** Full teardown: stop the loop and free GPU resources. */
  dispose(): void;
}

/// Placeholder equirectangular texture until the real world texture lands: a
/// parchment field with a graticule (meridians + parallels + a bold equator) so
/// the sphere visibly rotates and previews the map's eventual shape.
function placeholderTexture(): CanvasTexture {
  const w = 1024;
  const h = 512;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = "#f4ebd0"; // parchment (matches the biomes sea/land base)
  ctx.fillRect(0, 0, w, h);
  ctx.strokeStyle = "#b59a6a";
  ctx.lineWidth = 1;
  for (let i = 1; i < 12; i++) {
    const x = (i / 12) * w;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, h);
    ctx.stroke();
  }
  for (let j = 1; j < 6; j++) {
    const y = (j / 6) * h;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(w, y);
    ctx.stroke();
  }
  ctx.strokeStyle = "#8a3324"; // bold equator (the brand red)
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(0, h / 2);
  ctx.lineTo(w, h / 2);
  ctx.stroke();
  return wrapTexture(new CanvasTexture(c));
}

/// Apply the equirectangular wrap/colour conventions shared by the placeholder
/// and the real world texture: longitude wraps, latitude clamps at the poles.
///
/// The planet world is now longitude-PERIODIC (the Phase 5 flip — a cylinder that
/// wraps in x), so `RepeatWrapping` meets itself at the antimeridian with the two
/// edge columns genuinely CONTINUOUS — no seam. The only remaining equirect
/// artifact is the pole pinch (latitude is not periodic), which `fadePoleCaps`
/// tidies into ocean caps. (The old seam-fade band is gone: fading the now-aligned
/// edges to ocean would have re-introduced a fake discontinuity where the map is
/// actually continuous.)
function wrapTexture(t: CanvasTexture): CanvasTexture {
  t.colorSpace = SRGBColorSpace;
  t.wrapS = RepeatWrapping; // longitude wraps around (the world is periodic in x)
  t.wrapT = ClampToEdgeWrapping; // latitude clamps at the poles
  // No mipmaps: the globe is viewed ~1:1, so the mip chain adds nothing visible,
  // but GENERATING it on every upload is costly — brutally so under software GL
  // (headless SwiftShader) and wasted work even on a real GPU. Skipping it is the
  // single biggest win for fluid per-year re-texturing while scrubbing.
  t.generateMipmaps = false;
  t.minFilter = LinearFilter;
  return t;
}

/// Pole-band height for [`fadePoleCaps`] — pure (no canvas), so the geometry is
/// trivially inspectable. The pole bands are ~8% of height each (the pinched
/// top/bottom rows). Narrow on purpose: only the artifact-prone pole rows fade;
/// the map interior — and the now-continuous longitude seam — is untouched.
function poleCapBand(_w: number, h: number): { pole: number } {
  return { pole: Math.max(1, Math.round(h * 0.08)) };
}

/// The deep-sea blue-grey the globe texture's open-ocean edges already are — the
/// abyss end of `style/planet.rs::sea_color`, which the encircling sea around the
/// continents renders in. The fade bands blend to THIS so the antimeridian and
/// poles read as a continuation of that open ocean. A fixed colour (not sampled
/// from the canvas): the globe is always the `globe` style, so its sea colour is
/// known — and sampling via `getImageData` forced a multi-hundred-ms GPU→CPU
/// readback per call (the dominant cost of a per-year scrub re-texture).
const GLOBE_SEA = "rgb(93,122,134)";

/// Tidy the equirectangular POLE pinch: an equirect texture's top/bottom rows
/// aren't single points, so on the sphere they smear into a starburst at each
/// pole. We fade the two pole bands to the open-ocean colour, so the pinched
/// poles read as tidy ocean caps. The geometric pinch is inherent to
/// equirectangular-on-a-sphere; this removes the visible artifact.
///
/// The LONGITUDE seam is NOT faded: the planet world is periodic (Phase 5), so
/// its left/right edge columns are continuous and `RepeatWrapping` joins them
/// seamlessly — fading them would re-introduce a fake ocean band over real,
/// continuous geography (the exact "vertical blur line where the edges don't
/// connect" this removes). Mutates `canvas` in place (a throwaway texture canvas).
function fadePoleCaps(canvas: HTMLCanvasElement): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const w = canvas.width;
  const h = canvas.height;
  const opaque = GLOBE_SEA;
  const clear = "rgba(93,122,134,0)"; // GLOBE_SEA, fully transparent
  const { pole } = poleCapBand(w, h);
  // Paint one pole band: opaque sea for the inner `solid` fraction (covering the
  // worst-compressed rows outright), then a gradient fading to transparent so the
  // interior shows through untouched. A large solid cap because equirect
  // compression is extreme right at the pole point — a pure gradient there leaves
  // a faint land starburst.
  const band = (ry: number, gy0: number, gy1: number) => {
    const grad = ctx.createLinearGradient(0, gy0, 0, gy1);
    grad.addColorStop(0, opaque);
    grad.addColorStop(0.6, opaque);
    grad.addColorStop(1, clear);
    ctx.fillStyle = grad;
    ctx.fillRect(0, ry, w, pole);
  };
  band(0, 0, pole); // top edge → down
  band(h - pole, h, h - pole); // bottom edge → up
}

export function mountGlobe(canvas: HTMLCanvasElement): GlobeHandle {
  const renderer = new WebGLRenderer({ canvas, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.setClearColor(0x15110c, 1); // dark sepia backdrop — the sphere pops

  const scene = new Scene();
  const camera = new PerspectiveCamera(42, 1, 0.1, 100);
  // Pulled back enough that the unit sphere reads as a ball with breathing room
  // around it (at this distance + fov the sphere spans ~60% of the view height).
  camera.position.set(0, 0, 3.6);

  const geometry = new SphereGeometry(1, 64, 48);
  const material = new MeshBasicMaterial({ map: placeholderTexture() });
  const sphere = new Mesh(geometry, material);
  scene.add(sphere);

  const controls = new OrbitControls(camera, canvas);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.enablePan = false; // a globe spins in place; no panning
  controls.rotateSpeed = 0.5;
  controls.minDistance = 1.25; // can't dive inside the sphere
  controls.maxDistance = 8.0; // can pull back to a small distant globe
  controls.autoRotate = false; // deterministic for e2e (no free-running spin)

  let raf = 0;
  let running = false;
  let painted = false;
  let textureCount = 0; // bumped on each real texture upload (drives the slider e2e)
  let pickable = true; // false while drilled into a region (1a) — a click won't re-fly

  // Free-fly globe-flight camera (increment 1a). Non-null ⇒ DRILLED mode: the
  // camera is driven each frame from this state via `globeCamPose` and
  // OrbitControls is disabled (it owns the level-0 overview only). Drag pans the
  // sub-point across the surface; wheel changes altitude. pitch/heading are in the
  // state (and the pose math) but UNWIRED in 1a — pan+zoom is the increment.
  let flight: GlobeCamState | null = null;
  let dragLast: { x: number; y: number } | null = null; // active flight-pan drag origin
  let pitchLast: { x: number; y: number } | null = null; // active flight-pitch (tilt) drag origin
  const PAN_SPEED = 0.0022; // rad of sub-point travel per px, scaled by altitude
  const ZOOM_RATE = 0.001; // altitude multiplier per wheel-delta unit
  const PITCH_SPEED = 0.005; // rad of tilt per px of vertical drag (R3)
  const MIN_ALT = 0.05;
  const MAX_ALT = 2.5;
  const clampLat = (lat: number) => Math.max(-LAT_MAX, Math.min(LAT_MAX, lat));

  // STREAMING patch cache (increment ST-1): the in-view set of curved high-detail
  // segments over the base globe, keyed by sector. Bounded by the LOD reconcile
  // policy (the count/byte caps live in patchcache.ts; main.ts drives load/evict).
  // globe.ts is just the GL-touching glue. Each patch is depthTest=false +
  // renderOrder so it composites over its base region — no z-fight under SwiftShader.
  interface PatchEntry {
    mesh: Mesh;
    sector: Sector;
    texBytes: number;
    lastSeen: number;
  }
  const patches = new Map<string, PatchEntry>();
  let patchTexCount = 0;
let reliefMaxSeen = 0; // running per-drill displacement witness (data-relief-max) // cumulative texture uploads (data-patch-textures)
  let refineCount = 0; // cumulative patch loads (data-refines) — streaming activity
  const PATCH_STYLE = "globe"; // the globe always renders the fontless `globe` style
  const writeLive = () => {
    canvas.dataset.livePatches = String(patches.size);
    // The live SECTOR keys (level:sx:sy). The e2e reads this to prove a PAN streamed
    // a genuinely NEW sector — not just that the cumulative `data-refines` counter
    // ticked up from the initial drill's still-in-flight tiles (a false-green the
    // review caught). Sorted for a stable, diffable string.
    canvas.dataset.patchKeys = Array.from(patches.values())
      .map((e) => `${e.sector.level}:${e.sector.sx}:${e.sector.sy}`)
      .sort()
      .join(",");
  };
  const disposeEntry = (e: PatchEntry) => {
    scene.remove(e.mesh);
    e.mesh.geometry.dispose();
    const m = e.mesh.material as MeshBasicMaterial;
    m.map?.dispose();
    m.dispose();
  };
  const disposePatches = () => {
    for (const e of patches.values()) disposeEntry(e);
    patches.clear();
    delete canvas.dataset.patch;
    delete canvas.dataset.livePatches;
    delete canvas.dataset.patchKeys;
  };
  // A patch texture is NOT periodic (it's a continent interior), so — unlike the
  // base sphere's `setTexture` — it skips `fadeMapEdges` and clamps both axes.
  const patchTexture = (source: HTMLCanvasElement): CanvasTexture => {
    const t = new CanvasTexture(source);
    t.colorSpace = SRGBColorSpace;
    t.wrapS = ClampToEdgeWrapping;
    t.wrapT = ClampToEdgeWrapping;
    t.generateMipmaps = false;
    t.minFilter = LinearFilter;
    t.anisotropy = renderer.capabilities.getMaxAnisotropy();
    return t;
  };

  // Camera state for the LOD selector (ST-1). The sphere sits at the origin with
  // no pivot, so camera.position IS the unit-sphere frame.
  const getCameraState = (): CamState => {
    const f = new Vector3();
    camera.getWorldDirection(f);
    // R3 oblique LOD: in flight the camera may be TILTED, so its sub-point (posUnit)
    // sits behind what it looks at. Hand the selector the look anchor (lookDir) to
    // center the window on; the horizon cull still uses posUnit. Absent flight
    // (overview) lookDir is omitted → byte-identical nadir centering.
    const lookDir = flight ? lonLatToUnit(flight.subLon, flight.subLat) : undefined;
    return {
      posUnit: [camera.position.x, camera.position.y, camera.position.z],
      forward: [f.x, f.y, f.z],
      lookDir,
      fovY: (camera.fov * Math.PI) / 180,
      aspect: camera.aspect,
      near: camera.near,
      far: camera.far,
    };
  };

  // Settle-driven streaming (the "detail follows on SETTLE" v1): fire `settleCb`
  // once the flight camera has been still for SETTLE_MS after any change (drill,
  // pan, zoom). main.ts reconciles the patch set on settle.
  let settleCb: (() => void) | null = null;
  let lastFlightChange = 0;
  let settleFired = true;
  let lastPumpAt = 0;
  let streamPumps = 0; // during-motion pump count (Phase B e2e witness)
  const SETTLE_MS = 140;
  // Phase B (continuous-follow): now the tile raster is off the main thread, fill
  // DURING motion — pump the reconcile every PUMP_MS while the camera moves, not
  // only after it settles. Each pump is cheap on the main thread (a bounded pure
  // selector + cache diff + ≤ BUDGET worker posts); the heavy raster is in the
  // worker. The settle fire still runs as the final, complete high-res reconcile.
  const PUMP_MS = 90;
  const markFlightChanged = () => {
    lastFlightChange = performance.now();
    settleFired = false;
  };

  // Region-name billboard (1b-ii): an HTML label anchored to the drilled region's
  // CENTRE (a fixed unit vector), tracked each frame by projecting that point
  // through the camera — so it stays glued to the continent as you pan, and hides
  // when the region rotates behind the globe's horizon. position:fixed so it needs
  // no positioned ancestor; removed on dispose.
  const regionLabel = document.createElement("div");
  regionLabel.className = "globe-region-label";
  Object.assign(regionLabel.style, {
    position: "fixed",
    left: "0",
    top: "0",
    pointerEvents: "none",
    zIndex: "5",
    font: "italic 20px 'EB Garamond', Georgia, serif",
    color: "#f4ebd0",
    // A dark sepia pill so the label reads over light terrain AND dark sea alike.
    background: "rgba(21,17,12,0.55)",
    padding: "1px 10px",
    borderRadius: "3px",
    textShadow: "0 1px 4px rgba(0,0,0,0.9)",
    whiteSpace: "nowrap",
    transform: "translate(-9999px,-9999px)",
  });
  regionLabel.hidden = true;
  document.body.appendChild(regionLabel);
  let labelAnchor: Vector3 | null = null; // fixed unit vec at the region centre, or null
  const hideLabel = () => {
    labelAnchor = null;
    regionLabel.hidden = true;
    regionLabel.textContent = "";
  };
  // Per-frame flight signals for the e2e (a GPU camera can't be diffed from the
  // DOM): the sub-point the camera looks at + its altitude. A pan must move these.
  const writeFlightData = () => {
    if (!flight) return;
    canvas.dataset.subLon = flight.subLon.toFixed(4);
    canvas.dataset.subLat = flight.subLat.toFixed(4);
    canvas.dataset.altitude = flight.altitude.toFixed(4);
    canvas.dataset.camPitch = flight.pitch.toFixed(4); // R3 tilt witness (e2e)
  };

  // Cinematic fly-to: on a click, animate the camera so the clicked point swings to
  // face the viewer and zooms in, THEN drill (hand off to the 2D sector). `null`
  // when idle. `start`/`dur` are wall-clock ms; `onArrive` fires the drill.
  const FLY_MS = 600;
  let flyTo: { from: Vector3; target: Vector3; start: number; onArrive: () => void } | null = null;
  const easeInOut = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);
  // 1f entry-ease: glide from the camera's CURRENT pose into the flight pose over
  // a short window instead of the one-frame snap (the fly-to end pose and the
  // flight pose differ slightly — centroid vs click, radius, roll — and a deeper
  // drill re-frames mid-flight). `from` is captured at enterRegion; the ease
  // interpolates toward the LIVE flight pose, so a pan during the glide still
  // converges. The look target the camera currently uses is tracked in
  // `lastLook` (lookAt doesn't persist a readable target on the camera).
  const ENTRY_MS = 350;
  let entryEase: { from: CamPose; start: number } | null = null;
  let entryEases = 0; // monotonic; data-entry-eases lets the e2e prove the glide path ran
  const lastLook = new Vector3(0, 0, 0);

  const tick = () => {
    if (flyTo) {
      // Manual camera drive while flying (OrbitControls is disabled, so skip its
      // update — it would fight the lerp). The globe stays centred at the origin.
      const t = Math.min(1, (performance.now() - flyTo.start) / FLY_MS);
      camera.position.lerpVectors(flyTo.from, flyTo.target, easeInOut(t));
      camera.lookAt(0, 0, 0);
      lastLook.set(0, 0, 0);
      if (t >= 1) {
        const arrive = flyTo.onArrive;
        flyTo = null;
        controls.enabled = true;
        arrive(); // drill now that the camera has settled on the point
      }
    } else if (flight) {
      // Drilled (free-fly) mode: drive the camera from the flight state. The
      // anchor is framed in its LOCAL up frame, so an equatorial region orbits
      // correctly (no orbiting through the planet core).
      let pose = globeCamPose(flight);
      if (entryEase) {
        const t = Math.min(1, (performance.now() - entryEase.start) / ENTRY_MS);
        pose = lerpPose(entryEase.from, pose, easeInOut(t));
        if (t >= 1) entryEase = null;
      }
      camera.position.set(pose.position[0], pose.position[1], pose.position[2]);
      camera.up.set(pose.up[0], pose.up[1], pose.up[2]);
      camera.lookAt(pose.target[0], pose.target[1], pose.target[2]);
      lastLook.set(pose.target[0], pose.target[1], pose.target[2]);
      // Track the region billboard: project its FIXED anchor; hide it once the
      // region rotates past the horizon (dot(anchor, camPos) < 1 ⇒ behind the limb).
      if (labelAnchor && regionLabel.textContent) {
        if (labelAnchor.dot(camera.position) >= 1) {
          const ndc = labelAnchor.clone().project(camera);
          const rect = canvas.getBoundingClientRect();
          const px = rect.left + (ndc.x * 0.5 + 0.5) * rect.width;
          const py = rect.top + (-ndc.y * 0.5 + 0.5) * rect.height;
          regionLabel.style.transform = `translate(${px}px, ${py}px) translate(-50%, -160%)`;
          regionLabel.hidden = false;
        } else {
          regionLabel.hidden = true;
        }
      }
      // Stream the in-view (+ predicted-ahead) detail. Phase B: pump DURING motion
      // every PUMP_MS so detail follows the camera, then fire once more on SETTLE as
      // the final complete pass. (Pre-Phase-B this fired only on settle — main-thread
      // rasterize made during-motion fill a frame-stall; off-thread raster lifts that.)
      if (!settleFired) {
        const now = performance.now();
        if (now - lastPumpAt > PUMP_MS) {
          lastPumpAt = now;
          // Count ONLY during-motion pumps (not the settle fire below) — the e2e
          // witness that the reconcile follows the camera while it is still moving.
          canvas.dataset.streamPumps = String((streamPumps += 1));
          settleCb?.();
        }
        if (now - lastFlightChange > SETTLE_MS) {
          settleFired = true;
          settleCb?.();
        }
      }
    } else {
      controls.update();
      // Overview camera-up.y, for the e2e to prove the globe returns UPRIGHT after a
      // drill (flight leaves camera.up as a surface tangent; exitRegion must reset it
      // to +Y or OrbitControls' lookAt renders the globe rolled). ~1 ⇒ upright.
      canvas.dataset.camUpY = camera.up.y.toFixed(3);
    }
    // Dynamic near plane (R2 altitude + R3 pitch — the measured black-screen
    // defect): at low altitude/oblique pitch the fixed near=0.1 swallowed the
    // whole FOV. Recompute from the LIVE camera radius (which encodes pitch by
    // construction), pitch-aware for free — nearFor is pure + unit-tested. >5%
    // hysteresis before updateProjectionMatrix so a tilt/zoom doesn't churn the
    // projection matrix every frame; snap to a clamp boundary (0.002 / 0.1) so the
    // endpoints are always reached exactly.
    const near = nearFor(camera.position.length());
    const atBound = near === 0.002 || near === 0.1;
    if (Math.abs(near - camera.near) > camera.near * 0.05 || (atBound && near !== camera.near)) {
      camera.near = near;
      camera.updateProjectionMatrix();
      canvas.dataset.camNear = near.toFixed(4);
    }
    renderer.render(scene, camera);
    if (!painted) {
      // Signal first paint: a frame actually rendered (renderer instantiated +
      // RAF loop running). The e2e keys off this — strictly more than "WebGL is
      // available" (a bare canvas would report that even with no globe mounted).
      painted = true;
      canvas.dataset.rendered = "1";
    }
    raf = requestAnimationFrame(tick);
  };

  const resize = () => {
    const w = canvas.clientWidth || canvas.parentElement?.clientWidth || 1;
    const h = canvas.clientHeight || canvas.parentElement?.clientHeight || 1;
    renderer.setSize(w, h, false); // false: CSS controls the displayed size
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };

  // Click-to-pick: a near-stationary press is a click (drill); a drag rotates
  // (OrbitControls). On a click, raycast the sphere and hand the hit's intrinsic
  // surface UV to the callback — reading UV avoids reconstructing lat/lon, so the
  // hemisphere convention can't flip.
  const raycaster = new Raycaster();
  let pickCb: ((u: number, v: number) => void) | null = null;
  let patchPickCb: ((u: number, v: number) => void) | null = null; // deeper drill (1c)
  let down: { x: number; y: number } | null = null;
  canvas.addEventListener("pointerdown", (e) => {
    // The globe canvas is a CHILD of #map, where PanZoom (2D pan/zoom of the now-
    // hidden SVG layer) binds pointerdown/wheel. Without this, a globe drag/zoom ALSO
    // drives PanZoom and silently corrupts the hidden 2D transform — which then paints
    // wrong on return to a same-dimension 2D map (fit() is skipped when dims are
    // unchanged). Contain globe input at the source; OrbitControls listens on this
    // same element, so stopPropagation (bubble-only) leaves it untouched. (Pre-existing
    // bubbling hazard, same class as the #map drillAt double-fire.)
    e.stopPropagation();
    // R3 TILT: right-button OR Shift+left starts a PITCH drag (never a pan or a
    // drill click). Plain left-button keeps panning + the click-to-drill candidate.
    if (flight && (e.button === 2 || (e.button === 0 && e.shiftKey))) {
      pitchLast = { x: e.clientX, y: e.clientY };
      down = null;
      return;
    }
    if (e.button !== 0) return; // only the left button pans / drills
    down = { x: e.clientX, y: e.clientY };
    if (flight) dragLast = { x: e.clientX, y: e.clientY }; // start a flight pan
  });
  // Right-drag (and Shift+left) tilt the camera; suppress the context menu so the
  // gesture isn't interrupted. preventDefault here only — the global menu elsewhere.
  canvas.addEventListener("contextmenu", (e) => {
    if (flight) e.preventDefault();
  });
  // Flight-mode pan/tilt: drag the surface under the camera, or tilt toward the
  // horizon (OrbitControls is disabled while drilled, so it ignores these — no
  // conflict). Sub-point travel scales with altitude; tilt is clamped so the
  // camera can never tunnel into the displaced terrain.
  canvas.addEventListener("pointermove", (e) => {
    if (flight && pitchLast) {
      const dy = e.clientY - pitchLast.y;
      pitchLast = { x: e.clientX, y: e.clientY };
      // Drag UP (dy < 0) tilts toward the horizon (pitch grows). Clamp to the
      // altitude-dependent ceiling maxPitch(alt) — a POSE INVARIANT (camera.test.ts).
      const next = flight.pitch - dy * PITCH_SPEED;
      flight.pitch = Math.max(0, Math.min(maxPitch(flight.altitude), next));
      writeFlightData();
      markFlightChanged(); // tilt is camera motion: pump/settle treat it like pan/zoom
      return;
    }
    if (!flight || !dragLast) return;
    const dx = e.clientX - dragLast.x;
    const dy = e.clientY - dragLast.y;
    dragLast = { x: e.clientX, y: e.clientY };
    const next = panSubPoint(flight, dx, dy, PAN_SPEED); // pure → the SIGN is unit-tested
    flight.subLon = next.subLon;
    flight.subLat = next.subLat;
    writeFlightData();
    markFlightChanged(); // re-stream when this pan settles
  });
  // Flight-mode zoom: wheel changes altitude (OrbitControls owns the wheel only at
  // the overview). Multiplicative so each notch is a constant %.
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.stopPropagation(); // contain from #map's PanZoom (would zoom the hidden 2D layer) — see pointerdown
      if (!flight) return; // let OrbitControls handle the overview wheel
      e.preventDefault();
      flight.altitude = Math.max(MIN_ALT, Math.min(MAX_ALT, flight.altitude * Math.exp(e.deltaY * ZOOM_RATE)));
      // R3 re-clamp: zooming DOWN while tilted shrinks the clearance ceiling, so a
      // previously-valid pitch could tunnel into terrain — re-clamp to the new altitude.
      flight.pitch = Math.min(flight.pitch, maxPitch(flight.altitude));
      writeFlightData();
      markFlightChanged(); // re-stream when this zoom settles
    },
    { passive: false },
  );
  canvas.addEventListener("pointerup", (e) => {
    dragLast = null; // end any flight pan
    pitchLast = null; // end any flight tilt
    if (!down) return;
    const moved = Math.hypot(e.clientX - down.x, e.clientY - down.y);
    down = null;
    if (moved > 6) return; // a drag (pan / rotate), not a click
    const rect = canvas.getBoundingClientRect();
    const ndc = new Vector2(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1,
    );
    raycaster.setFromCamera(ndc, camera);
    // DRILLED (a patch set is shown): a click drills DEEPER (1c). Raycast the base
    // sphere (always present, radius 1) and derive the surface uv from the EXACT
    // hit point (`unitToUv`) — NOT the raycaster's `hit.uv`, which is interpolated
    // across the mesh's flat triangles and lands up to ~4° off (a click is a
    // precision contract). Same trusted convention the overview pick uses,
    // independent of which streamed patch happens to cover the click.
    if (flight && patchPickCb && patches.size > 0) {
      // R3 PATCHES-FIRST: raycast the LIVE DISPLACED patch meshes before the base
      // sphere. The displaced surface is what the user sees; radial displacement
      // preserves direction, so `normalize(hit.point)` IS the exact ground pick —
      // the pinned exact-hit-point mechanism (NOT the barycentric hit.uv), now
      // defended against the off-center / oblique parallax the smooth base sphere
      // introduces (~1.4° at pitch 50°; pick.test.ts discriminator). Fallback to the
      // base sphere for a click beyond the streamed set (undisplaced ⇒ parallax-free).
      const meshes = Array.from(patches.values(), (p) => p.mesh);
      const phit = raycaster.intersectObjects(meshes)[0] ?? raycaster.intersectObject(sphere)[0];
      if (phit?.point) {
        const pd = phit.point.clone().normalize();
        const { u: pu, v: pv } = unitToUv([pd.x, pd.y, pd.z]);
        patchPickCb(pu, pv);
      }
      return;
    }
    // OVERVIEW: a click flies to the clicked point + drills from the base sphere
    // (1a). Gated on pickable (false while drilled) and not already flying.
    if (!pickCb || !pickable || flyTo) return;
    const hit = raycaster.intersectObject(sphere)[0];
    if (!hit?.point) return; // clicked the backdrop, not the sphere
    // `hit.point` (world space) is geometrically exact; `hit.uv` is barycentric
    // across flat triangles (up to ~4° off on the 64×48 sphere) — derive uv from
    // the point so the drill lands precisely where the user clicked.
    const dir = hit.point.clone().normalize();
    const { u, v } = unitToUv([dir.x, dir.y, dir.z]);
    const dist = Math.max(controls.minDistance + 0.3, camera.position.length() * 0.55);
    controls.enabled = false; // hand the camera to the fly animation
    flyTo = {
      from: camera.position.clone(),
      target: dir.multiplyScalar(dist),
      start: performance.now(),
      onArrive: () => pickCb!(u, v),
    };
  });

  return {
    show() {
      canvas.hidden = false;
      resize();
      if (!running) {
        running = true;
        raf = requestAnimationFrame(tick);
      }
    },
    hide() {
      canvas.hidden = true;
      running = false;
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
      flyTo = null; // cancel any in-flight fly so a re-show doesn't resume it
      entryEase = null; // and any in-flight entry glide
      controls.enabled = true;
      // R3: reset the tilt-dynamic near to the overview default (the raf loop is
      // stopped on hide, so the tick can't restore it on re-show).
      camera.near = 0.1;
      camera.updateProjectionMatrix();
      disposePatches(); // free all patch GPU resources when leaving the globe
      hideLabel();
    },
    setTexture(source: HTMLCanvasElement) {
      fadePoleCaps(source); // tidy the pole pinch to sea; the longitude seam is continuous now
      const next = wrapTexture(new CanvasTexture(source));
      next.anisotropy = renderer.capabilities.getMaxAnisotropy();
      const prev = material.map as Texture | null;
      material.map = next;
      material.needsUpdate = true;
      prev?.dispose();
      // Signal a real world texture was applied (vs the constructor's placeholder
      // graticule, which never goes through setTexture). The e2e keys off this;
      // the monotonic `textures` count lets the time-slider e2e prove a per-year
      // RE-texture happened (a new frame uploaded), not just the first one.
      canvas.dataset.textured = "1";
      canvas.dataset.textures = String((textureCount += 1));
    },
    resize,
    onPick(cb: (u: number, v: number) => void) {
      pickCb = cb;
    },
    onPatchPick(cb: (u: number, v: number) => void) {
      patchPickCb = cb;
    },
    enterRegion(subLon: number, subLat: number, altitude: number, name: string) {
      // Enter free-fly mode over the drilled region: the camera is driven from
      // this state (tick → globeCamPose) and OrbitControls steps aside. The drill
      // fly-to has already swung the camera near the point; this re-centres it on
      // the region and sets the framing altitude.
      // 1f ENTRY-EASE: the fly-to end pose and this flight pose differ slightly
      // (centroid vs click, radius, roll) — and a deeper drill re-frames from the
      // previous flight pose — so instead of snapping, capture the camera's
      // CURRENT pose and let tick glide it into the flight pose over ENTRY_MS.
      // Drop the previous level's patches immediately so a re-drill (or a level
      // change) doesn't leave stale patches hanging; the new in-view set streams in
      // on settle (the coarse base covers the gap). ST-1 is one level at a time.
      disposePatches();
      const cl = clampLat(subLat);
      entryEase = {
        from: {
          position: [camera.position.x, camera.position.y, camera.position.z],
          target: [lastLook.x, lastLook.y, lastLook.z],
          up: [camera.up.x, camera.up.y, camera.up.z],
        },
        start: performance.now(),
      };
      canvas.dataset.entryEases = String((entryEases += 1));
      flight = { subLon, subLat: cl, altitude, heading: 0, pitch: 0 };
      flyTo = null;
      controls.enabled = false;
      pickable = false;
      canvas.dataset.region = "1";
      // 1b-ii: anchor the region-name billboard at the region centre (fixed); the
      // tick projector positions it each frame. Empty name → the projector hides it.
      const a = lonLatToUnit(subLon, cl);
      labelAnchor = new Vector3(a[0], a[1], a[2]);
      regionLabel.textContent = name;
      reliefMaxSeen = 0; // new drill, fresh displacement witness
      canvas.dataset.reliefMax = "0";
      writeFlightData();
      markFlightChanged(); // trigger the streaming reconcile once the drill settles
    },
    showPatch(sector: Sector, source: HTMLCanvasElement, _params: PatchParams, gpuBytes: number, displaced: DisplacedPatch) {
      // Add (or replace) ONE patch in the streaming cache: a RELIEF-DISPLACED
      // grid over the sector's lon/lat span (R2 — built by the pure
      // displacedPatchArrays from the worker's seam-banded heightfield; sea sits
      // at exactly the legacy 1.001 shell). depthTest:true — displaced terrain
      // needs real self-occlusion at oblique pitch, and the spike measured ZERO
      // z-fight (the base sphere's facet chords dip below r=1, so the 0.001
      // offset is enormous vs depth precision). renderOrder stays as an inert
      // tiebreak only (enterRegion disposes the previous level synchronously —
      // no transition overlap exists for it to order).
      const key = patchKey(sector, PATCH_STYLE);
      const existing = patches.get(key);
      if (existing) disposeEntry(existing);
      const geo = new BufferGeometry();
      geo.setAttribute("position", new BufferAttribute(displaced.positions, 3));
      geo.setAttribute("uv", new BufferAttribute(displaced.uvs, 2));
      geo.setIndex(new BufferAttribute(displaced.index, 1));
      const mat = new MeshBasicMaterial({ map: patchTexture(source), depthTest: true });
      const mesh = new Mesh(geo, mat);
      mesh.renderOrder = 1 + sector.level; // inert tiebreak (see above)
      scene.add(mesh);
      // Running MAX across this drill's patches (NOT last-write-wins: an
      // all-sea tile landing last would overwrite a mountain tile's witness
      // with 0 and false-red the e2e).
      if (displaced.reliefMax > reliefMaxSeen) {
        reliefMaxSeen = displaced.reliefMax;
        canvas.dataset.reliefMax = reliefMaxSeen.toFixed(5);
      }
      patches.set(key, { mesh, sector, texBytes: gpuBytes, lastSeen: performance.now() });
      canvas.dataset.patch = String(sector.level);
      canvas.dataset.patchTextures = String((patchTexCount += 1));
      canvas.dataset.refines = String((refineCount += 1));
      writeLive();
    },
    evictPatch(key: string) {
      const e = patches.get(key);
      if (!e) return;
      disposeEntry(e);
      patches.delete(key);
      writeLive();
    },
    liveEntries() {
      return Array.from(patches.values()).map((e) => ({
        key: patchKey(e.sector, PATCH_STYLE),
        lastSeen: e.lastSeen,
        texBytes: e.texBytes,
      }));
    },
    markSeen(keys: string[]) {
      const now = performance.now();
      for (const k of keys) {
        const e = patches.get(k);
        if (e) e.lastSeen = now;
      }
    },
    getCameraState,
    onSettle(cb: () => void) {
      settleCb = cb;
    },
    exitRegion() {
      // Back to the whole-globe overview: drop flight mode and restore the canonical
      // overview camera. Flight left `camera.up` as a SURFACE TANGENT — it MUST be
      // reset to world +Y, or OrbitControls' lookAt renders the returned globe
      // rolled/tilted. Snap to the home framing (0,0,3.6) so the return is
      // predictable (and so a fresh-world re-entry, which reuses this, starts clean).
      disposePatches();
      hideLabel();
      flight = null;
      flyTo = null;
      entryEase = null; // a stale glide must not replay on the next drill
      pickable = true;
      delete canvas.dataset.region;
      delete canvas.dataset.subLon;
      delete canvas.dataset.subLat;
      delete canvas.dataset.altitude;
      controls.enabled = true;
      camera.up.set(0, 1, 0);
      camera.position.set(0, 0, 3.6);
      controls.target.set(0, 0, 0);
      lastLook.set(0, 0, 0); // the overview looks at the origin again
      // R3: restore the overview near plane (flight may have shrunk it under tilt /
      // low altitude) and clear the tilt witness — write data-cam-near directly so
      // the Globe-crumb return is observably restored (the tick's hysteresis would
      // otherwise skip the write when the value already matches).
      camera.near = 0.1;
      camera.updateProjectionMatrix();
      canvas.dataset.camNear = (0.1).toFixed(4);
      delete canvas.dataset.camPitch;
      controls.update();
    },
    dispose() {
      this.hide();
      regionLabel.remove();
      geometry.dispose();
      material.map?.dispose();
      material.dispose();
      controls.dispose();
      renderer.dispose();
    },
  };
}

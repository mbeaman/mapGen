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
  type GlobeCamState,
  globeCamPose,
  LAT_MAX,
  lonLatToUnit,
  panSubPoint,
  type PatchParams,
} from "./camera";

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
  /** Drill stays on the globe (increment 1a): enter free-fly mode framing the
   *  drilled region (sub-point `subLon`/`subLat` in radians, `altitude` above the
   *  unit sphere). The camera is then driven from this state — drag pans across
   *  the surface, wheel zooms — and the sphere is NOT hidden (no 2D handoff).
   *  Sets `data-region="1"` and the per-frame `data-sub-lon/lat`/`data-altitude`
   *  the e2e reads. (Deeper 3D drilling + the detail patch land in later increments.) */
  enterRegion(subLon: number, subLat: number, altitude: number, name: string): void;
  /** Lay the rasterized refined-sector render onto a curved partial-sphere patch
   *  over the base globe (increment 1b), occupying the sector's lon/lat span
   *  (`params` from `sectorPatchParams`). At most one patch exists; this disposes
   *  any previous one. Sets `data-patch=<level>` + bumps `data-patch-textures`. */
  showPatch(source: HTMLCanvasElement, params: PatchParams, level: number): void;
  /** Return to the whole-globe overview: drop flight mode, dispose the patch, pull
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
/// KNOWN LIMITATION (accepted): the generated world is a FLAT, non-periodic grid
/// — its left/right edges are independent coastlines, not a cylinder, and its
/// top/bottom rows aren't single points. So `RepeatWrapping` makes the texture
/// meet itself at the antimeridian but the two coastlines won't align (a faint
/// vertical seam at lon ±180°), and the poles show mild pinch distortion. This
/// is inherent to the data; hiding it would need a Rust-side equirectangular
/// render that fades the edge columns (future work, tracked in the backlog).
function wrapTexture(t: CanvasTexture): CanvasTexture {
  t.colorSpace = SRGBColorSpace;
  t.wrapS = RepeatWrapping; // longitude wraps around
  t.wrapT = ClampToEdgeWrapping; // latitude clamps at the poles
  // No mipmaps: the globe is viewed ~1:1, so the mip chain adds nothing visible,
  // but GENERATING it on every upload is costly — brutally so under software GL
  // (headless SwiftShader) and wasted work even on a real GPU. Skipping it is the
  // single biggest win for fluid per-year re-texturing while scrubbing.
  t.generateMipmaps = false;
  t.minFilter = LinearFilter;
  return t;
}

/// Edge-band widths for [`fadeMapEdges`] — pure (no canvas), so the geometry is
/// trivially inspectable. The seam bands are ~5% of width each (the antimeridian
/// at lon ±180°), the pole bands ~7% of height (the pinched top/bottom rows).
/// Narrow on purpose: only the artifact-prone edges fade; the map interior is
/// untouched.
function edgeFadeBands(w: number, h: number): { seam: number; pole: number } {
  return { seam: Math.max(1, Math.round(w * 0.05)), pole: Math.max(1, Math.round(h * 0.08)) };
}

/// The deep-sea blue-grey the globe texture's open-ocean edges already are — the
/// abyss end of `style/planet.rs::sea_color`, which the encircling sea around the
/// continents renders in. The fade bands blend to THIS so the antimeridian and
/// poles read as a continuation of that open ocean. A fixed colour (not sampled
/// from the canvas): the globe is always the `globe` style, so its sea colour is
/// known — and sampling via `getImageData` forced a multi-hundred-ms GPU→CPU
/// readback per call (the dominant cost of a per-year scrub re-texture).
const GLOBE_SEA = "rgb(93,122,134)";

/// Hide the equirectangular wrap artifacts the `wrapTexture` note documents: the
/// flat world's left/right edges are DIFFERENT coastlines that don't meet at lon
/// ±180° (a seam), and its top/bottom rows aren't single points (pole pinch). We
/// fade the four edge bands of the texture to the open-ocean colour, so the
/// antimeridian and poles read as open sea — both seam edges become water and
/// meet cleanly, and the pinched poles become tidy ocean caps instead of a smear.
/// The geometric pinch is inherent to equirectangular-on-a-sphere; this removes
/// the visible artifact. Mutates `canvas` in place (a throwaway texture canvas).
function fadeMapEdges(canvas: HTMLCanvasElement): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const w = canvas.width;
  const h = canvas.height;
  const opaque = GLOBE_SEA;
  const clear = "rgba(93,122,134,0)"; // GLOBE_SEA, fully transparent
  const { seam, pole } = edgeFadeBands(w, h);
  // Paint one edge band: opaque sea for the inner `solid` fraction (covering the
  // worst-compressed rows/columns outright), then a gradient fading to
  // transparent so the interior shows through untouched. The poles get a larger
  // solid cap than the seam because equirect compression is extreme right at the
  // pole point — a pure gradient there leaves a faint land starburst.
  const band = (
    rx: number,
    ry: number,
    rw: number,
    rh: number,
    gx0: number,
    gy0: number,
    gx1: number,
    gy1: number,
    solid: number,
  ) => {
    const grad = ctx.createLinearGradient(gx0, gy0, gx1, gy1);
    grad.addColorStop(0, opaque);
    grad.addColorStop(solid, opaque);
    grad.addColorStop(1, clear);
    ctx.fillStyle = grad;
    ctx.fillRect(rx, ry, rw, rh);
  };
  band(0, 0, seam, h, 0, 0, seam, 0, 0.35); // left edge → inward
  band(w - seam, 0, seam, h, w, 0, w - seam, 0, 0.35); // right edge → inward
  band(0, 0, w, pole, 0, 0, 0, pole, 0.6); // top edge → down
  band(0, h - pole, w, pole, 0, h, 0, h - pole, 0.6); // bottom edge → up
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
  const PAN_SPEED = 0.0022; // rad of sub-point travel per px, scaled by altitude
  const ZOOM_RATE = 0.001; // altitude multiplier per wheel-delta unit
  const MIN_ALT = 0.05;
  const MAX_ALT = 2.5;
  const clampLat = (lat: number) => Math.max(-LAT_MAX, Math.min(LAT_MAX, lat));

  // Curved high-detail patch (increment 1b): a partial-sphere segment over the
  // base globe, textured with the rasterized refined-sector render. At most ONE
  // exists (the hard line against an unbounded tile pyramid); a new drill / return
  // disposes it. Drawn with depthTest=false + renderOrder=1 so it composites over
  // its base region unconditionally — no z-fighting to depend on under SwiftShader.
  let patch: Mesh | null = null;
  let patchTexCount = 0; // bumped per patch texture upload — its OWN e2e signal
  const disposePatch = () => {
    if (!patch) return;
    scene.remove(patch);
    patch.geometry.dispose();
    const m = patch.material as MeshBasicMaterial;
    m.map?.dispose();
    m.dispose();
    patch = null;
    delete canvas.dataset.patch;
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
  };

  // Cinematic fly-to: on a click, animate the camera so the clicked point swings to
  // face the viewer and zooms in, THEN drill (hand off to the 2D sector). `null`
  // when idle. `start`/`dur` are wall-clock ms; `onArrive` fires the drill.
  const FLY_MS = 600;
  let flyTo: { from: Vector3; target: Vector3; start: number; onArrive: () => void } | null = null;
  const easeInOut = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

  const tick = () => {
    if (flyTo) {
      // Manual camera drive while flying (OrbitControls is disabled, so skip its
      // update — it would fight the lerp). The globe stays centred at the origin.
      const t = Math.min(1, (performance.now() - flyTo.start) / FLY_MS);
      camera.position.lerpVectors(flyTo.from, flyTo.target, easeInOut(t));
      camera.lookAt(0, 0, 0);
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
      const pose = globeCamPose(flight);
      camera.position.set(pose.position[0], pose.position[1], pose.position[2]);
      camera.up.set(pose.up[0], pose.up[1], pose.up[2]);
      camera.lookAt(pose.target[0], pose.target[1], pose.target[2]);
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
    } else {
      controls.update();
      // Overview camera-up.y, for the e2e to prove the globe returns UPRIGHT after a
      // drill (flight leaves camera.up as a surface tangent; exitRegion must reset it
      // to +Y or OrbitControls' lookAt renders the globe rolled). ~1 ⇒ upright.
      canvas.dataset.camUpY = camera.up.y.toFixed(3);
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
  let down: { x: number; y: number } | null = null;
  canvas.addEventListener("pointerdown", (e) => {
    down = { x: e.clientX, y: e.clientY };
    if (flight) dragLast = { x: e.clientX, y: e.clientY }; // start a flight pan
  });
  // Flight-mode pan: drag the surface under the camera (OrbitControls is disabled
  // while drilled, so it ignores these — no conflict). Sub-point travel scales
  // with altitude, so the drag feels the same at every zoom.
  canvas.addEventListener("pointermove", (e) => {
    if (!flight || !dragLast) return;
    const dx = e.clientX - dragLast.x;
    const dy = e.clientY - dragLast.y;
    dragLast = { x: e.clientX, y: e.clientY };
    const next = panSubPoint(flight, dx, dy, PAN_SPEED); // pure → the SIGN is unit-tested
    flight.subLon = next.subLon;
    flight.subLat = next.subLat;
    writeFlightData();
  });
  // Flight-mode zoom: wheel changes altitude (OrbitControls owns the wheel only at
  // the overview). Multiplicative so each notch is a constant %.
  canvas.addEventListener(
    "wheel",
    (e) => {
      if (!flight) return; // let OrbitControls handle the overview wheel
      e.preventDefault();
      flight.altitude = Math.max(MIN_ALT, Math.min(MAX_ALT, flight.altitude * Math.exp(e.deltaY * ZOOM_RATE)));
      writeFlightData();
    },
    { passive: false },
  );
  canvas.addEventListener("pointerup", (e) => {
    dragLast = null; // end any flight pan
    if (!down) return;
    const moved = Math.hypot(e.clientX - down.x, e.clientY - down.y);
    down = null;
    if (moved > 6 || !pickCb || !pickable) return; // a drag, or already drilled
    const rect = canvas.getBoundingClientRect();
    const ndc = new Vector2(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1,
    );
    raycaster.setFromCamera(ndc, camera);
    const hit = raycaster.intersectObject(sphere)[0];
    if (!hit?.uv || !hit.point) return; // clicked the backdrop, not the sphere
    const u = hit.uv.x;
    const v = hit.uv.y;
    // Fly the camera to face the clicked point (its direction from the globe
    // centre) and zoom partway in, then drill. Reading `hit.point` (world space)
    // is independent of the UV → no hemisphere flip. Skip if already flying.
    if (flyTo) return;
    const dir = hit.point.clone().normalize();
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
      controls.enabled = true;
      disposePatch(); // free the patch's GPU resources when leaving the globe
      hideLabel();
    },
    setTexture(source: HTMLCanvasElement) {
      fadeMapEdges(source); // fade the seam + poles to sea before uploading
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
    enterRegion(subLon: number, subLat: number, altitude: number, name: string) {
      // Enter free-fly mode over the drilled region: the camera is driven from
      // this state (tick → globeCamPose) and OrbitControls steps aside. The drill
      // fly-to has already swung the camera near the point; this re-centres it on
      // the region and sets the framing altitude.
      // DEFERRED (1a, cosmetic): the fly-to end pose and this flight pose differ
      // slightly (centroid vs click, radius, roll), so there's a one-frame snap
      // here; a short entry-ease is a polish increment, not a 1a correctness gap.
      // Drop any previous patch immediately so a re-drill doesn't leave the OLD
      // region's patch hanging off to the side during the new refine (1b); the new
      // patch arrives once its refine resolves (coarse base covers the gap).
      disposePatch();
      const cl = clampLat(subLat);
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
      writeFlightData();
    },
    showPatch(source: HTMLCanvasElement, params: PatchParams, level: number) {
      // Lay the refined sector onto a curved segment occupying its lon/lat span on
      // the unit sphere (radius 1.001 → just above the base skin). depthTest=false
      // + renderOrder=1 draw it over the base region with no z-fight dependency.
      disposePatch();
      const geo = new SphereGeometry(
        1.001,
        params.segW,
        params.segH,
        params.phiStart,
        params.phiLength,
        params.thetaStart,
        params.thetaLength,
      );
      const mat = new MeshBasicMaterial({ map: patchTexture(source), depthTest: false });
      patch = new Mesh(geo, mat);
      patch.renderOrder = 1;
      scene.add(patch);
      canvas.dataset.patch = String(level);
      canvas.dataset.patchTextures = String((patchTexCount += 1));
    },
    exitRegion() {
      // Back to the whole-globe overview: drop flight mode and restore the canonical
      // overview camera. Flight left `camera.up` as a SURFACE TANGENT — it MUST be
      // reset to world +Y, or OrbitControls' lookAt renders the returned globe
      // rolled/tilted. Snap to the home framing (0,0,3.6) so the return is
      // predictable (and so a fresh-world re-entry, which reuses this, starts clean).
      disposePatch();
      hideLabel();
      flight = null;
      flyTo = null;
      pickable = true;
      delete canvas.dataset.region;
      delete canvas.dataset.subLon;
      delete canvas.dataset.subLat;
      delete canvas.dataset.altitude;
      controls.enabled = true;
      camera.up.set(0, 1, 0);
      camera.position.set(0, 0, 3.6);
      controls.target.set(0, 0, 0);
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

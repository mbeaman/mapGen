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
  Scene,
  SphereGeometry,
  Texture,
  CanvasTexture,
  WebGLRenderer,
  SRGBColorSpace,
  RepeatWrapping,
  ClampToEdgeWrapping,
} from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

export interface GlobeHandle {
  /** Reveal the canvas and start the render loop. */
  show(): void;
  /** Hide the canvas and stop the render loop (renderer is kept for re-show). */
  hide(): void;
  /** Replace the sphere's map texture with a (canvas) source. Disposes the old. */
  setTexture(source: HTMLCanvasElement): void;
  /** Re-read the container size (call on resize / on show). */
  resize(): void;
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
function wrapTexture(t: CanvasTexture): CanvasTexture {
  t.colorSpace = SRGBColorSpace;
  t.wrapS = RepeatWrapping; // longitude wraps around
  t.wrapT = ClampToEdgeWrapping; // latitude clamps at the poles
  return t;
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
  const tick = () => {
    controls.update();
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
    },
    setTexture(source: HTMLCanvasElement) {
      const next = wrapTexture(new CanvasTexture(source));
      next.anisotropy = renderer.capabilities.getMaxAnisotropy();
      const prev = material.map as Texture | null;
      material.map = next;
      material.needsUpdate = true;
      prev?.dispose();
      // Signal a real world texture was applied (vs the constructor's placeholder
      // graticule, which never goes through setTexture). The e2e keys off this.
      canvas.dataset.textured = "1";
    },
    resize,
    dispose() {
      this.hide();
      geometry.dispose();
      material.map?.dispose();
      material.dispose();
      controls.dispose();
      renderer.dispose();
    },
  };
}

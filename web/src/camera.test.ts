import { Mesh, PerspectiveCamera, Raycaster, SphereGeometry, Vector2, Vector3 } from "three";
import { describe, expect, it } from "vitest";
import {
  type CamPose,
  type GlobeCamState,
  globeCamPose,
  lerpPose,
  lonLatToUnit,
  panSubPoint,
  sectorPatchParams,
  worldToLonLat,
} from "./camera";
import { latLonToWorld, patchUvToWorld, ROOT, sectorRect, uvToWorld } from "./sector";

const W = 2048;
const H = 1280;
const near = (a: number, b: number, eps = 1e-6) => expect(Math.abs(a - b)).toBeLessThan(eps);
const length = (v: [number, number, number]) => Math.hypot(v[0], v[1], v[2]);
const dot = (a: [number, number, number], b: [number, number, number]) =>
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

describe("lonLatToUnit", () => {
  it("north pole → +Y, (lon0,lat0) → +X, always unit length", () => {
    const np = lonLatToUnit(0, Math.PI / 2);
    near(np[0], 0);
    near(np[1], 1);
    near(np[2], 0);
    const eq = lonLatToUnit(0, 0);
    near(eq[0], 1);
    near(eq[1], 0);
    near(eq[2], 0);
    for (const [lon, lat] of [
      [0.3, 0.4],
      [2.0, -0.9],
      [-1.5, 0.2],
    ]) {
      near(length(lonLatToUnit(lon, lat)), 1);
    }
  });
});

describe("worldToLonLat", () => {
  it("inverts sector.ts latLonToWorld", () => {
    for (const [lat, lon] of [
      [0, 0],
      [0.5, 1.0],
      [-0.8, -2.0],
    ]) {
      const w = latLonToWorld(lat, lon, W, H);
      const ll = worldToLonLat(w.x, w.y, W, H);
      near(ll.lon, lon);
      near(ll.lat, lat);
    }
  });
});

describe("globeCamPose", () => {
  it("pitch 0: looks straight down at the anchor from altitude, up ⊥ view, camera outside the sphere", () => {
    // Equatorial AND mid-latitude sub-points: the equatorial case is the one an
    // origin-+Y polar clamp gets wrong (orbits through the core), so it must pass.
    for (const [subLon, subLat] of [
      [0, 0],
      [1.2, 0.0],
      [2.5, -0.7],
      [-2.0, 0.9],
    ]) {
      const s: GlobeCamState = { subLon, subLat, altitude: 0.8, heading: 0, pitch: 0 };
      const { position, target, up } = globeCamPose(s);
      near(length(target), 1); // anchor on the surface
      near(length(position), 1.8); // camera at radius 1 + altitude
      expect(length(position)).toBeGreaterThan(1); // outside the sphere — never inside
      const view: [number, number, number] = [
        target[0] - position[0],
        target[1] - position[1],
        target[2] - position[2],
      ];
      const vlen = length(view);
      near(dot(up, view) / vlen, 0); // up ⊥ view → no degenerate roll
      near(length(up), 1);
      // Screen-up must carry a NORTHWARD (+Y pole) component — pins the DIRECTION
      // of `north`, not just orthogonality. A cross-product arg-flip
      // (cross(east,P) instead of cross(P,east)) inverts the horizon yet keeps the
      // orthogonality + raycast-view tests green; this is the assertion that fails it.
      expect(dot(up, [0, 1, 0])).toBeGreaterThan(0);
    }
  });

  it("heading rotates screen-up ~90°; pitch tilts the view yet keeps the camera outside", () => {
    const base: GlobeCamState = { subLon: 0.5, subLat: 0.3, altitude: 0.5, heading: 0, pitch: 0 };
    const up0 = globeCamPose(base).up;
    const up90 = globeCamPose({ ...base, heading: Math.PI / 2 }).up;
    near(dot(up0, up90), 0); // a 90° heading change rotates up by ~90°
    const tilt = globeCamPose({ ...base, pitch: 1.0 });
    expect(length(tilt.position)).toBeGreaterThan(1); // still outside the sphere when tilted
  });
});

describe("sectorPatchParams", () => {
  it("ROOT covers the whole sphere; a level-2 sector maps to its exact lon/lat sub-ranges", () => {
    const root = sectorPatchParams(ROOT, W, H);
    near(root.phiStart, 0);
    near(root.phiLength, 2 * Math.PI);
    near(root.thetaStart, 0);
    near(root.thetaLength, Math.PI);
    // level-2 (1,1) on a 4×4 grid → world rect {512,320,512,320} (see sector.test.ts)
    const p = sectorPatchParams({ level: 2, sx: 1, sy: 1 }, W, H);
    near(p.phiStart, Math.PI / 2); // 512/2048 · 2π
    near(p.phiLength, Math.PI / 2);
    near(p.thetaStart, Math.PI / 4); // 320/1280 · π
    near(p.thetaLength, Math.PI / 4);
  });

  it("segW/segH track the lon/lat aspect (else the cartography squashes) and centerDir matches the sector center", () => {
    const p = sectorPatchParams({ level: 2, sx: 0, sy: 1 }, W, H); // wide-ish sector
    // The segment aspect must follow the angular aspect within rounding.
    expect(Math.abs(p.segW / p.segH - p.phiLength / p.thetaLength)).toBeLessThan(0.25);
    // centerDir is the unit vector at the sector center (cross-checked vs lonLatToUnit).
    const cx = 0 + 512 / 2;
    const cyWorld = 320 + 320 / 2;
    const { lon, lat } = worldToLonLat(cx, cyWorld, W, H);
    const expected = lonLatToUnit(lon, lat);
    near(p.centerDir[0], expected[0]);
    near(p.centerDir[1], expected[1]);
    near(p.centerDir[2], expected[2]);
  });

  it("a ray toward centerDir hits the PATCH at its centre (uv ≈ 0.5,0.5) — pins the geometry on the real partial sphere", () => {
    const p = sectorPatchParams({ level: 2, sx: 2, sy: 1 }, W, H);
    const patch = new Mesh(
      new SphereGeometry(1.001, p.segW, p.segH, p.phiStart, p.phiLength, p.thetaStart, p.thetaLength),
    );
    patch.updateMatrixWorld(true);
    const ray = new Raycaster();
    ray.set(
      new Vector3(p.centerDir[0] * 3, p.centerDir[1] * 3, p.centerDir[2] * 3),
      new Vector3(-p.centerDir[0], -p.centerDir[1], -p.centerDir[2]).normalize(),
    );
    const hit = ray.intersectObject(patch)[0];
    expect(hit?.uv).toBeTruthy();
    expect(hit!.uv!.x).toBeCloseTo(0.5, 1);
    expect(hit!.uv!.y).toBeCloseTo(0.5, 1);
  });

  it("patch uv orientation: north → top (uv.y>0.5), west → left (uv.x<0.5) — pins a flip/mirror off-GPU", () => {
    // Pins the GEOMETRY's phi→u, theta→uv.y layout against a flip/mirror (the
    // residual texture-row flipY composition stays screenshot-only). North = theta
    // toward thetaStart (the +Y pole); west = lower phi (lower longitude).
    const p = sectorPatchParams({ level: 2, sx: 2, sy: 1 }, W, H);
    const patch = new Mesh(
      new SphereGeometry(1.001, p.segW, p.segH, p.phiStart, p.phiLength, p.thetaStart, p.thetaLength),
    );
    patch.updateMatrixWorld(true);
    const ray = new Raycaster();
    // three.js SphereGeometry vertex for (phi, theta)
    const at = (phi: number, theta: number): [number, number, number] => [
      -Math.cos(phi) * Math.sin(theta),
      Math.cos(theta),
      Math.sin(phi) * Math.sin(theta),
    ];
    const hitUv = (phi: number, theta: number) => {
      const d = at(phi, theta);
      ray.set(
        new Vector3(d[0] * 3, d[1] * 3, d[2] * 3),
        new Vector3(-d[0], -d[1], -d[2]).normalize(),
      );
      return ray.intersectObject(patch)[0]!.uv!;
    };
    const north = hitUv(p.phiStart + p.phiLength * 0.5, p.thetaStart + p.thetaLength * 0.25);
    expect(north.y).toBeGreaterThan(0.5); // north quarter → upper half
    const west = hitUv(p.phiStart + p.phiLength * 0.25, p.thetaStart + p.thetaLength * 0.5);
    expect(west.x).toBeLessThan(0.5); // west quarter → left half
  });

  it("deeper-drill (1c): a patch hit.uv → patchUvToWorld recovers the aimed world point", () => {
    // The end-to-end pin for deeper drilling: a ray at a known interior param point
    // of the patch → hit.uv → patchUvToWorld must land on that point's world coords.
    // Catches a uv-convention mismatch between three.js and patchUvToWorld.
    const sec = { level: 2, sx: 2, sy: 1 };
    const p = sectorPatchParams(sec, W, H);
    const patch = new Mesh(
      new SphereGeometry(1.001, p.segW, p.segH, p.phiStart, p.phiLength, p.thetaStart, p.thetaLength),
    );
    patch.updateMatrixWorld(true);
    const u = 0.7;
    const vParam = 0.3; // fraction along theta (north→south)
    const phi = p.phiStart + u * p.phiLength;
    const theta = p.thetaStart + vParam * p.thetaLength;
    const d: [number, number, number] = [
      -Math.cos(phi) * Math.sin(theta),
      Math.cos(theta),
      Math.sin(phi) * Math.sin(theta),
    ];
    const ray = new Raycaster();
    ray.set(
      new Vector3(d[0] * 3, d[1] * 3, d[2] * 3),
      new Vector3(-d[0], -d[1], -d[2]).normalize(),
    );
    const hit = ray.intersectObject(patch)[0]!;
    const w = patchUvToWorld(hit.uv!.x, hit.uv!.y, sec, W, H);
    const r = sectorRect(sec, W, H);
    expect(w.x).toBeCloseTo(r.x0 + u * r.w, 0); // u → west→east
    expect(w.y).toBeCloseTo(r.y0 + vParam * r.h, 0); // vParam → north→south
  });
});

describe("panSubPoint", () => {
  const base: GlobeCamState = { subLon: 0.5, subLat: 0.1, altitude: 0.5, heading: 0, pitch: 0 };
  it("drag SIGN follows the grab-the-map convention (not just 'it moved')", () => {
    // Drag right → reveal west → subLon DECREASES; drag left → subLon increases.
    expect(panSubPoint(base, 100, 0, 0.002).subLon).toBeLessThan(base.subLon);
    expect(panSubPoint(base, -100, 0, 0.002).subLon).toBeGreaterThan(base.subLon);
    // Drag down → reveal north → subLat INCREASES; drag up → subLat decreases.
    expect(panSubPoint(base, 0, 100, 0.002).subLat).toBeGreaterThan(base.subLat);
    expect(panSubPoint(base, 0, -100, 0.002).subLat).toBeLessThan(base.subLat);
  });
  it("travel scales with altitude and clamps subLat shy of the poles", () => {
    const lo = panSubPoint({ ...base, altitude: 0.2 }, 100, 0, 0.002);
    const hi = panSubPoint({ ...base, altitude: 1.0 }, 100, 0, 0.002);
    // Higher altitude → larger sub-point travel for the same drag.
    expect(Math.abs(hi.subLon - base.subLon)).toBeGreaterThan(Math.abs(lo.subLon - base.subLon));
    // A huge downward drag can't push the sub-point past the pole.
    expect(panSubPoint(base, 0, 1e6, 0.002).subLat).toBeLessThan(Math.PI / 2);
  });
});

// THE CONVENTION PIN. Aim the camera at a known sector center via the camera math,
// cast its center ray at the REAL three.js SphereGeometry, and require the hit's
// intrinsic uv → `uvToWorld` to land back on that sector center. This round-trips
// through the already-trusted drill path (raycaster hit.uv → uvToWorld), so a
// flipped lon/lat→unit-vector convention fails HERE, off-GPU — three.js raycasting
// is pure math (no WebGL/renderer), so it runs in Vitest, not just e2e.
describe("camera ↔ sphere convention (raycast round-trip pin)", () => {
  it("the camera's center ray hits the very sub-point it was aimed at", () => {
    const sphere = new Mesh(new SphereGeometry(1, 64, 48));
    sphere.updateMatrixWorld(true);
    const ray = new Raycaster();
    for (const center of [
      { x: W * 0.5, y: H * 0.5 }, // lon 0, lat 0 → +X
      { x: W * 0.25, y: H * 0.35 },
      { x: W * 0.7, y: H * 0.6 },
      { x: W * 0.9, y: H * 0.5 }, // near the antimeridian, still equatorial
    ]) {
      const { lon, lat } = worldToLonLat(center.x, center.y, W, H);
      const pose = globeCamPose({ subLon: lon, subLat: lat, altitude: 0.6, heading: 0, pitch: 0 });
      const cam = new PerspectiveCamera(42, 1, 0.1, 100);
      cam.position.set(pose.position[0], pose.position[1], pose.position[2]);
      cam.up.set(pose.up[0], pose.up[1], pose.up[2]);
      cam.lookAt(new Vector3(pose.target[0], pose.target[1], pose.target[2]));
      cam.updateMatrixWorld(true);
      ray.setFromCamera(new Vector2(0, 0), cam); // screen center
      const hit = ray.intersectObject(sphere)[0];
      expect(hit?.uv).toBeTruthy();
      const w = uvToWorld(hit!.uv!.x, hit!.uv!.y, W, H);
      expect(Math.abs(w.x - center.x)).toBeLessThan(W * 0.01);
      expect(Math.abs(w.y - center.y)).toBeLessThan(H * 0.01);
    }
  });
});

describe("lerpPose (1f entry-ease)", () => {
  const a: CamPose = { position: [0, 0, 3.6], target: [0, 0, 0], up: [0, 1, 0] };
  const b: CamPose = { position: [1.2, 0.4, 0.9], target: [0.8, 0.3, 0.5], up: [0.2, 0.9, 0.1] };

  it("returns the endpoints exactly at t=0 and t=1 (up normalized)", () => {
    const p0 = lerpPose(a, b, 0);
    near(p0.position[2], 3.6);
    near(p0.target[0], 0);
    near(p0.up[1], 1);
    const p1 = lerpPose(a, b, 1);
    near(p1.position[0], 1.2);
    near(p1.target[1], 0.3);
    // up is NORMALIZED at the endpoint, not b.up verbatim (b.up is non-unit here).
    near(length(p1.up), 1);
    near(p1.up[1] / p1.up[0], b.up[1] / b.up[0], 1e-4); // same direction as b.up
  });

  it("the midpoint lies strictly between the endpoints on every channel", () => {
    const m = lerpPose(a, b, 0.5);
    for (const k of [0, 1, 2] as const) {
      const lo = Math.min(a.position[k], b.position[k]);
      const hi = Math.max(a.position[k], b.position[k]);
      expect(m.position[k]).toBeGreaterThanOrEqual(lo);
      expect(m.position[k]).toBeLessThanOrEqual(hi);
      expect(m.target[k]).toBeGreaterThanOrEqual(Math.min(a.target[k], b.target[k]));
      expect(m.target[k]).toBeLessThanOrEqual(Math.max(a.target[k], b.target[k]));
    }
    // Strictly between where the endpoints differ (the glide is real, not a snap:
    // a t-clamped-to-1 implementation would sit AT b and fail these).
    expect(m.position[0]).toBeGreaterThan(0.1);
    expect(m.position[0]).toBeLessThan(1.1);
    near(length(m.up), 1); // up stays unit mid-glide
  });

  it("clamps t outside [0,1] (no overshoot from a late frame)", () => {
    const over = lerpPose(a, b, 1.7);
    near(over.position[0], 1.2);
    const under = lerpPose(a, b, -0.3);
    near(under.position[2], 3.6);
  });
});

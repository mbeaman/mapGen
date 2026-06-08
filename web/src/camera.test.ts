import { Mesh, PerspectiveCamera, Raycaster, SphereGeometry, Vector2, Vector3 } from "three";
import { describe, expect, it } from "vitest";
import { type GlobeCamState, globeCamPose, lonLatToUnit, panSubPoint, worldToLonLat } from "./camera";
import { latLonToWorld, uvToWorld } from "./sector";

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

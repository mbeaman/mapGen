import {
  BufferAttribute,
  BufferGeometry,
  Mesh,
  PerspectiveCamera,
  Raycaster,
  SphereGeometry,
  Vector2,
  Vector3,
} from "three";
import { describe, expect, it } from "vitest";
import {
  globeCamPose,
  lonLatToUnit,
  MAX_PITCH,
  sectorPatchParams,
  unitToUv,
  type Vec3,
  worldToLonLat,
} from "./camera";
import { displacedPatchArrays, GH, GW, VERT_EXAG } from "./relief";
import { uvToWorld } from "./sector";

const W = 2048;
const H = 1024; // globe-mode dims (2:1 equirect)

// Angle (degrees) between two directions.
const angleDeg = (a: Vec3, b: Vec3): number => {
  const al = Math.hypot(a[0], a[1], a[2]);
  const bl = Math.hypot(b[0], b[1], b[2]);
  const d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / (al * bl);
  return (Math.acos(Math.max(-1, Math.min(1, d))) * 180) / Math.PI;
};

// A uniformly-RAISED patch over a sector (every cell at raw height `h`), so the
// displaced surface is an analytic R_disp shell — the parallax discriminator
// needs a closed-form ground truth (`ray ∩ sphere(R_disp)`).
function raisedPatch(sec: { level: number; sx: number; sy: number }, rawHeight: number) {
  const params = sectorPatchParams(sec, W, H);
  const heights = new Float32Array(GW * GH).fill(rawHeight);
  const d = displacedPatchArrays(params, heights, GW, GH);
  const geo = new BufferGeometry();
  geo.setAttribute("position", new BufferAttribute(d.positions, 3));
  geo.setAttribute("uv", new BufferAttribute(d.uvs, 2));
  geo.setIndex(new BufferAttribute(d.index, 1));
  const mesh = new Mesh(geo);
  mesh.updateMatrixWorld(true);
  return { mesh, params };
}

// Analytic nearest intersection of a ray (origin o, unit dir u) with a sphere of
// radius R centred at the origin — the ground truth the patch raycast approximates.
function raySphere(o: Vector3, u: Vector3, R: number): Vector3 | null {
  const b = 2 * o.dot(u);
  const c = o.dot(o) - R * R;
  const disc = b * b - 4 * c;
  if (disc < 0) return null;
  const t = (-b - Math.sqrt(disc)) / 2;
  if (t < 0) return null;
  return o.clone().add(u.clone().multiplyScalar(t));
}

describe("pick under displacement (R3 patches-first raycast, CLAIMS PICK-1/PICK-2)", () => {
  // The discriminator pair: an OBLIQUE ray that strikes raised terrain. The
  // patches-first pick — normalize(patchHit.point) — recovers the ground point the
  // user sees to within tessellation error (≤0.1°). The OLD base-sphere pick
  // (raycast the smooth radius-1 sphere) mis-lands by the parallax of the radial
  // gap (≥1° at pitch 50° — the spike's ~1.4°). SAME ray, both meshes.
  const sec = { level: 3, sx: 4, sy: 4 };
  const params = sectorPatchParams(sec, W, H);
  const anchor = worldDirToLonLat(params.centerDir);
  const R_DISP = 1.001 + 1.4 * VERT_EXAG; // raw height 1.4 → the analytic shell radius

  it("displaced-mesh pick lands ≤0.1° on the ground point; base-sphere pick on the SAME ray breaches ≥1°", () => {
    const { mesh } = raisedPatch(sec, 1.4);
    const sphere = new Mesh(new SphereGeometry(1, 64, 48));
    sphere.updateMatrixWorld(true);

    const pose = globeCamPose({
      subLon: anchor.lon,
      subLat: anchor.lat,
      altitude: 0.3,
      heading: 0,
      pitch: MAX_PITCH,
    });
    const cam = new PerspectiveCamera(42, 1, 0.002, 100);
    cam.position.set(pose.position[0], pose.position[1], pose.position[2]);
    cam.up.set(pose.up[0], pose.up[1], pose.up[2]);
    cam.lookAt(new Vector3(pose.target[0], pose.target[1], pose.target[2]));
    cam.updateMatrixWorld(true);

    // Screen point biased toward the lower frustum (the bottom edge grazes the
    // raised terrain at the steepest incidence — where parallax is worst and the
    // base-sphere pick fails hardest).
    const ray = new Raycaster();
    ray.setFromCamera(new Vector2(0, -0.5), cam);

    const patchHit = ray.intersectObject(mesh)[0];
    const baseHit = ray.intersectObject(sphere)[0];
    expect(patchHit?.point).toBeTruthy();
    expect(baseHit?.point).toBeTruthy();

    // Ground truth: the analytic ray∩R_disp projected radially (what the user sees).
    const o = ray.ray.origin;
    const u = ray.ray.direction;
    const truth = raySphere(o, u, R_DISP)!;
    const truthDir: Vec3 = [truth.x, truth.y, truth.z];

    const patchDir: Vec3 = [patchHit.point.x, patchHit.point.y, patchHit.point.z];
    const baseDir: Vec3 = [baseHit.point.x, baseHit.point.y, baseHit.point.z];

    // PATCHES-FIRST: normalize(hit.point) recovers the ground direction (radial
    // displacement preserves direction); only the tessellation grid error remains.
    expect(angleDeg(patchDir, truthDir)).toBeLessThan(0.1);
    // BASE-SPHERE (the deferred-from-R2 defect): the same ray pierces the unit
    // sphere at a parallax-shifted direction — the breach the patch pick fixes.
    expect(angleDeg(baseDir, truthDir)).toBeGreaterThanOrEqual(1);

    // And the production conversion is sound: unitToUv(normalize(patchHit)) →
    // uvToWorld lands within ≤0.1° of the ground point (the patchPickCb contract).
    const pn = norm(patchDir);
    const { u: uu, v: vv } = unitToUv(pn);
    const wp = uvToWorld(uu, vv, W, H);
    const llPick = worldToLonLat(wp.x, wp.y, W, H);
    expect(angleDeg(lonLatToUnit(llPick.lon, llPick.lat), pn)).toBeLessThan(0.1);
  });
});

function norm(a: Vec3): Vec3 {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
}
function worldDirToLonLat(dir: Vec3): { lon: number; lat: number } {
  return { lat: Math.asin(Math.max(-1, Math.min(1, dir[1]))), lon: Math.atan2(-dir[2], dir[0]) };
}

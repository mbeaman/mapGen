import { describe, expect, it } from "vitest";
import { lonLatToUnit, type Vec3, worldToLonLat } from "./camera";
import { type CamState, desiredSectors, type LodConfig } from "./lod";
import { latLonToWorld, sectorAt, sectorRect } from "./sector";

const W = 2048;
const H = 1024; // globe-mode dims (2:1 equirect)
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

/// A pitch-0 camera directly over (lon, lat) at the given altitude.
function camOver(lon: number, lat: number, altitude: number): CamState {
  const u = lonLatToUnit(lon, lat);
  const r = 1 + altitude;
  return {
    posUnit: [u[0] * r, u[1] * r, u[2] * r],
    forward: [-u[0], -u[1], -u[2]],
    fovY: 0.7,
    aspect: 1,
    near: 0.1,
    far: 100,
  };
}
const cfg = (maxPatches: number, window: number): LodConfig => ({ maxPatches, window });

describe("desiredSectors", () => {
  it("includes the sub-point's sector and stays at the requested level", () => {
    const cam = camOver(0.6, -0.3, 0.4);
    const res = desiredSectors(cam, 3, W, H, cfg(24, 2));
    expect(res.length).toBeGreaterThan(0);
    expect(res.every((s) => s.level === 3)).toBe(true);
    const subW = latLonToWorld(-0.3, 0.6, W, H);
    const sub = sectorAt(subW.x, subW.y, 3, W, H);
    expect(res.some((s) => s.sx === sub.sx && s.sy === sub.sy)).toBe(true);
  });

  it("every returned sector's NEAREST point is above the horizon (drops the far hemisphere), and the set is non-empty", () => {
    const cam = camOver(1.0, 0.2, 0.25);
    const posLen = Math.hypot(...cam.posUnit);
    const camDir: Vec3 = [cam.posUnit[0] / posLen, cam.posUnit[1] / posLen, cam.posUnit[2] / posLen];
    const subW = latLonToWorld(0.2, 1.0, W, H);
    const res = desiredSectors(cam, 3, W, H, cfg(24, 3));
    // Non-vacuous: the cull must NOT empty the set at this moderate altitude (the
    // per-element check below is trivially true on []). A window-3 view over a
    // surface point keeps at least its inner 3×3 neighbourhood.
    expect(res.length).toBeGreaterThanOrEqual(9);
    for (const s of res) {
      // The point of s NEAREST the sub-point (matches the production cull), not its centre.
      const r = sectorRect(s, W, H);
      const nx = Math.max(r.x0, Math.min(r.x0 + r.w, subW.x));
      const ny = Math.max(r.y0, Math.min(r.y0 + r.h, subW.y));
      const { lon, lat } = worldToLonLat(nx, ny, W, H);
      expect(dot(lonLatToUnit(lon, lat), camDir)).toBeGreaterThanOrEqual(1 / posLen - 1e-9);
    }
    // A sector on the FAR side (antipode of the sub-point) is never returned.
    const farLon = 1.0 - Math.PI; // opposite longitude
    const farW = latLonToWorld(-0.2, farLon, W, H);
    const far = sectorAt(farW.x, farW.y, 3, W, H);
    expect(res.some((s) => s.sx === far.sx && s.sy === far.sy)).toBe(false);
  });

  it("keeps the sub-point's own sector at the lowest production altitude — no empty detail set (regression)", () => {
    // L3 is the mandatory first-drill level; the user can wheel-zoom to MIN_ALT (0.05)
    // WITHOUT drilling deeper. (0,0) lands on a 4-sector corner — the worst case for a
    // sector-CENTRE cull, which dropped the sub-point's OWN sector here and returned []
    // (the detail vanished exactly where the user was looking). Nearest-point culling
    // keeps it. RED on the old centre-cull, GREEN now.
    const cam = camOver(0, 0, 0.05); // production MIN_ALT
    const res = desiredSectors(cam, 3, W, H, cfg(24, 1)); // window 1 = the TIGHTEST window (worst case for emptiness)
    expect(res.length).toBeGreaterThan(0);
    const subW = latLonToWorld(0, 0, W, H);
    const sub = sectorAt(subW.x, subW.y, 3, W, H);
    expect(res.some((s) => s.sx === sub.sx && s.sy === sub.sy)).toBe(true);
  });

  it("is bounded by maxPatches (the clamp binds) AND keeps the sectors NEAREST the sub-point", () => {
    const cam = camOver(0, 0, 2.0); // high altitude → big visible cap
    const res = desiredSectors(cam, 5, W, H, cfg(7, 6)); // big window, small cap
    expect(res.length).toBe(7);
    // The cap keeps the CLOSEST sectors — the sub-point's own sector must survive.
    // Inverting the rank sort (keep the FARTHEST/horizon-edge sectors) drops it → red.
    const subW = latLonToWorld(0, 0, W, H);
    const sub = sectorAt(subW.x, subW.y, 5, W, H);
    expect(res.some((s) => s.sx === sub.sx && s.sy === sub.sy)).toBe(true);
  });

  it("WRAPS longitude across the antimeridian (periodic cylinder)", () => {
    // Just east of the antimeridian (lon ≈ -π) → sub sx ≈ 0. The planet is a
    // longitude-cylinder (Phase 5 periodic flip), so the window MUST stitch across
    // the seam: the sectors immediately west of the sub-point sit at the EAST edge
    // (sx near span-1) and are geometrically adjacent — they must appear, not be
    // clamped away onto a faded base. (Clamping instead of wrapping → none ≥ 14.)
    const cam = camOver(-Math.PI + 0.05, 0, 0.3);
    const res = desiredSectors(cam, 4, W, H, cfg(24, 2)); // span = 16
    expect(res.length).toBeGreaterThan(0);
    expect(res.some((s) => s.sx >= 14)).toBe(true); // wrapped to the east edge
    expect(res.some((s) => s.sx <= 1)).toBe(true); // and the sub-point's own side
  });

  it("CLAMPS latitude at the poles (no wrap top↔bottom)", () => {
    // Near the north pole → sub sy ≈ 0. Latitude is NOT periodic, so the window
    // must clamp at the pole, never wrap to the south edge (sy near span-1).
    const cam = camOver(0, Math.PI / 2 - 0.05, 0.3);
    const res = desiredSectors(cam, 4, W, H, cfg(24, 2)); // span = 16
    expect(res.length).toBeGreaterThan(0);
    expect(res.every((s) => s.sy <= 3)).toBe(true); // clamped near 0, never 15
  });

  it("retains a near-but-clearly-visible neighbour (pins the cull is not TOO strict)", () => {
    // camOver(1.0,0.2,0.25): sub-sector {3,5,3}; the west neighbour {3,4,3} has
    // centerDir·camDir ≈ 0.83, well above the true horizon ≈ 0.80 — it MUST survive
    // (an over-strict cull drops it → this goes red).
    const cam = camOver(1.0, 0.2, 0.25);
    const res = desiredSectors(cam, 3, W, H, cfg(24, 3));
    expect(res.some((s) => s.sx === 4 && s.sy === 3)).toBe(true);
  });

  it("a malformed cap fails CLOSED (never unbounded)", () => {
    const cam = camOver(0, 0, 2.0); // ~156-sector horizon pool at level 5, window 6
    for (const bad of [-1, Number.NaN, Number.POSITIVE_INFINITY]) {
      const res = desiredSectors(cam, 5, W, H, cfg(bad, 6));
      expect(res.length).toBeLessThanOrEqual(30); // bounded — not the 156-sector pool
    }
  });

  it("POLAR CAP: rows centred beyond ±75° never stream (wedge-streak band)", () => {
    // Camera near the north pole: without the cap the window would include the
    // top rows, whose equirect tiles render as converging wedges on the sphere.
    const cam = camOver(0, Math.PI / 2 - 0.05, 0.3);
    const res = desiredSectors(cam, 4, W, H, cfg(24, 2)); // span = 16, rows 11.25°
    expect(res.length).toBeGreaterThan(0);
    // Row centre lat for sy: 90° − (sy+0.5)/16·180°. sy=0 → 84.4°, sy=1 → 73.1°.
    // The cap (75°) excludes sy=0 only; dropping the cap re-admits it → red.
    expect(res.every((s) => s.sy >= 1)).toBe(true);
  });
});

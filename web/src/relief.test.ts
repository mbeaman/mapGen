// Relief displaced-geometry contracts (addendum §6). Pure math, off-GPU.
import { describe, expect, it } from "vitest";
import { sectorPatchParams } from "./camera";
import { displacedPatchArrays, GH, GW, patchGpuBytes, VERT_EXAG } from "./relief";

const params = (sx: number, sy = 3) =>
  sectorPatchParams({ level: 3, sx, sy }, 2048, 1024);

const flat = (gw: number, gh: number) => new Float32Array(gw * gh);

describe("displacedPatchArrays", () => {
  it("sea-flush: an all-zero field builds every surface vertex at exactly baseRadius", () => {
    const { positions, reliefMax } = displacedPatchArrays(params(1), flat(GW, GH), GW, GH);
    expect(reliefMax).toBe(0);
    for (let v = 0; v < GW * GH; v++) {
      const r = Math.hypot(positions[v * 3], positions[v * 3 + 1], positions[v * 3 + 2]);
      expect(r).toBeCloseTo(1.001, 6);
    }
  });

  it("displacement is real and reliefMax comes from the BUILT radii", () => {
    const h = flat(GW, GH);
    const peak = 0.8;
    h[32 * GW + 64] = peak; // one interior peak
    const { positions, reliefMax } = displacedPatchArrays(params(1), h, GW, GH);
    // reliefMax is the built radial excess — peak × VERT_EXAG, not the raw field.
    expect(reliefMax).toBeCloseTo(peak * VERT_EXAG, 8);
    const v = 32 * GW + 64;
    const r = Math.hypot(positions[v * 3], positions[v * 3 + 1], positions[v * 3 + 2]);
    expect(r).toBeCloseTo(1.001 + peak * VERT_EXAG, 6);
    // A different exaggeration changes the output (the knob is live).
    const { reliefMax: doubled } = displacedPatchArrays(params(1), h, GW, GH, VERT_EXAG * 2);
    expect(doubled).toBeCloseTo(peak * VERT_EXAG * 2, 8);
  });

  it("uv layout reproduces SphereGeometry's (u, 1-v): row 0 = north, col 0 = west", () => {
    const { uvs } = displacedPatchArrays(params(1), flat(GW, GH), GW, GH);
    expect([uvs[0], uvs[1]]).toEqual([0, 1]); // NW corner
    const last = GW * GH - 1;
    expect([uvs[last * 2], uvs[last * 2 + 1]]).toEqual([1, 0]); // SE corner
    const midRow = Math.floor(GH / 2);
    const mid = midRow * GW + (GW - 1) / 2;
    expect(uvs[mid * 2]).toBeCloseTo(0.5, 10);
  });

  it("adjacent sectors' shared edge builds the same wall: bit-equal heights → matching edge vertices", () => {
    // The Rust seam contract delivers bit-identical edge heights; this pins the
    // JS half — same heights on the shared column ⇒ the two patches' edge
    // vertices coincide (within one f32 rounding of the identical f64 angles).
    const hA = new Float32Array(GW * GH);
    const hB = new Float32Array(GW * GH);
    for (let r = 0; r < GH; r++) {
      const edge = 0.1 + 0.5 * Math.abs(Math.sin(r)); // arbitrary shared column
      hA[r * GW + (GW - 1)] = edge; // A's east column
      hB[r * GW] = edge; // B's west column
    }
    const a = displacedPatchArrays(params(1), hA, GW, GH);
    const b = displacedPatchArrays(params(2), hB, GW, GH);
    for (let r = 0; r < GH; r++) {
      const va = r * GW + (GW - 1);
      const vb = r * GW;
      for (let k = 0; k < 3; k++) {
        const pa = a.positions[va * 3 + k];
        const pb = b.positions[vb * 3 + k];
        expect(Math.abs(pa - pb)).toBeLessThanOrEqual(Math.abs(pa) * 1e-6);
      }
    }
  });

  it("the skirt ring drops below base and reuses the edge uv", () => {
    const { positions, uvs, index } = displacedPatchArrays(params(1), flat(GW, GH), GW, GH);
    const surf = GW * GH;
    const perim = 2 * GW + 2 * (GH - 2);
    expect(positions.length).toBe((surf + perim) * 3);
    // First skirt vertex duplicates the NW corner's uv but sits below base.
    expect([uvs[surf * 2], uvs[surf * 2 + 1]]).toEqual([0, 1]);
    const r = Math.hypot(positions[surf * 3], positions[surf * 3 + 1], positions[surf * 3 + 2]);
    expect(r).toBeCloseTo(1.001 - 0.004, 6);
    // Index references stay in range (the strip is well-formed).
    for (const i of index) expect(i).toBeLessThan(surf + perim);
  });

  it("patchGpuBytes accounts texture + geometry honestly", () => {
    const bytes = patchGpuBytes(1024, 512, GW, GH);
    const tex = 1024 * 512 * 4;
    expect(bytes).toBeGreaterThan(tex); // strictly more than texture-only
    expect(bytes - tex).toBeLessThan(600_000); // geometry ≈ 0.38 MB, not runaway
  });
});

// Relief displaced-patch geometry (the Relief addendum §6) — PURE, no three.js
// imports, so the vertex math is unit-testable off-GPU (the same discipline as
// lod.ts / camera.ts).
//
// The worker ships a seam-banded gh×gw heightfield per tile (row 0 = NORTH,
// col 0 = WEST, RAW sea-clamped heights); this module turns it into the typed
// arrays a BufferGeometry consumes. Adjacent tiles' heightfields are
// BIT-IDENTICAL along shared edges (the Rust seam contract), and the vertex
// math below is a pure function of (params, heights), so adjacent displaced
// patches meet exactly — no cracks by construction.

import type { PatchParams } from "./camera";

/** Relief grid dims — FIXED constants, decoupled from PATCH_ANGULAR_RES /
 * segW/segH (tessellation no longer controls relief density; geometry is built
 * FROM the grid). 129×65 matches the globe tiles' universal 2:1 aspect at ~the
 * refined mesh's Nyquist (~4k cells ≈ 64×64), at the upper end of the spike's
 * measured envelope (IDW 4 ms, geometry build ~3.7 ms, payload 33 KB). */
export const GW = 129;
export const GH = 65;

/** Radial exaggeration: peak land (raw elevation ~0.86 on seed-42) displaces
 * ~2% of the sphere radius — the spike's screenshot-verified value. Heights
 * are per-world relative (RAW, not normalized) and are NOT bounded to ≤1.0 —
 * erosion + fill_depressions (Rust) can push a peak past 1.0 (measured max
 * ~1.05). The camera-clearance ceiling must therefore track the ACTUAL built
 * displacement (globe.ts reliefMaxSeen), not a constant; see tuning_log.md. */
export const VERT_EXAG = 0.023;

/** The displaced patch's sea-level shell radius (flush with the legacy 1.001
 * globe). Single source of truth: the camera-clearance ceiling in globe.ts is
 * `PATCH_BASE_RADIUS + reliefMaxSeen`, and `displacedPatchArrays` builds from it. */
export const PATCH_BASE_RADIUS = 1.001;

/** Skirt drop below the patch base — closes the patch-to-base gap at the
 * streamed-set edge / polar cap into a short textured wall. */
export const SKIRT_DEPTH = 0.004;

export interface DisplacedPatch {
  positions: Float32Array;
  uvs: Float32Array;
  index: Uint32Array;
  /** Max radial displacement of the BUILT vertices above baseRadius — the
   * e2e witness (`data-relief-max`); computed from the output so it cannot be
   * stubbed from the input field. */
  reliefMax: number;
}

/**
 * Build displaced-patch arrays from a tile heightfield.
 *
 * Vertex (row r, col c): the EXACT three.js SphereGeometry parameterization
 * the standing camera.test.ts orientation pins assert —
 * `phi = phiStart + (c/(gw-1))·phiLength`, `theta = thetaStart + (r/(gh-1))·thetaLength`,
 * `dir = (-cosφ·sinθ, cosθ, sinφ·sinθ)`, `pos = dir · (baseRadius + max(0,h)·vertExag)`.
 * Sea (h = 0) sits at exactly `baseRadius` — flush with the legacy shell.
 *
 * UV layout (LOAD-BEARING): `uv = (c/(gw-1), 1 − r/(gh-1))`, row 0 = thetaStart
 * = NORTH — reproduces SphereGeometry's (u, 1−v) so `patchUvToWorld`, the
 * orientation raycast pins, and the 1c hit-uv round-trip all survive the
 * geometry-source swap unmodified.
 *
 * NO normals attribute: the material is unlit (shade is baked in the texture);
 * no consumer ⇒ not built. A perimeter SKIRT ring drops to
 * `baseRadius − skirtDepth` with the edge uv (texture stretches down):
 * interior skirts are invisible by construction (neighbours' bit-identical
 * edges cover them under depth testing); at the streamed-set edge and the
 * polar cap they close the gap to the base sphere.
 */
export function displacedPatchArrays(
  params: PatchParams,
  heights: Float32Array,
  gw: number,
  gh: number,
  vertExag: number = VERT_EXAG,
  baseRadius = PATCH_BASE_RADIUS,
  skirtDepth = SKIRT_DEPTH,
): DisplacedPatch {
  if (heights.length !== gw * gh) throw new Error("heights len != gw*gh");
  const surfVerts = gw * gh;
  const perim = 2 * gw + 2 * (gh - 2); // perimeter vertex count (no corner dupes)
  const positions = new Float32Array((surfVerts + perim) * 3);
  const uvs = new Float32Array((surfVerts + perim) * 2);
  let reliefMax = 0;
  const writeVert = (slot: number, r: number, c: number, radius: number) => {
    const phi = params.phiStart + (c / (gw - 1)) * params.phiLength;
    const theta = params.thetaStart + (r / (gh - 1)) * params.thetaLength;
    const sinT = Math.sin(theta);
    positions[slot * 3] = -Math.cos(phi) * sinT * radius;
    positions[slot * 3 + 1] = Math.cos(theta) * radius;
    positions[slot * 3 + 2] = Math.sin(phi) * sinT * radius;
    uvs[slot * 2] = c / (gw - 1);
    uvs[slot * 2 + 1] = 1 - r / (gh - 1);
  };
  for (let r = 0; r < gh; r++) {
    for (let c = 0; c < gw; c++) {
      const h = Math.max(0, heights[r * gw + c]); // arrives sea-clamped; defensive
      const radius = baseRadius + h * vertExag;
      if (radius - baseRadius > reliefMax) reliefMax = radius - baseRadius;
      writeVert(r * gw + c, r, c, radius);
    }
  }
  // Surface triangles: two per grid cell.
  const surfTris = (gw - 1) * (gh - 1) * 2;
  const index = new Uint32Array((surfTris + perim * 2) * 3);
  let k = 0;
  for (let r = 0; r < gh - 1; r++) {
    for (let c = 0; c < gw - 1; c++) {
      const a = r * gw + c;
      const b = a + 1;
      const d = a + gw;
      const e = d + 1;
      index[k++] = a;
      index[k++] = d;
      index[k++] = b;
      index[k++] = b;
      index[k++] = d;
      index[k++] = e;
    }
  }
  // Skirt: walk the perimeter (N row → E col → S row reversed → W col
  // reversed), duplicating each vertex at baseRadius − skirtDepth and stitching
  // a quad strip from surface-edge to skirt ring.
  const ring: Array<[number, number]> = [];
  for (let c = 0; c < gw; c++) ring.push([0, c]);
  for (let r = 1; r < gh - 1; r++) ring.push([r, gw - 1]);
  for (let c = gw - 1; c >= 0; c--) ring.push([gh - 1, c]);
  for (let r = gh - 2; r >= 1; r--) ring.push([r, 0]);
  const skirtBase = surfVerts;
  for (let i = 0; i < ring.length; i++) {
    const [r, c] = ring[i];
    writeVert(skirtBase + i, r, c, baseRadius - skirtDepth);
  }
  for (let i = 0; i < ring.length; i++) {
    const [r, c] = ring[i];
    const [r2, c2] = ring[(i + 1) % ring.length];
    const surfA = r * gw + c;
    const surfB = r2 * gw + c2;
    const skirtA = skirtBase + i;
    const skirtB = skirtBase + ((i + 1) % ring.length);
    index[k++] = surfA;
    index[k++] = skirtA;
    index[k++] = surfB;
    index[k++] = surfB;
    index[k++] = skirtA;
    index[k++] = skirtB;
  }
  return { positions, uvs, index, reliefMax };
}

/** Honest per-patch GPU byte estimate: the RGBA texture + the displaced
 * geometry's positions/uvs/index (the cache cap is a GPU budget, not a
 * texture-only budget). */
export function patchGpuBytes(w: number, h: number, gw: number, gh: number): number {
  const verts = gw * gh + 2 * gw + 2 * (gh - 2);
  const tris = (gw - 1) * (gh - 1) * 2 + (2 * gw + 2 * (gh - 2)) * 2;
  return w * h * 4 + verts * (3 + 2) * 4 + tris * 3 * 4;
}

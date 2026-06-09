/// Continuous-LOD streaming — the PURE view→sector selector (increment ST-1). No
/// three.js crosses this boundary (it consumes a plain `CamState`, the kind
/// `globe.ts getCameraState` packs), so the boundedness + horizon-cull logic that
/// keeps the streaming engine from becoming an unbounded tile pyramid is
/// unit-testable off-GPU. See docs/design/globe-ground-3d-navigation.md
/// (continuous-LOD streaming addendum).
///
/// SCOPE (ST-1): visible sectors at a FIXED level (the drilled level; the
/// altitude→level snap is ST-2). The camera is pitch-0 (top-down) today, so an
/// N×N window around the sub-point + horizon cull + clamp-to-K is correct and
/// rigorously bounded — the frustum-corner footprint + frustum cull are the
/// oblique-camera upgrade (deferred with pitch). Longitude AND latitude are
/// CLAMPED at the grid edge (no wrap → the antimeridian stays on the faded base).

import { lonLatToUnit, unitToLonLat, type Vec3, worldToLonLat } from "./camera";
import { latLonToWorld, type Sector, sectorAt, sectorRect } from "./sector";

/// Camera state for the selector, in the unit-sphere frame (pivot identity). ST-1
/// reads ONLY `posUnit` (the sub-point + horizon + radial ranking). `forward`,
/// `vpMatrix`, and the `fov*` fields are carried for the oblique frustum-cull
/// upgrade (deferred with pitch) — not consumed yet.
export interface CamState {
  posUnit: Vec3;
  forward: Vec3;
  vpMatrix?: number[];
  fovY: number;
  aspect: number;
  near: number;
  far: number;
}

export interface LodConfig {
  /** hard cap on the live/desired patch count (MAX_LIVE_PATCHES). */
  maxPatches: number;
  /** half-width of the sector window around the sub-point (N → (2N+1)² candidates). */
  window: number;
}

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const length = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);

/// The unit vector to the point of `sec` NEAREST the sub-point (the sub-point's
/// world coords clamped into the sector's world rect). Used for BOTH the horizon
/// cull and the closeness rank. Culling on the nearest point — not the sector
/// CENTRE — is what makes the cull correct at low altitude: the sub-point's OWN
/// sector clamps to the sub-point itself, whose direction IS `camDir`, so its dot
/// is 1 ≥ horizon and it can never be culled. (Centre-culling dropped the
/// sub-point's own coarse sector once the camera dropped below ~0.2 altitude — an
/// empty detail set exactly where the user was looking.)
function sectorNearestDir(
  sec: Sector,
  subX: number,
  subY: number,
  worldW: number,
  worldH: number,
): Vec3 {
  const r = sectorRect(sec, worldW, worldH);
  const nx = Math.max(r.x0, Math.min(r.x0 + r.w, subX));
  const ny = Math.max(r.y0, Math.min(r.y0 + r.h, subY));
  const { lon, lat } = worldToLonLat(nx, ny, worldW, worldH);
  return lonLatToUnit(lon, lat);
}

/// Visible sectors at `level` for the camera. Bounded by construction:
/// (2·window+1)² candidates → horizon cull → rank by closeness to the sub-point →
/// truncate to `maxPatches`, so `result.length ≤ maxPatches` always.
export function desiredSectors(
  cam: CamState,
  level: number,
  worldW: number,
  worldH: number,
  cfg: LodConfig,
): Sector[] {
  const posLen = length(cam.posUnit) || 1;
  // The camera (radius 1+altitude) is above its sub-point; subDir = posUnit/|posUnit|.
  const camDir: Vec3 = [cam.posUnit[0] / posLen, cam.posUnit[1] / posLen, cam.posUnit[2] / posLen];
  const horizon = 1 / posLen; // dot(centerDir, camDir) ≥ this ⇒ above the tangent horizon
  const { lat, lon } = unitToLonLat(camDir);
  const sub = latLonToWorld(lat, lon, worldW, worldH);
  const subSec = sectorAt(sub.x, sub.y, level, worldW, worldH);

  const span = 2 ** level;
  const N = cfg.window;
  const seen = new Set<string>();
  const ranked: { sec: Sector; rank: number }[] = [];
  for (let dy = -N; dy <= N; dy++) {
    for (let dx = -N; dx <= N; dx++) {
      const sx = Math.max(0, Math.min(span - 1, subSec.sx + dx)); // CLAMP longitude
      const sy = Math.max(0, Math.min(span - 1, subSec.sy + dy)); // CLAMP latitude
      const key = `${sx}:${sy}`;
      if (seen.has(key)) continue; // edge clamping creates duplicates
      seen.add(key);
      const sec: Sector = { level, sx, sy };
      const d = dot(sectorNearestDir(sec, sub.x, sub.y, worldW, worldH), camDir);
      if (d < horizon) continue; // nearest point behind the horizon — far hemisphere
      ranked.push({ sec, rank: d }); // higher dot ⇒ nearer the sub-point
    }
  }
  // Nearest first (sx/sy tiebreak → deterministic). The consumer LOADS in this order,
  // so the worker rasterizes the sectors UNDER the camera before the prefetch ring —
  // breadth-first from the sub-point outward, the "near sections render first" the
  // prefetch ring (ST-2) needs. The cache (markSeen/reconcile) is order-insensitive.
  ranked.sort((a, b) => b.rank - a.rank || a.sec.sx - b.sec.sx || a.sec.sy - b.sec.sy);
  // Clamp the cap defensively: a non-finite / negative maxPatches must NOT fail
  // open (slice(0, Infinity/-1) would un-bound or mis-truncate the firewall).
  const cap = Math.max(0, Math.floor(Number.isFinite(cfg.maxPatches) ? cfg.maxPatches : 0));
  return ranked.slice(0, cap).map((o) => o.sec);
}

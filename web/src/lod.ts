/// Continuous-LOD streaming — the PURE view→sector selector (increment ST-1). No
/// three.js crosses this boundary (it consumes a plain `CamState`, the kind
/// `globe.ts getCameraState` packs), so the boundedness + horizon-cull logic that
/// keeps the streaming engine from becoming an unbounded tile pyramid is
/// unit-testable off-GPU. See docs/design/globe-ground-3d-navigation.md
/// (continuous-LOD streaming addendum).
///
/// SCOPE (ST-1): visible sectors at a FIXED level (the drilled level; the
/// altitude→level snap is ST-2). The camera can now TILT (R3), so the window
/// centers on the LOOK ANCHOR (lookDir) rather than the nadir sub-point, while the
/// horizon cull keeps posUnit (geometric truth). The full frustum-corner footprint
/// selector stays a registered follow-up; residual limb coarseness at max pitch is
/// a named, screenshot-measured gap. Longitude WRAPS at the antimeridian (the
/// periodic planet is a longitude-cylinder, so the window stitches across the
/// seam — Phase 6); latitude stays CLAMPED at the poles.

import { lonLatToUnit, norm, unitToLonLat, type Vec3, worldToLonLat } from "./camera";
import { latLonToWorld, type Sector, sectorAt, sectorRect } from "./sector";

/// Camera state for the selector, in the unit-sphere frame (pivot identity). The
/// window CENTERS on `lookDir` when present (the look anchor under an oblique
/// camera), else on `posUnit` (the nadir sub-point); the horizon cull ALWAYS uses
/// `posUnit` (geometric truth). `forward`, `vpMatrix`, and the `fov*` fields are
/// carried for the full frustum-cull upgrade (a registered follow-up) — not
/// consumed yet.
export interface CamState {
  posUnit: Vec3;
  forward: Vec3;
  /// Unit direction the camera LOOKS at (the flight sub-point). R3: an oblique
  /// (pitched) camera's nadir sits ~26° behind its look anchor, so centering the
  /// window on `posUnit` wastes the budget behind the camera. Absent (overview /
  /// pitch-0 nadir) ⇒ the window centers on `posUnit`, byte-identical to before.
  lookDir?: Vec3;
  vpMatrix?: number[];
  fovY: number;
  aspect: number;
  near: number;
  far: number;
}

/// Sector rows whose centre |latitude| exceeds this never stream a patch — the
/// pole band the base texture already fades to ocean (~|lat| > 75°). Equirect
/// tiles mapped onto the converging polar sphere render as wedge-streaks; the
/// faded base is the honest presentation there.
export const POLAR_CAP_LAT = (75 * Math.PI) / 180;

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
  // Minimum-image nearest x: longitude wraps, so a sector across the seam may be
  // reached more cheaply by going the other way around. Clamp the sub-point AND
  // its ±worldW images into the sector's x-span, keep whichever lands closest. nx
  // stays within [r.x0, r.x0+r.w] ⊆ [0, worldW], so worldToLonLat is in range.
  let nx = Math.max(r.x0, Math.min(r.x0 + r.w, subX));
  let bestErr = Math.abs(subX - nx);
  for (const img of [subX - worldW, subX + worldW]) {
    const c = Math.max(r.x0, Math.min(r.x0 + r.w, img));
    const err = Math.abs(img - c);
    if (err < bestErr) {
      bestErr = err;
      nx = c;
    }
  }
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
  const horizon = 1 / posLen; // dot(nearestDir, camDir) ≥ this ⇒ above the tangent horizon (posUnit truth)
  // The window CENTERS on the look anchor (where the camera points) so an oblique
  // camera details what it's looking at, not the nadir ground below it. Absent
  // lookDir (overview / pitch-0) the center IS camDir → byte-identical to before.
  const centerDir = cam.lookDir ? norm(cam.lookDir) : camDir;
  const { lat, lon } = unitToLonLat(centerDir);
  const sub = latLonToWorld(lat, lon, worldW, worldH);
  const subSec = sectorAt(sub.x, sub.y, level, worldW, worldH);

  const span = 2 ** level;
  const N = cfg.window;
  const seen = new Set<string>();
  const ranked: { sec: Sector; rank: number }[] = [];
  for (let dy = -N; dy <= N; dy++) {
    for (let dx = -N; dx <= N; dx++) {
      const sx = (((subSec.sx + dx) % span) + span) % span; // WRAP longitude (periodic)
      const sy = Math.max(0, Math.min(span - 1, subSec.sy + dy)); // CLAMP latitude (poles)
      // POLAR CAP: skip rows whose centre latitude is beyond ±75° — the same band
      // the base texture's pole fade covers. Equirect tiles there render as
      // converging wedge-streaks on the sphere (texels compress to a point), which
      // reads as broken; the faded base showing through reads as intended low-fi
      // polar ocean. (screenshot-confirmed on a polar drill.)
      const rowCenterLat = Math.abs(Math.PI / 2 - ((sy + 0.5) / span) * Math.PI);
      if (rowCenterLat > POLAR_CAP_LAT) continue;
      const key = `${sx}:${sy}`;
      if (seen.has(key)) continue; // pole clamping (and a window ≥ span) creates duplicates
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

/// Velocity-predictive prefetch ring (Phase B): the sectors one tick AHEAD of the
/// camera — constant-velocity extrapolation `pos + (pos - prevPos)` = `2·pos −
/// prevPos` — that the in-view window omits, so the next sectors are warm on
/// arrival. EMPTY when stationary (prevPos == pos ⇒ predicted == pos ⇒ the ahead
/// window equals the in-view window ⇒ everything is filtered). Calls `desiredSectors`
/// VERBATIM at the predicted sub-point, inheriting the horizon-cull, polar-cap, and
/// antimeridian wrap, then drops anything already in `inView`. Pure → unit-testable
/// off-GPU (the production extrapolation was inline + untestable before).
export function predictedAhead(
  cam: CamState,
  prevPosUnit: Vec3,
  inView: Sector[],
  level: number,
  worldW: number,
  worldH: number,
  cfg: LodConfig,
): Sector[] {
  const predicted: Vec3 = [
    2 * cam.posUnit[0] - prevPosUnit[0],
    2 * cam.posUnit[1] - prevPosUnit[1],
    2 * cam.posUnit[2] - prevPosUnit[2],
  ];
  // |predicted| is NOT re-normalized to the camera radius — deliberately. For two
  // same-altitude samples θ apart, |2·pos − prev| = r·√(5 − 4·cosθ) ≥ r, so a fast
  // pan hands desiredSectors a slightly inflated radius → a slightly WIDER horizon
  // cull. That is a self-correcting superset (faster ⇒ look further; the extras sit
  // AFTER in-view in nearest-first order and are re-evaluated next pump). Re-scaling
  // to the real radius was tried and REVERTED: when the radius more than halves
  // between samples (a fast zoom-in), 2·pos − prev flips sign, and rescuing that
  // backward vector to a valid radius turns a self-culling overshoot (measured: 0
  // sectors) into a confident ANTIPODAL camera (measured: 16 garbage prefetches).
  const have = new Set(inView.map((s) => `${s.level}:${s.sx}:${s.sy}`));
  // Prefetch centers on the EXTRAPOLATED posUnit and DROPS lookDir (R3): the ring
  // leads the camera's motion, an idle-priority guess — using a stale look anchor
  // would aim the ring where the camera WAS pointing, not where it's heading.
  return desiredSectors(
    { ...cam, posUnit: predicted, lookDir: undefined },
    level,
    worldW,
    worldH,
    cfg,
  ).filter((s) => !have.has(`${s.level}:${s.sx}:${s.sy}`));
}

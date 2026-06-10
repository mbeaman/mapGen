/// Globe-flight camera math (increment 1a, free-fly variant). PURE — no three.js
/// crosses this boundary (the streaming LOD selector consumes the same kind of
/// plain camera state, and the test pins this against the real `SphereGeometry`
/// via a raycast). The camera frames a surface ANCHOR (the sub-point) in its
/// local east–north–up frame, so the world-+Y polar-clamp singularity that an
/// origin-targeted OrbitControls suffers (orbiting straight through the planet
/// core for an off-pole region — the reported bug) never arises. Only the
/// geographic poles degenerate, and `subLat` is clamped shy of them.
///
/// Conventions match `sector.ts`: equirectangular world (x→lon east+, y→lat
/// north+, see `latLonToWorld`), and the three.js `SphereGeometry` unit-sphere
/// parameterization — so a surface point's intrinsic `uv` → `uvToWorld`
/// round-trips back to where the camera aimed. That round-trip is asserted in
/// `camera.test.ts` against a real raycast, NOT hand-trusted, because a flipped
/// sign here would silently fly the camera to the mirror-image region.

import { type Sector, sectorRect } from "./sector";

export type Vec3 = [number, number, number];

export interface GlobeCamState {
  /** radians, east +, in [-π, π]. */
  subLon: number;
  /** radians, north +, clamped to ±LAT_MAX (the ENU frame degenerates at a pole). */
  subLat: number;
  /** height above the unit sphere surface (radius 1); > 0 keeps the camera outside. */
  altitude: number;
  /** radians; 0 = north is screen-up. */
  heading: number;
  /** radians; 0 = straight-down (nadir), → π/2 looks at the horizon. UNWIRED in 1a
   *  (pan+zoom only) — the math carries it for the relief increment + oblique view. */
  pitch: number;
}

/** Clamp `subLat` shy of the poles, where the east/north tangent frame is undefined. */
export const LAT_MAX = Math.PI / 2 - 1e-3;

const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const scale = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const norm = (a: Vec3): Vec3 => {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

/// Unit vector on the sphere for a geographic (lon, lat). North pole → +Y;
/// (lon 0, lat 0) → +X. Matches the three.js `SphereGeometry` uv parameterization
/// composed with `sector.ts` `uvToWorld` — pinned by the raycast round-trip test.
export function lonLatToUnit(lon: number, lat: number): Vec3 {
  const cl = Math.cos(lat);
  return [cl * Math.cos(lon), Math.sin(lat), -cl * Math.sin(lon)];
}

/// (lon, lat) in radians for a unit vector on the sphere — the inverse of
/// `lonLatToUnit`. Used by the streaming LOD selector to find the camera's
/// sub-point. (lat = asin(y); lon = atan2(-z, x), matching lonLatToUnit's signs.)
export function unitToLonLat(u: Vec3): { lon: number; lat: number } {
  return {
    lat: Math.asin(Math.max(-1, Math.min(1, u[1]))),
    lon: Math.atan2(-u[2], u[0]),
  };
}

/// (lon, lat) in radians for an equirectangular world point — the inverse of
/// `sector.ts` `latLonToWorld`.
export function worldToLonLat(
  wx: number,
  wy: number,
  worldW: number,
  worldH: number,
): { lon: number; lat: number } {
  return {
    lon: (wx / worldW - 0.5) * 2 * Math.PI,
    lat: (0.5 - wy / worldH) * Math.PI,
  };
}

/// Camera position / look-at target / up for a flight state, in the unit-sphere
/// world frame (the globe's pivot stays identity, so this IS world space).
/// pitch 0 looks straight down at the anchor with `heading` rotating screen-up;
/// pitch → π/2 swings the camera back to look at the horizon.
export function globeCamPose(s: GlobeCamState): { position: Vec3; target: Vec3; up: Vec3 } {
  const P = lonLatToUnit(s.subLon, s.subLat); // anchor ON the surface (|P| = 1)
  const east = norm(cross([0, 1, 0], P)); // tangent east at the anchor
  const north = norm(cross(P, east)); // tangent north (toward +Y pole)
  // Compass-forward: the horizontal direction the camera faces, from `heading`.
  const fwd = add(scale(north, Math.cos(s.heading)), scale(east, Math.sin(s.heading)));
  // The camera sits `altitude` from the anchor: straight up (radial) at pitch 0,
  // swinging back along -fwd toward the horizon as pitch grows.
  const off = add(scale(P, Math.cos(s.pitch) * s.altitude), scale(fwd, -Math.sin(s.pitch) * s.altitude));
  const position = add(P, off);
  // Screen-up interpolates fwd (north-ish, at pitch 0) → radial-up (at pitch π/2),
  // so it is never parallel to the view direction (no undefined roll).
  const up = norm(add(scale(fwd, Math.cos(s.pitch)), scale(P, Math.sin(s.pitch))));
  return { position, target: P, up };
}

/// A full camera pose: where it sits, what it looks at, which way is screen-up.
/// The shape `globeCamPose` returns, named so the entry-ease can interpolate it.
export interface CamPose {
  position: Vec3;
  target: Vec3;
  up: Vec3;
}

/// Interpolate between two camera poses (increment 1f entry-ease): linear in
/// position and look-target, normalized-linear in up (nlerp — exact at the
/// endpoints, monotone in between; the entry poses differ by a small angle, so
/// nlerp ≈ slerp without the trig). `t` is raw [0,1] — the CALLER applies easing,
/// keeping this pure and trivially testable. Used to glide the camera from the
/// fly-to end pose (or the previous drill's pose) into the flight pose instead of
/// the one-frame snap the 1a note deferred.
export function lerpPose(a: CamPose, b: CamPose, t: number): CamPose {
  const k = Math.max(0, Math.min(1, t));
  const lerp3 = (p: Vec3, q: Vec3): Vec3 => [
    p[0] + (q[0] - p[0]) * k,
    p[1] + (q[1] - p[1]) * k,
    p[2] + (q[2] - p[2]) * k,
  ];
  return {
    position: lerp3(a.position, b.position),
    target: lerp3(a.target, b.target),
    up: norm(lerp3(a.up, b.up)),
  };
}

/// Partial-sphere geometry for a drilled sector (increment 1b). Maps the sector's
/// equirectangular world rect to a three.js `SphereGeometry` segment occupying
/// exactly that lon/lat span on the unit sphere, so a high-detail texture of the
/// sector lays over its slice of the base globe. PURE — pinned (ranges + the
/// segW/segH aspect that must track phiLength/thetaLength, else the cartography is
/// anisotropically squashed; + a raycast that the patch centers on the sector).
export interface PatchParams {
  phiStart: number;
  phiLength: number;
  thetaStart: number;
  thetaLength: number;
  /** unit vector to the sector center — for camera framing / horizon culling. */
  centerDir: Vec3;
  /** tessellation segments; segW/segH track the lon/lat aspect for curvature + UV. */
  segW: number;
  segH: number;
}

/** ~rad per patch segment (≈ the base sphere's 64×48 density); clamped [4, 64]. */
const PATCH_ANGULAR_RES = 0.06;
const patchSegs = (span: number) => Math.max(4, Math.min(64, Math.round(span / PATCH_ANGULAR_RES)));

export function sectorPatchParams(sector: Sector, worldW: number, worldH: number): PatchParams {
  const r = sectorRect(sector, worldW, worldH);
  // three.js SphereGeometry: phi (azimuth) ↔ longitude = (x/W)·2π; theta (polar
  // from +Y) ↔ latitude = (y/H)·π. Matches `lonLatToUnit`/`worldToUnit` (pinned).
  const phiStart = (r.x0 / worldW) * 2 * Math.PI;
  const phiLength = (r.w / worldW) * 2 * Math.PI;
  const thetaStart = (r.y0 / worldH) * Math.PI;
  const thetaLength = (r.h / worldH) * Math.PI;
  const { lon, lat } = worldToLonLat(r.x0 + r.w / 2, r.y0 + r.h / 2, worldW, worldH);
  return {
    phiStart,
    phiLength,
    thetaStart,
    thetaLength,
    centerDir: lonLatToUnit(lon, lat),
    segW: patchSegs(phiLength),
    segH: patchSegs(thetaLength),
  };
}

/// Pan the sub-point for a screen drag of (dx, dy) pixels. PURE so the pan-feel
/// SIGN is unit-testable (the e2e can only see that the sub-point moved, not which
/// way). Travel scales with altitude so a drag feels the same at every zoom.
/// Convention: drag right (dx>0) reveals what's to the WEST (subLon decreases);
/// drag down (dy>0) reveals the NORTH (subLat increases) — the "grab the map" feel.
export function panSubPoint(
  s: GlobeCamState,
  dx: number,
  dy: number,
  panSpeed: number,
): { subLon: number; subLat: number } {
  const k = panSpeed * s.altitude;
  const subLat = Math.max(-LAT_MAX, Math.min(LAT_MAX, s.subLat + dy * k));
  return { subLon: s.subLon - dx * k, subLat };
}

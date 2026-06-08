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

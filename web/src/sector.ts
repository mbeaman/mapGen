/// Pure multi-scale navigation geometry (Phase 7) — no DOM, no wasm, so it is
/// unit-testable in isolation. `main.ts` (drill-in + breadcrumb) and `worker.ts`
/// (per-stage style) use these so the tested code is the shipped code.

export interface Sector {
  level: number;
  sx: number;
  sy: number;
}

export interface Rect {
  x0: number;
  y0: number;
  w: number;
  h: number;
}

export const ROOT: Sector = { level: 0, sx: 0, sy: 0 };

/// World-space rectangle a sector occupies, for full-world dims `(worldW, worldH)`.
export function sectorRect(s: Sector, worldW: number, worldH: number): Rect {
  const span = 2 ** s.level;
  const w = worldW / span;
  const h = worldH / span;
  return { x0: s.sx * w, y0: s.sy * h, w, h };
}

/// The sector at an absolute `level` that contains world-space point `(wx, wy)`.
/// Coordinates are clamped into range, so an out-of-bounds point is harmless.
export function sectorAt(
  wx: number,
  wy: number,
  level: number,
  worldW: number,
  worldH: number,
): Sector {
  const span = 2 ** level;
  const clamp = (v: number, hi: number) => Math.max(0, Math.min(hi, v));
  const sx = clamp(Math.floor(wx / (worldW / span)), span - 1);
  const sy = clamp(Math.floor(wy / (worldH / span)), span - 1);
  return { level, sx, sy };
}

/// The child sector one level finer (capped at `maxLevel`) that contains the
/// world-space point `(wx, wy)`. Returns `null` if already at `maxLevel`.
export function childSectorAt(
  wx: number,
  wy: number,
  fromLevel: number,
  worldW: number,
  worldH: number,
  maxLevel: number,
): Sector | null {
  if (fromLevel >= maxLevel) return null;
  return sectorAt(wx, wy, fromLevel + 1, worldW, worldH);
}

/// World-space point under a raycast hit on a sector's curved patch (increment
/// 1c). The patch (a partial `SphereGeometry` from `sectorPatchParams`) has
/// intrinsic uv running `u` 0→1 west→east across its longitude span and `uv.y`
/// 1→0 north→south across its latitude span (the three.js `uv = (u, 1-v)`
/// convention, pinned by `camera.test.ts`'s patch orientation raycast). So a hit
/// `(u, v)` maps into the sector's world rect: `x` lerps west→east, `y` lerps
/// north(top)→south as `v` falls. Feeds `childSectorAt` to drill deeper in 3D.
export function patchUvToWorld(
  u: number,
  v: number,
  sector: Sector,
  worldW: number,
  worldH: number,
): { x: number; y: number } {
  const r = sectorRect(sector, worldW, worldH);
  return { x: r.x0 + u * r.w, y: r.y0 + (1 - v) * r.h };
}

/// The level a globe FIRST drill zooms to. Fixed (not continent-size-derived):
/// the old continent-aware drill landed every click on the continent CENTROID and,
/// for a big continent, picked a shallow level (L1 = a 180°×90° quarter-globe) —
/// a hugely curved, texture-stretched patch. L3 (a 45° sector) is the shallowest
/// level whose patch isn't grossly distorted; deeper clicks (1c) go +1 from there.
export const GLOBE_FIRST_DRILL_LEVEL = 3;

/// The sector a globe click drills into: the sector at the CLICKED world point —
/// NOT the continent centroid. So you land where you clicked. Pinned by
/// `sector.test.ts` (the target contains the click; distinct clicks → distinct
/// sectors) so the centroid-snap regression can't return silently.
export function globeDrillTarget(clickX: number, clickY: number, worldW: number, worldH: number): Sector {
  return sectorAt(clickX, clickY, GLOBE_FIRST_DRILL_LEVEL, worldW, worldH);
}

/// Quadtree level to drill to for a continent occupying `cellCount` of
/// `totalCells`. A sector at level L is `1/4^L` of the world, so matching the
/// continent's share gives `L ≈ ½·log2(total/count)`. Clamped to `[1, maxLevel]`
/// — always at least one level in (a click should zoom), never past the cap.
export function continentDrillLevel(
  cellCount: number,
  totalCells: number,
  maxLevel: number,
): number {
  const ratio = totalCells / Math.max(1, cellCount);
  const level = Math.round(0.5 * Math.log2(ratio));
  return Math.max(1, Math.min(maxLevel, level));
}

/// Breadcrumb chain from the world root down to and including `s` (one entry per
/// level). Each ancestor's coordinates are `s`'s shifted right by the depth gap.
export function ancestors(s: Sector): Sector[] {
  const out: Sector[] = [];
  for (let k = 0; k <= s.level; k++) {
    const shift = s.level - k;
    out.push({ level: k, sx: s.sx >> shift, sy: s.sy >> shift });
  }
  return out;
}

/// Breadcrumb label for a sector, given the generation scale. Only the *root*
/// changes with scale — a continent world's root is "World", a planet's root is
/// "Planet" (the planisphere). Deeper crumbs stay generic `L<level> (sx,sy)` in
/// both scales: a quadtree quadrant isn't a continent (32 plates routinely
/// bisect a landmass), so naming the landmasses is a separate task.
export function crumbLabel(s: Sector, planet: boolean): string {
  if (s.level === 0) return planet ? "Planet" : "World";
  return `L${s.level} (${s.sx},${s.sy})`;
}

/// The style to render at a given nav level for the active scale. At the planet
/// root we always draw the planisphere overview (`"planet"`), whatever style the
/// user picked for the continental view; everywhere else the user's style wins.
/// Continent scale never substitutes a style.
export function navStyle(level: number, planet: boolean, userStyle: string): string {
  return planet && level === 0 ? "planet" : userStyle;
}

// ---- 3D globe drill mapping ----
// The Globe view textures a sphere with the flat equirectangular world render
// and, on click, reads the raycaster's intrinsic surface UV. These pure
// functions convert that UV to the world (x,y) the existing `continentAt` query
// expects, so a globe click reuses the very same drill the 2D planet uses.
//
// World coords are equirectangular in [0,worldW]×[0,worldH]: x = longitude
// (x=0 is the west/antimeridian edge, x=worldW the east), y = latitude (y=0 is
// the NORTH edge, y=worldH the south) — the same convention as the Mollweide
// inverse above. The texture's top row is the north edge, and three.js's default
// `texture.flipY` places the image top at v=1; so v=1 ⇒ north ⇒ world y=0, hence
// the `1 - v`. (That flip is a three.js convention validated end-to-end by the
// globe click e2e; a regression in this formula is caught by the corner anchors
// in sector.test.ts.)

/// Sphere surface UV (u,v ∈ [0,1]) → equirectangular world (x,y). The production
/// drill path: raycaster `intersection.uv` → here → `continentAt(x,y)`.
export function uvToWorld(
  u: number,
  v: number,
  worldW: number,
  worldH: number,
): { x: number; y: number } {
  return { x: u * worldW, y: (1 - v) * worldH };
}

/// World (x,y) → the texture UV that samples it — the inverse of [`uvToWorld`].
/// Used by the tests (round-trip) and any future "aim the camera at a world
/// point" path.
export function worldToUv(
  wx: number,
  wy: number,
  worldW: number,
  worldH: number,
): { u: number; v: number } {
  return { u: wx / worldW, v: 1 - wy / worldH };
}

/// World (x,y) for a geographic lat/lon, in the SAME equirectangular convention
/// the Mollweide inverse returns: `lat ∈ [-π/2, π/2]` (north +), `lon ∈ [-π, π]`
/// (east +). Cross-checks [`uvToWorld`] in the tests (north pole → top-centre,
/// which a v-flip would send to the bottom) and seeds a future camera fly-to.
export function latLonToWorld(
  lat: number,
  lon: number,
  worldW: number,
  worldH: number,
): { x: number; y: number } {
  return {
    x: worldW * (lon / (2 * Math.PI) + 0.5),
    y: worldH * (0.5 - lat / Math.PI),
  };
}

// ---- Mollweide projection (the planet planisphere's globe-edge look) ----
// The planet renders as a Mollweide oval (equal-area, whole-world). These mirror
// the Rust `project`/inverse in `style/planet.rs` EXACTLY — cross-language
// agreement is pinned by shared reference points in the tests, because a drifted
// constant would pass both round-trip suites yet land a click in the wrong
// ocean. World coords are equirectangular in `[0,worldW]×[0,worldH]`; screen
// coords are the projected oval inscribed in the same box.

const SQRT2 = Math.SQRT2;

/// Solve Mollweide's `2θ + sin2θ = π·sin(lat)` for the auxiliary angle θ
/// (Newton). The poles (θ = ±π/2) are special-cased — there the derivative
/// `2 + 2cos2θ` vanishes.
function mollweideTheta(lat: number): number {
  const HALF_PI = Math.PI / 2;
  if (Math.abs(lat) >= HALF_PI - 1e-6) return Math.sign(lat) * HALF_PI;
  let theta = lat;
  const target = Math.PI * Math.sin(lat);
  for (let i = 0; i < 8; i++) {
    const den = 2 + 2 * Math.cos(2 * theta);
    if (Math.abs(den) < 1e-9) break;
    theta -= (2 * theta + Math.sin(2 * theta) - target) / den;
  }
  return theta;
}

/// World → projected oval (forward). Maps `[0,worldW]×[0,worldH]` onto the
/// Mollweide ellipse inscribed in the same box; the centre stays the centre,
/// the equator's ends touch the left/right edges, the poles pinch to points.
export function mollweideProject(
  wx: number,
  wy: number,
  worldW: number,
  worldH: number,
): { x: number; y: number } {
  const lon = (wx / worldW - 0.5) * 2 * Math.PI; // [-π, π]
  const lat = (0.5 - wy / worldH) * Math.PI; // [π/2 (north) .. -π/2]
  const theta = mollweideTheta(lat);
  const mx = ((2 * SQRT2) / Math.PI) * lon * Math.cos(theta); // [-2√2, 2√2]
  const my = SQRT2 * Math.sin(theta); // [-√2, √2]
  return {
    x: worldW * (0.5 + mx / (4 * SQRT2)),
    y: worldH * (0.5 - my / (2 * SQRT2)), // flip: north (my>0) → small y
  };
}

/// Projected oval → world (inverse, closed-form). Returns `null` for a point
/// outside the ellipse (the bare parchment corners) — the caller treats that as
/// an inert click, not a drill.
export function mollweideUnproject(
  px: number,
  py: number,
  worldW: number,
  worldH: number,
): { x: number; y: number } | null {
  const mx = (px / worldW - 0.5) * 4 * SQRT2; // [-2√2, 2√2]
  const my = (0.5 - py / worldH) * 2 * SQRT2; // [-√2, √2]
  // Outside the Mollweide ellipse (semi-axes 2√2, √2) → bare corner, inert.
  if ((mx / (2 * SQRT2)) ** 2 + (my / SQRT2) ** 2 > 1) return null;
  const theta = Math.asin(Math.max(-1, Math.min(1, my / SQRT2)));
  const cosTheta = Math.cos(theta);
  if (cosTheta < 1e-6) return null; // at a pole: longitude is undefined
  const lon = (mx * Math.PI) / (2 * SQRT2 * cosTheta);
  if (Math.abs(lon) > Math.PI + 1e-3) return null;
  const lat = Math.asin(Math.max(-1, Math.min(1, (2 * theta + Math.sin(2 * theta)) / Math.PI)));
  return {
    x: worldW * (lon / (2 * Math.PI) + 0.5),
    y: worldH * (0.5 - lat / Math.PI),
  };
}

/// The projected bounding box of a world-space rectangle on the Mollweide oval —
/// frames the coarse-first zoom when drilling out of the projected planet. The
/// projected edges curve, so it samples a grid rather than trusting the corners.
export function projectedBounds(
  x0: number,
  y0: number,
  w: number,
  h: number,
  worldW: number,
  worldH: number,
): { x0: number; y0: number; w: number; h: number } {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  const N = 8;
  for (let i = 0; i <= N; i++) {
    for (let j = 0; j <= N; j++) {
      const p = mollweideProject(x0 + (w * i) / N, y0 + (h * j) / N, worldW, worldH);
      minX = Math.min(minX, p.x);
      maxX = Math.max(maxX, p.x);
      minY = Math.min(minY, p.y);
      maxY = Math.max(maxY, p.y);
    }
  }
  return { x0: minX, y0: minY, w: maxX - minX, h: maxY - minY };
}

export type StageStyle = "greyscale" | "biomes" | "cultures";

/// Richest style whose inputs exist by a given build stage (used to render the
/// live build-up). Ornate is never rendered mid-build — too costly.
export function styleForStage(stage: string): StageStyle {
  switch (stage) {
    case "terrain":
    case "erosion":
      return "greyscale";
    case "hydrology":
    case "ocean":
    case "climate":
    case "biomes":
    case "sea_lanes":
      return "biomes";
    default:
      return "cultures";
  }
}

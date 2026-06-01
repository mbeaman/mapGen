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
      return "biomes";
    default:
      return "cultures";
  }
}

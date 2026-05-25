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

/// The child sector one level finer (capped at `maxLevel`) that contains the
/// world-space point `(wx, wy)`. Returns `null` if already at `maxLevel`.
/// Coordinates are clamped into range, so an out-of-bounds click is harmless.
export function childSectorAt(
  wx: number,
  wy: number,
  fromLevel: number,
  worldW: number,
  worldH: number,
  maxLevel: number,
): Sector | null {
  if (fromLevel >= maxLevel) return null;
  const level = fromLevel + 1;
  const span = 2 ** level;
  const clamp = (v: number, hi: number) => Math.max(0, Math.min(hi, v));
  const sx = clamp(Math.floor(wx / (worldW / span)), span - 1);
  const sy = clamp(Math.floor(wy / (worldH / span)), span - 1);
  return { level, sx, sy };
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

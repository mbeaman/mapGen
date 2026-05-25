import { describe, expect, it } from "vitest";
import { ancestors, childSectorAt, ROOT, sectorRect, styleForStage } from "./sector";

const W = 2048;
const H = 1280;

describe("sectorRect", () => {
  it("root covers the whole world", () => {
    expect(sectorRect(ROOT, W, H)).toEqual({ x0: 0, y0: 0, w: W, h: H });
  });

  it("level-2 (1,1) is the second cell of a 4x4 grid", () => {
    expect(sectorRect({ level: 2, sx: 1, sy: 1 }, W, H)).toEqual({
      x0: 512,
      y0: 320,
      w: 512,
      h: 320,
    });
  });

  it("the four children of a sector tile it exactly", () => {
    const parent = { level: 1, sx: 1, sy: 0 };
    const pr = sectorRect(parent, W, H);
    let area = 0;
    for (const [dx, dy] of [
      [0, 0],
      [1, 0],
      [0, 1],
      [1, 1],
    ]) {
      const c = { level: 2, sx: parent.sx * 2 + dx, sy: parent.sy * 2 + dy };
      const cr = sectorRect(c, W, H);
      expect(cr.x0).toBeGreaterThanOrEqual(pr.x0);
      expect(cr.x0 + cr.w).toBeLessThanOrEqual(pr.x0 + pr.w + 1e-6);
      area += cr.w * cr.h;
    }
    expect(area).toBeCloseTo(pr.w * pr.h, 3);
  });
});

describe("childSectorAt", () => {
  it("maps a point to the child sector containing it", () => {
    // A point in the top-right quadrant drills to level-1 (1,0).
    expect(childSectorAt(1500, 100, 0, W, H, 6)).toEqual({ level: 1, sx: 1, sy: 0 });
  });

  it("the child always contains the clicked point", () => {
    for (const [wx, wy] of [
      [10, 10],
      [1000, 600],
      [2040, 1270],
    ]) {
      const c = childSectorAt(wx, wy, 1, W, H, 6)!;
      const r = sectorRect(c, W, H);
      expect(wx).toBeGreaterThanOrEqual(r.x0);
      expect(wx).toBeLessThan(r.x0 + r.w + 1e-6);
      expect(wy).toBeGreaterThanOrEqual(r.y0);
      expect(wy).toBeLessThan(r.y0 + r.h + 1e-6);
    }
  });

  it("clamps out-of-bounds points into range", () => {
    const c = childSectorAt(99999, -10, 0, W, H, 6)!;
    expect(c.sx).toBe(1);
    expect(c.sy).toBe(0);
  });

  it("returns null at the max level", () => {
    expect(childSectorAt(100, 100, 6, W, H, 6)).toBeNull();
  });
});

describe("ancestors", () => {
  it("is the chain from root to the sector", () => {
    expect(ancestors({ level: 2, sx: 3, sy: 2 })).toEqual([
      { level: 0, sx: 0, sy: 0 },
      { level: 1, sx: 1, sy: 1 },
      { level: 2, sx: 3, sy: 2 },
    ]);
  });

  it("is just the root for the root", () => {
    expect(ancestors(ROOT)).toEqual([ROOT]);
  });
});

describe("styleForStage", () => {
  it("progresses greyscale → biomes → cultures across the pipeline", () => {
    expect(styleForStage("terrain")).toBe("greyscale");
    expect(styleForStage("erosion")).toBe("greyscale");
    expect(styleForStage("climate")).toBe("biomes");
    expect(styleForStage("biomes")).toBe("biomes");
    expect(styleForStage("cultures")).toBe("cultures");
    expect(styleForStage("history")).toBe("cultures");
  });
});

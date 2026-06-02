import { describe, expect, it } from "vitest";
import {
  ancestors,
  childSectorAt,
  continentDrillLevel,
  crumbLabel,
  mollweideProject,
  mollweideUnproject,
  navStyle,
  ROOT,
  sectorAt,
  sectorRect,
  styleForStage,
} from "./sector";

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

describe("crumbLabel", () => {
  it("names the root by scale — 'World' for a continent, 'Planet' for a planet", () => {
    expect(crumbLabel(ROOT, false)).toBe("World");
    expect(crumbLabel(ROOT, true)).toBe("Planet");
  });

  // A quadtree quadrant is NOT a continent (32 plates routinely bisect a
  // landmass), so deeper crumbs stay generic in both scales — naming the
  // landmasses is a separate task (grounded continent names).
  it("labels deeper sectors generically, independent of scale", () => {
    const s = { level: 2, sx: 3, sy: 1 };
    expect(crumbLabel(s, false)).toBe("L2 (3,1)");
    expect(crumbLabel(s, true)).toBe("L2 (3,1)");
    expect(crumbLabel({ level: 1, sx: 1, sy: 0 }, true)).toBe("L1 (1,0)");
  });
});

describe("sectorAt", () => {
  it("is the absolute level-L sector containing a point", () => {
    // Level 1, span 2: (1500,100) → column 1, row 0.
    expect(sectorAt(1500, 100, 1, W, H)).toEqual({ level: 1, sx: 1, sy: 0 });
    // Level 3, span 8, cw=256 ch=160: (100,100) → (0,0).
    expect(sectorAt(100, 100, 3, W, H)).toEqual({ level: 3, sx: 0, sy: 0 });
  });

  it("agrees with childSectorAt for a one-level step", () => {
    // childSectorAt is the +1 special case of sectorAt.
    expect(sectorAt(1500, 100, 1, W, H)).toEqual(childSectorAt(1500, 100, 0, W, H, 6));
  });

  it("clamps an out-of-bounds point into range", () => {
    expect(sectorAt(99999, -10, 2, W, H)).toEqual({ level: 2, sx: 3, sy: 0 });
  });
});

describe("continentDrillLevel", () => {
  it("sizes the drill from the continent's share of the world", () => {
    // A continent that's 1/16 of the cells fits a level-2 sector (4^2 = 16).
    expect(continentDrillLevel(1000, 16_000, 6)).toBe(2);
    expect(continentDrillLevel(1000, 4_000, 6)).toBe(1); // 1/4 → level 1
    expect(continentDrillLevel(1000, 64_000, 6)).toBe(3); // 1/64 → level 3
  });

  it("never drills shallower than 1 (always zoom in) or past maxLevel", () => {
    expect(continentDrillLevel(9000, 10_000, 6)).toBe(1); // near-whole-world → still zoom in one
    expect(continentDrillLevel(1, 1_000_000, 6)).toBe(6); // a speck-sized ratio clamps to maxLevel
  });
});

describe("Mollweide projection", () => {
  // Planet dims (2:1). The oval inscribes exactly in this box.
  const PW = 2048;
  const PH = 1024;

  // CROSS-LANGUAGE PIN: these exact projected values are ALSO asserted in the
  // Rust test (style/planet.rs `project`). A drifted constant in either language
  // breaks here — without this, both round-trip suites pass while a click lands
  // in the wrong ocean. Do not "fix" these by recomputing one side.
  it("maps reference world points to the known projected oval coords", () => {
    const ref: [number, number, number, number][] = [
      [1024, 512, 1024.0, 512.0], // centre → centre
      [2048, 512, 2048.0, 512.0], // equator east end → right edge
      [1536, 256, 1436.625, 208.875], // mid-latitude, off-centre
      [1024, 64, 1024.0, 32.73], // near the north pole (pinched up)
    ];
    for (const [wx, wy, ex, ey] of ref) {
      const p = mollweideProject(wx, wy, PW, PH);
      expect(p.x).toBeCloseTo(ex, 2);
      expect(p.y).toBeCloseTo(ey, 2);
    }
  });

  it("round-trips world → oval → world for points inside the oval", () => {
    for (const [wx, wy] of [
      [1024, 512],
      [1536, 256],
      [700, 800],
      [1024, 64],
      [1400, 700],
    ]) {
      const p = mollweideProject(wx, wy, PW, PH);
      const back = mollweideUnproject(p.x, p.y, PW, PH);
      expect(back).not.toBeNull();
      expect(back!.x).toBeCloseTo(wx, 1);
      expect(back!.y).toBeCloseTo(wy, 1);
    }
  });

  it("returns null for clicks in the bare oval corners (inert, not a drill)", () => {
    // The four corners of the 2:1 box are well outside the inscribed ellipse.
    expect(mollweideUnproject(20, 20, PW, PH)).toBeNull();
    expect(mollweideUnproject(PW - 20, 20, PW, PH)).toBeNull();
    expect(mollweideUnproject(20, PH - 20, PW, PH)).toBeNull();
    expect(mollweideUnproject(PW - 20, PH - 20, PW, PH)).toBeNull();
  });
});

describe("navStyle", () => {
  it("renders the planisphere only at the planet root, the chosen style elsewhere", () => {
    // Planet root → the planisphere overview regardless of the picked style.
    expect(navStyle(0, true, "ornate")).toBe("planet");
    // Drill into the planet → the chosen continental style takes over.
    expect(navStyle(1, true, "ornate")).toBe("ornate");
    expect(navStyle(2, true, "biomes")).toBe("biomes");
    // Continent scale never substitutes a style, not even at the root.
    expect(navStyle(0, false, "ornate")).toBe("ornate");
    expect(navStyle(0, false, "greyscale")).toBe("greyscale");
  });
});

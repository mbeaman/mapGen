import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  ancestors,
  childSectorAt,
  continentDrillLevel,
  crumbLabel,
  GLOBE_FIRST_DRILL_LEVEL,
  globeDrillTarget,
  latLonToWorld,
  mollweideProject,
  mollweideUnproject,
  navStyle,
  patchUvToWorld,
  ROOT,
  sectorAt,
  sectorRect,
  styleForStage,
  uvToWorld,
  worldToUv,
} from "./sector";

const W = 2048;
const H = 1280;

describe("globeDrillTarget", () => {
  it("drills to the CLICKED point — the target sector CONTAINS the click, at the fixed level", () => {
    // The centroid-snap bug: every click loaded the continent's centre. This pins
    // that the drill is the clicked point's sector (a fixed-centroid target would
    // not contain an off-centre click).
    for (const [x, y] of [
      [300, 200],
      [1500, 800],
      [100, 1100],
    ]) {
      const t = globeDrillTarget(x, y, W, H);
      expect(t.level).toBe(GLOBE_FIRST_DRILL_LEVEL);
      const r = sectorRect(t, W, H);
      expect(x >= r.x0 && x <= r.x0 + r.w).toBe(true);
      expect(y >= r.y0 && y <= r.y0 + r.h).toBe(true);
    }
  });
  it("distinct clicks in different sectors → distinct targets (a centroid-snap collapses them)", () => {
    expect(globeDrillTarget(200, 200, W, H)).not.toEqual(globeDrillTarget(1500, 800, W, H));
  });
  it("never yields a hugely-curved shallow patch (level ≥ 3 ⇒ sector span ≤ 45°)", () => {
    const span = (2 * Math.PI) / 2 ** globeDrillTarget(1000, 600, W, H).level;
    expect(span).toBeLessThanOrEqual(Math.PI / 4 + 1e-9); // ≤ 45° longitude
  });
});

describe("patchUvToWorld", () => {
  const sec = { level: 2, sx: 1, sy: 1 }; // rect {x0:512, y0:320, w:512, h:320}
  it("centre → sector centre; u maps west→east, v maps south→north (top)", () => {
    expect(patchUvToWorld(0.5, 0.5, sec, W, H)).toEqual({ x: 768, y: 480 });
    expect(patchUvToWorld(0, 0.5, sec, W, H).x).toBe(512); // u=0 → west edge
    expect(patchUvToWorld(1, 0.5, sec, W, H).x).toBe(1024); // u=1 → east edge
    expect(patchUvToWorld(0.5, 1, sec, W, H).y).toBe(320); // v=1 → north (top, y0)
    expect(patchUvToWorld(0.5, 0, sec, W, H).y).toBe(640); // v=0 → south (y0+h)
  });
  it("a patch click drills to a child INSIDE the parent sector", () => {
    const w = patchUvToWorld(0.5, 0.5, sec, W, H);
    const child = childSectorAt(w.x, w.y, sec.level, W, H, 6)!;
    expect(child.level).toBe(3);
    const pr = sectorRect(sec, W, H);
    const cr = sectorRect(child, W, H);
    expect(cr.x0).toBeGreaterThanOrEqual(pr.x0);
    expect(cr.x0 + cr.w).toBeLessThanOrEqual(pr.x0 + pr.w);
    expect(cr.y0).toBeGreaterThanOrEqual(pr.y0);
    expect(cr.y0 + cr.h).toBeLessThanOrEqual(pr.y0 + pr.h);
  });
});

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
    // sea_lanes runs after biomes but before cultures — cultures don't exist
    // yet, so the richest available style is biomes, NOT the cultures default.
    expect(styleForStage("sea_lanes")).toBe("biomes");
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

  // CROSS-LANGUAGE PIN: the committed vector grid is the SINGLE SOURCE OF TRUTH,
  // asserted here AND in the Rust test (style/planet.rs). A drifted constant in
  // either language fails its side against the shared grid — without this, both
  // round-trip suites pass while a click lands in the wrong ocean. Do not "fix"
  // the file by recomputing one side; regenerate it from the exact math.
  it("reproduces the committed cross-language projection vectors (forward + inverse)", () => {
    const file = readFileSync(
      new URL("../../crates/mapgen-render/tests/mollweide_vectors.txt", import.meta.url),
      "utf-8",
    );
    let checked = 0;
    for (const line of file.split("\n")) {
      if (line.startsWith("#") || line.trim() === "") continue;
      const [wx, wy, sx, sy] = line.trim().split(/\s+/).map(Number);
      // Forward: TS exact ≈ the exact committed value.
      const p = mollweideProject(wx, wy, PW, PH);
      expect(p.x).toBeCloseTo(sx, 2);
      expect(p.y).toBeCloseTo(sy, 2);
      // Inverse round-trips back to world space (null only at the singular poles).
      const back = mollweideUnproject(sx, sy, PW, PH);
      if (back) {
        expect(back.x).toBeCloseTo(wx, 0);
        expect(back.y).toBeCloseTo(wy, 0);
      }
      checked++;
    }
    expect(checked).toBeGreaterThanOrEqual(15);
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

describe("globe drill mapping (uvToWorld)", () => {
  // Planet dims (2:1). MUST be 1024 high, NOT the 1280 continent default — a
  // globe drill that inherited 1280 would skew every latitude, so the test pins
  // the planet height explicitly.
  const GW = 2048;
  const GH = 1024;

  // THE LOAD-BEARING ASSERTION (the "wrong ocean" guard): the UV corners map to
  // the exact equirectangular world corners. v=1 is the texture top = NORTH =
  // world y=0 (three.js flipY); v=0 = south = y=GH. A flipped `1-v` sends north
  // to the bottom and fails here. (The flip's *correctness* — that v=1 really is
  // north on the sphere — is what the globe click e2e validates end-to-end;
  // this test guards the formula against regression once that's pinned.)
  it("maps the UV corners to the equirectangular world corners", () => {
    expect(uvToWorld(0, 1, GW, GH)).toEqual({ x: 0, y: 0 }); // NW: west edge, north
    expect(uvToWorld(1, 1, GW, GH)).toEqual({ x: GW, y: 0 }); // NE
    expect(uvToWorld(0, 0, GW, GH)).toEqual({ x: 0, y: GH }); // SW
    expect(uvToWorld(1, 0, GW, GH)).toEqual({ x: GW, y: GH }); // SE
    expect(uvToWorld(0.5, 0.5, GW, GH)).toEqual({ x: GW / 2, y: GH / 2 }); // equator/centre
  });

  it("round-trips world → uv → world", () => {
    for (const [x, y] of [
      [0, 0],
      [GW, GH],
      [512, 768],
      [2000, 10],
      [1024, 512],
    ]) {
      const { u, v } = worldToUv(x, y, GW, GH);
      const w = uvToWorld(u, v, GW, GH);
      expect(w.x).toBeCloseTo(x, 6);
      expect(w.y).toBeCloseTo(y, 6);
    }
  });

  // Independent cross-check: derive the world point two ways — via the geographic
  // lat/lon convention (latLonToWorld, the same one the Mollweide inverse uses)
  // and via the texture UV (uvToWorld) — and assert they agree. The north pole on
  // the prime meridian is top-centre (y=0); a v-flip in uvToWorld would put it at
  // the bottom (y=GH) and break this, tying the globe's world convention to the
  // rest of the app's.
  it("agrees with the equirectangular lat/lon convention", () => {
    const cases: [number, number][] = [
      [Math.PI / 2, 0], // north pole, prime meridian → (GW/2, 0)
      [-Math.PI / 2, 0], // south pole → (GW/2, GH)
      [0, 0], // equator, prime meridian → (GW/2, GH/2)
      [0, -Math.PI], // equator, west edge → (0, GH/2)
      [0.4, 1.2], // arbitrary interior point
    ];
    for (const [lat, lon] of cases) {
      const world = latLonToWorld(lat, lon, GW, GH);
      // The UV that samples that world point, by the texture convention.
      const u = lon / (2 * Math.PI) + 0.5;
      const v = lat / Math.PI + 0.5; // north (lat=+π/2) → v=1
      const viaUv = uvToWorld(u, v, GW, GH);
      expect(viaUv.x).toBeCloseTo(world.x, 6);
      expect(viaUv.y).toBeCloseTo(world.y, 6);
    }
  });
});

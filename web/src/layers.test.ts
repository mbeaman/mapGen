import { describe, expect, it } from "vitest";
import {
  applyLayers,
  defaultLayerState,
  LAYERS,
  PRESETS,
  presetState,
  svgLayerClasses,
  toggleLayer,
} from "./layers";

describe("defaultLayerState", () => {
  it("enables features and disables overlays", () => {
    const s = defaultLayerState();
    expect(s.has("rivers")).toBe(true);
    expect(s.has("political")).toBe(false); // overlay, off by default
  });
});

describe("manifest-backed lists", () => {
  it("loads the layers + presets from the shared Rust manifest", () => {
    // Sanity that the JSON import resolved to real data (not an empty stub).
    expect(LAYERS.length).toBeGreaterThan(10);
    expect(LAYERS.find((l) => l.name === "climate")?.label).toBe("Temperature");
    expect(PRESETS.map((p) => p.name)).toContain("rainfall");
  });
});

describe("toggleLayer", () => {
  it("toggles a feature layer independently", () => {
    const s = defaultLayerState();
    expect(toggleLayer(s, "rivers", false).has("rivers")).toBe(false);
    expect(toggleLayer(s, "rivers", true).has("rivers")).toBe(true);
  });

  it("enabling one overlay turns the others off (single-overlay invariant)", () => {
    let s = defaultLayerState();
    s = toggleLayer(s, "climate", true);
    s = toggleLayer(s, "relief", true); // should evict climate
    expect(s.has("relief")).toBe(true);
    expect(s.has("climate")).toBe(false);
    expect(s.has("precip")).toBe(false);
    // At most one overlay ever enabled.
    expect([...s].filter((n) => LAYERS.find((l) => l.name === n)?.overlay).length).toBe(1);
  });

  it("does not mutate the input set", () => {
    const s = defaultLayerState();
    const before = s.size;
    toggleLayer(s, "climate", true);
    expect(s.size).toBe(before);
    expect(s.has("climate")).toBe(false);
  });
});

describe("svgLayerClasses", () => {
  it("emits nothing for the default state (matches the rasterized default)", () => {
    expect(svgLayerClasses(defaultLayerState())).toEqual([]);
  });

  it("emits off-NAME for a disabled feature", () => {
    const s = defaultLayerState();
    s.delete("rivers");
    expect(svgLayerClasses(s)).toContain("off-rivers");
  });

  it("emits on-NAME for an enabled overlay (and not off-)", () => {
    const s = defaultLayerState();
    s.add("political");
    const cls = svgLayerClasses(s);
    expect(cls).toContain("on-political");
    expect(cls).not.toContain("off-political");
  });
});

describe("applyLayers", () => {
  it("reconciles class list and clears stale classes", () => {
    // Minimal classList stub.
    const set = new Set<string>(["off-rivers", "stale"]);
    const el = {
      classList: {
        add: (c: string) => set.add(c),
        remove: (...cs: string[]) => cs.forEach((c) => set.delete(c)),
      },
    } as unknown as Element;

    const enabled = defaultLayerState();
    enabled.add("political");
    enabled.delete("labels");
    applyLayers(el, enabled);

    expect(set.has("off-rivers")).toBe(false); // rivers re-enabled → class cleared
    expect(set.has("on-political")).toBe(true);
    expect(set.has("off-labels")).toBe(true);
    expect(set.has("stale")).toBe(true); // non-layer classes are left alone
  });

  it("every layer has a label and a unique name", () => {
    const names = new Set(LAYERS.map((l) => l.name));
    expect(names.size).toBe(LAYERS.length);
    for (const l of LAYERS) expect(l.label.length).toBeGreaterThan(0);
  });

  it("ships scalar data overlays (off by default, shown via on-NAME)", () => {
    for (const name of ["climate", "relief", "precip"]) {
      expect(LAYERS.find((l) => l.name === name)?.overlay).toBe(true);
      const s = defaultLayerState();
      expect(s.has(name)).toBe(false);
      s.add(name);
      expect(svgLayerClasses(s)).toContain(`on-${name}`);
    }
  });
});

describe("PRESETS", () => {
  const known = new Set(LAYERS.map((l) => l.name));

  it("every preset references only known layers and has a unique name", () => {
    const names = new Set(PRESETS.map((p) => p.name));
    expect(names.size).toBe(PRESETS.length);
    for (const p of PRESETS) {
      expect(p.label.length).toBeGreaterThan(0);
      for (const n of p.enabled) expect(known.has(n)).toBe(true);
    }
  });

  it("the antique preset reproduces the default state", () => {
    const antique = PRESETS.find((p) => p.name === "antique")!;
    expect(presetState(antique)).toEqual(defaultLayerState());
  });

  it("each thematic lens preset turns on exactly its overlay", () => {
    for (const [preset, overlay] of [
      ["climate", "climate"],
      ["relief", "relief"],
      ["rainfall", "precip"],
    ]) {
      const p = PRESETS.find((x) => x.name === preset)!;
      const cls = svgLayerClasses(presetState(p));
      expect(cls).toContain(`on-${overlay}`);
      // No other data overlay rides along.
      for (const other of ["political", "climate", "relief", "precip"]) {
        if (other !== overlay) expect(cls).not.toContain(`on-${other}`);
      }
    }
  });
});

import { describe, expect, it } from "vitest";
import { applyLayers, defaultLayerState, LAYERS, svgLayerClasses } from "./layers";

describe("defaultLayerState", () => {
  it("enables features and disables overlays", () => {
    const s = defaultLayerState();
    expect(s.has("rivers")).toBe(true);
    expect(s.has("political")).toBe(false); // overlay, off by default
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
});

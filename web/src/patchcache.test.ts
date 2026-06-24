import { describe, expect, it } from "vitest";
import { type CacheConfig, type LiveEntry, patchKey, reconcile } from "./patchcache";
import type { Sector } from "./sector";

const sec = (sx: number, sy: number, level = 3): Sector => ({ level, sx, sy });
const live = (key: string, lastSeen: number, gpuBytes = 1): LiveEntry => ({ key, lastSeen, gpuBytes });
const cfg = (maxPatches: number, maxBytes = 96_000_000, estBytes = 1): CacheConfig => ({
  maxPatches,
  maxBytes,
  estBytes,
});

describe("patchKey", () => {
  it("includes level/sx/sy AND style (so a lens toggle can't serve a stale patch)", () => {
    expect(patchKey(sec(1, 2), "globe")).toBe("3:1:2:globe");
    expect(patchKey(sec(1, 2), "on-faith")).not.toBe(patchKey(sec(1, 2), "globe"));
  });
});

describe("reconcile", () => {
  it("empty cache → load all desired, evict nothing", () => {
    const desired = [sec(0, 0), sec(1, 0)];
    const { toLoad, toEvict } = reconcile([], desired, "globe", cfg(24));
    expect(toLoad).toEqual(desired);
    expect(toEvict).toEqual([]);
  });

  it("loads only the missing desired", () => {
    const liveSet = [live(patchKey(sec(0, 0), "globe"), 5), live(patchKey(sec(9, 9), "globe"), 6)];
    const desired = [sec(0, 0), sec(1, 0)];
    const { toLoad } = reconcile(liveSet, desired, "globe", cfg(24));
    expect(toLoad).toEqual([sec(1, 0)]);
  });

  it("NEVER evicts an in-view entry, even when it's the LRU under cap pressure", () => {
    // cap 3. (0,0) is in-view AND the oldest. A mutant that treated in-view as
    // LRU-evictable would evict (0,0); the correct policy evicts the out-of-view LRU.
    const liveSet = [
      live(patchKey(sec(0, 0), "globe"), 1), // in-view, OLDEST
      live("3:6:6:globe", 2), // out-of-view
      live("3:7:7:globe", 3), // out-of-view, newest
    ];
    const { toLoad, toEvict } = reconcile(liveSet, [sec(0, 0), sec(1, 0)], "globe", cfg(3));
    expect(toLoad).toEqual([sec(1, 0)]);
    expect(toEvict).toEqual(["3:6:6:globe"]); // the out-of-view LRU, NOT in-view (0,0)
    expect(toEvict).not.toContain(patchKey(sec(0, 0), "globe"));
  });

  it("enforces the COUNT cap, evicting out-of-view LRU first", () => {
    const liveSet = [live("3:5:5:globe", 1), live("3:6:6:globe", 2), live("3:7:7:globe", 3)];
    const { toEvict } = reconcile(liveSet, [sec(0, 0), sec(1, 0)], "globe", cfg(3));
    expect(toEvict.sort()).toEqual(["3:5:5:globe", "3:6:6:globe"]); // the two LRU
    expect(toEvict).not.toContain("3:7:7:globe"); // MRU retained
  });

  it("THE FIREWALL: a desired set larger than the cap never loads more than the cap", () => {
    // The selector should clamp |desired| ≤ maxPatches, but reconcile must NOT trust
    // it — otherwise toLoad is unbounded (the tile-pyramid risk). Feed 40 desired.
    const desired = Array.from({ length: 40 }, (_, i) => sec(i, 0, 6)); // span 64 → valid
    const { toLoad, toEvict } = reconcile([], desired, "globe", cfg(24));
    expect(toLoad.length).toBe(24); // truncated to the cap (the rest stay on base)
    // eventual live = inView(0) + toLoad(24) + retained(0) = 24 ≤ cap.
    expect(toLoad.length + toEvict.length).toBeLessThanOrEqual(40);
    expect(toLoad.length).toBeLessThanOrEqual(24);
  });

  it("THE FIREWALL (byte half): the BYTE cap bounds toLoad even when the COUNT cap has room", () => {
    // 10 desired, none live. countRoom = 24 (plenty) but byteRoom = floor(10/3) = 3,
    // so the BYTE half of the load-path firewall must bind: toLoad = 3, not 10. A
    // mutant that dropped byteRoom (`room = countRoom`) loads all 10 → over maxBytes.
    const desired = Array.from({ length: 10 }, (_, i) => sec(i, 0, 6)); // span 64 → valid
    const { toLoad } = reconcile([], desired, "globe", cfg(24, 10, 3));
    expect(toLoad.length).toBe(3);
  });

  it("a style change makes the old-style patches out-of-view (different key) and evicts them", () => {
    const liveSet = [live(patchKey(sec(0, 0), "globe"), 5)];
    const { toLoad, toEvict } = reconcile(liveSet, [sec(0, 0)], "on-faith", cfg(1));
    expect(toLoad).toEqual([sec(0, 0)]); // re-load under the new style key
    expect(toEvict).toEqual([patchKey(sec(0, 0), "globe")]); // the stale-style patch evicted
  });

  it("reserves toLoad byte headroom — post-load live bytes stay ≤ maxBytes", () => {
    // maxBytes 10, estBytes 3 (per in-flight load), two out-of-view at 4 bytes each.
    const liveSet = [live("3:5:5:globe", 1, 4), live("3:6:6:globe", 2, 4)];
    const r = reconcile(liveSet, [sec(0, 0)], "globe", cfg(24, 10, 3));
    expect(r.toLoad).toEqual([sec(0, 0)]); // 1 load reserves 3 bytes
    // budget: inView 0 + toLoad 3 = 3; retain 6:6 (3+4=7≤10), evict 5:5 (7+4=11>10).
    expect(r.toEvict).toEqual(["3:5:5:globe"]);
  });
});

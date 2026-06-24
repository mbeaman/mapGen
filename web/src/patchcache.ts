/// Continuous-LOD streaming — the PURE cache policy (increment ST-1). `reconcile`
/// diffs the live patch set against the desired set and returns what to LOAD and
/// what to EVICT, enforcing the hard caps (count + GPU-texture bytes) that keep
/// the engine bounded. No three.js: the three.js `PatchCache` (in globe.ts) is the
/// only GL-touching glue; this policy is unit-tested off-GPU. See
/// docs/design/globe-ground-3d-navigation.md (streaming addendum: Patch cache).

import type { Sector } from "./sector";

/// The hard caps — a SINGLE source of truth so the selector's `maxPatches` and the
/// cache's `maxPatches` cannot drift (pass `MAX_LIVE_PATCHES` to BOTH at the call
/// site). Per the streaming addendum.
// Sized so the streamed patch set COVERS THE VISIBLE CAP at the first drill level
// (window 3 = 7×7 candidates): at L3 framing altitude (~1.1) the horizon sits
// ~61° from the sub-point — a visible cap ~123° across, ~2.7 L3 sectors (45°) in
// radius. The previous 5×5 window (~112° centred) left the patch set's EDGE
// inside the view: a permanent sharp straight line where crisp streamed detail
// abutted the blurry coarse base sphere — the dominant "nature unnaturally
// shifts" seam artifact. 7×7 puts that boundary at/past the horizon at L3 (and
// horizon foreshortening hides the residual at deeper levels). ~52 patches × the
// real ~2.1 MB/tile ≈ 110 MB GPU.
export const MAX_LIVE_PATCHES = 52;
export const MAX_TEXTURE_BYTES = 208_000_000; // estBytes-based room → 52 loads
export const ESTIMATED_PATCH_BYTES = 4_000_000; // 1024px-long-edge RGBA, no mipmaps

/// Stable cache key — `style` is IN the key so a lens toggle can't serve a stale
/// patch for the rest of its cache life (a red-team finding). Year is NOT a key
/// dimension: the time-slider is scoped out of the streaming globe in v1.
export function patchKey(sec: Sector, style: string): string {
  return `${sec.level}:${sec.sx}:${sec.sy}:${style}`;
}

/// A live cache entry, as the three.js `PatchCache` tracks it (plain state so the
/// policy is pure). `lastSeen` drives LRU; `gpuBytes` the memory cap.
export interface LiveEntry {
  key: string;
  lastSeen: number;
  gpuBytes: number;
}

export interface CacheConfig {
  /** hard COUNT cap (MAX_LIVE_PATCHES, e.g. 24). */
  maxPatches: number;
  /** hard GPU-texture-bytes cap (MAX_TEXTURE_BYTES, e.g. ~96 MB). */
  maxBytes: number;
  /** per-patch byte estimate for in-flight `toLoad` (reserves headroom). */
  estBytes: number;
}

/// Diff live vs desired → { toLoad, toEvict }. ENFORCES both hard caps on the LOAD
/// path itself — it does NOT trust `|desired| ≤ maxPatches`: `toLoad` is truncated
/// so the eventual live set (in-view + toLoad + retained) provably stays within the
/// count AND byte caps (the excess simply stays on the base sphere — the design's
/// graceful degrade; the selector already clamps `|desired|`, so the truncation is
/// normally a no-op). In-view live entries are NEVER evicted; out-of-view entries
/// are retained most-recently-seen first up to both caps, the rest evicted (LRU).
/// Pure over plain state.
export function reconcile(
  live: LiveEntry[],
  desired: Sector[],
  style: string,
  cfg: CacheConfig,
): { toLoad: Sector[]; toEvict: string[] } {
  const desiredKeyed = desired.map((s) => ({ s, key: patchKey(s, style) }));
  const desiredKeys = new Set(desiredKeyed.map((d) => d.key));
  const liveKeys = new Set(live.map((e) => e.key));

  const inView = live.filter((e) => desiredKeys.has(e.key)); // never evicted
  const outOfView = live.filter((e) => !desiredKeys.has(e.key));
  const inViewBytes = inView.reduce((s, e) => s + e.gpuBytes, 0);

  // Truncate toLoad to the room left after the never-evicted in-view set, under
  // BOTH caps (so the eventual live set is provably bounded even if the selector
  // ever over-produced). The dropped patches stay on the base sphere.
  const countRoom = Math.max(0, cfg.maxPatches - inView.length);
  const byteRoom = Math.max(0, Math.floor((cfg.maxBytes - inViewBytes) / Math.max(1, cfg.estBytes)));
  const room = Math.min(countRoom, byteRoom);
  const toLoad = desiredKeyed
    .filter((d) => !liveKeys.has(d.key))
    .map((d) => d.s)
    .slice(0, room);

  // Retain out-of-view (MRU-first) up to both caps, RESERVING the in-flight toLoad
  // budget so retention can't overshoot once those textures land; evict the rest.
  let count = inView.length + toLoad.length;
  let bytes = inViewBytes + toLoad.length * cfg.estBytes;
  const toEvict: string[] = [];
  const mru = [...outOfView].sort((a, b) => b.lastSeen - a.lastSeen);
  for (const e of mru) {
    if (count < cfg.maxPatches && bytes + e.gpuBytes <= cfg.maxBytes) {
      count++;
      bytes += e.gpuBytes;
    } else {
      toEvict.push(e.key);
    }
  }
  return { toLoad, toEvict };
}

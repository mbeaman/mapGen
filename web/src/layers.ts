/// Map layer toggles (pure, unit-tested). The renderer emits each component as a
/// `<g class="layer-NAME">` and a stylesheet keyed off root-`<svg>` classes
/// (`off-NAME` hides a feature, `on-NAME` shows a data overlay). This module owns
/// the root-class computation + the DOM-free toggle logic; `main.ts` wires it up.
///
/// The layer + preset *lists* are NOT declared here — they're imported from
/// `layers.manifest.json`, which is generated from the single Rust source of
/// truth (`crates/mapgen-render/src/layers.rs::manifest_json`), so the frontend
/// and the renderer/atlas can never silently diverge (a Rust test fails if the
/// committed manifest drifts).
import manifest from "./layers.manifest.json";

export interface Layer {
  name: string;
  label: string;
  /// A data overlay (off by default, shown via `on-NAME`) vs. a feature layer
  /// (on by default, hidden via `off-NAME`).
  overlay: boolean;
}

export const LAYERS: Layer[] = manifest.layers;

/// The set of enabled layer names at startup: feature layers on, overlays off.
export function defaultLayerState(): Set<string> {
  return new Set(LAYERS.filter((l) => !l.overlay).map((l) => l.name));
}

/// A named "lens": one click swaps the whole enabled set to a curated view.
export interface Preset {
  name: string;
  label: string;
  /// The exact set of enabled layers for this view.
  enabled: string[];
}

export const PRESETS: Preset[] = manifest.presets;

/// The enabled set for a preset, as a fresh mutable `Set`.
export function presetState(preset: Preset): Set<string> {
  return new Set(preset.enabled);
}

/// Toggle one layer in `enabled`, enforcing that at most one data *overlay* is
/// active at a time: enabling an overlay turns the others off, so tints never
/// stack and the single bottom-left legend never collides. Pure — returns a new
/// set (feature layers toggle independently).
export function toggleLayer(enabled: Set<string>, name: string, on: boolean): Set<string> {
  const next = new Set(enabled);
  if (!on) {
    next.delete(name);
    return next;
  }
  next.add(name);
  if (LAYERS.find((l) => l.name === name)?.overlay) {
    for (const l of LAYERS) if (l.overlay && l.name !== name) next.delete(l.name);
  }
  return next;
}

/// Root-`<svg>` classes for a given enabled set: `off-NAME` for a disabled
/// feature, `on-NAME` for an enabled overlay. (An enabled feature / disabled
/// overlay is the default and needs no class.)
export function svgLayerClasses(enabled: Set<string>): string[] {
  const classes: string[] = [];
  for (const l of LAYERS) {
    const on = enabled.has(l.name);
    if (l.overlay && on) classes.push(`on-${l.name}`);
    else if (!l.overlay && !on) classes.push(`off-${l.name}`);
  }
  return classes;
}

/// Reconcile an `<svg>` element's class list to `enabled` — removing any stale
/// layer classes first, so it's safe to call on a freshly-injected SVG.
export function applyLayers(svg: Element, enabled: Set<string>): void {
  for (const l of LAYERS) {
    svg.classList.remove(`off-${l.name}`, `on-${l.name}`);
  }
  for (const c of svgLayerClasses(enabled)) svg.classList.add(c);
}

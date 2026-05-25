/// Map layer toggles (pure, unit-tested). The renderer emits each component as a
/// `<g class="layer-NAME">` and a stylesheet keyed off root-`<svg>` classes
/// (`off-NAME` hides a feature, `on-NAME` shows a data overlay). This module
/// owns the layer list + the root-class computation; `main.ts` wires it to the
/// DOM. Keep `LAYERS` in sync with `LAYER_STYLE` in
/// `crates/mapgen-render/src/style/ornate_antique.rs`.

export interface Layer {
  name: string;
  label: string;
  /// A data overlay (off by default, shown via `on-NAME`) vs. a feature layer
  /// (on by default, hidden via `off-NAME`).
  overlay: boolean;
}

export const LAYERS: Layer[] = [
  { name: "political", label: "Political territory", overlay: true },
  { name: "climate", label: "Temperature", overlay: true },
  { name: "relief", label: "Elevation", overlay: true },
  { name: "precip", label: "Rainfall", overlay: true },
  { name: "labels", label: "Labels", overlay: false },
  { name: "settlements", label: "Settlements", overlay: false },
  { name: "sacred", label: "Sacred sites", overlay: false },
  { name: "borders", label: "Borders", overlay: false },
  { name: "roads", label: "Roads", overlay: false },
  { name: "rivers", label: "Rivers", overlay: false },
  { name: "forests", label: "Forests", overlay: false },
  { name: "mountains", label: "Mountains", overlay: false },
  { name: "coastline", label: "Coastline", overlay: false },
  { name: "ocean", label: "Ocean hatching", overlay: false },
  { name: "land", label: "Land fill", overlay: false },
];

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

/// Curated views. `enabled` lists *only* the layers that should be on; every
/// other layer is off. (Keep names in sync with `LAYERS`.)
export const PRESETS: Preset[] = [
  // The default cartographic view: every feature, no data overlay.
  { name: "antique", label: "Antique", enabled: LAYERS.filter((l) => !l.overlay).map((l) => l.name) },
  // Realm tint + human geography over a calm base (terrain decluttered).
  {
    name: "political",
    label: "Political",
    enabled: ["land", "ocean", "coastline", "rivers", "roads", "borders", "settlements", "sacred", "labels", "political"],
  },
  // Natural features only — no human geography.
  { name: "physical", label: "Physical", enabled: ["land", "ocean", "coastline", "rivers", "mountains", "forests", "labels"] },
  // Thematic lenses: one data overlay over a stripped base (sea shows the tint).
  { name: "climate", label: "Climate", enabled: ["coastline", "rivers", "labels", "climate"] },
  { name: "relief", label: "Relief", enabled: ["coastline", "rivers", "mountains", "labels", "relief"] },
  { name: "rainfall", label: "Rainfall", enabled: ["coastline", "rivers", "labels", "precip"] },
];

/// The enabled set for a preset, as a fresh mutable `Set`.
export function presetState(preset: Preset): Set<string> {
  return new Set(preset.enabled);
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

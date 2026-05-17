# mapgen

A lore-rich fantasy map generator. Rust + WASM core, hand-drawn SVG output, hybrid
simulation + LLM lore engine.

## Pipeline

```
seed ─► mesh (Voronoi) ─► plates ─► noise + erosion ─► hydrology ─► climate ─►
         biomes ─► society (capitals, hierarchy, roads) ─► history (500y sim) ─►
         render (ornate SVG) ─► optional: Claude-narrated chronicles
```

## Layout

| Crate               | Role                                                         |
| ------------------- | ------------------------------------------------------------ |
| `mapgen-core`       | Entity schema, event log types, RNG harness, libm shim       |
| `mapgen-geom`       | Voronoi mesh, Poisson-disk sampling, Lloyd relaxation        |
| `mapgen-world`      | Geography pipeline: plates → biomes → society                |
| `mapgen-history`    | Agent-based history simulation + append-only event log       |
| `mapgen-render`     | SVG renderer with pluggable style modules                    |
| `mapgen-lore`       | Claude integration (native only)                             |
| `mapgen-cli`        | Native dev binary                                            |
| `mapgen-wasm`       | Browser-facing wasm-bindgen façade                           |

## Quick start

```sh
cargo run --release -p mapgen-cli -- generate --seed 42 --out worlds/w42.json.gz
cargo run --release -p mapgen-cli -- render   --in  worlds/w42.json.gz --out maps/w42.svg
```

## License

Dual-licensed under MIT OR Apache-2.0.

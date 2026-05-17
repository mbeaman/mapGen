//! Patch application hooks: each scientific stage calls one of these
//! at the end of its run() to fold any matching LorePatches into the
//! cell values.

use mapgen_core::WorldData;

/// Apply temperature deltas and precipitation multipliers from any
/// patches over each cell. Call at the end of `climate::run`.
pub fn apply_climate_patches(world: &mut WorldData) {
    if world.patches.cell_index.is_empty() {
        return;
    }
    let n = world.mesh.cell_count();
    let patches = &world.patches.patches;
    let index = &world.patches.cell_index;
    let temp = &mut world.climate.temperature;
    let precip = &mut world.climate.precipitation;
    for i in 0..n {
        if index[i].is_empty() {
            continue;
        }
        for &(pi, strength) in &index[i] {
            let p = &patches[pi as usize];
            if let Some(dt) = p.layers.temperature_delta {
                temp[i] += dt * strength;
            }
            if let Some(mp) = p.layers.precipitation_mul {
                precip[i] *= 1.0 + (mp - 1.0) * strength;
            }
        }
    }
}

/// Apply biome overrides (strongest patch wins) and soil overrides.
/// Call at the end of `biomes::classify`.
pub fn apply_biome_patches(world: &mut WorldData) {
    if world.patches.cell_index.is_empty() {
        return;
    }
    let n = world.mesh.cell_count();
    let patches = &world.patches.patches;
    let index = &world.patches.cell_index;
    let biome = &mut world.climate.biome;
    for i in 0..n {
        if index[i].is_empty() {
            continue;
        }
        let mut best: Option<(f32, u8)> = None;
        for &(pi, strength) in &index[i] {
            let p = &patches[pi as usize];
            if let Some(b) = p.layers.biome_override {
                match best {
                    None => best = Some((strength, b)),
                    Some((s, _)) if strength > s => best = Some((strength, b)),
                    _ => {}
                }
            }
        }
        if let Some((_, b)) = best {
            biome[i] = b;
        }
    }
}

/// Apply elevation deltas. Call at the end of erosion (or wherever the
/// final heightmap is fixed) but BEFORE hydrology runs, because raised
/// or lowered cells affect drainage.
pub fn apply_elevation_patches(world: &mut WorldData) {
    if world.patches.cell_index.is_empty() {
        return;
    }
    let n = world.mesh.cell_count();
    let patches = &world.patches.patches;
    let index = &world.patches.cell_index;
    let elev = &mut world.terrain.elevation;
    for i in 0..n {
        for &(pi, strength) in &index[i] {
            if let Some(de) = patches[pi as usize].layers.elevation_delta {
                elev[i] = (elev[i] + de * strength).clamp(-1.0, 1.0);
            }
        }
    }
}

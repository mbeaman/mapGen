//! Ocean currents — simplified gyre model.
//!
//! Each ocean basin has an idealized 5-gyre circulation:
//!   * Western boundary (= EAST coast of a basin = WEST shore of a
//!     continent at mid-low latitudes): carries warm equatorial water
//!     poleward (Gulf Stream / Kuroshio analogue). Bumps SST and warms
//!     adjacent land cells.
//!   * Eastern boundary (= WEST coast of a basin = EAST shore of a
//!     continent at mid-low latitudes): carries cold polar water
//!     equatorward (California / Canary / Benguela current analogue).
//!     Cools adjacent land and suppresses convection (rain).
//!
//! We don't need real gyre geometry — the *signed direction* per coast
//! cell already encodes the asymmetry. Run as one pass over coast cells
//! after `detect_coast` and before `climate::run`; we store the inferred
//! temperature anomaly on a new per-cell vec that climate then reads.

use mapgen_core::WorldData;

/// Compute a coastal sea-surface-temperature anomaly per cell. Sets
/// `world.climate.coastal_temp_anomaly`. Land cells get the average
/// anomaly of their adjacent coast sea cells, weighted by inverse
/// distance.
///
/// Convention: y=0 is north pole; y=height is south pole; mid-band is
/// the equator. Mid-latitudes are y in [~0.15h, 0.4h] (NH) and
/// [~0.6h, 0.85h] (SH).
pub fn run(world: &mut WorldData) {
    let n = world.mesh.cell_count();
    let mut anomaly = vec![0.0_f32; n];

    let sites = &world.mesh.sites;
    let elev = &world.terrain.elevation;
    let coast = &world.mesh.coast;
    let neighbors = &world.mesh.neighbors;
    let h = world.mesh.height.max(1.0);
    let half_h = h * 0.5;

    // Per sea-cell anomaly first: scaled by latitude band + which limb
    // of the gyre it sits on.
    for i in 0..n {
        if elev[i] > 0.0 {
            continue;
        }
        let y = sites[i][1];
        let lat_norm = (y - half_h) / half_h; // -1 north pole, +1 south pole
                                              // Mid-latitude band where gyre limbs are strongest.
        let band_strength = (1.0 - (lat_norm.abs() - 0.55).abs() * 4.0).clamp(0.0, 1.0);
        if band_strength <= 0.0 {
            continue;
        }
        // Find nearest coast cell and decide: west coast of a continent?
        // Heuristic: this sea cell is adjacent to a land cell. The land
        // cell's relative position tells us which limb we're on.
        let mut land_dx_sum = 0.0_f32;
        let mut land_count = 0;
        for &j in &neighbors[i] {
            if elev[j as usize] > 0.0 && coast[j as usize] {
                land_dx_sum += sites[j as usize][0] - sites[i][0];
                land_count += 1;
            }
        }
        if land_count == 0 {
            continue;
        }
        let avg_dx = land_dx_sum / land_count as f32;
        // Sign convention (corrected):
        //   avg_dx > 0  →  land is east of sea  →  sea is at the EAST
        //                 boundary of an ocean basin  →  this is the
        //                 WEST coast of a continent  →  COLD eastern-
        //                 boundary current (California, Canary, Benguela).
        //   avg_dx < 0  →  land is west of sea  →  sea is at the WEST
        //                 boundary of an ocean basin  →  this is the
        //                 EAST coast of a continent  →  WARM western-
        //                 boundary current (Gulf Stream, Kuroshio).
        let sign = if avg_dx > 0.0 { -1.0 } else { 1.0 };
        anomaly[i] = sign * 0.4 * band_strength;
    }

    // Propagate anomaly to coastal land cells (distance-1 from sea).
    for i in 0..n {
        if elev[i] <= 0.0 || !coast[i] {
            continue;
        }
        let mut sum = 0.0;
        let mut cnt = 0;
        for &j in &neighbors[i] {
            if elev[j as usize] <= 0.0 && anomaly[j as usize].abs() > 0.0 {
                sum += anomaly[j as usize];
                cnt += 1;
            }
        }
        if cnt > 0 {
            anomaly[i] = sum / cnt as f32;
        }
    }

    // One more diffusion step inland to soften the boundary.
    let mut diffused = anomaly.clone();
    for i in 0..n {
        if elev[i] <= 0.0 || coast[i] {
            continue;
        }
        // Inland land cell: average over land neighbors.
        let mut sum = 0.0;
        let mut cnt = 0;
        for &j in &neighbors[i] {
            if elev[j as usize] > 0.0 && anomaly[j as usize].abs() > 0.0 {
                sum += anomaly[j as usize];
                cnt += 1;
            }
        }
        if cnt > 0 {
            diffused[i] = (sum / cnt as f32) * 0.5;
        }
    }

    world.climate.coastal_temp_anomaly = diffused;
}

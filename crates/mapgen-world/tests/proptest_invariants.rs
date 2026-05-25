//! Proptest-driven pipeline invariants. Two kinds:
//!
//!   * **Cheap algebraic invariants** (e.g., `LorePatch::strength_at ∈ [0,1]`)
//!     — exercise with the proptest default budget. These have no
//!     pipeline cost.
//!   * **Pipeline invariants** (e.g., `generate_full` doesn't panic; flow
//!     directions strictly descend) — exercise with a low case count
//!     across a wide seed range, because each case runs the full
//!     geography pipeline.
//!
//! These exist to catch *classes* of regression that fixed-seed specs
//! can miss. A seed-specific failure surfaced here belongs in
//! `realism_spec` / `river_realism_spec` (or a new spec) with the
//! offending seed pinned.

use mapgen_core::patch::{LorePatch, PatchLayers, PatchShape, Signature};
use mapgen_world::{generate, generate_full, hydrology, GenerateParams};
use proptest::prelude::*;

fn proptest_params(seed: u64) -> GenerateParams {
    // 2000 cells keeps each pipeline run fast enough that 16 proptest
    // cases finish in a couple of seconds in release.
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 2_000,
        plate_count: 10,
        nation_count: 6,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Cheap algebraic invariants
// ──────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Per `patch.rs` doc: strength is bounded to `[0, 1]` by construction
    /// (1 deep inside, 0 outside the falloff band, smoothstep across).
    /// This proptest exercises the bound under arbitrary disk + rect
    /// configurations and query points.
    #[test]
    fn patch_strength_at_is_in_unit_interval(
        cx in -2000.0f32..2000.0,
        cy in -2000.0f32..2000.0,
        radius in 1.0f32..500.0,
        falloff in 0.1f32..200.0,
        px in -3000.0f32..3000.0,
        py in -3000.0f32..3000.0,
    ) {
        let disk = LorePatch {
            name: "p".into(),
            shape: PatchShape::Disk { center: [cx, cy], radius },
            falloff_radius: falloff,
            signature: Signature::Mundane,
            layers: PatchLayers::default(),
            cause_event: None,
            seed: 0,
        };
        let s = disk.strength_at([px, py]);
        prop_assert!(s.is_finite(), "disk strength not finite: {s}");
        prop_assert!((0.0..=1.0).contains(&s), "disk strength out of range: {s}");
    }

    #[test]
    fn rect_patch_strength_at_is_in_unit_interval(
        x0 in -1000.0f32..1000.0,
        y0 in -1000.0f32..1000.0,
        w in 1.0f32..1000.0,
        h in 1.0f32..1000.0,
        falloff in 0.1f32..200.0,
        px in -3000.0f32..3000.0,
        py in -3000.0f32..3000.0,
    ) {
        let rect = LorePatch {
            name: "p".into(),
            shape: PatchShape::Rect { min: [x0, y0], max: [x0 + w, y0 + h] },
            falloff_radius: falloff,
            signature: Signature::Mundane,
            layers: PatchLayers::default(),
            cause_event: None,
            seed: 0,
        };
        let s = rect.strength_at([px, py]);
        prop_assert!(s.is_finite(), "rect strength not finite: {s}");
        prop_assert!((0.0..=1.0).contains(&s), "rect strength out of range: {s}");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Pipeline invariants — low case count, wide seed range
// ──────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// `generate_full` must not panic for any seed in a wide range. The
    /// fixed-seed specs (1, 7, 42, 100, 999) covered this for those
    /// seeds; this widens to a uniform sweep across the first 100.
    #[test]
    fn generate_full_does_not_panic(seed in 1u64..100) {
        let world = generate_full(proptest_params(seed));
        // Some surface checks so this isn't purely a smoke test:
        prop_assert_eq!(world.terrain.elevation.len(), world.mesh.cell_count());
        prop_assert_eq!(world.climate.biome.len(), world.mesh.cell_count());
    }

    /// Cross-cutting well-formedness of the *full* world (geography → history)
    /// across a wide seed sweep. The fixed-seed `history_spec` checks these on
    /// a handful of seeds (1,2,3,7,42); this widens to catch the long tail the
    /// curated seeds miss — every event id contiguous, salience bounded, causes
    /// acyclic, entity references resolvable, biome ids valid, control sane.
    #[test]
    fn full_world_invariants_hold(seed in 1u64..200) {
        let world = generate_full(proptest_params(seed));

        for (i, e) in world.events.events.iter().enumerate() {
            prop_assert_eq!(e.id.0 as usize, i, "seed {}: event id != index", seed);
            prop_assert!(
                e.salience.is_finite() && (0.0..=1.0).contains(&e.salience),
                "seed {}: event {} salience {} out of [0,1]", seed, i, e.salience
            );
            for c in &e.cause_ids {
                prop_assert!(
                    (c.0 as usize) < i,
                    "seed {}: event {} cites non-earlier cause {} (cycle/forward ref)", seed, i, c.0
                );
            }
            for a in e.actors.iter().chain(e.patients.iter()) {
                prop_assert!(
                    world.entities.by_id.contains_key(a),
                    "seed {}: event {} references unknown entity {:?}", seed, i, a
                );
            }
        }
        // Biome ids stay within the 15-value palette (0..=14).
        for (i, &b) in world.climate.biome.iter().enumerate() {
            prop_assert!(b <= 14, "seed {}: cell {} biome id {} out of range", seed, i, b);
        }
        // Every controlled cell points at a real polity.
        let n_pol = world.society.nations.len();
        for (i, c) in world.society.control.iter().enumerate() {
            if let Some(pid) = c {
                prop_assert!(
                    (*pid as usize) < n_pol,
                    "seed {}: cell {} control {} >= {} nations", seed, i, pid, n_pol
                );
            }
        }
    }

    /// `flow_directions` returns a per-cell downhill pointer. Every
    /// `Some(j)` entry at cell `i` must satisfy `elev[j] < elev[i]`. A
    /// violation means either a self-loop or an upstream pointer — both
    /// would break flow accumulation downstream. (Also enforced for
    /// seed 42 in `phase2_spec`; this widens coverage.)
    #[test]
    fn flow_directions_strictly_descend(seed in 1u64..100) {
        let mut world = generate(proptest_params(seed));
        hydrology::detect_coast(&mut world);
        hydrology::fill_depressions(&mut world);
        let flow_dir = hydrology::flow_directions(&world);

        let elev = &world.terrain.elevation;
        for (i, &to) in flow_dir.iter().enumerate() {
            if let Some(j) = to {
                prop_assert!(
                    elev[j as usize] < elev[i],
                    "seed {}: flow at cell {} (e={}) goes to {} (e={}) — not strictly descending",
                    seed, i, elev[i], j, elev[j as usize]
                );
            }
        }
    }
}

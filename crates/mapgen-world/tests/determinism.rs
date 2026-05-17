//! Phase 1 determinism + invariant gates. These run native-only; the
//! cross-platform Tier-A hash (native vs wasm32 byte-identical) is tracked
//! separately and needs `wasm-bindgen-test` infrastructure that lands with
//! Phase 5.

use mapgen_world::{generate, GenerateParams};

fn fixed_params(seed: u64) -> GenerateParams {
    GenerateParams {
        seed,
        width: 1024.0,
        height: 640.0,
        cell_count: 2000,
        plate_count: 10,
        nation_count: 6,
    }
}

#[test]
fn same_seed_same_world() {
    let a = generate(fixed_params(42));
    let b = generate(fixed_params(42));

    let a_bytes = serde_json::to_vec(&a).unwrap();
    let b_bytes = serde_json::to_vec(&b).unwrap();
    assert_eq!(
        a_bytes, b_bytes,
        "two generations with the same seed must produce byte-identical output"
    );
}

#[test]
fn different_seed_different_world() {
    let a = generate(fixed_params(1));
    let b = generate(fixed_params(2));
    // Different seeds should not give identical heightmaps. (Identical
    // would mean the seed isn't being honored at all.)
    assert_ne!(a.terrain.elevation, b.terrain.elevation);
}

#[test]
fn mesh_invariants() {
    let world = generate(fixed_params(42));
    let mesh = &world.mesh;
    let n = mesh.cell_count();

    assert!(n > 1500, "expected ~2000 cells, got {n}");
    assert_eq!(mesh.sites.len(), n);
    assert_eq!(mesh.cell_vertices.len(), n);
    assert_eq!(mesh.neighbors.len(), n);

    for (i, verts) in mesh.cell_vertices.iter().enumerate() {
        assert!(verts.len() >= 3, "cell {i} has only {} vertices", verts.len());
        for &v in verts {
            assert!(
                (v as usize) < mesh.vertices.len(),
                "cell {i} references vertex {v} but only {} exist",
                mesh.vertices.len()
            );
        }
    }

    // Neighbor relation must be symmetric.
    for i in 0..n {
        for &j in &mesh.neighbors[i] {
            assert!(
                mesh.neighbors[j as usize].contains(&(i as u32)),
                "neighbor asymmetry: {i} -> {j} but not back"
            );
        }
    }
}

#[test]
fn terrain_invariants() {
    let world = generate(fixed_params(42));
    let n = world.mesh.cell_count();
    let elev = &world.terrain.elevation;

    assert_eq!(elev.len(), n);
    assert_eq!(world.terrain.plate_id.len(), n);
    assert_eq!(world.terrain.plates.len(), 10);

    // Every elevation in the clamp range.
    for (i, &e) in elev.iter().enumerate() {
        assert!(
            (-1.0..=1.0).contains(&e),
            "cell {i} elevation {e} out of range"
        );
        assert!(e.is_finite(), "cell {i} elevation is NaN/Inf");
    }

    // Every plate_id is a valid index.
    for (i, p) in world.terrain.plate_id.iter().enumerate() {
        assert!(
            (p.0 as usize) < world.terrain.plates.len(),
            "cell {i} plate_id {} out of range",
            p.0
        );
    }

    // Sanity: we should have both land and sea cells. (If all one or the
    // other, the plate kinds + uplift are misconfigured.)
    let land = elev.iter().filter(|&&e| e > 0.0).count();
    let sea = elev.iter().filter(|&&e| e <= 0.0).count();
    assert!(land > n / 20, "almost no land: {land}/{n}");
    assert!(sea > n / 20, "almost no sea: {sea}/{n}");
}

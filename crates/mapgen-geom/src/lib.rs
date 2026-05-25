//! Mesh primitives: Poisson-disk sampling, Voronoi tessellation, Lloyd relaxation.

pub mod lloyd;
pub mod mesh;
pub mod poisson;

pub use mesh::{Mesh, MeshBuildParams, RegionMeshParams};
pub use poisson::poisson_disk_2d;

//! Core schema, IDs, event log, and RNG harness shared by all mapgen crates.

pub mod entities;
pub mod event;
pub mod fmath;
pub mod ids;
pub mod rng;
pub mod world_data;

pub use entities::*;
pub use event::*;
pub use ids::*;
pub use rng::{Stage, StageRng};
pub use world_data::*;

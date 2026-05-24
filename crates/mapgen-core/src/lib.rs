//! Core schema, IDs, event log, and RNG harness shared by all mapgen crates.

pub mod entities;
pub mod event;
pub mod fmath;
pub mod history;
pub mod ids;
pub mod naming;
pub mod patch;
pub mod rng;
pub mod world_data;

pub use entities::*;
pub use event::*;
pub use history::*;
pub use ids::*;
pub use naming::generate_name;
pub use rng::{splitmix64, Stage, StageRng};
pub use world_data::*;

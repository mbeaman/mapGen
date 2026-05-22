//! Pluggable render styles.
//!
//! * `Greyscale` — flat heightmap, Phase 1 dev visualization.
//! * `Biomes` — Phase 2 data visualization with biome-colored cells, a
//!   traced coastline, and river polylines. Honest dev view of
//!   everything Phase 2 produces.
//! * `Cultures` — Phase 3a data visualization with cells colored by the
//!   `Race` of the assigned culture (one hue per race, stable across
//!   seeds), with the same coastline + river overlays as the biomes
//!   style. Honest dev view of what `cultures::populate` produced.
//! * `OrnateAntique` — Phase 3e "exotic map shop" hand-drawn aesthetic.
//!   Variant is reserved; calling [`crate::render`] with it returns
//!   `Err` until the Phase 3e implementation lands. The submodule will
//!   come back at that point.

pub mod biomes;
pub mod cultures;
pub mod greyscale;

#[derive(Copy, Clone, Debug, Default)]
pub enum Style {
    /// Flat heightmap visualization for development.
    #[default]
    Greyscale,
    /// Phase 2 data visualization with biomes, coastlines, rivers.
    Biomes,
    /// Phase 3a data visualization — cells colored by `Race` of the
    /// assigned culture; sea + coastlines + rivers as in `Biomes`.
    Cultures,
    /// "Exotic map shop" hand-drawn antique aesthetic. **Not yet
    /// implemented** — [`crate::render`] returns `Err` for this
    /// variant. Reserved for Phase 3e.
    OrnateAntique,
}

impl std::str::FromStr for Style {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "greyscale" | "grayscale" => Ok(Self::Greyscale),
            "biomes" | "biome" => Ok(Self::Biomes),
            "cultures" | "culture" => Ok(Self::Cultures),
            "ornate" | "ornate_antique" | "antique" => Ok(Self::OrnateAntique),
            other => Err(format!("unknown style: {other}")),
        }
    }
}

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
//!   Parchment background, multi-offset coastline ripples, Tolkien
//!   triangular mountain glyphs, biome-keyed tree scatter, settlement
//!   icons keyed by polity color and tier, road polylines, sacred-site
//!   diamonds. MVP scope — `roughr` pen jitter, embedded typography,
//!   compass / cartouche, and Imhof label placement land in follow-up
//!   commits.

pub mod biomes;
pub mod cultures;
pub mod greyscale;
pub mod ornate_antique;

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
    /// Phase 3e "exotic map shop" hand-drawn antique aesthetic. MVP
    /// scope; see [`crate::style::ornate_antique`] for what's included
    /// and what's deferred.
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

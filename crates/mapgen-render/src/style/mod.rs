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
pub mod planet;

/// Render palette for the FAITH wash — one colour per religion (by roster index,
/// mod-wrapped). Deliberately jewel-toned and distinct from the muted political
/// palette so the Faith lens reads as a different map. Up to ~12 religions exist
/// post-history (founders + schism sects), so 10 entries keep reuse rare.
pub(crate) const FAITH_PALETTE: &[[u8; 3]] = &[
    [150, 40, 50],  // crimson
    [40, 90, 150],  // sapphire
    [200, 160, 40], // gold
    [60, 130, 90],  // jade
    [120, 60, 150], // amethyst
    [210, 110, 40], // amber
    [60, 140, 160], // turquoise
    [170, 70, 120], // magenta
    [95, 115, 45],  // olive
    [110, 90, 175], // iris
];

/// The faith wash colour for a religion roster index.
pub(crate) fn faith_color(religion_id: u16) -> [u8; 3] {
    FAITH_PALETTE[religion_id as usize % FAITH_PALETTE.len()]
}

/// The naval gate at or below which a sea lane is "crossable" — what the Trade
/// lens draws (planet + ornate). MUST stay equal to the data carriers' reach
/// (`mapgen_history` `TRADE_NAVAL` / `DIFFUSION_NAVAL` = 40) so the lens shows
/// exactly the lanes that actually carry trade/faith; render can't import
/// `mapgen_history`, so the value is mirrored here (and `trade_overlay.rs` keeps a
/// matching test mirror whose `==` line-count would trip on any drift).
pub(crate) const MAX_CROSSABLE_NAVAL: u8 = 40;

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
    /// Zoomed-*out* planetary overview — the whole world as an antique
    /// planisphere (continents, sea basins, graticule, major rivers/ranges,
    /// engraved labels). See [`crate::style::planet`].
    Planet,
    /// Equirectangular, fontless texture for the 3D globe sphere — the planet's
    /// parchment fill + depth-shaded sea + political wash + coast + major rivers on
    /// a flat lon/lat grid (no Mollweide oval, no labels/chrome). The richer skin
    /// the sphere wears in place of flat `Biomes`. See
    /// [`crate::style::planet::render_globe_texture`].
    GlobeTexture,
}

impl std::str::FromStr for Style {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "greyscale" | "grayscale" => Ok(Self::Greyscale),
            "biomes" | "biome" => Ok(Self::Biomes),
            "cultures" | "culture" => Ok(Self::Cultures),
            "ornate" | "ornate_antique" | "antique" => Ok(Self::OrnateAntique),
            "planet" | "planisphere" | "world" => Ok(Self::Planet),
            "globe" | "globe_texture" => Ok(Self::GlobeTexture),
            other => Err(format!("unknown style: {other}")),
        }
    }
}

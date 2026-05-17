//! Pluggable render styles. Phase 1 ships `Greyscale`; Phase 3 ships
//! `OrnateAntique`.

pub mod greyscale;
pub mod ornate_antique;

#[derive(Copy, Clone, Debug, Default)]
pub enum Style {
    /// Flat heightmap visualization for development.
    #[default]
    Greyscale,
    /// "Exotic map shop" hand-drawn antique aesthetic.
    OrnateAntique,
}

impl std::str::FromStr for Style {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "greyscale" | "grayscale" => Ok(Self::Greyscale),
            "ornate" | "ornate_antique" | "antique" => Ok(Self::OrnateAntique),
            other => Err(format!("unknown style: {other}")),
        }
    }
}

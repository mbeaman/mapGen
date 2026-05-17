//! Phase 3: the "exotic map shop" style. Stub until Phase 3.

use mapgen_core::WorldData;

pub fn render(_world: &WorldData) -> String {
    // Phase 3 will fill in: parchment, roughr coastlines, mountain icons,
    // forest scatter, typography, compass, cartouche, vignette.
    String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><text x="50" y="50" text-anchor="middle">ornate_antique: phase 3</text></svg>"#,
    )
}

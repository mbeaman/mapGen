//! LorePatch overlay. Every scientific stage queries patches at the end
//! of its computation and applies deltas / multipliers / overrides with
//! smoothstep falloff at polygon edges. Patches carry causal provenance
//! (the event that created them) so the lore engine can always answer
//! "why is this here?"
//!
//! Layering rules:
//!   * `*_delta` fields add. Multiple patches → sum of deltas weighted by
//!     each patch's falloff strength.
//!   * `*_mul` fields multiply. final = base * Π (1 + (mul-1) * strength).
//!   * `*_override` (e.g. biome) replaces. Tie-break by patch index for
//!     determinism.
//!
//! Determinism contract: per-cell patch index built once after geography
//! is final, used for read-only queries. Patch ordering = insertion
//! order. Each patch carries its own RNG seed for any random effects.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::EventId;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PatchData {
    pub patches: Vec<LorePatch>,
    /// Per cell: list of `(patch_idx, strength_after_falloff)` pairs.
    /// Rebuilt from `patches` after geography is final; never serialized
    /// (saves bytes and lets schema evolve).
    #[serde(skip)]
    pub cell_index: Vec<SmallVec<[(u16, f32); 2]>>,
}

impl PatchData {
    pub fn push(&mut self, patch: LorePatch) -> usize {
        let idx = self.patches.len();
        self.patches.push(patch);
        idx
    }

    /// Build (or rebuild) the per-cell patch index. Call once after all
    /// patches are added, before the first stage queries them.
    pub fn rebuild_index(&mut self, sites: &[[f32; 2]], _width: f32, _height: f32) {
        let n = sites.len();
        let mut index: Vec<SmallVec<[(u16, f32); 2]>> = vec![SmallVec::new(); n];
        for (pi, patch) in self.patches.iter().enumerate() {
            for (ci, site) in sites.iter().enumerate() {
                let s = patch.strength_at(*site);
                if s > 0.0 {
                    index[ci].push((pi as u16, s));
                }
            }
        }
        self.cell_index = index;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LorePatch {
    pub name: String,
    pub shape: PatchShape,
    /// Width of the soft edge (units = world coords). Inside the shape
    /// minus this band, strength is 1; outside the shape plus this band,
    /// strength is 0; across it, smoothstep.
    pub falloff_radius: f32,
    pub signature: Signature,
    pub layers: PatchLayers,
    /// History event that produced this patch (Tolkien-style "deep memory" —
    /// the chronicle can always cite the cause).
    pub cause_event: Option<EventId>,
    /// Sub-stream seed for any random effects this patch evaluates.
    pub seed: u64,
}

impl LorePatch {
    /// Strength of this patch at a given world point in `[0, 1]`. Returns
    /// 0 outside the shape + falloff band.
    pub fn strength_at(&self, p: [f32; 2]) -> f32 {
        let d = self.shape.signed_distance(p); // negative inside, positive outside
        let r = self.falloff_radius.max(1e-6);
        if d <= -r {
            1.0
        } else if d >= 0.0 {
            // Outside the polygon entirely — the falloff band sits *inside*
            // the polygon, so anything outside is zero strength.
            0.0
        } else {
            // d in (-r, 0]: smoothstep from full strength at d=-r to 0 at d=0.
            let t = (-d) / r; // 0 at edge, 1 at -r
            t * t * (3.0 - 2.0 * t)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PatchShape {
    /// Disk of the given radius centered on a point. Negative signed-
    /// distance inside, positive outside.
    Disk { center: [f32; 2], radius: f32 },
    /// Axis-aligned rectangle [x0,y0]–[x1,y1].
    Rect { min: [f32; 2], max: [f32; 2] },
}

impl PatchShape {
    /// Negative = inside; positive = outside.
    pub fn signed_distance(&self, p: [f32; 2]) -> f32 {
        match self {
            PatchShape::Disk { center, radius } => {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                (dx * dx + dy * dy).sqrt() - radius
            }
            PatchShape::Rect { min, max } => {
                // Standard rectangle SDF (Inigo Quilez).
                let cx = (min[0] + max[0]) * 0.5;
                let cy = (min[1] + max[1]) * 0.5;
                let bx = (max[0] - min[0]) * 0.5;
                let by = (max[1] - min[1]) * 0.5;
                let qx = (p[0] - cx).abs() - bx;
                let qy = (p[1] - cy).abs() - by;
                let outside = crate::fmath::hypot(qx.max(0.0), qy.max(0.0));
                let inside = qx.max(qy).min(0.0);
                outside + inside
            }
        }
    }
}

/// What the world senses about a patch — runes, planes, divine sources.
/// Used by characters / events to "perceive" or interact with the patch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Signature {
    Mundane,
    Arcane,
    Divine {
        deity: Option<crate::ids::EntityId>,
    },
    Demonic,
    Planar {
        plane_name: String,
    },
    /// Per-system arbitrary tag for game-specific signatures.
    Tagged(String),
}

/// Which scientific stages this patch touches. `None` = no touch.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PatchLayers {
    pub elevation_delta: Option<f32>,
    pub temperature_delta: Option<f32>,
    pub precipitation_mul: Option<f32>,
    pub biome_override: Option<u8>,
    pub magic_field: Option<f32>,
    pub soil_override: Option<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_signed_distance() {
        let s = PatchShape::Disk {
            center: [0.0, 0.0],
            radius: 10.0,
        };
        assert!((s.signed_distance([0.0, 0.0]) - -10.0).abs() < 1e-5);
        assert!((s.signed_distance([10.0, 0.0]) - 0.0).abs() < 1e-5);
        assert!((s.signed_distance([20.0, 0.0]) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn falloff_smoothsteps_to_zero() {
        let p = LorePatch {
            name: "x".into(),
            shape: PatchShape::Disk {
                center: [0.0, 0.0],
                radius: 50.0,
            },
            falloff_radius: 10.0,
            signature: Signature::Mundane,
            layers: PatchLayers::default(),
            cause_event: None,
            seed: 0,
        };
        // Deep inside: strength 1.
        assert!((p.strength_at([0.0, 0.0]) - 1.0).abs() < 1e-5);
        // At the edge: strength 0.
        assert!(p.strength_at([50.0, 0.0]).abs() < 1e-5);
        // Outside the polygon entirely: strength 0.
        assert!(p.strength_at([60.0, 0.0]).abs() < 1e-5);
        // Midway in the falloff (5 in from edge): smoothstep(0.5) = 0.5.
        let mid = p.strength_at([45.0, 0.0]);
        assert!(
            (mid - 0.5).abs() < 1e-5,
            "smoothstep at midpoint should be 0.5, got {mid}"
        );
    }
}

//! Cartographic line generalisation — Visvalingam–Whyatt simplification.
//!
//! The detail-DECREASING complement to `ornate_antique`'s detail-increasing glyph
//! fidelity (ADR 0001 §3): at the zoom-OUT overview, natural lines (rivers today,
//! coastlines next) carry far more vertices than the scale can resolve, so we drop
//! the ones that contribute least to the line's shape.
//!
//! Visvalingam–Whyatt (not Douglas–Peucker) because it preserves AREA: it removes
//! the vertex whose triangle with its two neighbours is smallest, repeatedly, until
//! the smallest remaining triangle exceeds a tolerance. On wiggly natural lines this
//! reads better than Douglas–Peucker's perpendicular-distance criterion (which can
//! shave whole bays). Endpoints are always kept.
//!
//! Pure + deterministic: triangle area is an exact f32 cross-product (no
//! transcendentals, no RNG), so the same line + tolerance always yields the same
//! result — which is what lets the seam contract apply the SAME tolerance on both
//! sides of a sector edge and get matching simplified lines. Operate in WORLD space
//! (before projection) so the result is projection-independent.

/// Twice the area of triangle `abc` (the cross product magnitude). Visvalingam
/// ranks vertices by this, so the ×2 constant is irrelevant to the ordering — we
/// fold it into the tolerance rather than dividing every term by two.
fn double_area(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs()
}

/// Visvalingam–Whyatt simplification of a polyline. Repeatedly removes the interior
/// vertex with the smallest effective triangle until every remaining triangle's
/// (doubled) area is `>= min_double_area`. Endpoints are always kept; a line of
/// fewer than 3 points is returned unchanged. Output is a subsequence of the input
/// in order, so it never adds points and never exceeds the input length.
pub fn visvalingam(points: &[[f32; 2]], min_double_area: f32) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    // Indices still kept, in order. Naive repeated-min removal: O(n²) overall for
    // the small polylines this serves (a major river is tens of points). The
    // priority-queue form is a later optimisation if coastlines need it.
    let mut keep: Vec<usize> = (0..points.len()).collect();
    while keep.len() > 2 {
        let mut min_area = f32::INFINITY;
        let mut min_at = 0usize; // position within `keep` (interior only)
        for i in 1..keep.len() - 1 {
            let area = double_area(points[keep[i - 1]], points[keep[i]], points[keep[i + 1]]);
            if area < min_area {
                min_area = area;
                min_at = i;
            }
        }
        if min_area >= min_double_area {
            break; // every remaining vertex matters at this tolerance
        }
        keep.remove(min_at);
    }
    keep.into_iter().map(|i| points[i]).collect()
}

#[cfg(test)]
mod tests {
    use super::{double_area, visvalingam};

    #[test]
    fn short_lines_pass_through_unchanged() {
        assert_eq!(visvalingam(&[], 10.0), Vec::<[f32; 2]>::new());
        assert_eq!(visvalingam(&[[0.0, 0.0]], 10.0), vec![[0.0, 0.0]]);
        let two = [[0.0, 0.0], [5.0, 5.0]];
        assert_eq!(visvalingam(&two, 10.0), two.to_vec());
    }

    #[test]
    fn collinear_interior_points_collapse_to_the_endpoints() {
        // A straight run: every interior triangle has zero area, so all interior
        // vertices are removed regardless of how small the tolerance is.
        let line = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0], [4.0, 0.0]];
        assert_eq!(visvalingam(&line, 0.001), vec![[0.0, 0.0], [4.0, 0.0]]);
    }

    #[test]
    fn endpoints_are_always_kept() {
        let line = [[0.0, 0.0], [1.0, 10.0], [2.0, 0.0]];
        let out = visvalingam(&line, f32::INFINITY); // drop everything droppable
        assert_eq!(out.first(), Some(&[0.0, 0.0]));
        assert_eq!(out.last(), Some(&[2.0, 0.0]));
        assert_eq!(
            out.len(),
            2,
            "only the two endpoints survive an infinite tolerance"
        );
    }

    #[test]
    fn a_significant_peak_survives_a_modest_tolerance() {
        // A tall spike between two flats: its triangle area (base 4, height 10 →
        // doubled area 40) is well above the tolerance, so it is NOT removed.
        let line = [[0.0, 0.0], [2.0, 10.0], [4.0, 0.0]];
        assert_eq!(visvalingam(&line, 20.0), line.to_vec());
        assert_eq!(double_area([0.0, 0.0], [2.0, 10.0], [4.0, 0.0]), 40.0);
    }

    #[test]
    fn sub_tolerance_wiggle_is_dropped_but_shape_is_preserved() {
        // A long ramp with a tiny jitter vertex (doubled area 2 << tolerance) plus a
        // genuine corner (doubled area 200 >> tolerance). The jitter goes; the
        // corner and endpoints stay.
        let line = [
            [0.0, 0.0],
            [10.0, 1.0], // jitter: double_area with neighbours is small
            [20.0, 0.0],
            [20.0, 20.0], // a real corner
            [40.0, 20.0],
        ];
        let out = visvalingam(&line, 50.0);
        assert!(
            out.len() < line.len(),
            "the jitter vertex should be dropped"
        );
        assert!(out.contains(&[20.0, 20.0]), "the genuine corner survives");
        assert_eq!(out.first(), Some(&[0.0, 0.0]));
        assert_eq!(out.last(), Some(&[40.0, 20.0]));
        // Output is an in-order subsequence of the input.
        let mut idx = 0usize;
        for p in &out {
            while idx < line.len() && &line[idx] != p {
                idx += 1;
            }
            assert!(
                idx < line.len(),
                "output must be a subsequence of the input"
            );
            idx += 1;
        }
    }

    #[test]
    fn never_grows_and_is_deterministic() {
        let line = [
            [0.0, 0.0],
            [1.0, 2.0],
            [2.0, 1.0],
            [3.0, 3.0],
            [4.0, 0.0],
            [5.0, 2.0],
        ];
        let a = visvalingam(&line, 3.0);
        let b = visvalingam(&line, 3.0);
        assert_eq!(a, b, "deterministic for the same input + tolerance");
        assert!(a.len() <= line.len());
    }
}

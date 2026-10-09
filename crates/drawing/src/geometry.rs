//! Flattening of drawing shapes into polylines and rings, and exact bounds.
//!
//! Exporters that cannot draw arcs natively, the hatch generator and the dash generator work on straight-line
//! geometry. The functions here turn arcs and circles into polylines whose chords deviate from the true curve by at
//! most a given tolerance (in drawing units).

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::{Pt, Rect, Shape, pt};

/// Chord tolerance for flattening, in paper millimetres. 0.01 mm is far below what a printer or screen resolves.
pub const FLATTEN_TOLERANCE_MM: f64 = 0.01;

/// Maximum number of segments used for one arc or circle.
const MAX_ARC_SEGMENTS: usize = 4096;

/// Number of segments for an arc of `radius` sweeping `sweep` radians with chord deviation `tolerance`.
fn arc_segments(radius: f64, sweep: f64, tolerance: f64) -> usize {
    let r = radius.abs();
    let sweep = sweep.abs();
    if !(r.is_finite() && sweep.is_finite()) || r <= 0.0 {
        return 1;
    }
    let tol = if tolerance.is_finite() && tolerance > 0.0 { tolerance } else { r * 1e-3 };
    let step = if tol >= r { FRAC_PI_2 } else { 2.0 * (1.0 - tol / r).acos() };
    let n = if step > 0.0 { (sweep / step).ceil() } else { MAX_ARC_SEGMENTS as f64 };
    // At least 4 segments per full turn so small circles stay round-ish.
    let min = ((sweep / TAU) * 4.0).ceil().max(1.0);
    (n.max(min) as usize).clamp(1, MAX_ARC_SEGMENTS)
}

/// Points along an arc from `start` sweeping `sweep` radians (positive counter-clockwise), both ends included.
/// Returns an empty list for non-finite input.
pub fn arc_points(centre: Pt, radius: f64, start: f64, sweep: f64, tolerance: f64) -> Vec<Pt> {
    if !(centre.x.is_finite() && centre.y.is_finite() && radius.is_finite() && start.is_finite())
        || !sweep.is_finite()
    {
        return Vec::new();
    }
    let n = arc_segments(radius, sweep, tolerance);
    (0..=n)
        .map(|i| {
            let a = start + sweep * i as f64 / n as f64;
            pt(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect()
}

/// A circle as a closed ring (first point not repeated), counter-clockwise from angle 0.
pub fn circle_ring(centre: Pt, radius: f64, tolerance: f64) -> Vec<Pt> {
    let mut pts = arc_points(centre, radius, 0.0, TAU, tolerance);
    pts.pop();
    pts
}

/// The stroked paths of a shape as `(points, closed)` pairs, with arcs flattened. Text has no stroked paths.
pub fn stroke_paths(shape: &Shape, tolerance: f64) -> Vec<(Vec<Pt>, bool)> {
    match shape {
        Shape::Line { a, b } => vec![(vec![*a, *b], false)],
        Shape::Polyline { points, closed } => {
            if points.len() < 2 {
                Vec::new()
            } else {
                vec![(points.clone(), *closed)]
            }
        }
        Shape::Polygon { rings } => {
            rings.iter().filter(|r| r.len() >= 2).map(|r| (r.clone(), true)).collect()
        }
        Shape::Arc { centre, radius, start, sweep } => {
            let pts = arc_points(*centre, *radius, *start, *sweep, tolerance);
            if pts.len() < 2 { Vec::new() } else { vec![(pts, false)] }
        }
        Shape::Circle { centre, radius } => {
            let ring = circle_ring(*centre, *radius, tolerance);
            if ring.len() < 3 { Vec::new() } else { vec![(ring, true)] }
        }
        Shape::Text(_) => Vec::new(),
    }
}

/// The rings that a fill covers (even-odd), or nothing for shapes that cannot be filled (lines, open polylines,
/// arcs, text). Rings with fewer than three points or non-finite coordinates are dropped.
pub fn fill_rings(shape: &Shape, tolerance: f64) -> Vec<Vec<Pt>> {
    let rings = match shape {
        Shape::Polygon { rings } => rings.clone(),
        Shape::Polyline { points, closed: true } => vec![points.clone()],
        Shape::Circle { centre, radius } => vec![circle_ring(*centre, *radius, tolerance)],
        _ => Vec::new(),
    };
    rings
        .into_iter()
        .filter(|r| r.len() >= 3 && r.iter().all(|p| p.x.is_finite() && p.y.is_finite()))
        .collect()
}

/// Exact bounding box of a shape's geometry (arcs by their extreme points). Text is not measured here (see
/// [`crate::text::text_corners`]); its anchor point is returned. `None` for empty or non-finite shapes.
pub fn shape_bounds(shape: &Shape) -> Option<Rect> {
    let pts: Vec<Pt> = match shape {
        Shape::Arc { centre, radius, start, sweep } => {
            let mut v = vec![
                pt(centre.x + radius * start.cos(), centre.y + radius * start.sin()),
                pt(centre.x + radius * (start + sweep).cos(), centre.y + radius * (start + sweep).sin()),
            ];
            // Add each axis extreme (multiples of 90°) the arc passes.
            let (lo, hi) = if *sweep >= 0.0 { (*start, start + sweep) } else { (start + sweep, *start) };
            if lo.is_finite() && hi.is_finite() && hi - lo < 4.0 * TAU {
                let mut k = (lo / FRAC_PI_2).ceil();
                while k * FRAC_PI_2 <= hi {
                    let a = k * FRAC_PI_2;
                    v.push(pt(centre.x + radius * a.cos(), centre.y + radius * a.sin()));
                    k += 1.0;
                }
            }
            v
        }
        other => other.extent_points(),
    };
    bounds_of(&pts)
}

/// Bounding box of points, ignoring non-finite ones.
pub fn bounds_of(points: &[Pt]) -> Option<Rect> {
    let mut it = points.iter().filter(|p| p.x.is_finite() && p.y.is_finite());
    let first = *it.next()?;
    let mut r = Rect { min: first, max: first };
    for p in it {
        r.include(*p);
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Text, pt};

    #[test]
    fn arc_flattening_respects_tolerance() {
        let tol = 0.001;
        let pts = arc_points(pt(0.0, 0.0), 1.0, 0.0, TAU, tol);
        assert!(pts.first().unwrap().distance(*pts.last().unwrap()) < 1e-9);
        for w in pts.windows(2) {
            let mid = w[0].midpoint(w[1]);
            let dev = 1.0 - mid.distance(pt(0.0, 0.0));
            assert!(dev <= tol + 1e-12, "deviation {dev}");
        }
        // Negative sweeps run clockwise.
        let cw = arc_points(pt(0.0, 0.0), 1.0, 0.0, -FRAC_PI_2, tol);
        assert!(cw.last().unwrap().distance(pt(0.0, -1.0)) < 1e-9);
    }

    #[test]
    fn degenerate_arcs_do_not_explode() {
        assert!(arc_points(pt(0.0, 0.0), f64::NAN, 0.0, 1.0, 0.1).is_empty());
        assert_eq!(arc_points(pt(0.0, 0.0), 0.0, 0.0, 1.0, 0.1).len(), 2);
        assert!(arc_points(pt(0.0, 0.0), 1e9, 0.0, TAU, 1e-9).len() <= MAX_ARC_SEGMENTS + 1);
    }

    #[test]
    fn arc_bounds_include_axis_extremes() {
        // Quarter arc from 45° to 135° reaches y = 1 at 90°.
        let s = Shape::Arc {
            centre: pt(0.0, 0.0),
            radius: 1.0,
            start: 0.25 * std::f64::consts::PI,
            sweep: FRAC_PI_2,
        };
        let b = shape_bounds(&s).unwrap();
        assert!((b.max.y - 1.0).abs() < 1e-12);
        assert!((b.min.y - 0.5_f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn fill_rings_only_for_closed_shapes() {
        let open = Shape::Polyline { points: vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0)], closed: false };
        assert!(fill_rings(&open, 0.01).is_empty());
        let closed = Shape::Polyline { points: vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0)], closed: true };
        assert_eq!(fill_rings(&closed, 0.01).len(), 1);
        let circle = Shape::Circle { centre: pt(0.0, 0.0), radius: 1.0 };
        assert!(fill_rings(&circle, 0.01)[0].len() > 8);
        assert!(fill_rings(&Shape::Text(Text::new(pt(0.0, 0.0), "x", 2.5)), 0.01).is_empty());
        let bad = Shape::Polygon { rings: vec![vec![pt(0.0, 0.0), pt(f64::NAN, 0.0), pt(1.0, 1.0)]] };
        assert!(fill_rings(&bad, 0.01).is_empty());
    }
}

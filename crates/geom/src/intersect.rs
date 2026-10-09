//! Intersections of segments, lines and circles.
//!
//! Segment classification uses the exact [`orient2d`](crate::orient2d) predicate for the topological decision
//! (do they cross, touch or overlap) and floating point only for the coordinates of the result. Touching and
//! overlap are decided within the given [`Tolerance`].

use std::cmp::Ordering;

use kanavos_units::Tolerance;

use crate::d2::{Point2, Segment2, Vec2};
use crate::predicates::orient2d;

/// How two segments meet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegmentIntersection {
    /// They do not meet.
    None,
    /// They meet at one point; `t` and `u` are the parameters on the first and second segment (0 at start,
    /// 1 at end).
    Point {
        /// The common point.
        point: Point2,
        /// Parameter on the first segment.
        t: f64,
        /// Parameter on the second segment.
        u: f64,
    },
    /// They are collinear and share a stretch, given as parameters on the first segment (`t0 <= t1`) and the
    /// corresponding parameters on the second.
    Overlap {
        /// The shared stretch, oriented like the first segment.
        segment: Segment2,
        /// Parameters on the first segment.
        t: (f64, f64),
        /// Parameters on the second segment, matching `t.0` and `t.1`.
        u: (f64, f64),
    },
}

fn xy(p: Point2) -> [f64; 2] {
    p.xy()
}

/// Classifies how two segments meet.
///
/// Tolerance: endpoints closer than `tol.length` to the other segment count as touching, and collinear segments
/// whose distance is below `tol.length` count as overlapping. Failure modes: zero-length segments are treated as
/// points.
pub fn segment_intersection(a: &Segment2, b: &Segment2, tol: &Tolerance) -> SegmentIntersection {
    let eps = tol.length.as_m();
    let la = a.length().as_m();
    let lb = b.length().as_m();
    if la <= eps || lb <= eps {
        // Degenerate: point against segment.
        let (p, s, swap) = if la <= eps { (a.start, b, false) } else { (b.start, a, true) };
        if s.distance_to(p).as_m() <= eps {
            let u = s.project_param(p).clamp(0.0, 1.0);
            let (t, u) = if swap { (u, 0.0) } else { (0.0, u) };
            return SegmentIntersection::Point { point: p, t, u };
        }
        return SegmentIntersection::None;
    }
    let o1 = orient2d(xy(a.start), xy(a.end), xy(b.start));
    let o2 = orient2d(xy(a.start), xy(a.end), xy(b.end));
    let o3 = orient2d(xy(b.start), xy(b.end), xy(a.start));
    let o4 = orient2d(xy(b.start), xy(b.end), xy(a.end));
    let da0 = a.signed_distance(b.start).abs();
    let da1 = a.signed_distance(b.end).abs();
    let collinear = (o1 == Ordering::Equal && o2 == Ordering::Equal) || (da0 <= eps && da1 <= eps);
    if collinear {
        // Project b onto a.
        let u0 = a.project_param(b.start);
        let u1 = a.project_param(b.end);
        let (lo, hi) = (u0.min(u1), u0.max(u1));
        let ep = eps / la;
        let t0 = lo.max(0.0);
        let t1 = hi.min(1.0);
        if t1 < t0 - ep {
            return SegmentIntersection::None;
        }
        let pa = a.at(t0);
        let pb = a.at(t1);
        let ub0 = b.project_param(pa).clamp(0.0, 1.0);
        let ub1 = b.project_param(pb).clamp(0.0, 1.0);
        if (t1 - t0) * la <= eps {
            let t = (t0 + t1) / 2.0;
            return SegmentIntersection::Point { point: a.at(t), t, u: (ub0 + ub1) / 2.0 };
        }
        return SegmentIntersection::Overlap { segment: Segment2::new(pa, pb), t: (t0, t1), u: (ub0, ub1) };
    }
    let crosses = o1 != o2 && o3 != o4;
    let line_hit = crate::d2::Line2::from_segment(a).intersection_params(&crate::d2::Line2::from_segment(b));
    if crosses && let Some((t, u)) = line_hit {
        let t = t.clamp(0.0, 1.0);
        let u = u.clamp(0.0, 1.0);
        return SegmentIntersection::Point { point: a.at(t), t, u };
    }
    // Touching within tolerance: an endpoint near the other segment.
    let candidates = [
        (a.start, b.distance_to(a.start).as_m(), 0.0, b.project_param(a.start), true),
        (a.end, b.distance_to(a.end).as_m(), 1.0, b.project_param(a.end), true),
        (b.start, a.distance_to(b.start).as_m(), a.project_param(b.start), 0.0, false),
        (b.end, a.distance_to(b.end).as_m(), a.project_param(b.end), 1.0, false),
    ];
    let best = candidates.iter().filter(|c| c.1 <= eps).min_by(|x, y| x.1.total_cmp(&y.1));
    match best {
        Some(&(p, _, t, u, _)) => {
            SegmentIntersection::Point { point: p, t: t.clamp(0.0, 1.0), u: u.clamp(0.0, 1.0) }
        }
        None => SegmentIntersection::None,
    }
}

/// Intersection of the infinite line through `p` with direction `d` and the circle `(centre, radius)`.
///
/// Returns the line parameters `s` (points `p + s·d`) of zero, one (tangent within `1e-12` relative) or two
/// intersections, in increasing order. `d` need not be unit length; a zero `d` gives no intersection.
pub fn line_circle(p: Point2, d: Vec2, centre: Point2, radius: f64) -> Vec<f64> {
    let a = d.length_squared();
    if a <= 1e-300 || !radius.is_finite() {
        return Vec::new();
    }
    let f = p - centre;
    let b = 2.0 * f.dot(d);
    let c = f.length_squared() - radius * radius;
    let disc = b * b - 4.0 * a * c;
    let scale = (b * b).max((4.0 * a * c).abs()).max(1e-300);
    if disc < -1e-12 * scale {
        return Vec::new();
    }
    if disc.abs() <= 1e-12 * scale {
        return vec![-b / (2.0 * a)];
    }
    let sq = disc.sqrt();
    // Numerically stable roots.
    let q = -0.5 * (b + b.signum() * sq);
    let mut r = if q.abs() > 1e-300 {
        vec![q / a, c / q]
    } else {
        vec![(-b - sq) / (2.0 * a), (-b + sq) / (2.0 * a)]
    };
    r.sort_by(f64::total_cmp);
    r
}

/// Intersection points of two circles. Concentric or non-meeting circles give none; tangent circles one.
pub fn circle_circle(c0: Point2, r0: f64, c1: Point2, r1: f64) -> Vec<Point2> {
    let d = c1 - c0;
    let dist = d.length();
    if dist <= 1e-12 || !(r0.is_finite() && r1.is_finite()) {
        return Vec::new();
    }
    let tol = 1e-12 * (r0 + r1 + dist);
    if dist > r0 + r1 + tol || dist < (r0 - r1).abs() - tol {
        return Vec::new();
    }
    let a = (r0 * r0 - r1 * r1 + dist * dist) / (2.0 * dist);
    let h2 = r0 * r0 - a * a;
    let u = d * (1.0 / dist);
    let base = c0 + u * a;
    if h2 <= tol * tol {
        return vec![base];
    }
    let h = h2.sqrt();
    vec![base + u.perp() * h, base - u.perp() * h]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: (f64, f64), b: (f64, f64)) -> Segment2 {
        Segment2::new(Point2::new(a.0, a.1), Point2::new(b.0, b.1))
    }

    #[test]
    fn crossing_segments_meet_in_one_point() {
        let tol = Tolerance::default();
        match segment_intersection(&seg((0.0, 0.0), (4.0, 4.0)), &seg((0.0, 4.0), (4.0, 0.0)), &tol) {
            SegmentIntersection::Point { point, t, u } => {
                assert!(point.coincides(Point2::new(2.0, 2.0), &tol));
                assert!((t - 0.5).abs() < 1e-12 && (u - 0.5).abs() < 1e-12);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn touching_and_disjoint_segments() {
        let tol = Tolerance::default();
        // T-touch: the end of b lies on a.
        assert!(matches!(
            segment_intersection(&seg((0.0, 0.0), (4.0, 0.0)), &seg((2.0, 3.0), (2.0, 0.00005)), &tol),
            SegmentIntersection::Point { .. }
        ));
        assert_eq!(
            segment_intersection(&seg((0.0, 0.0), (4.0, 0.0)), &seg((2.0, 3.0), (2.0, 0.1)), &tol),
            SegmentIntersection::None
        );
        // Parallel, apart.
        assert_eq!(
            segment_intersection(&seg((0.0, 0.0), (4.0, 0.0)), &seg((0.0, 1.0), (4.0, 1.0)), &tol),
            SegmentIntersection::None
        );
    }

    #[test]
    fn collinear_segments_overlap() {
        let tol = Tolerance::default();
        match segment_intersection(&seg((0.0, 0.0), (4.0, 0.0)), &seg((6.0, 0.0), (2.0, 0.0)), &tol) {
            SegmentIntersection::Overlap { segment, t, u } => {
                assert!(segment.start.coincides(Point2::new(2.0, 0.0), &tol));
                assert!(segment.end.coincides(Point2::new(4.0, 0.0), &tol));
                assert!((t.0 - 0.5).abs() < 1e-12 && (t.1 - 1.0).abs() < 1e-12);
                assert!((u.0 - 1.0).abs() < 1e-12 && (u.1 - 0.5).abs() < 1e-12);
            }
            other => panic!("unexpected {other:?}"),
        }
        // Collinear, end to end: one shared point.
        assert!(matches!(
            segment_intersection(&seg((0.0, 0.0), (4.0, 0.0)), &seg((4.0, 0.0), (6.0, 0.0)), &tol),
            SegmentIntersection::Point { .. }
        ));
    }

    #[test]
    fn line_meets_circle() {
        let r = line_circle(Point2::new(-5.0, 0.0), Vec2::X, Point2::ORIGIN, 2.0);
        assert_eq!(r.len(), 2);
        assert!((r[0] - 3.0).abs() < 1e-12 && (r[1] - 7.0).abs() < 1e-12);
        assert!(line_circle(Point2::new(-5.0, 3.0), Vec2::X, Point2::ORIGIN, 2.0).is_empty());
        assert_eq!(line_circle(Point2::new(-5.0, 2.0), Vec2::X, Point2::ORIGIN, 2.0).len(), 1);
    }

    #[test]
    fn circles_meet() {
        let p = circle_circle(Point2::ORIGIN, 5.0, Point2::new(8.0, 0.0), 5.0);
        assert_eq!(p.len(), 2);
        let tol = Tolerance::default();
        assert!(p.iter().any(|q| q.coincides(Point2::new(4.0, 3.0), &tol)));
        assert!(p.iter().any(|q| q.coincides(Point2::new(4.0, -3.0), &tol)));
        assert!(circle_circle(Point2::ORIGIN, 1.0, Point2::new(8.0, 0.0), 1.0).is_empty());
    }
}

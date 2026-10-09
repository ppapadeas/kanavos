//! Convex hull.

use std::cmp::Ordering;

use crate::d2::Point2;
use crate::polygon::Polygon2;
use crate::predicates::orient2d;

/// Convex hull of a point set, counter-clockwise, without collinear points (Andrew's monotone chain, see de Berg
/// et al., "Computational Geometry", 3rd ed., §1.1). Exact: uses the [`orient2d`] predicate.
///
/// Returns fewer than three vertices when all points are collinear or coincide (the two extreme points, or one
/// point). Non-finite points are ignored.
pub fn convex_hull(points: &[Point2]) -> Polygon2 {
    let mut pts: Vec<[f64; 2]> = points.iter().filter(|p| p.is_finite()).map(|p| p.xy()).collect();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup_by(|a, b| a[0].total_cmp(&b[0]) == Ordering::Equal && a[1].total_cmp(&b[1]) == Ordering::Equal);
    if pts.len() < 3 {
        return Polygon2::new(pts.iter().map(|p| Point2::new(p[0], p[1])).collect());
    }
    let mut lower: Vec<[f64; 2]> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 {
            let (Some(&a), Some(&b)) = (lower.get(lower.len() - 2), lower.last()) else { break };
            if orient2d(a, b, p) == Ordering::Greater {
                break;
            }
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f64; 2]> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 {
            let (Some(&a), Some(&b)) = (upper.get(upper.len() - 2), upper.last()) else { break };
            if orient2d(a, b, p) == Ordering::Greater {
                break;
            }
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    Polygon2::new(lower.into_iter().map(|p| Point2::new(p[0], p[1])).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hull_of_square_with_inner_points() {
        let pts: Vec<Point2> =
            [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (1.0, 1.0), (1.0, 0.0), (0.5, 1.5)]
                .iter()
                .map(|&(x, y)| Point2::new(x, y))
                .collect();
        let h = convex_hull(&pts);
        assert_eq!(h.len(), 4);
        assert!(h.is_ccw());
        assert!((h.area().as_m2() - 4.0).abs() < 1e-12);
    }

    #[test]
    fn collinear_points_give_a_segment() {
        let pts: Vec<Point2> = (0..5).map(|i| Point2::new(f64::from(i), f64::from(i))).collect();
        assert_eq!(convex_hull(&pts).len(), 2);
    }
}

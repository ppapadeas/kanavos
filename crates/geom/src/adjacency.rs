//! Shared boundaries between regions (e.g. the ridge and hip lines between roof planes).

use kanavos_units::Tolerance;

use crate::d2::Segment2;
use crate::intersect::{SegmentIntersection, segment_intersection};
use crate::polygon::PolygonWithHoles;

/// A stretch of boundary shared by two regions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharedEdge {
    /// Index of the first region.
    pub a: usize,
    /// Index of the second region (`a < b`).
    pub b: usize,
    /// The shared stretch.
    pub segment: Segment2,
}

/// Finds the boundary stretches that pairs of regions share: edges of different regions that are collinear and
/// overlap over more than `tol.length`.
///
/// Each region is a set of polygons. Cost O(E²) in the total number of edges, meant for small inputs such as
/// the planes of one roof.
pub fn shared_boundaries(regions: &[Vec<PolygonWithHoles>], tol: &Tolerance) -> Vec<SharedEdge> {
    let mut edges: Vec<(usize, Segment2)> = Vec::new();
    for (i, r) in regions.iter().enumerate() {
        for p in r {
            for ring in p.rings() {
                edges.extend(ring.edges().map(|e| (i, e)));
            }
        }
    }
    let eps = tol.length.as_m();
    let mut out = Vec::new();
    for (k, (ra, ea)) in edges.iter().enumerate() {
        for (rb, eb) in edges.iter().skip(k + 1) {
            if ra == rb {
                continue;
            }
            let (Some(ba), Some(bb)) = (
                crate::d2::Bbox2::from_points([ea.start, ea.end]),
                crate::d2::Bbox2::from_points([eb.start, eb.end]),
            ) else {
                continue;
            };
            if !ba.expanded(tol.length).intersects(&bb) {
                continue;
            }
            if let SegmentIntersection::Overlap { segment, .. } = segment_intersection(ea, eb, tol)
                && segment.length().as_m() > eps
            {
                let (a, b) = if ra < rb { (*ra, *rb) } else { (*rb, *ra) };
                out.push(SharedEdge { a, b, segment });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Point2, Polygon2};

    #[test]
    fn neighbours_share_one_edge() {
        let a: PolygonWithHoles = Polygon2::rectangle(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0)).into();
        let b: PolygonWithHoles = Polygon2::rectangle(Point2::new(2.0, 1.0), Point2::new(4.0, 5.0)).into();
        let c: PolygonWithHoles = Polygon2::rectangle(Point2::new(10.0, 0.0), Point2::new(11.0, 1.0)).into();
        let s = shared_boundaries(&[vec![a], vec![b], vec![c]], &Tolerance::default());
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].a, s[0].b), (0, 1));
        assert!((s[0].segment.length().as_m() - 1.0).abs() < 1e-12);
    }
}

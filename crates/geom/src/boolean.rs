//! Boolean operations on sets of polygons with holes: union, intersection, difference and exclusive or.
//!
//! The work is done by the `i_overlay` crate (MIT OR Apache-2.0) on an integer grid, which makes it robust
//! against coincident edges, touching vertices and self-intersections. Kanavos uses one fixed, project-wide grid
//! of 2⁻²⁷ m (about 7.5 nm) centred on the origin, with 64-bit integers, so that results of separate operations
//! that share a boundary (two pieces of a wall cut at the same line) land on exactly the same coordinates and
//! later unions close without hairline gaps. Input orientation does not matter:
//! every polygon is normalised (outer ring counter-clockwise, holes clockwise) and filled with the non-zero
//! rule, so overlapping polygons within one operand are merged.
//!
//! Output: outer rings counter-clockwise, holes clockwise, no collinear vertices, contours with an area below
//! [`MIN_AREA`] removed.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::ShapeType;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::core::solver::Solver;
use i_overlay::float::overlay::{FloatOverlay, OverlayOptions};
use i_overlay::i_float::adapter::FloatPointAdapter;
use i_overlay::i_float::float::rect::FloatRect;

use crate::GeomError;
use crate::clip::HalfPlane;
use crate::d2::{Bbox2, Point2};
use crate::polygon::{Polygon2, PolygonWithHoles};

/// Contours smaller than this (square metres) are dropped from results.
pub const MIN_AREA: f64 = 1e-10;

/// Largest coordinate magnitude accepted (metres). Far beyond any project; guards the integer conversion.
const MAX_COORD: f64 = 1e7;
/// Grid points per metre (a power of two, so snapping near the origin is exact).
const GRID_SCALE: f64 = 134_217_728.0; // 2^27

/// A Boolean operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BooleanOp {
    /// Area in either operand.
    Union,
    /// Area in both operands.
    Intersection,
    /// Area in the subject but not the clip.
    Difference,
    /// Area in exactly one operand.
    Xor,
}

type Contour = Vec<[f64; 2]>;

fn ring(p: &Polygon2) -> Contour {
    p.vertices.iter().map(|q| q.xy()).collect()
}

fn to_shapes(polys: &[PolygonWithHoles]) -> Result<Vec<Vec<Contour>>, GeomError> {
    let mut out = Vec::with_capacity(polys.len());
    for p in polys {
        if p.outer.len() < 3 {
            continue;
        }
        let o = p.oriented();
        let mut shape = Vec::with_capacity(1 + o.holes.len());
        for r in o.rings() {
            for q in &r.vertices {
                let [x, y] = q.xy();
                if !(x.is_finite() && y.is_finite()) || x.abs() > MAX_COORD || y.abs() > MAX_COORD {
                    return Err(GeomError::Invalid(format!("coordinate out of range: ({x}, {y})")));
                }
            }
            if r.len() >= 3 {
                shape.push(ring(r));
            }
        }
        out.push(shape);
    }
    Ok(out)
}

fn from_shapes(shapes: Vec<Vec<Contour>>) -> Vec<PolygonWithHoles> {
    let mut out = Vec::with_capacity(shapes.len());
    for shape in shapes {
        let mut rings =
            shape.into_iter().map(|c| Polygon2::new(c.into_iter().map(|[x, y]| Point2::new(x, y)).collect()));
        let Some(outer) = rings.next() else { continue };
        if outer.len() < 3 || outer.area().as_m2() <= MIN_AREA {
            continue;
        }
        let holes: Vec<Polygon2> = rings.filter(|h| h.len() >= 3 && h.area().as_m2() > MIN_AREA).collect();
        out.push(PolygonWithHoles {
            outer: outer.to_ccw(),
            holes: holes.into_iter().map(|h| h.to_cw()).collect(),
        });
    }
    out
}

fn options() -> OverlayOptions<f64, i64> {
    let mut o = OverlayOptions::<f64, i64>::default();
    o.min_output_area = MIN_AREA;
    o
}

/// An empty overlay on the fixed project-wide grid. Inputs must lie within ±[`MAX_COORD`] (checked by the callers).
fn new_overlay(capacity: usize) -> Result<FloatOverlay<[f64; 2], i64>, GeomError> {
    let rect = FloatRect::new(-MAX_COORD, MAX_COORD, -MAX_COORD, MAX_COORD)
        .map_err(|e| GeomError::Invalid(format!("Boolean grid: {e:?}")))?;
    let adapter = FloatPointAdapter::<[f64; 2], i64>::try_with_scale_conservative(rect, GRID_SCALE)
        .map_err(|e| GeomError::Invalid(format!("Boolean grid: {e:?}")))?;
    Ok(FloatOverlay::new_custom(adapter, options(), Solver::default(), capacity))
}

fn run(
    subject: &[PolygonWithHoles],
    clip: &[PolygonWithHoles],
    rule: OverlayRule,
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    let s = to_shapes(subject)?;
    let c = to_shapes(clip)?;
    if s.is_empty() && c.is_empty() {
        return Ok(Vec::new());
    }
    let capacity = s.iter().chain(c.iter()).flatten().map(Vec::len).sum();
    let mut overlay = new_overlay(capacity)?
        .unsafe_add_source(&s, ShapeType::Subject)
        .unsafe_add_source(&c, ShapeType::Clip);
    Ok(from_shapes(overlay.overlay(rule, FillRule::NonZero)))
}

/// Applies a Boolean operation to two sets of polygons.
///
/// Failure modes: non-finite coordinates or coordinates beyond 10⁷ m from the origin give [`GeomError::Invalid`]. Degenerate
/// rings (fewer than three vertices) are ignored.
pub fn boolean(
    subject: &[PolygonWithHoles],
    clip: &[PolygonWithHoles],
    op: BooleanOp,
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    let rule = match op {
        BooleanOp::Union => OverlayRule::Union,
        BooleanOp::Intersection => OverlayRule::Intersect,
        BooleanOp::Difference => OverlayRule::Difference,
        BooleanOp::Xor => OverlayRule::Xor,
    };
    // Shortcuts that keep empty operands cheap and exact.
    match op {
        BooleanOp::Intersection if subject.is_empty() || clip.is_empty() => return Ok(Vec::new()),
        BooleanOp::Difference if subject.is_empty() => return Ok(Vec::new()),
        _ => {}
    }
    run(subject, clip, rule)
}

/// Union of all polygons in the set (merges overlapping and touching polygons, resolves self-intersections).
pub fn union_all(polys: &[PolygonWithHoles]) -> Result<Vec<PolygonWithHoles>, GeomError> {
    if polys.is_empty() {
        return Ok(Vec::new());
    }
    let s = to_shapes(polys)?;
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let capacity = s.iter().flatten().map(Vec::len).sum();
    let mut overlay = new_overlay(capacity)?.unsafe_add_source(&s, ShapeType::Subject);
    Ok(from_shapes(overlay.overlay(OverlayRule::Subject, FillRule::NonZero)))
}

/// Union of two sets.
pub fn union(a: &[PolygonWithHoles], b: &[PolygonWithHoles]) -> Result<Vec<PolygonWithHoles>, GeomError> {
    boolean(a, b, BooleanOp::Union)
}

/// Intersection of two sets.
pub fn intersection(
    a: &[PolygonWithHoles],
    b: &[PolygonWithHoles],
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    boolean(a, b, BooleanOp::Intersection)
}

/// `a` minus `b`.
pub fn difference(
    a: &[PolygonWithHoles],
    b: &[PolygonWithHoles],
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    boolean(a, b, BooleanOp::Difference)
}

/// Area in exactly one of `a` and `b`.
pub fn xor(a: &[PolygonWithHoles], b: &[PolygonWithHoles]) -> Result<Vec<PolygonWithHoles>, GeomError> {
    boolean(a, b, BooleanOp::Xor)
}

/// Resolves self-intersections of rings drawn with the given orientation semantics: the result covers every
/// point with a positive winding number (counter-clockwise rings add, clockwise rings subtract). Used to clean
/// raw offset curves.
pub fn resolve_positive(rings: &[Polygon2]) -> Result<Vec<PolygonWithHoles>, GeomError> {
    let mut contours: Vec<Contour> = Vec::with_capacity(rings.len());
    for r in rings {
        if r.len() < 3 {
            continue;
        }
        for q in &r.vertices {
            let [x, y] = q.xy();
            if !(x.is_finite() && y.is_finite()) || x.abs() > MAX_COORD || y.abs() > MAX_COORD {
                return Err(GeomError::Invalid(format!("coordinate out of range: ({x}, {y})")));
            }
        }
        contours.push(ring(r));
    }
    if contours.is_empty() {
        return Ok(Vec::new());
    }
    let capacity = contours.iter().map(Vec::len).sum();
    let mut overlay = new_overlay(capacity)?.unsafe_add_source(&contours, ShapeType::Subject);
    Ok(from_shapes(overlay.overlay(OverlayRule::Subject, FillRule::Positive)))
}

/// Total area of a set of polygons with holes (assumed disjoint).
pub fn total_area(polys: &[PolygonWithHoles]) -> f64 {
    polys.iter().map(|p| p.area().as_m2()).sum()
}

/// Clips arbitrary polygons (convex or not, with holes) by a half-plane.
pub fn clip_by_half_plane(
    polys: &[PolygonWithHoles],
    hp: &HalfPlane,
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    let Some(bounds) = polys.iter().filter_map(PolygonWithHoles::bbox).reduce(|a, b| a.union(&b)) else {
        return Ok(Vec::new());
    };
    let big = hp.as_polygon(&bounds, 1.0);
    intersection(polys, &[big.into()])
}

/// Splits polygons by a line into the parts on its left and right (as seen along `hp.direction()`, the kept side
/// of `hp` is the left part).
pub fn split_by_half_plane(
    polys: &[PolygonWithHoles],
    hp: &HalfPlane,
) -> Result<(Vec<PolygonWithHoles>, Vec<PolygonWithHoles>), GeomError> {
    Ok((clip_by_half_plane(polys, hp)?, clip_by_half_plane(polys, &hp.flipped())?))
}

/// Bounding box of a set of polygons.
pub fn bbox_of(polys: &[PolygonWithHoles]) -> Option<Bbox2> {
    polys.iter().filter_map(PolygonWithHoles::bbox).reduce(|a, b| a.union(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PolygonWithHoles {
        Polygon2::rectangle(Point2::new(x0, y0), Point2::new(x1, y1)).into()
    }

    #[test]
    fn union_of_overlapping_squares() {
        let r = union(&[rect(0.0, 0.0, 2.0, 2.0)], &[rect(1.0, 1.0, 3.0, 3.0)]).unwrap();
        assert_eq!(r.len(), 1);
        assert!((total_area(&r) - 7.0).abs() < 1e-9);
        assert!(r[0].outer.is_ccw());
        assert_eq!(r[0].outer.len(), 8);
    }

    #[test]
    fn intersection_difference_xor() {
        let a = [rect(0.0, 0.0, 2.0, 2.0)];
        let b = [rect(1.0, 1.0, 3.0, 3.0)];
        assert!((total_area(&intersection(&a, &b).unwrap()) - 1.0).abs() < 1e-9);
        assert!((total_area(&difference(&a, &b).unwrap()) - 3.0).abs() < 1e-9);
        assert!((total_area(&xor(&a, &b).unwrap()) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn difference_makes_a_hole() {
        let r = difference(&[rect(0.0, 0.0, 10.0, 10.0)], &[rect(4.0, 4.0, 6.0, 6.0)]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].holes.len(), 1);
        assert!(!r[0].holes[0].is_ccw());
        assert!((total_area(&r) - 96.0).abs() < 1e-9);
    }

    #[test]
    fn walls_sharing_faces_merge_cleanly() {
        // Two wall bands meeting exactly at a corner: coincident edges.
        let a = rect(0.0, 0.0, 5.0, 0.25);
        let b = rect(4.75, 0.0, 5.0, 4.0);
        let r = union_all(&[a, b]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].outer.len(), 6);
        assert!((total_area(&r) - (5.0 * 0.25 + 0.25 * 3.75)).abs() < 1e-9);
    }

    #[test]
    fn ring_of_walls_encloses_a_hole() {
        let walls = [
            rect(0.0, 0.0, 10.0, 0.3),
            rect(9.7, 0.0, 10.0, 8.0),
            rect(0.0, 7.7, 10.0, 8.0),
            rect(0.0, 0.0, 0.3, 8.0),
        ];
        let r = union_all(&walls).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].holes.len(), 1);
        assert!((r[0].holes[0].area().as_m2() - 9.4 * 7.4).abs() < 1e-6);
    }

    #[test]
    fn clockwise_input_and_holes_are_normalised() {
        let mut outer = Polygon2::from_m(&[(0.0, 0.0), (0.0, 4.0), (4.0, 4.0), (4.0, 0.0)]); // clockwise
        outer.vertices.rotate_left(1);
        let hole = Polygon2::from_m(&[(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0)]); // counter-clockwise hole
        let p = PolygonWithHoles { outer, holes: vec![hole] };
        let r = union_all(&[p]).unwrap();
        assert!((total_area(&r) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn half_plane_clip_of_concave_polygon() {
        // A U shape cut across both arms gives two pieces.
        let u = PolygonWithHoles::from(Polygon2::from_m(&[
            (0.0, 0.0),
            (3.0, 0.0),
            (3.0, 3.0),
            (2.0, 3.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 3.0),
            (0.0, 3.0),
        ]));
        let hp = HalfPlane::new(Point2::new(0.0, 2.0), crate::d2::Vec2::Y).unwrap();
        let r = clip_by_half_plane(&[u], &hp).unwrap();
        assert_eq!(r.len(), 2);
        assert!((total_area(&r) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_non_finite_input() {
        let bad = PolygonWithHoles::from(Polygon2::from_m(&[(0.0, 0.0), (f64::NAN, 0.0), (1.0, 1.0)]));
        assert!(union_all(&[bad]).is_err());
    }
}

//! Polygon booleans and buffers used by the regulation checks.
//!
//! **Local stand-in.** These functions wrap the `i_overlay` crate (MIT OR Apache-2.0) until `kanavos-geom` offers
//! polygon booleans and offsets; then they become thin calls into it and the dependency goes. Callers in this
//! crate use only the functions below, never `i_overlay` directly.
//!
//! Tolerance: results are exact up to the overlay engine's integer snapping, which is far below the project
//! tolerance (0.1 mm) for building-sized coordinates. Rings with fewer than three finite vertices or with an area
//! below 1 mm² are dropped on input and output. Non-finite coordinates are never passed to the engine (it would
//! panic on them), so these functions never panic.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, StrokeStyle};
use kanavos_geom::{Point2, Polygon2, PolygonWithHoles};
use kanavos_units::Area;

type Contour = Vec<[f64; 2]>;
type Shape = Vec<Contour>;

/// Smallest ring area kept, m² (1 mm²).
const MIN_AREA: f64 = 1e-6;
/// Largest coordinate accepted, metres (a guard far above any national grid value).
const MAX_COORD: f64 = 1e9;

fn ring_area(c: &[[f64; 2]]) -> f64 {
    let n = c.len();
    let mut s = 0.0;
    for (i, a) in c.iter().enumerate() {
        if let Some(b) = c.get((i + 1) % n) {
            s += a[0] * b[1] - b[0] * a[1];
        }
    }
    s / 2.0
}

fn contour(p: &Polygon2, ccw: bool) -> Option<Contour> {
    let c: Contour = p
        .vertices
        .iter()
        .map(|q| q.xy())
        .filter(|[x, y]| x.is_finite() && y.is_finite() && x.abs() < MAX_COORD && y.abs() < MAX_COORD)
        .collect();
    if c.len() < 3 {
        return None;
    }
    let a = ring_area(&c);
    if a.abs() < MIN_AREA {
        return None;
    }
    Some(if (a > 0.0) == ccw { c } else { c.into_iter().rev().collect() })
}

fn shape(p: &PolygonWithHoles) -> Option<Shape> {
    let mut s = vec![contour(&p.outer, true)?];
    s.extend(p.holes.iter().filter_map(|h| contour(h, false)));
    Some(s)
}

fn shapes(polys: &[PolygonWithHoles]) -> Vec<Shape> {
    polys.iter().filter_map(shape).collect()
}

fn polygon(c: &[[f64; 2]]) -> Polygon2 {
    Polygon2::new(c.iter().map(|[x, y]| Point2::new(*x, *y)).collect())
}

fn from_shapes(result: Vec<Shape>) -> Vec<PolygonWithHoles> {
    result
        .into_iter()
        .filter_map(|s| {
            let mut rings = s.into_iter().filter(|c| c.len() >= 3 && ring_area(c).abs() >= MIN_AREA);
            let outer = rings.next()?;
            Some(PolygonWithHoles { outer: polygon(&outer), holes: rings.map(|c| polygon(&c)).collect() })
        })
        .collect()
}

fn run(subject: &[Shape], clip: &[Shape], rule: OverlayRule) -> Vec<PolygonWithHoles> {
    if subject.is_empty() && clip.is_empty() {
        return Vec::new();
    }
    let subject: Vec<Shape> = subject.to_vec();
    let clip: Vec<Shape> = clip.to_vec();
    let result = FloatOverlay::with_subj_and_clip(&subject, &clip).overlay(rule, FillRule::NonZero);
    from_shapes(result)
}

/// Union of polygons (outer rings counter-clockwise, holes clockwise in the result).
pub fn union(polys: &[PolygonWithHoles]) -> Vec<PolygonWithHoles> {
    let s = shapes(polys);
    if s.is_empty() {
        return Vec::new();
    }
    run(&s, &[], OverlayRule::Subject)
}

/// Intersection of two sets of polygons (each set is first taken as its union).
#[cfg_attr(not(test), allow(dead_code))]
pub fn intersection(a: &[PolygonWithHoles], b: &[PolygonWithHoles]) -> Vec<PolygonWithHoles> {
    let (sa, sb) = (shapes(a), shapes(b));
    if sa.is_empty() || sb.is_empty() {
        return Vec::new();
    }
    run(&sa, &sb, OverlayRule::Intersect)
}

/// `a` minus `b` (each set is first taken as its union).
pub fn difference(a: &[PolygonWithHoles], b: &[PolygonWithHoles]) -> Vec<PolygonWithHoles> {
    let sa = shapes(a);
    if sa.is_empty() {
        return Vec::new();
    }
    let sb = shapes(b);
    if sb.is_empty() {
        return union(a);
    }
    run(&sa, &sb, OverlayRule::Difference)
}

/// The region within `radius` metres of the segment `a`–`b` (a stadium with round ends), approximated by
/// straight segments no more than about 1 % of the radius away from the true arc.
pub fn buffer_segment(a: Point2, b: Point2, radius: f64) -> Vec<PolygonWithHoles> {
    let (pa, pb) = (a.xy(), b.xy());
    let finite = pa.iter().chain(pb.iter()).all(|v| v.is_finite() && v.abs() < MAX_COORD);
    if !finite || !(1e-4..=MAX_COORD).contains(&radius) {
        return Vec::new();
    }
    let path: Contour =
        if a.distance(b).as_m() < 1e-6 { vec![pa, [pa[0] + 1e-5, pa[1]]] } else { vec![pa, pb] };
    let style = StrokeStyle::new(2.0 * radius)
        .line_join(LineJoin::Round(0.1))
        .start_cap(LineCap::Round(0.1))
        .end_cap(LineCap::Round(0.1));
    from_shapes(path.stroke(style, false))
}

/// The outer rings only (holes filled), e.g. the outline of a storey from the union of its walls.
pub fn fill_holes(polys: &[PolygonWithHoles]) -> Vec<PolygonWithHoles> {
    let outers: Vec<PolygonWithHoles> =
        polys.iter().map(|p| PolygonWithHoles::from(p.outer.clone())).collect();
    union(&outers)
}

/// Total net area of a set of non-overlapping polygons.
pub fn total_area(polys: &[PolygonWithHoles]) -> Area {
    polys.iter().map(PolygonWithHoles::area).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PolygonWithHoles {
        Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]).into()
    }

    #[test]
    fn union_of_overlapping_rectangles() {
        let u = union(&[rect(0.0, 0.0, 4.0, 4.0), rect(2.0, 2.0, 6.0, 6.0)]);
        assert_eq!(u.len(), 1);
        assert!((total_area(&u).as_m2() - 28.0).abs() < 1e-6);
    }

    #[test]
    fn ring_of_walls_has_a_hole_until_filled() {
        let walls = [
            rect(0.0, 0.0, 10.0, 0.3),
            rect(9.7, 0.0, 10.0, 8.0),
            rect(0.0, 7.7, 10.0, 8.0),
            rect(0.0, 0.0, 0.3, 8.0),
        ];
        let u = union(&walls);
        assert_eq!(u.len(), 1);
        assert_eq!(u.first().map(|p| p.holes.len()), Some(1));
        assert!((total_area(&fill_holes(&u)).as_m2() - 80.0).abs() < 1e-6);
    }

    #[test]
    fn difference_and_intersection() {
        let a = [rect(0.0, 0.0, 10.0, 10.0)];
        let b = [rect(8.0, 0.0, 12.0, 10.0)];
        assert!((total_area(&difference(&a, &b)).as_m2() - 80.0).abs() < 1e-6);
        assert!((total_area(&intersection(&a, &b)).as_m2() - 20.0).abs() < 1e-6);
        assert!(intersection(&a, &[]).is_empty());
        assert!((total_area(&difference(&a, &[])).as_m2() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn buffer_of_a_segment_is_a_stadium() {
        let b = buffer_segment(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), 3.0);
        let area = total_area(&b).as_m2();
        let exact = 10.0 * 6.0 + std::f64::consts::PI * 9.0;
        assert!((area - exact).abs() < 0.01 * exact, "{area} vs {exact}");
    }

    #[test]
    fn invalid_input_is_ignored() {
        let bad =
            Polygon2::new(vec![Point2::new(f64::NAN, 0.0), Point2::new(1.0, 0.0), Point2::new(1.0, 1.0)]);
        assert!(union(&[bad.into()]).is_empty());
        assert!(buffer_segment(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), f64::INFINITY).is_empty());
    }
}

//! Polygon booleans and clipping used by the views.
//!
//! This is a local stand-in for the shared polygon booleans planned in `kanavos-geom`. It wraps `i_overlay`
//! (MIT) with a 64-bit integer engine, so results are exact on a grid far finer than the project tolerance.
//! When `kanavos-geom` offers union, difference and intersection, these functions become thin adapters.
//!
//! Shapes are lists of contours: the first contour is the outer ring (counter-clockwise), further contours are
//! holes (clockwise). Coordinates are raw metres.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use kanavos_geom::{Point2, Polygon2, PolygonWithHoles};

/// A closed ring of points without repeating the first one.
pub(crate) type Contour = Vec<[f64; 2]>;
/// An outer ring followed by its holes.
pub(crate) type Shape = Vec<Contour>;

/// Areas below this (square metres) are dropped from boolean results as numerical slivers.
const SLIVER_AREA: f64 = 1e-9;

/// Signed area of a ring (positive when counter-clockwise).
pub(crate) fn signed_area(c: &[[f64; 2]]) -> f64 {
    let n = c.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n {
        let (Some(a), Some(b)) = (c.get(i), c.get((i + 1) % n)) else { continue };
        s += a[0] * b[1] - b[0] * a[1];
    }
    s / 2.0
}

/// Net area of a shape (outer minus holes).
pub(crate) fn shape_area(s: &Shape) -> f64 {
    s.iter().map(|c| signed_area(c)).sum::<f64>().abs()
}

/// Perimeter of a shape (all rings).
pub(crate) fn shape_perimeter(s: &Shape) -> f64 {
    s.iter()
        .map(|c| {
            let n = c.len();
            (0..n)
                .filter_map(|i| Some((c.get(i)?, c.get((i + 1) % n)?)))
                .map(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]))
                .sum::<f64>()
        })
        .sum()
}

/// Bounding box `[xmin, ymin, xmax, ymax]` of a shape, or `None` when empty.
pub(crate) fn shape_bbox(s: &Shape) -> Option<[f64; 4]> {
    let mut it = s.iter().flatten();
    let first = it.next()?;
    let mut b = [first[0], first[1], first[0], first[1]];
    for p in it {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    Some(b)
}

/// Whether two boxes overlap (touching counts) after growing the first by `margin`.
pub(crate) fn boxes_overlap(a: &[f64; 4], b: &[f64; 4], margin: f64) -> bool {
    a[0] - margin <= b[2] && b[0] <= a[2] + margin && a[1] - margin <= b[3] && b[1] <= a[3] + margin
}

fn ring(p: &Polygon2) -> Contour {
    p.vertices.iter().map(|q| q.xy()).collect()
}

/// A model polygon with holes as a shape, normalised to outer counter-clockwise and holes clockwise.
pub(crate) fn shape_of(p: &PolygonWithHoles) -> Shape {
    let o = p.oriented();
    std::iter::once(&o.outer).chain(o.holes.iter()).map(ring).filter(|c| c.len() >= 3).collect()
}

/// A simple model polygon as a shape.
pub(crate) fn shape_of_polygon(p: &Polygon2) -> Shape {
    let mut c = ring(p);
    if signed_area(&c) < 0.0 {
        c.reverse();
    }
    if c.len() >= 3 { vec![c] } else { Vec::new() }
}

/// A shape back as a model polygon with holes, or `None` when it has no outer ring.
pub(crate) fn polygon_of(s: &Shape) -> Option<PolygonWithHoles> {
    let mut it = s.iter();
    let outer = it.next()?;
    let to_poly = |c: &Contour| Polygon2::new(c.iter().map(|q| Point2::new(q[0], q[1])).collect());
    Some(PolygonWithHoles { outer: to_poly(outer), holes: it.map(to_poly).collect() })
}

/// Contours with the outer ring of each shape counter-clockwise and holes clockwise, ready for the non-zero rule.
fn oriented(shapes: &[Shape]) -> Vec<Contour> {
    let mut out = Vec::new();
    for s in shapes {
        for (i, c) in s.iter().enumerate() {
            if c.len() < 3 {
                continue;
            }
            let a = signed_area(c);
            let mut c = c.clone();
            if (i == 0 && a < 0.0) || (i > 0 && a > 0.0) {
                c.reverse();
            }
            out.push(c);
        }
    }
    out
}

fn clean(shapes: Vec<Shape>) -> Vec<Shape> {
    shapes
        .into_iter()
        .map(|s| s.into_iter().filter(|c| c.len() >= 3).collect::<Shape>())
        .filter(|s: &Shape| !s.is_empty() && shape_area(s) > SLIVER_AREA)
        .collect()
}

fn run(subj: &[Contour], clip: &[Contour], rule: OverlayRule, fill: FillRule) -> Vec<Shape> {
    if subj.is_empty() && clip.is_empty() {
        return Vec::new();
    }
    let subj = subj.to_vec();
    let clip = clip.to_vec();
    clean(subj.overlay_as::<i64>(&clip, rule, fill))
}

/// Union of shapes (non-zero rule over consistently oriented rings).
pub(crate) fn union(shapes: &[Shape]) -> Vec<Shape> {
    run(&oriented(shapes), &[], OverlayRule::Subject, FillRule::NonZero)
}

/// `a` minus `b`.
pub(crate) fn difference(a: &[Shape], b: &[Shape]) -> Vec<Shape> {
    if b.is_empty() {
        return clean(a.to_vec());
    }
    run(&oriented(a), &oriented(b), OverlayRule::Difference, FillRule::NonZero)
}

/// Intersection of `a` and `b`.
pub(crate) fn intersection(a: &[Shape], b: &[Shape]) -> Vec<Shape> {
    run(&oriented(a), &oriented(b), OverlayRule::Intersect, FillRule::NonZero)
}

/// Shapes enclosed by loops under the even-odd rule (used to turn section loops into regions with holes).
pub(crate) fn even_odd(loops: &[Contour]) -> Vec<Shape> {
    run(loops, &[], OverlayRule::Subject, FillRule::EvenOdd)
}

/// An axis-aligned rectangle as a shape.
pub(crate) fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));
    vec![vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]]
}

/// Whether a point is inside a shape (even-odd over all rings; boundary points may go either way).
pub(crate) fn contains(s: &Shape, p: [f64; 2]) -> bool {
    let mut inside = false;
    for c in s {
        let n = c.len();
        for i in 0..n {
            let (Some(a), Some(b)) = (c.get(i), c.get((i + 1) % n)) else { continue };
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if p[0] < x {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_of_touching_squares_is_one_rectangle() {
        let u = union(&[rect(0.0, 0.0, 1.0, 1.0), rect(1.0, 0.0, 2.0, 1.0)]);
        assert_eq!(u.len(), 1);
        assert!((shape_area(&u[0]) - 2.0).abs() < 1e-9);
        assert_eq!(u[0].len(), 1);
    }

    #[test]
    fn difference_makes_a_hole() {
        let d = difference(&[rect(0.0, 0.0, 4.0, 4.0)], &[rect(1.0, 1.0, 2.0, 2.0)]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].len(), 2);
        assert!((shape_area(&d[0]) - 15.0).abs() < 1e-9);
        assert!(signed_area(&d[0][0]) > 0.0 && signed_area(&d[0][1]) < 0.0);
    }

    #[test]
    fn even_odd_nests_loops() {
        let outer = rect(0.0, 0.0, 4.0, 4.0).remove(0);
        let inner = rect(1.0, 1.0, 3.0, 3.0).remove(0);
        let s = even_odd(&[outer, inner]);
        assert_eq!(s.len(), 1);
        assert!((shape_area(&s[0]) - 12.0).abs() < 1e-9);
        assert!(contains(&s[0], [0.5, 0.5]) && !contains(&s[0], [2.0, 2.0]));
    }
}

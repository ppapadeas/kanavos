//! Polygons and polygons with holes.

use kanavos_units::{Area, Length, Tolerance};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::d2::{Bbox2, Point2, Segment2, Transform2, Vec2};

/// Where a point lies relative to a polygon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Containment {
    /// Strictly inside.
    Inside,
    /// On the boundary, within tolerance.
    Boundary,
    /// Strictly outside.
    Outside,
}

/// A simple polygon (no self-intersections), as a closed ring of vertices without repeating the first one.
/// Serialises as a list of `[x, y]` pairs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Polygon2 {
    /// Vertices in order. Orientation is not assumed unless a method says so.
    pub vertices: Vec<Point2>,
}

impl Polygon2 {
    /// Creates a polygon from vertices.
    pub fn new(vertices: Vec<Point2>) -> Self {
        Polygon2 { vertices }
    }

    /// Creates a polygon from `(x, y)` pairs in metres.
    pub fn from_m(coords: &[(f64, f64)]) -> Self {
        Polygon2 { vertices: coords.iter().map(|&(x, y)| Point2::new(x, y)).collect() }
    }

    /// An axis-aligned rectangle from two opposite corners, counter-clockwise.
    pub fn rectangle(a: Point2, b: Point2) -> Self {
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        Polygon2 {
            vertices: vec![
                Point2::from_lengths(x0, y0),
                Point2::from_lengths(x1, y0),
                Point2::from_lengths(x1, y1),
                Point2::from_lengths(x0, y1),
            ],
        }
    }

    /// Number of vertices.
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    /// Whether the polygon has no vertices.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Signed area (positive for counter-clockwise rings), by the shoelace formula.
    pub fn signed_area(&self) -> f64 {
        let n = self.vertices.len();
        if n < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for (i, a) in self.vertices.iter().enumerate() {
            let b = self.vertices.get((i + 1) % n).copied().unwrap_or(*a);
            sum += a.x.as_m() * b.y.as_m() - b.x.as_m() * a.y.as_m();
        }
        sum / 2.0
    }

    /// Unsigned area.
    pub fn area(&self) -> Area {
        Area::m2(self.signed_area().abs())
    }

    /// Whether the ring is counter-clockwise.
    pub fn is_ccw(&self) -> bool {
        self.signed_area() > 0.0
    }

    /// The polygon with counter-clockwise orientation.
    pub fn to_ccw(&self) -> Polygon2 {
        if self.signed_area() < 0.0 { self.reversed() } else { self.clone() }
    }

    /// The polygon with clockwise orientation.
    pub fn to_cw(&self) -> Polygon2 {
        if self.signed_area() > 0.0 { self.reversed() } else { self.clone() }
    }

    /// The ring in reverse order.
    pub fn reversed(&self) -> Polygon2 {
        let mut v = self.vertices.clone();
        v.reverse();
        Polygon2 { vertices: v }
    }

    /// Perimeter length.
    pub fn perimeter(&self) -> Length {
        self.edges().map(|e| e.length()).sum()
    }

    /// Edges in order, closing back to the first vertex.
    pub fn edges(&self) -> impl Iterator<Item = Segment2> + '_ {
        let n = self.vertices.len();
        let count = if n < 2 { 0 } else { n };
        (0..count).filter_map(move |i| {
            let a = *self.vertices.get(i)?;
            let b = *self.vertices.get((i + 1) % n)?;
            Some(Segment2::new(a, b))
        })
    }

    /// Edge `i` (from vertex `i` to vertex `i + 1`, wrapping).
    pub fn edge(&self, i: usize) -> Option<Segment2> {
        let n = self.vertices.len();
        if n < 2 {
            return None;
        }
        Some(Segment2::new(*self.vertices.get(i % n)?, *self.vertices.get((i + 1) % n)?))
    }

    /// Area centroid; falls back to the vertex average for degenerate rings.
    pub fn centroid(&self) -> Option<Point2> {
        let n = self.vertices.len();
        if n == 0 {
            return None;
        }
        let a = self.signed_area();
        if a.abs() < 1e-12 {
            let s = self.vertices.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec());
            return Some(Point2::new(s.x / n as f64, s.y / n as f64));
        }
        let (mut cx, mut cy) = (0.0, 0.0);
        for e in self.edges() {
            let [x0, y0] = e.start.xy();
            let [x1, y1] = e.end.xy();
            let f = x0 * y1 - x1 * y0;
            cx += (x0 + x1) * f;
            cy += (y0 + y1) * f;
        }
        Some(Point2::new(cx / (6.0 * a), cy / (6.0 * a)))
    }

    /// Bounding box, or `None` when empty.
    pub fn bbox(&self) -> Option<Bbox2> {
        Bbox2::from_points(self.vertices.iter().copied())
    }

    /// Classifies a point against the polygon (even-odd rule, boundary within tolerance).
    pub fn classify(&self, p: Point2, tol: &Tolerance) -> Containment {
        if self.edges().any(|e| tol.is_zero(e.distance_to(p))) {
            return Containment::Boundary;
        }
        let [px, py] = p.xy();
        let mut inside = false;
        for e in self.edges() {
            let [x0, y0] = e.start.xy();
            let [x1, y1] = e.end.xy();
            if (y0 > py) != (y1 > py) {
                let x = x0 + (py - y0) / (y1 - y0) * (x1 - x0);
                if px < x {
                    inside = !inside;
                }
            }
        }
        if inside { Containment::Inside } else { Containment::Outside }
    }

    /// Whether the point is inside or on the boundary.
    pub fn contains(&self, p: Point2, tol: &Tolerance) -> bool {
        self.classify(p, tol) != Containment::Outside
    }

    /// Whether the polygon is convex (collinear vertices allowed).
    pub fn is_convex(&self) -> bool {
        let n = self.vertices.len();
        if n < 3 {
            return false;
        }
        let mut sign: Option<bool> = None;
        for i in 0..n {
            let (Some(a), Some(b), Some(c)) =
                (self.vertices.get(i), self.vertices.get((i + 1) % n), self.vertices.get((i + 2) % n))
            else {
                return false;
            };
            let cross = (*b - *a).cross(*c - *b);
            if cross.abs() < 1e-12 {
                continue;
            }
            match sign {
                None => sign = Some(cross > 0.0),
                Some(positive) if positive != (cross > 0.0) => return false,
                Some(_) => {}
            }
        }
        true
    }

    /// Removes consecutive duplicate vertices and collinear middle vertices (within tolerance).
    pub fn simplified(&self, tol: &Tolerance) -> Polygon2 {
        let mut out: Vec<Point2> = Vec::with_capacity(self.vertices.len());
        for &p in &self.vertices {
            if out.last().is_none_or(|q| !q.coincides(p, tol)) {
                out.push(p);
            }
        }
        while out.len() > 1 && out.first().zip(out.last()).is_some_and(|(a, b)| a.coincides(*b, tol)) {
            out.pop();
        }
        let mut changed = true;
        while changed && out.len() > 3 {
            changed = false;
            let n = out.len();
            for i in 0..n {
                let (Some(&a), Some(&b), Some(&c)) =
                    (out.get((i + n - 1) % n), out.get(i), out.get((i + 1) % n))
                else {
                    break;
                };
                if tol.is_zero(Segment2::new(a, c).distance_to(b)) {
                    out.remove(i);
                    changed = true;
                    break;
                }
            }
        }
        Polygon2 { vertices: out }
    }

    /// The polygon transformed. Mirroring transforms keep the original orientation sense by reversing the ring.
    pub fn transformed(&self, t: &Transform2) -> Polygon2 {
        let mut v: Vec<Point2> = self.vertices.iter().map(|&p| t.apply(p)).collect();
        if t.is_mirroring() {
            v.reverse();
        }
        Polygon2 { vertices: v }
    }

    /// Whether the polygon has at least three vertices, finite coordinates and a non-zero area.
    pub fn is_valid(&self, tol: &Tolerance) -> bool {
        self.vertices.len() >= 3
            && self.vertices.iter().all(|p| p.is_finite())
            && self.area().as_m2() > tol.length.as_m() * tol.length.as_m()
    }
}

/// A polygon with holes, e.g. a slab outline with shafts. Serialises as `{"outer": [...], "holes": [[...]]}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct PolygonWithHoles {
    /// Outer boundary.
    pub outer: Polygon2,
    /// Holes, inside the outer boundary and not overlapping each other.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Polygon2>,
}

impl From<Polygon2> for PolygonWithHoles {
    fn from(outer: Polygon2) -> Self {
        PolygonWithHoles { outer, holes: Vec::new() }
    }
}

impl PolygonWithHoles {
    /// Net area: outer minus holes.
    pub fn area(&self) -> Area {
        let holes: Area = self.holes.iter().map(Polygon2::area).sum();
        self.outer.area() - holes
    }

    /// Outer perimeter plus hole perimeters.
    pub fn perimeter(&self) -> Length {
        self.outer.perimeter() + self.holes.iter().map(Polygon2::perimeter).sum()
    }

    /// Whether the point is inside the outer ring and not strictly inside a hole.
    pub fn contains(&self, p: Point2, tol: &Tolerance) -> bool {
        self.outer.contains(p, tol) && self.holes.iter().all(|h| h.classify(p, tol) != Containment::Inside)
    }

    /// Bounding box of the outer ring.
    pub fn bbox(&self) -> Option<Bbox2> {
        self.outer.bbox()
    }

    /// All rings: the outer one first.
    pub fn rings(&self) -> impl Iterator<Item = &Polygon2> {
        std::iter::once(&self.outer).chain(self.holes.iter())
    }

    /// The polygon transformed.
    pub fn transformed(&self, t: &Transform2) -> PolygonWithHoles {
        PolygonWithHoles {
            outer: self.outer.transformed(t),
            holes: self.holes.iter().map(|h| h.transformed(t)).collect(),
        }
    }

    /// Normalised orientation: outer counter-clockwise, holes clockwise.
    pub fn oriented(&self) -> PolygonWithHoles {
        PolygonWithHoles {
            outer: self.outer.to_ccw(),
            holes: self.holes.iter().map(Polygon2::to_cw).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_area_and_perimeter() {
        let p = Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)]);
        assert!((p.area().as_m2() - 80.0).abs() < 1e-9);
        assert!((p.perimeter().as_m() - 36.0).abs() < 1e-9);
        let c = p.centroid().unwrap();
        assert!(c.coincides(Point2::new(5.0, 4.0), &Tolerance::default()));
    }

    #[test]
    fn orientation_does_not_change_area() {
        let ccw = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)]);
        let cw = Polygon2::from_m(&[(0.0, 0.0), (4.0, 3.0), (4.0, 0.0)]);
        assert!(ccw.signed_area() > 0.0 && cw.signed_area() < 0.0);
        assert!((ccw.area().as_m2() - cw.area().as_m2()).abs() < 1e-12);
    }

    #[test]
    fn holes_are_subtracted() {
        let slab = PolygonWithHoles {
            outer: Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
            holes: vec![Polygon2::from_m(&[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)])],
        };
        assert!((slab.area().as_m2() - 96.0).abs() < 1e-9);
        let tol = Tolerance::default();
        assert!(!slab.contains(Point2::new(2.0, 2.0), &tol));
        assert!(slab.contains(Point2::new(5.0, 5.0), &tol));
    }

    #[test]
    fn classifies_points() {
        let tol = Tolerance::default();
        let l = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        assert_eq!(l.classify(Point2::new(1.0, 1.0), &tol), Containment::Inside);
        assert_eq!(l.classify(Point2::new(3.0, 3.0), &tol), Containment::Outside);
        assert_eq!(l.classify(Point2::new(4.0, 1.0), &tol), Containment::Boundary);
        assert!(!l.is_convex());
        assert!(Polygon2::rectangle(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)).is_convex());
    }

    #[test]
    fn simplifies_collinear_points() {
        let tol = Tolerance::default();
        let p = Polygon2::from_m(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (0.0, 0.0)]);
        assert_eq!(p.simplified(&tol).len(), 4);
    }
}

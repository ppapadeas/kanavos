//! 2D and 3D geometry primitives, predicates and polygon operations.
//!
//! Layer L1. See `docs/architecture.md`.

use domos_units::{Area, Length, Tolerance};

/// A point in the plane, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point2 {
    /// X coordinate (east in plan).
    pub x: Length,
    /// Y coordinate (north in plan).
    pub y: Length,
}

impl Point2 {
    /// Creates a point from metres.
    pub const fn new(x: f64, y: f64) -> Self {
        Point2 { x: Length::m(x), y: Length::m(y) }
    }
    /// Distance to another point.
    pub fn distance(self, other: Point2) -> Length {
        let dx = (other.x - self.x).as_m();
        let dy = (other.y - self.y).as_m();
        Length::m(dx.hypot(dy))
    }
    /// Whether two points coincide within tolerance.
    pub fn coincides(self, other: Point2, tol: &Tolerance) -> bool {
        tol.is_zero(self.distance(other))
    }
}

/// A straight segment between two points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment2 {
    /// Start point.
    pub start: Point2,
    /// End point.
    pub end: Point2,
}

impl Segment2 {
    /// Length of the segment.
    pub fn length(&self) -> Length {
        self.start.distance(self.end)
    }
}

/// A simple polygon (no self-intersections), as a closed ring of vertices without repeating the first one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Polygon2 {
    /// Vertices in order. Orientation is not assumed.
    pub vertices: Vec<Point2>,
}

impl Polygon2 {
    /// Creates a polygon from `(x, y)` pairs in metres.
    pub fn from_m(coords: &[(f64, f64)]) -> Self {
        Polygon2 { vertices: coords.iter().map(|&(x, y)| Point2::new(x, y)).collect() }
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

    /// Perimeter length.
    pub fn perimeter(&self) -> Length {
        let n = self.vertices.len();
        if n < 2 {
            return Length::ZERO;
        }
        self.vertices
            .iter()
            .enumerate()
            .map(|(i, a)| a.distance(self.vertices.get((i + 1) % n).copied().unwrap_or(*a)))
            .fold(Length::ZERO, |acc, l| acc + l)
    }
}

/// A polygon with holes, e.g. a slab outline with shafts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PolygonWithHoles {
    /// Outer boundary.
    pub outer: Polygon2,
    /// Holes, assumed inside the outer boundary and not overlapping each other.
    pub holes: Vec<Polygon2>,
}

impl PolygonWithHoles {
    /// Net area: outer minus holes.
    pub fn area(&self) -> Area {
        let holes: Area = self.holes.iter().map(Polygon2::area).sum();
        self.outer.area() - holes
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
    }
}

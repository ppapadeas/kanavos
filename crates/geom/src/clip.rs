//! Half-planes and clipping of convex polygons.

use crate::d2::{Bbox2, Point2, Vec2};
use crate::polygon::Polygon2;

/// A closed half-plane: the points `p` with `normal · (p - point) >= 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfPlane {
    /// A point on the boundary line.
    pub point: Point2,
    /// Unit normal pointing into the kept side.
    pub normal: Vec2,
}

impl HalfPlane {
    /// A half-plane from a boundary point and a normal pointing into the kept side; `None` for a zero normal.
    pub fn new(point: Point2, normal: Vec2) -> Option<HalfPlane> {
        Some(HalfPlane { point, normal: normal.normalized()? })
    }

    /// The half-plane to the left of the directed line `a → b`; `None` when `a` and `b` coincide.
    pub fn left_of(a: Point2, b: Point2) -> Option<HalfPlane> {
        HalfPlane::new(a, (b - a).perp())
    }

    /// Signed distance of a point from the boundary: positive inside.
    pub fn signed_distance(&self, p: Point2) -> f64 {
        self.normal.dot(p - self.point)
    }

    /// Whether a point is inside, allowing `eps` metres outside.
    pub fn contains(&self, p: Point2, eps: f64) -> bool {
        self.signed_distance(p) >= -eps
    }

    /// The complementary half-plane (same boundary, other side).
    pub fn flipped(&self) -> HalfPlane {
        HalfPlane { point: self.point, normal: -self.normal }
    }

    /// The half-plane moved by `distance` along its normal (positive shrinks the kept side).
    pub fn shifted(&self, distance: f64) -> HalfPlane {
        HalfPlane { point: self.point + self.normal * distance, normal: self.normal }
    }

    /// Direction of the boundary line (the normal turned clockwise, so the kept side is on its left).
    pub fn direction(&self) -> Vec2 {
        -self.normal.perp()
    }

    /// A large convex polygon (counter-clockwise) covering the kept side within `bounds` grown by `margin`.
    /// Used to clip arbitrary polygons with Boolean operations.
    pub fn as_polygon(&self, bounds: &Bbox2, margin: f64) -> Polygon2 {
        let big = bounds.width().as_m().max(bounds.height().as_m()) + 2.0 * margin.abs() + 1.0;
        let c = bounds.centre();
        // Foot of the centre on the boundary line, then a square on the kept side.
        let foot = c - self.normal * self.signed_distance(c);
        let along = self.direction();
        let reach = big * 2.0 + self.signed_distance(c).abs();
        Polygon2::new(vec![
            foot - along * reach,
            foot + along * reach,
            foot + along * reach + self.normal * reach,
            foot - along * reach + self.normal * reach,
        ])
    }
}

/// Clips a polygon by a half-plane (Sutherland–Hodgman, one plane).
///
/// Exact for convex polygons. For non-convex polygons the result can contain zero-width bridges where the
/// boundary crosses the line more than twice; use [`crate::clip_by_half_plane`] for those. Returns `None` when
/// less than `eps` square metres remain. Points within `eps` metres of the line count as inside.
pub fn clip_convex(poly: &Polygon2, hp: &HalfPlane, eps: f64) -> Option<Polygon2> {
    let n = poly.vertices.len();
    if n < 3 {
        return None;
    }
    let mut out: Vec<Point2> = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (Some(&a), Some(&b)) = (poly.vertices.get(i), poly.vertices.get((i + 1) % n)) else { continue };
        let da = hp.signed_distance(a);
        let db = hp.signed_distance(b);
        let ina = da >= -eps;
        let inb = db >= -eps;
        if ina {
            out.push(a);
        }
        if ina != inb {
            // Strict crossing (one side beyond eps): intersection point.
            let denom = da - db;
            if denom.abs() > 1e-300 {
                let t = (da / denom).clamp(0.0, 1.0);
                let p = a.lerp(b, t);
                if out.last().is_none_or(|q| (*q - p).length() > eps * 1e-3) {
                    out.push(p);
                }
            }
        }
    }
    // Remove consecutive duplicates.
    out.dedup_by(|a, b| (*a - *b).length() <= 1e-12);
    while out.len() > 1 && out.first().zip(out.last()).is_some_and(|(a, b)| (*a - *b).length() <= 1e-12) {
        out.pop();
    }
    let p = Polygon2::new(out);
    if p.len() >= 3 && p.area().as_m2() > eps * eps { Some(p) } else { None }
}

/// Clips a convex polygon by several half-planes in turn. Returns `None` when nothing remains.
pub fn clip_convex_all(poly: &Polygon2, planes: &[HalfPlane], eps: f64) -> Option<Polygon2> {
    let mut p = poly.clone();
    for hp in planes {
        p = clip_convex(&p, hp, eps)?;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_cut_in_half() {
        let sq = Polygon2::from_m(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let hp = HalfPlane::new(Point2::new(1.0, 0.0), Vec2::new(-1.0, 0.0)).unwrap();
        let c = clip_convex(&sq, &hp, 1e-9).unwrap();
        assert!((c.area().as_m2() - 2.0).abs() < 1e-12);
        assert!(c.is_ccw());
        // Fully outside.
        let away = HalfPlane::new(Point2::new(5.0, 0.0), Vec2::X).unwrap();
        assert!(clip_convex(&sq, &away, 1e-9).is_none());
        // Fully inside.
        let all = HalfPlane::new(Point2::new(-5.0, 0.0), Vec2::X).unwrap();
        assert!((clip_convex(&sq, &all, 1e-9).unwrap().area().as_m2() - 4.0).abs() < 1e-12);
    }

    #[test]
    fn diagonal_cut_and_left_of() {
        let sq = Polygon2::from_m(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let hp = HalfPlane::left_of(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0)).unwrap();
        let c = clip_convex(&sq, &hp, 1e-9).unwrap();
        assert!((c.area().as_m2() - 2.0).abs() < 1e-12);
        assert!(c.vertices.iter().all(|p| p.y.as_m() >= p.x.as_m() - 1e-12));
    }

    #[test]
    fn as_polygon_covers_kept_side() {
        let hp = HalfPlane::new(Point2::new(1.0, 1.0), Vec2::new(1.0, 1.0)).unwrap();
        let b = Bbox2 { min: Point2::new(0.0, 0.0), max: Point2::new(3.0, 3.0) };
        let poly = hp.as_polygon(&b, 1.0);
        let tol = kanavos_units::Tolerance::default();
        assert!(poly.contains(Point2::new(3.0, 3.0), &tol));
        assert!(!poly.contains(Point2::new(0.0, 0.0), &tol));
    }
}

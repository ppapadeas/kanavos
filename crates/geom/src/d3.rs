//! Spatial primitives. Raw metres, Z up.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::d2::Point2;

/// A point in space, metres. Serialises as `{"x":…, "y":…, "z":…}`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Point3 {
    /// X (east).
    pub x: f64,
    /// Y (north).
    pub y: f64,
    /// Z (up).
    pub z: f64,
}

/// A displacement in space, metres.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Vec3 {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

impl Point3 {
    /// The origin.
    pub const ORIGIN: Point3 = Point3 { x: 0.0, y: 0.0, z: 0.0 };
    /// Creates a point.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Point3 { x, y, z }
    }
    /// A plan point lifted to height `z`.
    pub fn from_plan(p: Point2, z: f64) -> Self {
        let [x, y] = p.xy();
        Point3 { x, y, z }
    }
    /// The plan projection.
    pub fn to_plan(self) -> Point2 {
        Point2::new(self.x, self.y)
    }
    /// Position vector.
    pub fn to_vec(self) -> Vec3 {
        Vec3 { x: self.x, y: self.y, z: self.z }
    }
    /// Coordinates as an array.
    pub fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
    /// Distance to another point.
    pub fn distance(self, o: Point3) -> f64 {
        (o - self).length()
    }
    /// Linear interpolation.
    pub fn lerp(self, o: Point3, t: f64) -> Point3 {
        self + (o - self) * t
    }
}

impl Vec3 {
    /// The zero vector.
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };
    /// Unit X.
    pub const X: Vec3 = Vec3 { x: 1.0, y: 0.0, z: 0.0 };
    /// Unit Y.
    pub const Y: Vec3 = Vec3 { x: 0.0, y: 1.0, z: 0.0 };
    /// Unit Z (up).
    pub const Z: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 1.0 };

    /// Creates a vector.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }
    /// Dot product.
    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    /// Cross product.
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3 {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }
    /// Euclidean length.
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    /// Unit vector, or `None` if shorter than `1e-12`.
    pub fn normalized(self) -> Option<Vec3> {
        let l = self.length();
        if l > 1e-12 && l.is_finite() { Some(self * (1.0 / l)) } else { None }
    }
    /// Coordinates as an array.
    pub fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
}

impl Add<Vec3> for Point3 {
    type Output = Point3;
    fn add(self, v: Vec3) -> Point3 {
        Point3 { x: self.x + v.x, y: self.y + v.y, z: self.z + v.z }
    }
}
impl Sub<Vec3> for Point3 {
    type Output = Point3;
    fn sub(self, v: Vec3) -> Point3 {
        Point3 { x: self.x - v.x, y: self.y - v.y, z: self.z - v.z }
    }
}
impl Sub for Point3 {
    type Output = Vec3;
    fn sub(self, o: Point3) -> Vec3 {
        Vec3 { x: self.x - o.x, y: self.y - o.y, z: self.z - o.z }
    }
}
impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x + o.x, y: self.y + o.y, z: self.z + o.z }
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        self.x += o.x;
        self.y += o.y;
        self.z += o.z;
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x - o.x, y: self.y - o.y, z: self.z - o.z }
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3 { x: -self.x, y: -self.y, z: -self.z }
    }
}
impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f64) -> Vec3 {
        Vec3 { x: self.x * k, y: self.y * k, z: self.z * k }
    }
}

/// An axis-aligned box in space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bbox3 {
    /// Minimum corner.
    pub min: Point3,
    /// Maximum corner.
    pub max: Point3,
}

impl Bbox3 {
    /// Bounding box of points, or `None` when empty.
    pub fn from_points<I: IntoIterator<Item = Point3>>(points: I) -> Option<Bbox3> {
        let mut it = points.into_iter();
        let first = it.next()?;
        let mut b = Bbox3 { min: first, max: first };
        for p in it {
            b.include(p);
        }
        Some(b)
    }
    /// Grows the box to include a point.
    pub fn include(&mut self, p: Point3) {
        self.min = Point3::new(self.min.x.min(p.x), self.min.y.min(p.y), self.min.z.min(p.z));
        self.max = Point3::new(self.max.x.max(p.x), self.max.y.max(p.y), self.max.z.max(p.z));
    }
    /// Union of two boxes.
    pub fn union(&self, o: &Bbox3) -> Bbox3 {
        let mut b = *self;
        b.include(o.min);
        b.include(o.max);
        b
    }
    /// Centre.
    pub fn centre(&self) -> Point3 {
        self.min.lerp(self.max, 0.5)
    }
    /// Length of the diagonal.
    pub fn diagonal(&self) -> f64 {
        self.min.distance(self.max)
    }
}

/// An oriented plane: points `p` with `normal · (p - origin) = 0`. The normal points to the "front" side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Plane {
    /// A point on the plane.
    pub origin: Point3,
    /// Unit normal.
    pub normal: Vec3,
}

impl Plane {
    /// A plane from a point and a (not necessarily unit) normal; `None` for a zero normal.
    pub fn new(origin: Point3, normal: Vec3) -> Option<Plane> {
        Some(Plane { origin, normal: normal.normalized()? })
    }
    /// A horizontal plane at height `z`, facing up.
    pub fn horizontal(z: f64) -> Plane {
        Plane { origin: Point3::new(0.0, 0.0, z), normal: Vec3::Z }
    }
    /// Signed distance of a point (positive on the front side).
    pub fn signed_distance(&self, p: Point3) -> f64 {
        self.normal.dot(p - self.origin)
    }
    /// Orthogonal projection of a point onto the plane.
    pub fn project(&self, p: Point3) -> Point3 {
        p - self.normal * self.signed_distance(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_product_is_right_handed() {
        let z = Vec3::X.cross(Vec3::Y);
        assert!((z - Vec3::Z).length() < 1e-12);
    }

    #[test]
    fn plane_distance() {
        let p = Plane::horizontal(3.0);
        assert!((p.signed_distance(Point3::new(1.0, 2.0, 5.0)) - 2.0).abs() < 1e-12);
        assert!((p.project(Point3::new(1.0, 2.0, 5.0)).z - 3.0).abs() < 1e-12);
    }
}

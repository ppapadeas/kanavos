//! Affine frames in space, for placements and representation items.

use kanavos_geom::{Point2, Point3, Vec2, Vec3};

/// An affine map `p ↦ origin + x·p.x + y·p.y + z·p.z`. For placements the axes are orthonormal; mapped items
/// may scale them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame3 {
    /// Image of the local X axis.
    pub x: Vec3,
    /// Image of the local Y axis.
    pub y: Vec3,
    /// Image of the local Z axis.
    pub z: Vec3,
    /// Image of the local origin.
    pub origin: Point3,
}

impl Default for Frame3 {
    fn default() -> Self {
        Frame3::IDENTITY
    }
}

impl Frame3 {
    /// The identity.
    pub const IDENTITY: Frame3 = Frame3 { x: Vec3::X, y: Vec3::Y, z: Vec3::Z, origin: Point3::ORIGIN };

    /// A placement from a location, an optional Z axis and an optional reference X direction, following the
    /// IFC rules for `IfcAxis2Placement3D`: Z defaults to +Z, X is the reference direction made orthogonal to
    /// Z (default +X, or +Y when Z is along X), Y = Z × X.
    pub fn from_axes(origin: Point3, axis: Option<Vec3>, ref_direction: Option<Vec3>) -> Frame3 {
        let z = axis.and_then(Vec3::normalized).unwrap_or(Vec3::Z);
        let candidate = ref_direction
            .and_then(Vec3::normalized)
            .unwrap_or_else(|| if (z - Vec3::X).length() < 1e-9 { Vec3::Y } else { Vec3::X });
        let projected = candidate - z * candidate.dot(z);
        let x = projected.normalized().unwrap_or_else(|| {
            let other = if z.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
            (other - z * other.dot(z)).normalized().unwrap_or(Vec3::X)
        });
        let y = z.cross(x);
        Frame3 { x, y, z, origin }
    }

    /// A horizontal placement: origin at `(at, elevation)`, X axis at `angle` (radians) from +X in plan.
    pub fn plan(at: Point2, elevation: f64, angle: f64) -> Frame3 {
        let (s, c) = angle.sin_cos();
        let [x, y] = at.xy();
        Frame3 {
            x: Vec3::new(c, s, 0.0),
            y: Vec3::new(-s, c, 0.0),
            z: Vec3::Z,
            origin: Point3::new(x, y, elevation),
        }
    }

    /// A translation.
    pub fn translation(v: Vec3) -> Frame3 {
        Frame3 { origin: Point3::ORIGIN + v, ..Frame3::IDENTITY }
    }

    /// Maps a point.
    pub fn apply(&self, p: Point3) -> Point3 {
        self.origin + self.apply_vec(p.to_vec())
    }

    /// Maps a vector (linear part only).
    pub fn apply_vec(&self, v: Vec3) -> Vec3 {
        self.x * v.x + self.y * v.y + self.z * v.z
    }

    /// Maps a point of the local XY plane.
    pub fn apply2(&self, x: f64, y: f64) -> Point3 {
        self.apply(Point3::new(x, y, 0.0))
    }

    /// Composition: `self.then(inner)` maps a point by `inner` first, then by `self`.
    pub fn then(&self, inner: &Frame3) -> Frame3 {
        Frame3 {
            x: self.apply_vec(inner.x),
            y: self.apply_vec(inner.y),
            z: self.apply_vec(inner.z),
            origin: self.apply(inner.origin),
        }
    }

    /// Determinant of the linear part.
    pub fn determinant(&self) -> f64 {
        self.x.dot(self.y.cross(self.z))
    }

    /// The inverse map, or `None` when the axes are degenerate.
    pub fn inverse(&self) -> Option<Frame3> {
        let det = self.determinant();
        if det.abs() < 1e-15 || !det.is_finite() {
            return None;
        }
        // Rows of the inverse are the cross products of the columns, divided by the determinant.
        let r0 = self.y.cross(self.z) * (1.0 / det);
        let r1 = self.z.cross(self.x) * (1.0 / det);
        let r2 = self.x.cross(self.y) * (1.0 / det);
        let x = Vec3::new(r0.x, r1.x, r2.x);
        let y = Vec3::new(r0.y, r1.y, r2.y);
        let z = Vec3::new(r0.z, r1.z, r2.z);
        let o = self.origin.to_vec();
        let origin = Point3::new(-r0.dot(o), -r1.dot(o), -r2.dot(o));
        Some(Frame3 { x, y, z, origin })
    }
}

/// A 2D affine map (for profile positions), `p ↦ origin + x·p.x + y·p.y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame2 {
    /// Image of the local X axis.
    pub x: Vec2,
    /// Image of the local Y axis.
    pub y: Vec2,
    /// Image of the origin.
    pub origin: Vec2,
}

impl Frame2 {
    /// The identity.
    pub const IDENTITY: Frame2 = Frame2 { x: Vec2::X, y: Vec2::Y, origin: Vec2::ZERO };

    /// A placement from a location and an optional X direction (`IfcAxis2Placement2D`).
    pub fn from_axes(origin: Vec2, ref_direction: Option<Vec2>) -> Frame2 {
        let x = ref_direction.and_then(Vec2::normalized).unwrap_or(Vec2::X);
        Frame2 { x, y: x.perp(), origin }
    }

    /// Maps a point.
    pub fn apply(&self, p: Vec2) -> Vec2 {
        self.origin + self.x * p.x + self.y * p.y
    }

    /// Composition: `inner` first, then `self`.
    pub fn then(&self, inner: &Frame2) -> Frame2 {
        Frame2 {
            x: self.x * inner.x.x + self.y * inner.x.y,
            y: self.x * inner.y.x + self.y * inner.y.y,
            origin: self.apply(inner.origin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point3, b: Point3) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn placement_axes_follow_ifc_rules() {
        let f = Frame3::from_axes(Point3::new(1.0, 2.0, 3.0), Some(Vec3::new(1.0, 0.0, 0.0)), Some(Vec3::Y));
        assert!((f.y - Vec3::Z).length() < 1e-12);
        let g = Frame3::from_axes(Point3::ORIGIN, None, Some(Vec3::new(1.0, 1.0, 5.0)));
        assert!(g.x.dot(Vec3::Z).abs() < 1e-12);
        assert!((g.determinant() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn composition_and_inverse() {
        let a = Frame3::plan(Point2::new(5.0, 1.0), 3.0, 0.7);
        let b = Frame3::from_axes(Point3::new(0.5, 0.0, 0.0), Some(Vec3::X), Some(Vec3::Y));
        let ab = a.then(&b);
        let p = Point3::new(0.3, -0.2, 1.1);
        assert!(close(ab.apply(p), a.apply(b.apply(p))));
        let inv = ab.inverse().unwrap();
        assert!(close(inv.apply(ab.apply(p)), p));
        let scaled = Frame3 { x: Vec3::X * 2.0, ..Frame3::IDENTITY };
        assert!(close(
            scaled.inverse().unwrap().apply(Point3::new(4.0, 1.0, 0.0)),
            Point3::new(2.0, 1.0, 0.0)
        ));
    }
}

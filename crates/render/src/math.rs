//! Double-precision matrix maths for cameras and projections.
//!
//! Conventions: right-handed world with Z up, column vectors (`p' = M · p`), column-major storage. View space looks
//! down −Z with +Y up. Clip space follows wgpu: x and y in [−1, 1], depth in [0, 1].
//!
//! Projections use **reverse-Z**: the near plane maps to depth 1 and the far plane (or infinity) to depth 0, so
//! a 32-bit float depth buffer keeps roughly constant relative precision at every distance. Depth tests in the
//! renderer therefore use "greater" comparisons.
//!
//! Everything here is `f64` so that cameras far from the origin (projected ΕΓΣΑ87 coordinates are in the
//! hundreds of kilometres) lose no precision. Matrices are converted to `f32` only after the scene origin has
//! been subtracted.

use kanavos_geom::{Plane, Point3, Vec3};

/// A 4×4 matrix of `f64`, column-major.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// Elements in column-major order: element (row `r`, column `c`) is at index `c * 4 + r`.
    m: [f64; 16],
}

impl Default for Mat4 {
    fn default() -> Self {
        Mat4::IDENTITY
    }
}

impl Mat4 {
    /// The identity matrix.
    pub const IDENTITY: Mat4 =
        Mat4 { m: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] };

    /// A matrix from its four columns.
    pub const fn from_cols(c0: [f64; 4], c1: [f64; 4], c2: [f64; 4], c3: [f64; 4]) -> Mat4 {
        Mat4 {
            m: [
                c0[0], c0[1], c0[2], c0[3], c1[0], c1[1], c1[2], c1[3], c2[0], c2[1], c2[2], c2[3], c3[0],
                c3[1], c3[2], c3[3],
            ],
        }
    }

    /// A matrix from its four rows (handy for writing matrices the way textbooks print them).
    pub const fn from_rows(r0: [f64; 4], r1: [f64; 4], r2: [f64; 4], r3: [f64; 4]) -> Mat4 {
        Mat4::from_cols(
            [r0[0], r1[0], r2[0], r3[0]],
            [r0[1], r1[1], r2[1], r3[1]],
            [r0[2], r1[2], r2[2], r3[2]],
            [r0[3], r1[3], r2[3], r3[3]],
        )
    }

    /// Element at `row`, `col` (both 0..4); out-of-range indices read as zero.
    pub fn at(&self, row: usize, col: usize) -> f64 {
        if row < 4 && col < 4 { self.m.get(col * 4 + row).copied().unwrap_or(0.0) } else { 0.0 }
    }

    /// The columns as arrays.
    pub fn cols(&self) -> [[f64; 4]; 4] {
        std::array::from_fn(|c| std::array::from_fn(|r| self.at(r, c)))
    }

    /// The columns converted to `f32`, ready for a GPU uniform buffer.
    pub fn to_cols_f32(&self) -> [[f32; 4]; 4] {
        std::array::from_fn(|c| std::array::from_fn(|r| self.at(r, c) as f32))
    }

    /// Matrix product `self · rhs` (apply `rhs` first, then `self`).
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let cols: [[f64; 4]; 4] = std::array::from_fn(|c| {
            std::array::from_fn(|r| (0..4).map(|k| self.at(r, k) * rhs.at(k, c)).sum())
        });
        Mat4::from_cols(cols[0], cols[1], cols[2], cols[3])
    }

    /// Transforms a homogeneous vector.
    pub fn transform(&self, v: [f64; 4]) -> [f64; 4] {
        std::array::from_fn(|r| (0..4).map(|k| self.at(r, k) * v.get(k).copied().unwrap_or(0.0)).sum())
    }

    /// Transforms a point (w = 1) and divides by the resulting w. `None` when w is (almost) zero.
    pub fn transform_point(&self, p: Point3) -> Option<Point3> {
        let [x, y, z, w] = self.transform([p.x, p.y, p.z, 1.0]);
        if w.abs() < 1e-300 || !w.is_finite() {
            return None;
        }
        Some(Point3::new(x / w, y / w, z / w))
    }

    /// Transforms a direction (w = 0).
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let [x, y, z, _] = self.transform([v.x, v.y, v.z, 0.0]);
        Vec3::new(x, y, z)
    }

    /// A translation.
    pub fn translation(v: Vec3) -> Mat4 {
        Mat4::from_rows(
            [1.0, 0.0, 0.0, v.x],
            [0.0, 1.0, 0.0, v.y],
            [0.0, 0.0, 1.0, v.z],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// World-to-view matrix of a camera at `eye` with orthonormal axes `right`, `up` and viewing direction
    /// `forward` (`right × up = −forward`).
    pub fn view(eye: Point3, right: Vec3, up: Vec3, forward: Vec3) -> Mat4 {
        let e = eye.to_vec();
        Mat4::from_rows(
            [right.x, right.y, right.z, -right.dot(e)],
            [up.x, up.y, up.z, -up.dot(e)],
            [-forward.x, -forward.y, -forward.z, forward.dot(e)],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// Reverse-Z perspective projection with an infinite far plane.
    ///
    /// `fov_y` is the full vertical field of view in radians, `aspect` is width / height and `near` the distance
    /// of the near plane (depth 1). Depth tends to 0 at infinity.
    pub fn perspective_reverse_z(fov_y: f64, aspect: f64, near: f64) -> Mat4 {
        let f = 1.0 / (0.5 * fov_y).tan();
        Mat4::from_rows(
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, 0.0, near],
            [0.0, 0.0, -1.0, 0.0],
        )
    }

    /// Reverse-Z orthographic projection of the view-space box `[−half_w, half_w] × [−half_h, half_h]` between
    /// distances `near` (depth 1) and `far` (depth 0) in front of the camera. `near` may be negative.
    pub fn orthographic_reverse_z(half_w: f64, half_h: f64, near: f64, far: f64) -> Mat4 {
        let range = far - near;
        Mat4::from_rows(
            [1.0 / half_w, 0.0, 0.0, 0.0],
            [0.0, 1.0 / half_h, 0.0, 0.0],
            [0.0, 0.0, 1.0 / range, far / range],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// Conventional (not reversed) orthographic projection: `near` maps to depth 0 and `far` to depth 1. Used for
    /// the sun's shadow map.
    pub fn orthographic(half_w: f64, half_h: f64, near: f64, far: f64) -> Mat4 {
        let range = far - near;
        Mat4::from_rows(
            [1.0 / half_w, 0.0, 0.0, 0.0],
            [0.0, 1.0 / half_h, 0.0, 0.0],
            [0.0, 0.0, -1.0 / range, -near / range],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// The inverse, or `None` for a singular matrix.
    pub fn inverse(&self) -> Option<Mat4> {
        let m = &self.m;
        let mut inv = [0.0f64; 16];
        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
            + m[9] * m[7] * m[14]
            + m[13] * m[6] * m[11]
            - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
            - m[8] * m[7] * m[14]
            - m[12] * m[6] * m[11]
            + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
            + m[8] * m[7] * m[13]
            + m[12] * m[5] * m[11]
            - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
            - m[8] * m[6] * m[13]
            - m[12] * m[5] * m[10]
            + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
            - m[9] * m[3] * m[14]
            - m[13] * m[2] * m[11]
            + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
            + m[8] * m[3] * m[14]
            + m[12] * m[2] * m[11]
            - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
            - m[8] * m[3] * m[13]
            - m[12] * m[1] * m[11]
            + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
            + m[8] * m[2] * m[13]
            + m[12] * m[1] * m[10]
            - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
            + m[5] * m[3] * m[14]
            + m[13] * m[2] * m[7]
            - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
            - m[4] * m[3] * m[14]
            - m[12] * m[2] * m[7]
            + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
            + m[4] * m[3] * m[13]
            + m[12] * m[1] * m[7]
            - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
            - m[4] * m[2] * m[13]
            - m[12] * m[1] * m[6]
            + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
            - m[5] * m[3] * m[10]
            - m[9] * m[2] * m[7]
            + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
            + m[4] * m[3] * m[10]
            + m[8] * m[2] * m[7]
            - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
            - m[4] * m[3] * m[9]
            - m[8] * m[1] * m[7]
            + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
            + m[4] * m[2] * m[9]
            + m[8] * m[1] * m[6]
            - m[8] * m[2] * m[5];
        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if !det.is_finite() || det.abs() < 1e-300 {
            return None;
        }
        let k = 1.0 / det;
        Some(Mat4 { m: inv.map(|v| v * k) })
    }
}

/// A ray (or, for parallel projections, a line) in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    /// Start point. For a perspective camera this is the eye.
    pub origin: Point3,
    /// Unit direction.
    pub dir: Vec3,
}

impl Ray {
    /// The point at parameter `t` (metres along the ray).
    pub fn at(&self, t: f64) -> Point3 {
        self.origin + self.dir * t
    }

    /// Parameter of the intersection with a plane, or `None` when the ray is (nearly) parallel to it.
    /// The parameter may be negative (intersection behind the origin).
    pub fn intersect_plane(&self, plane: &Plane) -> Option<f64> {
        let denom = plane.normal.dot(self.dir);
        if denom.abs() < 1e-12 {
            return None;
        }
        let t = plane.normal.dot(plane.origin - self.origin) / denom;
        t.is_finite().then_some(t)
    }

    /// Parameter of the point on the ray closest to `p`.
    pub fn closest_parameter(&self, p: Point3) -> f64 {
        (p - self.origin).dot(self.dir)
    }
}

/// Wraps an angle to (−π, π].
pub fn wrap_angle(a: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut x = a.rem_euclid(tau);
    if x > std::f64::consts::PI {
        x -= tau;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn close_m(a: &Mat4, b: &Mat4) -> bool {
        a.m.iter().zip(b.m.iter()).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn rows_and_cols_agree() {
        let m = Mat4::from_rows(
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        );
        assert!(close(m.at(0, 3), 4.0));
        assert!(close(m.at(3, 0), 13.0));
        assert!(m.cols()[1].iter().zip([2.0, 6.0, 10.0, 14.0]).all(|(a, b)| close(*a, b)));
    }

    #[test]
    fn inverse_round_trips() {
        let m = Mat4::from_rows(
            [2.0, 0.5, 0.0, 3.0],
            [0.0, 1.5, -1.0, 2.0],
            [1.0, 0.0, 4.0, -1.0],
            [0.0, 0.0, 0.0, 1.0],
        );
        let inv = m.inverse().unwrap();
        assert!(close_m(&m.mul(&inv), &Mat4::IDENTITY));
        assert!(close_m(&inv.mul(&m), &Mat4::IDENTITY));
        let singular =
            Mat4::from_rows([1.0, 2.0, 3.0, 4.0], [2.0, 4.0, 6.0, 8.0], [0.0; 4], [0.0, 0.0, 0.0, 1.0]);
        assert!(singular.inverse().is_none());
    }

    #[test]
    fn reverse_z_perspective_maps_near_to_one_and_far_to_zero() {
        let p = Mat4::perspective_reverse_z(60f64.to_radians(), 1.5, 0.1);
        let near = p.transform_point(Point3::new(0.0, 0.0, -0.1)).unwrap();
        assert!(close(near.z, 1.0));
        let far = p.transform_point(Point3::new(0.0, 0.0, -1e9)).unwrap();
        assert!(far.z > 0.0 && far.z < 1e-9);
        // Monotonic: nearer means larger depth.
        let a = p.transform_point(Point3::new(0.0, 0.0, -5.0)).unwrap();
        let b = p.transform_point(Point3::new(0.0, 0.0, -6.0)).unwrap();
        assert!(a.z > b.z);
        // The inverse exists even with an infinite far plane.
        let inv = p.inverse().unwrap();
        let back = inv.transform_point(a).unwrap();
        assert!(close(back.z, -5.0));
    }

    #[test]
    fn reverse_z_orthographic_range() {
        let o = Mat4::orthographic_reverse_z(10.0, 5.0, -2.0, 20.0);
        assert!(close(o.transform_point(Point3::new(10.0, 5.0, 2.0)).unwrap().z, 1.0));
        assert!(close(o.transform_point(Point3::new(0.0, 0.0, -20.0)).unwrap().z, 0.0));
        let c = o.transform_point(Point3::new(10.0, -5.0, -9.0)).unwrap();
        assert!(close(c.x, 1.0) && close(c.y, -1.0));
    }

    #[test]
    fn conventional_orthographic_range() {
        let o = Mat4::orthographic(1.0, 1.0, 1.0, 11.0);
        assert!(close(o.transform_point(Point3::new(0.0, 0.0, -1.0)).unwrap().z, 0.0));
        assert!(close(o.transform_point(Point3::new(0.0, 0.0, -11.0)).unwrap().z, 1.0));
    }

    #[test]
    fn view_matrix_puts_eye_at_origin_looking_down_minus_z() {
        let eye = Point3::new(10.0, -5.0, 2.0);
        let v = Mat4::view(eye, Vec3::X, Vec3::Z, Vec3::Y);
        let o = v.transform_point(eye).unwrap();
        assert!(close(o.x, 0.0) && close(o.y, 0.0) && close(o.z, 0.0));
        let ahead = v.transform_point(eye + Vec3::Y * 3.0).unwrap();
        assert!(close(ahead.z, -3.0));
        let up = v.transform_point(eye + Vec3::Z).unwrap();
        assert!(close(up.y, 1.0));
    }

    #[test]
    fn ray_plane_intersection() {
        let r = Ray { origin: Point3::new(0.0, 0.0, 10.0), dir: Vec3::new(0.0, 0.0, -1.0) };
        let t = r.intersect_plane(&Plane::horizontal(2.0)).unwrap();
        assert!(close(t, 8.0));
        assert!(close(r.at(t).z, 2.0));
        let flat = Ray { origin: Point3::ORIGIN, dir: Vec3::X };
        assert!(flat.intersect_plane(&Plane::horizontal(1.0)).is_none());
    }

    #[test]
    fn angles_wrap() {
        assert!(close(wrap_angle(3.0 * std::f64::consts::PI), std::f64::consts::PI));
        assert!(close(wrap_angle(-0.5), -0.5));
        assert!(close(wrap_angle(std::f64::consts::TAU + 0.25), 0.25));
    }
}

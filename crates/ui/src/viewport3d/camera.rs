//! The orbit camera of the 3D viewport and its projection, in plain `f64` maths (Z up, metres).

use std::ops::{Add, Mul, Neg, Sub};

/// A 3D vector or point.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct V3 {
    /// X (east).
    pub x: f64,
    /// Y (north).
    pub y: f64,
    /// Z (up).
    pub z: f64,
}

/// Shorthand constructor.
pub const fn v3(x: f64, y: f64, z: f64) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    /// Dot product.
    pub fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    /// Cross product.
    pub fn cross(self, o: V3) -> V3 {
        v3(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    /// Length.
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    /// Unit vector, or `None` for a (near) zero vector.
    pub fn normalized(self) -> Option<V3> {
        let l = self.length();
        (l > 1e-12 && l.is_finite()).then(|| self * (1.0 / l))
    }
    /// Component-wise minimum.
    pub fn min(self, o: V3) -> V3 {
        v3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    /// Component-wise maximum.
    pub fn max(self, o: V3) -> V3 {
        v3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
    /// Whether all components are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f64> for V3 {
    type Output = V3;
    fn mul(self, k: f64) -> V3 {
        v3(self.x * k, self.y * k, self.z * k)
    }
}

impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    /// Minimum corner.
    pub min: V3,
    /// Maximum corner.
    pub max: V3,
}

impl Aabb {
    /// The box around points, or `None` when there are none (or none finite).
    pub fn from_points(points: impl IntoIterator<Item = V3>) -> Option<Aabb> {
        let mut it = points.into_iter().filter(|p| p.is_finite());
        let first = it.next()?;
        let mut b = Aabb { min: first, max: first };
        for p in it {
            b.min = b.min.min(p);
            b.max = b.max.max(p);
        }
        Some(b)
    }
    /// Union.
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    /// Centre.
    pub fn centre(&self) -> V3 {
        (self.min + self.max) * 0.5
    }
    /// Half the diagonal.
    pub fn radius(&self) -> f64 {
        (self.max - self.min).length() * 0.5
    }
    /// Whether a point is inside (boundary included, with a small tolerance).
    pub fn contains(&self, p: V3, tol: f64) -> bool {
        p.x >= self.min.x - tol
            && p.x <= self.max.x + tol
            && p.y >= self.min.y - tol
            && p.y <= self.max.y + tol
            && p.z >= self.min.z - tol
            && p.z <= self.max.z + tol
    }
    /// Whether the boxes overlap.
    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x
            && o.min.x <= self.max.x
            && self.min.y <= o.max.y
            && o.min.y <= self.max.y
            && self.min.z <= o.max.z
            && o.min.z <= self.max.z
    }
    /// Whether `o` lies completely inside.
    pub fn contains_box(&self, o: &Aabb) -> bool {
        self.contains(o.min, 0.0) && self.contains(o.max, 0.0)
    }
}

/// The standard views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardView {
    /// From above, north up.
    Top,
    /// From the south.
    Front,
    /// From the east.
    Side,
    /// From south-east and above.
    Iso,
}

/// A camera orbiting a target point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    /// Point looked at and orbited.
    pub target: V3,
    /// Direction from the target to the eye in plan, radians counter-clockwise from +X.
    pub yaw: f64,
    /// Elevation of the eye above the target's horizontal plane, radians.
    pub pitch: f64,
    /// Distance from the target to the eye, metres.
    pub distance: f64,
    /// Vertical field of view, radians (also sets the height seen in parallel projection).
    pub fov: f64,
    /// Parallel projection instead of perspective.
    pub parallel: bool,
}

/// Largest pitch (radians), just short of looking straight down.
pub const MAX_PITCH: f64 = 89.5 * std::f64::consts::PI / 180.0;
/// Distance limits, metres.
pub const DISTANCE_LIMITS: (f64, f64) = (0.05, 100_000.0);

impl Default for OrbitCamera {
    fn default() -> Self {
        let mut c = OrbitCamera {
            target: v3(5.0, 4.0, 1.5),
            yaw: 0.0,
            pitch: 0.0,
            distance: 30.0,
            fov: 40f64.to_radians(),
            parallel: false,
        };
        c.set_standard(StandardView::Iso);
        c
    }
}

impl OrbitCamera {
    /// Unit vector from the target towards the eye.
    pub fn back(&self) -> V3 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        v3(cp * cy, cp * sy, sp)
    }

    /// Eye position.
    pub fn eye(&self) -> V3 {
        self.target + self.back() * self.distance
    }

    /// Right, up and forward unit vectors of the view.
    pub fn basis(&self) -> (V3, V3, V3) {
        let f = -self.back();
        let (sy, cy) = self.yaw.sin_cos();
        // Right stays horizontal; derived from the yaw so that looking straight down is well defined.
        let r = v3(-sy, cy, 0.0);
        let u = r.cross(f).normalized().unwrap_or(v3(0.0, 0.0, 1.0));
        (r, u, f)
    }

    /// Height of the view in metres at the target distance (the parallel projection's visible height).
    pub fn view_height(&self) -> f64 {
        2.0 * self.distance * (self.fov / 2.0).tan()
    }

    /// The projection for a viewport of `size` points centred at `centre` on screen.
    pub fn projection(&self, size: [f64; 2], centre: [f64; 2]) -> Projection {
        let (right, up, forward) = self.basis();
        let h = size[1].max(1.0);
        let focal = (h / 2.0) / (self.fov / 2.0).tan().max(1e-6);
        let scale = h / self.view_height().max(1e-9);
        // In parallel projection, put the eye far back so that nothing in front of the target is behind it.
        let eye = if self.parallel {
            self.target + self.back() * (self.distance * 4.0 + 1000.0)
        } else {
            self.eye()
        };
        let near = if self.parallel { 0.0 } else { (self.distance * 0.002).clamp(0.005, 1.0) };
        Projection { eye, right, up, forward, focal, scale, centre, near, parallel: self.parallel }
    }

    /// Orbits by a pointer drag of `(dx, dy)` points.
    pub fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Pans by a pointer drag of `(dx, dy)` points in a viewport `height` points high, so that the target plane
    /// follows the pointer.
    pub fn pan(&mut self, dx: f64, dy: f64, height: f64) {
        let (r, u, _) = self.basis();
        let k = self.view_height() / height.max(1.0);
        self.target = self.target - r * (dx * k) + u * (dy * k);
    }

    /// Zooms by `factor` (< 1 moves closer) keeping `focus` fixed on screen; without a focus, zooms about the
    /// target.
    pub fn zoom(&mut self, factor: f64, focus: Option<V3>) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let d = (self.distance * factor).clamp(DISTANCE_LIMITS.0, DISTANCE_LIMITS.1);
        let f = d / self.distance;
        if let Some(p) = focus.filter(|p| p.is_finite()) {
            self.target = p + (self.target - p) * f;
        }
        self.distance = d;
    }

    /// Frames a box so that it fills the view (with a small margin) for a viewport aspect ratio (width/height).
    pub fn fit(&mut self, b: &Aabb, aspect: f64) {
        let r = b.radius().max(0.5);
        let half_v = self.fov / 2.0;
        let half_h = (half_v.tan() * aspect.max(0.1)).atan();
        let half = half_v.min(half_h);
        self.target = b.centre();
        self.distance = (r / half.sin() * 1.05).clamp(DISTANCE_LIMITS.0, DISTANCE_LIMITS.1);
    }

    /// Turns to a standard view (the target and distance stay).
    pub fn set_standard(&mut self, v: StandardView) {
        let (yaw, pitch) = match v {
            StandardView::Top => (-90f64, MAX_PITCH.to_degrees()),
            StandardView::Front => (-90.0, 0.0),
            StandardView::Side => (0.0, 0.0),
            StandardView::Iso => (-55.0, 30.0),
        };
        self.yaw = yaw.to_radians();
        self.pitch = pitch.to_radians();
    }
}

/// A camera's projection onto a viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// Eye position.
    pub eye: V3,
    /// Screen right.
    pub right: V3,
    /// Screen up.
    pub up: V3,
    /// Viewing direction.
    pub forward: V3,
    /// Perspective focal length, points.
    pub focal: f64,
    /// Parallel scale, points per metre.
    pub scale: f64,
    /// Viewport centre on screen.
    pub centre: [f64; 2],
    /// Near plane distance (perspective), metres.
    pub near: f64,
    /// Parallel projection.
    pub parallel: bool,
}

impl Projection {
    /// View-space coordinates: right, up and depth along the viewing direction.
    pub fn view(&self, p: V3) -> V3 {
        let d = p - self.eye;
        v3(d.dot(self.right), d.dot(self.up), d.dot(self.forward))
    }

    /// Screen position of a view-space point (`None` when it is behind the near plane).
    pub fn screen_of_view(&self, q: V3) -> Option<[f64; 2]> {
        if self.parallel {
            return Some([self.centre[0] + q.x * self.scale, self.centre[1] - q.y * self.scale]);
        }
        if q.z < self.near {
            return None;
        }
        let k = self.focal / q.z;
        Some([self.centre[0] + q.x * k, self.centre[1] - q.y * k])
    }

    /// Screen position of a world point.
    pub fn project(&self, p: V3) -> Option<[f64; 2]> {
        self.screen_of_view(self.view(p))
    }

    /// The pick ray through a screen point: origin and unit direction.
    pub fn ray(&self, s: [f64; 2]) -> (V3, V3) {
        let x = s[0] - self.centre[0];
        let y = self.centre[1] - s[1];
        if self.parallel {
            let o = self.eye + self.right * (x / self.scale) + self.up * (y / self.scale);
            (o, self.forward)
        } else {
            let d = self.forward + self.right * (x / self.focal) + self.up * (y / self.focal);
            (self.eye, d.normalized().unwrap_or(self.forward))
        }
    }
}

/// Distance along a ray to a triangle (Möller–Trumbore), double-sided; `None` when missed.
pub fn ray_triangle(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-9).then_some(t)
}

/// Whether a ray hits a box (slab test).
pub fn ray_hits_box(o: V3, d: V3, b: &Aabb) -> bool {
    let mut t0 = f64::NEG_INFINITY;
    let mut t1 = f64::INFINITY;
    for (oo, dd, lo, hi) in
        [(o.x, d.x, b.min.x, b.max.x), (o.y, d.y, b.min.y, b.max.y), (o.z, d.z, b.min.z, b.max.z)]
    {
        if dd.abs() < 1e-15 {
            if oo < lo || oo > hi {
                return false;
            }
        } else {
            let (a, c) = ((lo - oo) / dd, (hi - oo) / dd);
            t0 = t0.max(a.min(c));
            t1 = t1.min(a.max(c));
        }
    }
    t1 >= t0.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn target_projects_to_the_centre() {
        for parallel in [false, true] {
            let c = OrbitCamera { parallel, ..OrbitCamera::default() };
            let p = c.projection([800.0, 600.0], [400.0, 300.0]);
            let s = p.project(c.target).unwrap();
            assert!(close(s[0], 400.0) && close(s[1], 300.0), "{s:?}");
        }
    }

    #[test]
    fn up_is_up_on_screen() {
        let c = OrbitCamera::default();
        let p = c.projection([800.0, 600.0], [400.0, 300.0]);
        let a = p.project(c.target).unwrap();
        let b = p.project(c.target + v3(0.0, 0.0, 1.0)).unwrap();
        assert!(b[1] < a[1]);
    }

    #[test]
    fn top_view_has_north_up_and_east_right() {
        let mut c = OrbitCamera::default();
        c.set_standard(StandardView::Top);
        let p = c.projection([800.0, 600.0], [400.0, 300.0]);
        let o = p.project(c.target).unwrap();
        let n = p.project(c.target + v3(0.0, 1.0, 0.0)).unwrap();
        let e = p.project(c.target + v3(1.0, 0.0, 0.0)).unwrap();
        assert!(n[1] < o[1] - 1.0 && (n[0] - o[0]).abs() < 1.0);
        assert!(e[0] > o[0] + 1.0);
    }

    #[test]
    fn zoom_keeps_the_focus_on_screen() {
        for parallel in [false, true] {
            let mut c = OrbitCamera { parallel, ..OrbitCamera::default() };
            let focus = c.target + v3(2.0, -1.0, 0.5);
            let before = c.projection([800.0, 600.0], [400.0, 300.0]).project(focus).unwrap();
            c.zoom(0.5, Some(focus));
            let after = c.projection([800.0, 600.0], [400.0, 300.0]).project(focus).unwrap();
            assert!((before[0] - after[0]).abs() < 1e-6 && (before[1] - after[1]).abs() < 1e-6, "{parallel}");
        }
    }

    #[test]
    fn pan_moves_the_target_plane_with_the_pointer() {
        let mut c = OrbitCamera { parallel: true, ..OrbitCamera::default() };
        let p0 = c.projection([800.0, 600.0], [400.0, 300.0]);
        let grabbed = c.target;
        let s0 = p0.project(grabbed).unwrap();
        c.pan(40.0, -25.0, 600.0);
        let s1 = c.projection([800.0, 600.0], [400.0, 300.0]).project(grabbed).unwrap();
        assert!((s1[0] - s0[0] - 40.0).abs() < 1e-6 && (s1[1] - s0[1] + 25.0).abs() < 1e-6, "{s0:?} {s1:?}");
    }

    #[test]
    fn rays_go_through_their_pixel() {
        for parallel in [false, true] {
            let c = OrbitCamera { parallel, ..OrbitCamera::default() };
            let p = c.projection([800.0, 600.0], [400.0, 300.0]);
            let (o, d) = p.ray([523.0, 177.0]);
            let q = o + d * 17.0;
            let s = p.project(q).unwrap();
            assert!((s[0] - 523.0).abs() < 1e-6 && (s[1] - 177.0).abs() < 1e-6);
        }
    }

    #[test]
    fn fit_shows_the_whole_box() {
        let mut c = OrbitCamera::default();
        let b = Aabb { min: v3(0.0, 0.0, 0.0), max: v3(20.0, 10.0, 6.0) };
        c.fit(&b, 1.5);
        let p = c.projection([900.0, 600.0], [450.0, 300.0]);
        for x in [b.min.x, b.max.x] {
            for y in [b.min.y, b.max.y] {
                for z in [b.min.z, b.max.z] {
                    let s = p.project(v3(x, y, z)).unwrap();
                    assert!(s[0] > 0.0 && s[0] < 900.0 && s[1] > 0.0 && s[1] < 600.0, "{s:?}");
                }
            }
        }
    }

    #[test]
    fn ray_triangle_hits_and_misses() {
        let (a, b, c) = (v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0));
        let t = ray_triangle(v3(0.2, 0.2, 5.0), v3(0.0, 0.0, -1.0), a, b, c).unwrap();
        assert!(close(t, 5.0));
        assert!(ray_triangle(v3(0.8, 0.8, 5.0), v3(0.0, 0.0, -1.0), a, b, c).is_none());
        assert!(ray_triangle(v3(0.2, 0.2, 5.0), v3(0.0, 0.0, 1.0), a, b, c).is_none());
        let bx = Aabb { min: v3(0.0, 0.0, 0.0), max: v3(1.0, 1.0, 1.0) };
        assert!(ray_hits_box(v3(0.5, 0.5, 5.0), v3(0.0, 0.0, -1.0), &bx));
        assert!(!ray_hits_box(v3(2.5, 0.5, 5.0), v3(0.0, 0.0, -1.0), &bx));
    }
}

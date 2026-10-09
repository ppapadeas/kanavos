//! The orbiting viewport camera: navigation, standard views, projection and picking rays.
//!
//! Pure maths, no GPU. World coordinates are metres, right-handed, Z up, X east, Y north, the same as
//! `kanavos-geom`. Screen coordinates are pixels with the origin at the top-left corner and y pointing down; any
//! pixel unit works (physical or logical) as long as the cursor and the [`Viewport`] use the same one.
//!
//! The camera is described by a **target** (the orbit pivot), a **yaw** (compass direction of the eye seen from
//! the target, counter-clockwise from +X), a **pitch** (elevation of the eye above the target's horizon, positive
//! when looking down) and a **distance**. Perspective and parallel projections share the same parameters: the
//! parallel view shows the same height at the target as the perspective view (`2 · distance · tan(fov_y / 2)`),
//! so switching between them keeps the framing.

use kanavos_geom::{Bbox3, Plane, Point3, Vec3};

use crate::math::{Mat4, Ray, wrap_angle};

/// Shortest allowed orbit distance, metres.
pub const MIN_DISTANCE: f64 = 0.05;
/// Longest allowed orbit distance, metres.
pub const MAX_DISTANCE: f64 = 1.0e6;
/// Default vertical field of view, radians (45°).
pub const DEFAULT_FOV_Y: f64 = std::f64::consts::FRAC_PI_4;
/// Orbit speed used by [`OrbitCamera::orbit_pixels`], radians per pixel.
pub const ORBIT_RADIANS_PER_PIXEL: f64 = 0.006;

/// Pitch limit: straight down or straight up.
const PITCH_LIMIT: f64 = std::f64::consts::FRAC_PI_2;
/// Pitch of a true isometric view: `atan(1/√2)` ≈ 35.26°.
const ISO_PITCH: f64 = 0.615_479_708_670_387_3;

/// How the camera projects the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Projection {
    /// Perspective with the camera's vertical field of view.
    #[default]
    Perspective,
    /// Parallel (orthographic) projection showing `2 · distance · tan(fov_y / 2)` metres vertically.
    Parallel,
}

/// The size of the area the camera draws into, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Width in pixels.
    pub width: f64,
    /// Height in pixels.
    pub height: f64,
}

impl Viewport {
    /// A viewport; sizes below one pixel are raised to one.
    pub fn new(width: f64, height: f64) -> Viewport {
        Viewport { width: sanitize_size(width), height: sanitize_size(height) }
    }

    /// Width / height.
    pub fn aspect(&self) -> f64 {
        sanitize_size(self.width) / sanitize_size(self.height)
    }

    /// Normalised device coordinates (x right, y up, both in [−1, 1]) of a pixel position.
    pub fn to_ndc(&self, x: f64, y: f64) -> [f64; 2] {
        [2.0 * x / sanitize_size(self.width) - 1.0, 1.0 - 2.0 * y / sanitize_size(self.height)]
    }

    /// Pixel position of normalised device coordinates.
    pub fn from_ndc(&self, ndc: [f64; 2]) -> [f64; 2] {
        [(ndc[0] + 1.0) * 0.5 * self.width, (1.0 - ndc[1]) * 0.5 * self.height]
    }
}

fn sanitize_size(v: f64) -> f64 {
    if v.is_finite() && v >= 1.0 { v } else { 1.0 }
}

/// Predefined viewing directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardView {
    /// Looking straight down, north up.
    Top,
    /// Looking straight up from below, north up.
    Bottom,
    /// Looking north (from the south).
    Front,
    /// Looking south (from the north).
    Back,
    /// Looking east (from the west).
    Left,
    /// Looking west (from the east).
    Right,
    /// Isometric view from the south-west.
    IsoSouthWest,
    /// Isometric view from the south-east.
    IsoSouthEast,
    /// Isometric view from the north-east.
    IsoNorthEast,
    /// Isometric view from the north-west.
    IsoNorthWest,
}

impl StandardView {
    /// Every standard view, in menu order.
    pub const ALL: [StandardView; 10] = [
        StandardView::Top,
        StandardView::Bottom,
        StandardView::Front,
        StandardView::Back,
        StandardView::Left,
        StandardView::Right,
        StandardView::IsoSouthWest,
        StandardView::IsoSouthEast,
        StandardView::IsoNorthEast,
        StandardView::IsoNorthWest,
    ];

    /// `(yaw, pitch)` in radians.
    pub fn angles(self) -> (f64, f64) {
        use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        match self {
            StandardView::Top => (-FRAC_PI_2, PITCH_LIMIT),
            StandardView::Bottom => (-FRAC_PI_2, -PITCH_LIMIT),
            StandardView::Front => (-FRAC_PI_2, 0.0),
            StandardView::Back => (FRAC_PI_2, 0.0),
            StandardView::Left => (PI, 0.0),
            StandardView::Right => (0.0, 0.0),
            StandardView::IsoSouthWest => (-3.0 * FRAC_PI_4, ISO_PITCH),
            StandardView::IsoSouthEast => (-FRAC_PI_4, ISO_PITCH),
            StandardView::IsoNorthEast => (FRAC_PI_4, ISO_PITCH),
            StandardView::IsoNorthWest => (3.0 * FRAC_PI_4, ISO_PITCH),
        }
    }
}

/// Orthonormal camera axes in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraBasis {
    /// Screen right.
    pub right: Vec3,
    /// Screen up.
    pub up: Vec3,
    /// Viewing direction (into the screen).
    pub forward: Vec3,
}

/// Near and far distances of the depth range, metres in front of the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthRange {
    /// Near plane distance. Positive for perspective; may be negative for parallel projections.
    pub near: f64,
    /// Far plane distance; `None` means infinity (perspective only).
    pub far: Option<f64>,
}

/// A world point projected to the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    /// Pixels from the left edge.
    pub x: f64,
    /// Pixels from the top edge.
    pub y: f64,
    /// Distance in front of the eye along the viewing direction, metres.
    pub depth: f64,
}

/// One world axis as seen on screen, for drawing an orientation gizmo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoAxis {
    /// The world axis (unit X, Y or Z).
    pub axis: Vec3,
    /// Screen direction of the axis, x right and y down, with length in [0, 1] (1 when the axis lies in the
    /// screen plane, 0 when it points at the viewer).
    pub screen: [f64; 2],
    /// Component along the viewing direction: negative when the axis points towards the viewer. Draw axes in
    /// decreasing order of this value so the nearest is on top.
    pub depth: f64,
}

/// An orbiting camera with perspective or parallel projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    /// Orbit pivot and centre of the view, world metres.
    pub target: Point3,
    /// Compass direction from the target to the eye, radians counter-clockwise from +X.
    pub yaw: f64,
    /// Elevation of the eye above the target's horizon, radians in [−π/2, π/2]; positive looks down.
    pub pitch: f64,
    /// Eye-to-target distance, metres.
    pub distance: f64,
    /// Vertical field of view, radians.
    pub fov_y: f64,
    /// Projection kind.
    pub projection: Projection,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let (yaw, pitch) = StandardView::IsoSouthWest.angles();
        OrbitCamera {
            target: Point3::ORIGIN,
            yaw,
            pitch,
            distance: 30.0,
            fov_y: DEFAULT_FOV_Y,
            projection: Projection::Perspective,
        }
    }
}

impl OrbitCamera {
    /// A perspective camera looking at `target` from a standard direction.
    pub fn looking_at(target: Point3, view: StandardView, distance: f64) -> OrbitCamera {
        let mut c = OrbitCamera { target, ..OrbitCamera::default() };
        c.set_view(view);
        c.distance = clamp_distance(distance);
        c
    }

    /// Unit vector from the target towards the eye.
    fn offset_dir(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT).sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    /// Camera axes. Well defined for every pitch, including straight down and straight up.
    pub fn basis(&self) -> CameraBasis {
        let (sy, cy) = self.yaw.sin_cos();
        let forward = -self.offset_dir();
        let right = Vec3::new(-sy, cy, 0.0);
        let up = right.cross(forward);
        CameraBasis { right, up, forward }
    }

    /// Eye position, world metres.
    pub fn eye(&self) -> Point3 {
        self.target + self.offset_dir() * self.distance
    }

    /// Height of the visible area at the target distance, metres.
    pub fn view_height(&self) -> f64 {
        2.0 * self.distance * self.tan_half_fov()
    }

    fn tan_half_fov(&self) -> f64 {
        let fov = if self.fov_y.is_finite() { self.fov_y.clamp(1e-3, 3.0) } else { DEFAULT_FOV_Y };
        (0.5 * fov).tan()
    }

    /// Sets yaw and pitch to a standard view, keeping target and distance.
    pub fn set_view(&mut self, view: StandardView) {
        let (yaw, pitch) = view.angles();
        self.yaw = yaw;
        self.pitch = pitch;
    }

    /// World-to-view matrix.
    pub fn view_matrix(&self) -> Mat4 {
        self.view_matrix_relative(Point3::ORIGIN)
    }

    /// World-to-view matrix for geometry stored relative to `origin` (render space): it maps `p − origin` to
    /// view space. The subtraction happens in `f64`, so the result is exact even for large world coordinates.
    pub fn view_matrix_relative(&self, origin: Point3) -> Mat4 {
        let b = self.basis();
        let eye = Point3::ORIGIN + (self.eye() - origin);
        Mat4::view(eye, b.right, b.up, b.forward)
    }

    /// Depth range for the given scene bounds (world). Perspective uses an infinite far plane and a near plane
    /// proportional to the distance; parallel projection encloses the scene bounds (or a generous range around
    /// the target when there are none).
    pub fn depth_range(&self, scene_bounds: Option<&Bbox3>) -> DepthRange {
        match self.projection {
            Projection::Perspective => {
                DepthRange { near: (self.distance * 0.002).clamp(0.001, 0.25), far: None }
            }
            Projection::Parallel => {
                let f = self.basis().forward;
                let eye = self.eye();
                let (lo, hi) = match scene_bounds {
                    Some(b) => {
                        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                        for c in bbox_corners(b) {
                            let s = (c - eye).dot(f);
                            lo = lo.min(s);
                            hi = hi.max(s);
                        }
                        (lo, hi)
                    }
                    None => (-10.0 * self.distance, 10.0 * self.distance),
                };
                let margin = 0.01 * (hi - lo).abs() + 0.05;
                let (near, far) = (lo - margin, hi + margin);
                if near.is_finite() && far.is_finite() && far > near {
                    DepthRange { near, far: Some(far) }
                } else {
                    DepthRange { near: -10.0 * self.distance, far: Some(10.0 * self.distance) }
                }
            }
        }
    }

    /// Projection matrix (reverse-Z) for a viewport aspect ratio and depth range.
    pub fn projection_matrix(&self, aspect: f64, depth: DepthRange) -> Mat4 {
        let aspect = if aspect.is_finite() && aspect > 1e-6 { aspect } else { 1.0 };
        match self.projection {
            Projection::Perspective => {
                Mat4::perspective_reverse_z(2.0 * self.tan_half_fov().atan(), aspect, depth.near.max(1e-6))
            }
            Projection::Parallel => {
                let half_h = 0.5 * self.view_height();
                let far = depth.far.unwrap_or(depth.near + 1.0);
                Mat4::orthographic_reverse_z(half_h * aspect, half_h, depth.near, far.max(depth.near + 1e-6))
            }
        }
    }

    /// Combined world-to-clip matrix for geometry relative to `origin`.
    pub fn view_projection(&self, viewport: Viewport, scene_bounds: Option<&Bbox3>, origin: Point3) -> Mat4 {
        let depth = self.depth_range(scene_bounds);
        self.projection_matrix(viewport.aspect(), depth).mul(&self.view_matrix_relative(origin))
    }

    /// Rotates the view about the target by yaw and pitch increments (radians). Pitch is clamped.
    pub fn orbit(&mut self, d_yaw: f64, d_pitch: f64) {
        if d_yaw.is_finite() {
            self.yaw = wrap_angle(self.yaw + d_yaw);
        }
        if d_pitch.is_finite() {
            self.pitch = (self.pitch + d_pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
    }

    /// Orbits for a mouse drag of `dx`, `dy` pixels: dragging right turns the model to the right, dragging down
    /// tilts it towards the viewer (the eye rises).
    pub fn orbit_pixels(&mut self, dx: f64, dy: f64) {
        self.orbit(-dx * ORBIT_RADIANS_PER_PIXEL, dy * ORBIT_RADIANS_PER_PIXEL);
    }

    /// Rotates the view about an arbitrary pivot (for example the point under the cursor). The pivot keeps its
    /// screen position; target and eye move rigidly around it.
    pub fn orbit_about(&mut self, pivot: Point3, d_yaw: f64, d_pitch: f64) {
        let before = self.basis();
        let v = self.target - pivot;
        let (a, b, c) = (v.dot(before.right), v.dot(before.up), v.dot(before.forward));
        self.orbit(d_yaw, d_pitch);
        let after = self.basis();
        self.target = pivot + after.right * a + after.up * b + after.forward * c;
    }

    /// World size of one pixel at a given depth in front of the eye.
    fn world_per_pixel(&self, viewport: Viewport, depth: f64) -> f64 {
        let h = match self.projection {
            Projection::Perspective => 2.0 * depth.max(1e-6) * self.tan_half_fov(),
            Projection::Parallel => self.view_height(),
        };
        h / sanitize_size(viewport.height)
    }

    /// Pans the view so that the scene follows a mouse drag of `dx`, `dy` pixels. With an `anchor` (the point
    /// grabbed under the cursor) the anchor stays under the cursor; otherwise the target plane does.
    pub fn pan_pixels(&mut self, dx: f64, dy: f64, viewport: Viewport, anchor: Option<Point3>) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        let b = self.basis();
        let depth = match anchor {
            Some(p) if self.projection == Projection::Perspective => {
                let d = (p - self.eye()).dot(b.forward);
                if d > 1e-3 { d } else { self.distance }
            }
            _ => self.distance,
        };
        let k = self.world_per_pixel(viewport, depth);
        self.target = self.target + b.right * (-dx * k) + b.up * (dy * k);
    }

    /// Moves the eye towards (`factor` < 1) or away from (`factor` > 1) the target.
    pub fn dolly(&mut self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.distance = clamp_distance(self.distance * factor);
        }
    }

    /// Zooms by `factor` (< 1 zooms in) keeping the point under the cursor fixed on screen. `anchor` is the
    /// world point under the cursor when known (from a pick); otherwise the point on the target plane is used.
    pub fn zoom_at(&mut self, factor: f64, cursor: [f64; 2], viewport: Viewport, anchor: Option<Point3>) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let new_distance = clamp_distance(self.distance * factor);
        let k = new_distance / self.distance;
        let pivot = match (self.projection, anchor) {
            (Projection::Perspective, Some(p)) if (p - self.eye()).dot(self.basis().forward) > 1e-3 => p,
            _ => self.point_on_target_plane(cursor, viewport),
        };
        self.target = pivot + (self.target - pivot) * k;
        self.distance = new_distance;
    }

    /// The point under a pixel on the plane through the target facing the camera.
    pub fn point_on_target_plane(&self, cursor: [f64; 2], viewport: Viewport) -> Point3 {
        let ray = self.ray(cursor, viewport);
        let plane = Plane { origin: self.target, normal: -self.basis().forward };
        match ray.intersect_plane(&plane) {
            Some(t) => ray.at(t),
            None => self.target,
        }
    }

    /// Adjusts target and distance so that the box fills the viewport (with a small margin), keeping the
    /// viewing direction.
    pub fn fit_to(&mut self, bbox: &Bbox3, viewport: Viewport) {
        const MARGIN: f64 = 1.08;
        let c = bbox.centre();
        if !(c.x.is_finite() && c.y.is_finite() && c.z.is_finite()) {
            return;
        }
        let b = self.basis();
        let tan_y = self.tan_half_fov();
        let tan_x = tan_y * viewport.aspect();
        let corners = bbox_corners(bbox);
        match self.projection {
            Projection::Perspective => {
                let mut d: f64 = 0.0;
                for p in corners {
                    let v = p - c;
                    let (x, y, z) = (v.dot(b.right), v.dot(b.up), v.dot(b.forward));
                    d = d.max(x.abs() * MARGIN / tan_x - z).max(y.abs() * MARGIN / tan_y - z);
                }
                self.target = c;
                self.distance = clamp_distance(d);
            }
            Projection::Parallel => {
                let (mut hx, mut hy): (f64, f64) = (0.0, 0.0);
                for p in corners {
                    let v = p - c;
                    hx = hx.max(v.dot(b.right).abs());
                    hy = hy.max(v.dot(b.up).abs());
                }
                let half_h = hy.max(hx / viewport.aspect()) * MARGIN;
                self.target = c;
                self.distance = clamp_distance(half_h / tan_y);
            }
        }
    }

    /// The ray through a pixel. For perspective it starts at the eye; for parallel projection it starts on the
    /// plane through the eye and should be treated as a line (parameters may be negative).
    pub fn ray(&self, cursor: [f64; 2], viewport: Viewport) -> Ray {
        let b = self.basis();
        let [nx, ny] = viewport.to_ndc(cursor[0], cursor[1]);
        let tan_y = self.tan_half_fov();
        let tan_x = tan_y * viewport.aspect();
        match self.projection {
            Projection::Perspective => {
                let d = b.forward + b.right * (nx * tan_x) + b.up * (ny * tan_y);
                Ray { origin: self.eye(), dir: d.normalized().unwrap_or(b.forward) }
            }
            Projection::Parallel => {
                let half_h = 0.5 * self.view_height();
                let half_w = half_h * viewport.aspect();
                Ray { origin: self.eye() + b.right * (nx * half_w) + b.up * (ny * half_h), dir: b.forward }
            }
        }
    }

    /// Projects a world point to the screen. `None` when it is behind a perspective eye.
    pub fn project(&self, p: Point3, viewport: Viewport) -> Option<ScreenPoint> {
        let b = self.basis();
        let v = p - self.eye();
        let depth = v.dot(b.forward);
        let tan_y = self.tan_half_fov();
        let tan_x = tan_y * viewport.aspect();
        let (nx, ny) = match self.projection {
            Projection::Perspective => {
                if depth <= 1e-9 {
                    return None;
                }
                (v.dot(b.right) / (depth * tan_x), v.dot(b.up) / (depth * tan_y))
            }
            Projection::Parallel => {
                let half_h = 0.5 * self.view_height();
                (v.dot(b.right) / (half_h * viewport.aspect()), v.dot(b.up) / half_h)
            }
        };
        let [x, y] = viewport.from_ndc([nx, ny]);
        (x.is_finite() && y.is_finite()).then_some(ScreenPoint { x, y, depth })
    }

    /// Screen directions of the world X, Y and Z axes, for an orientation gizmo drawn by the interface.
    pub fn axis_gizmo(&self) -> [GizmoAxis; 3] {
        let b = self.basis();
        [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| GizmoAxis {
            axis,
            screen: [axis.dot(b.right), -axis.dot(b.up)],
            depth: axis.dot(b.forward),
        })
    }

    /// Interpolates towards `other` for animated view changes (`t` in [0, 1]; apply easing outside). Yaw takes
    /// the shorter way round, distance and field of view interpolate geometrically, and the projection switches
    /// at the end.
    pub fn interpolate(&self, other: &OrbitCamera, t: f64) -> OrbitCamera {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 1.0 };
        let geo = |a: f64, b: f64| if a > 0.0 && b > 0.0 { a * (b / a).powf(t) } else { a + (b - a) * t };
        OrbitCamera {
            target: self.target.lerp(other.target, t),
            yaw: wrap_angle(self.yaw + wrap_angle(other.yaw - self.yaw) * t),
            pitch: self.pitch + (other.pitch - self.pitch) * t,
            distance: geo(self.distance, other.distance),
            fov_y: geo(self.fov_y, other.fov_y),
            projection: if t >= 1.0 { other.projection } else { self.projection },
        }
    }
}

fn clamp_distance(d: f64) -> f64 {
    if d.is_finite() { d.clamp(MIN_DISTANCE, MAX_DISTANCE) } else { MIN_DISTANCE }
}

/// The eight corners of a box.
pub(crate) fn bbox_corners(b: &Bbox3) -> [Point3; 8] {
    let (lo, hi) = (b.min, b.max);
    std::array::from_fn(|i| {
        Point3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    fn close_p(a: Point3, b: Point3, tol: f64) -> bool {
        a.distance(b) <= tol
    }

    fn vp() -> Viewport {
        Viewport::new(800.0, 600.0)
    }

    #[test]
    fn basis_is_orthonormal_and_right_handed_for_all_views() {
        for v in StandardView::ALL {
            let c = OrbitCamera::looking_at(Point3::ORIGIN, v, 10.0);
            let b = c.basis();
            assert!(close(b.right.length(), 1.0, 1e-12), "{v:?}");
            assert!(close(b.up.length(), 1.0, 1e-12), "{v:?}");
            assert!(close(b.forward.length(), 1.0, 1e-12), "{v:?}");
            assert!(close(b.right.dot(b.up), 0.0, 1e-12));
            assert!(close(b.right.dot(b.forward), 0.0, 1e-12));
            // right × up = −forward (view space looks down −Z).
            assert!((b.right.cross(b.up) + b.forward).length() < 1e-12, "{v:?}");
        }
    }

    #[test]
    fn standard_views_look_the_right_way() {
        let top = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::Top, 10.0).basis();
        assert!((top.forward - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-12);
        assert!((top.right - Vec3::X).length() < 1e-12, "east is right in plan");
        assert!((top.up - Vec3::Y).length() < 1e-12, "north is up in plan");

        let front = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::Front, 10.0).basis();
        assert!((front.forward - Vec3::Y).length() < 1e-12);
        assert!((front.up - Vec3::Z).length() < 1e-12);
        assert!((front.right - Vec3::X).length() < 1e-12);

        let left = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::Left, 10.0).basis();
        assert!((left.forward - Vec3::X).length() < 1e-12);
        let right = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::Right, 10.0).basis();
        assert!((right.forward + Vec3::X).length() < 1e-12);
        let back = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::Back, 10.0).basis();
        assert!((back.forward + Vec3::Y).length() < 1e-12);

        let sw = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 10.0);
        let e = sw.eye();
        assert!(e.x < 0.0 && e.y < 0.0 && e.z > 0.0, "eye in the south-west, above");
        // True isometric: the eye direction makes equal angles with the three axes.
        assert!(close(e.x.abs(), e.z.abs(), 1e-9) && close(e.y.abs(), e.z.abs(), 1e-9));
    }

    #[test]
    fn view_matrix_maps_target_ahead_of_eye() {
        let c = OrbitCamera::looking_at(Point3::new(5.0, 6.0, 1.0), StandardView::IsoNorthEast, 12.0);
        let t = c.view_matrix().transform_point(c.target).unwrap();
        assert!(close(t.x, 0.0, 1e-9) && close(t.y, 0.0, 1e-9) && close(t.z, -12.0, 1e-9));
    }

    #[test]
    fn relative_view_matrix_handles_large_coordinates() {
        // A target in projected ΕΓΣΑ87 coordinates.
        let origin = Point3::new(476_532.0, 4_097_345.0, 120.0);
        let target = Point3::new(476_532.123_4, 4_097_345.567_8, 121.5);
        let c = OrbitCamera::looking_at(target, StandardView::IsoSouthWest, 20.0);
        let rel = c.view_matrix_relative(origin);
        let p = rel.transform_point(Point3::ORIGIN + (target - origin)).unwrap();
        assert!(close(p.z, -20.0, 1e-9) && close(p.x, 0.0, 1e-9));
        // f32 conversion of the relative matrix stays precise.
        let m32 = rel.to_cols_f32();
        assert!((m32[3][2] as f64 - rel.at(2, 3)).abs() < 1e-5);
    }

    #[test]
    fn project_and_ray_agree() {
        for projection in [Projection::Perspective, Projection::Parallel] {
            let mut c = OrbitCamera::looking_at(Point3::new(1.0, 2.0, 3.0), StandardView::IsoSouthEast, 25.0);
            c.projection = projection;
            let p = Point3::new(4.0, -1.0, 2.0);
            let s = c.project(p, vp()).unwrap();
            let r = c.ray([s.x, s.y], vp());
            let t = r.closest_parameter(p);
            assert!(close_p(r.at(t), p, 1e-9), "{projection:?}");
        }
    }

    #[test]
    fn projection_matrix_agrees_with_project() {
        for projection in [Projection::Perspective, Projection::Parallel] {
            let mut c = OrbitCamera::looking_at(Point3::new(0.0, 0.0, 1.0), StandardView::IsoNorthWest, 15.0);
            c.projection = projection;
            let vp = vp();
            let m = c.view_projection(vp, None, Point3::ORIGIN);
            let p = Point3::new(2.0, 3.0, -1.0);
            let ndc = m.transform_point(p).unwrap();
            let s = c.project(p, vp).unwrap();
            let [x, y] = vp.from_ndc([ndc.x, ndc.y]);
            assert!(close(x, s.x, 1e-6) && close(y, s.y, 1e-6), "{projection:?}");
            assert!(ndc.z > 0.0 && ndc.z < 1.0, "{projection:?} depth {}", ndc.z);
        }
    }

    #[test]
    fn centre_pixel_ray_hits_target() {
        let c = OrbitCamera::looking_at(Point3::new(3.0, 4.0, 0.0), StandardView::IsoSouthWest, 10.0);
        let r = c.ray([400.0, 300.0], vp());
        assert!(close_p(r.at(10.0), c.target, 1e-9));
    }

    #[test]
    fn orbit_clamps_pitch_and_wraps_yaw() {
        let mut c = OrbitCamera::default();
        c.orbit(10.0, 10.0);
        assert!(close(c.pitch, std::f64::consts::FRAC_PI_2, 1e-12));
        assert!(c.yaw > -std::f64::consts::PI && c.yaw <= std::f64::consts::PI);
        c.orbit(0.0, -100.0);
        assert!(close(c.pitch, -std::f64::consts::FRAC_PI_2, 1e-12));
    }

    #[test]
    fn orbit_about_keeps_pivot_on_screen() {
        let mut c = OrbitCamera::looking_at(Point3::new(0.0, 0.0, 1.0), StandardView::IsoSouthWest, 20.0);
        let pivot = Point3::new(3.0, 1.0, 0.5);
        let before = c.project(pivot, vp()).unwrap();
        c.orbit_about(pivot, 0.4, -0.2);
        let after = c.project(pivot, vp()).unwrap();
        assert!(close(before.x, after.x, 1e-6) && close(before.y, after.y, 1e-6));
        assert!(close(before.depth, after.depth, 1e-9));
    }

    #[test]
    fn pan_moves_scene_with_cursor() {
        for projection in [Projection::Perspective, Projection::Parallel] {
            let mut c = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 20.0);
            c.projection = projection;
            let p = c.target;
            let before = c.project(p, vp()).unwrap();
            c.pan_pixels(30.0, -12.0, vp(), Some(p));
            let after = c.project(p, vp()).unwrap();
            assert!(close(after.x - before.x, 30.0, 1e-6), "{projection:?}");
            assert!(close(after.y - before.y, -12.0, 1e-6), "{projection:?}");
        }
    }

    #[test]
    fn pan_with_anchor_keeps_anchor_under_cursor() {
        let mut c = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 20.0);
        let anchor = Point3::new(-3.0, -2.0, 1.0);
        let before = c.project(anchor, vp()).unwrap();
        c.pan_pixels(-25.0, 40.0, vp(), Some(anchor));
        let after = c.project(anchor, vp()).unwrap();
        assert!(close(after.x - before.x, -25.0, 1e-6) && close(after.y - before.y, 40.0, 1e-6));
    }

    #[test]
    fn zoom_at_keeps_cursor_point_fixed() {
        for projection in [Projection::Perspective, Projection::Parallel] {
            let mut c = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 20.0);
            c.projection = projection;
            let cursor = [620.0, 140.0];
            let p = c.point_on_target_plane(cursor, vp());
            c.zoom_at(0.8, cursor, vp(), None);
            let s = c.project(p, vp()).unwrap();
            assert!(close(s.x, cursor[0], 1e-6) && close(s.y, cursor[1], 1e-6), "{projection:?}");
            assert!(close(c.distance, 16.0, 1e-9));
        }
        // With a picked anchor off the target plane.
        let mut c = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 20.0);
        let anchor = Point3::new(2.0, 1.0, 3.0);
        let s0 = c.project(anchor, vp()).unwrap();
        c.zoom_at(0.5, [s0.x, s0.y], vp(), Some(anchor));
        let s1 = c.project(anchor, vp()).unwrap();
        assert!(close(s0.x, s1.x, 1e-6) && close(s0.y, s1.y, 1e-6));
    }

    #[test]
    fn zoom_respects_limits() {
        let mut c = OrbitCamera::default();
        for _ in 0..200 {
            c.zoom_at(0.5, [400.0, 300.0], vp(), None);
        }
        assert!(c.distance >= MIN_DISTANCE);
        c.dolly(f64::NAN);
        assert!(c.distance.is_finite());
    }

    #[test]
    fn fit_to_shows_every_corner() {
        let b = Bbox3 { min: Point3::new(-4.0, -3.0, 0.0), max: Point3::new(8.0, 5.0, 7.0) };
        for projection in [Projection::Perspective, Projection::Parallel] {
            for view in StandardView::ALL {
                let mut c = OrbitCamera::looking_at(Point3::new(100.0, 100.0, 0.0), view, 1.0);
                c.projection = projection;
                c.fit_to(&b, vp());
                let mut touches = false;
                for p in bbox_corners(&b) {
                    let s = c.project(p, vp()).unwrap();
                    assert!(s.x >= -1e-6 && s.x <= 800.0 + 1e-6, "{projection:?} {view:?} x {}", s.x);
                    assert!(s.y >= -1e-6 && s.y <= 600.0 + 1e-6, "{projection:?} {view:?} y {}", s.y);
                    touches |= s.x < 60.0 || s.x > 740.0 || s.y < 45.0 || s.y > 555.0;
                }
                assert!(touches, "{projection:?} {view:?}: the box should fill the view");
            }
        }
    }

    #[test]
    fn parallel_depth_range_encloses_scene() {
        let b = Bbox3 { min: Point3::new(-10.0, -10.0, 0.0), max: Point3::new(10.0, 10.0, 6.0) };
        let mut c = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 1.0);
        c.projection = Projection::Parallel;
        let m = c.view_projection(vp(), Some(&b), Point3::ORIGIN);
        for p in bbox_corners(&b) {
            let ndc = m.transform_point(p).unwrap();
            assert!(ndc.z > 0.0 && ndc.z < 1.0, "depth {}", ndc.z);
        }
    }

    #[test]
    fn gizmo_axes_in_top_view() {
        let c = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::Top, 10.0);
        let [x, y, z] = c.axis_gizmo();
        assert!(close(x.screen[0], 1.0, 1e-12) && close(x.screen[1], 0.0, 1e-12));
        assert!(close(y.screen[0], 0.0, 1e-12) && close(y.screen[1], -1.0, 1e-12), "north points up");
        assert!(close(z.depth, -1.0, 1e-12), "Z points at the viewer");
    }

    #[test]
    fn interpolation_takes_short_way_round() {
        let mut a = OrbitCamera { yaw: 3.0, ..OrbitCamera::default() };
        let b = OrbitCamera { yaw: -3.0, distance: 120.0, projection: Projection::Parallel, ..a };
        a.distance = 30.0;
        let mid = a.interpolate(&b, 0.5);
        assert!(mid.yaw.abs() > 3.0, "passes through ±π, not 0: {}", mid.yaw);
        assert!(close(mid.distance, 60.0, 1e-9));
        assert_eq!(mid.projection, Projection::Perspective);
        assert_eq!(a.interpolate(&b, 1.0).projection, Projection::Parallel);
    }
}

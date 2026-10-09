//! Per-frame view parameters: camera, viewport size, display style and clip planes.

use kanavos_drawing::Rgba;
use kanavos_geom::{Bbox3, Plane, Point3, Vec3};

use crate::camera::OrbitCamera;

/// Maximum number of clip planes the shaders evaluate. Extra planes are ignored.
pub const MAX_CLIP_PLANES: usize = 6;

/// Direction and strength of the sun.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sun {
    /// Compass bearing of the sun, degrees clockwise from north (+Y): 90 is east, 180 south.
    pub azimuth_deg: f64,
    /// Height of the sun above the horizon, degrees.
    pub altitude_deg: f64,
    /// Direct light strength (0 disables the sun).
    pub intensity: f32,
}

impl Default for Sun {
    fn default() -> Self {
        Sun { azimuth_deg: 150.0, altitude_deg: 42.0, intensity: 0.62 }
    }
}

impl Sun {
    /// Unit vector pointing from the scene towards the sun.
    pub fn direction(&self) -> Vec3 {
        let (sa, ca) = self.azimuth_deg.to_radians().sin_cos();
        let (se, ce) = self.altitude_deg.clamp(-90.0, 90.0).to_radians().sin_cos();
        Vec3::new(sa * ce, ca * ce, se).normalized().unwrap_or(Vec3::Z)
    }
}

/// How the scene looks. Colours are sRGB; the renderer converts them to linear light.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayStyle {
    /// Background at the top of the viewport.
    pub background_top: Rgba,
    /// Background at the bottom of the viewport.
    pub background_bottom: Rgba,
    /// Draw feature edges (creases and open boundaries).
    pub edges: bool,
    /// Draw silhouettes of smooth surfaces (cylinders and other curved meshes).
    pub silhouettes: bool,
    /// Edge line colour (alpha is honoured).
    pub edge_color: Rgba,
    /// Edge line width in points (multiplied by the pixels-per-point factor).
    pub edge_width: f32,
    /// The sun.
    pub sun: Sun,
    /// Colour of direct sunlight.
    pub sun_color: Rgba,
    /// Ambient light from above.
    pub sky_color: Rgba,
    /// Ambient light from below.
    pub ground_color: Rgba,
    /// Strength of the ambient (sky and ground) light.
    pub ambient: f32,
    /// Strength of a soft light from the viewer that keeps faces turned away from the sun readable.
    pub fill: f32,
    /// Cast sun shadows.
    pub shadows: bool,
    /// How much shadows remove direct sunlight (0 none, 1 all).
    pub shadow_strength: f32,
    /// Screen-space ambient occlusion: darkens corners and contact areas.
    pub ambient_occlusion: bool,
    /// Ambient occlusion strength (0–1).
    pub ao_strength: f32,
    /// Ambient occlusion sampling radius, metres.
    pub ao_radius: f32,
    /// Draw a ground grid.
    pub grid: bool,
    /// Grid line colour (alpha is the strength of major lines).
    pub grid_color: Rgba,
    /// World height of the grid, metres.
    pub grid_elevation: f64,
    /// Selection highlight colour (tint and edges).
    pub selection_color: Rgba,
    /// Hover highlight colour.
    pub hover_color: Rgba,
    /// Opacity of ghosted objects (0–1).
    pub ghost_alpha: f32,
    /// Colour of the inside of closed solids where clip planes cut them open.
    pub cut_color: Rgba,
    /// Draw the occluded edges of selected objects as dashed lines.
    pub show_hidden_selection: bool,
}

impl Default for DisplayStyle {
    fn default() -> Self {
        DisplayStyle {
            background_top: Rgba::rgb(214, 222, 228),
            background_bottom: Rgba::rgb(247, 246, 242),
            edges: true,
            silhouettes: true,
            edge_color: Rgba::new(38, 41, 46, 235),
            edge_width: 1.25,
            sun: Sun::default(),
            sun_color: Rgba::rgb(255, 247, 234),
            sky_color: Rgba::rgb(222, 232, 242),
            ground_color: Rgba::rgb(176, 168, 156),
            ambient: 0.62,
            fill: 0.10,
            shadows: true,
            shadow_strength: 0.75,
            ambient_occlusion: true,
            ao_strength: 0.65,
            ao_radius: 0.6,
            grid: true,
            grid_color: Rgba::new(104, 112, 120, 90),
            grid_elevation: 0.0,
            selection_color: Rgba::rgb(14, 124, 134),
            hover_color: Rgba::rgb(64, 182, 191),
            ghost_alpha: 0.16,
            cut_color: Rgba::rgb(92, 88, 84),
            show_hidden_selection: true,
        }
    }
}

/// Everything that describes one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewParams {
    /// The camera, in world coordinates.
    pub camera: OrbitCamera,
    /// Size of the rendered image in physical pixels.
    pub size: [u32; 2],
    /// Physical pixels per logical point (for line widths).
    pub pixels_per_point: f32,
    /// Display style.
    pub style: DisplayStyle,
    /// Clip planes, world coordinates. Geometry on the back side of any plane (negative signed distance) is cut
    /// away. At most [`MAX_CLIP_PLANES`] are used.
    pub clip_planes: Vec<Plane>,
}

impl ViewParams {
    /// Parameters with the default style and no clipping.
    pub fn new(camera: OrbitCamera, size: [u32; 2], pixels_per_point: f32) -> ViewParams {
        ViewParams { camera, size, pixels_per_point, style: DisplayStyle::default(), clip_planes: Vec::new() }
    }

    /// Replaces the clip planes with the six faces of a section box (the inside is kept).
    pub fn set_section_box(&mut self, bbox: &Bbox3) {
        self.clip_planes = section_box_planes(bbox).to_vec();
    }
}

/// The six inward-facing planes of a section box: everything outside the box is cut away.
pub fn section_box_planes(b: &Bbox3) -> [Plane; 6] {
    let (lo, hi) = (b.min, b.max);
    [
        Plane { origin: lo, normal: Vec3::X },
        Plane { origin: hi, normal: -Vec3::X },
        Plane { origin: lo, normal: Vec3::Y },
        Plane { origin: hi, normal: -Vec3::Y },
        Plane { origin: lo, normal: Vec3::Z },
        Plane { origin: hi, normal: -Vec3::Z },
    ]
}

/// A plane as `(nx, ny, nz, d)` in render space (relative to `origin`): points with `n·p + d ≥ 0` are kept.
pub(crate) fn plane_to_render_space(p: &Plane, origin: Point3) -> [f32; 4] {
    let n = p.normal.normalized().unwrap_or(Vec3::Z);
    let d = n.dot(origin - p.origin);
    [n.x as f32, n.y as f32, n.z as f32, d as f32]
}

/// sRGB 8-bit channel to linear light.
pub(crate) fn srgb_to_linear(c: u8) -> f32 {
    let x = f32::from(c) / 255.0;
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

/// An sRGB colour as linear RGB with linear alpha.
pub(crate) fn linear_rgba(c: Rgba) -> [f32; 4] {
    [srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b), f32::from(c.a) / 255.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_bearing_follows_compass() {
        let south = Sun { azimuth_deg: 180.0, altitude_deg: 0.0, intensity: 1.0 }.direction();
        assert!((south - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-9);
        let east_high = Sun { azimuth_deg: 90.0, altitude_deg: 45.0, intensity: 1.0 }.direction();
        assert!(east_high.x > 0.7 && east_high.z > 0.7 && east_high.y.abs() < 1e-9);
    }

    #[test]
    fn section_box_keeps_inside() {
        let b = Bbox3 { min: Point3::new(0.0, 0.0, 0.0), max: Point3::new(2.0, 3.0, 4.0) };
        let inside = Point3::new(1.0, 1.0, 1.0);
        let outside = Point3::new(1.0, 5.0, 1.0);
        assert!(section_box_planes(&b).iter().all(|p| p.signed_distance(inside) > 0.0));
        assert!(section_box_planes(&b).iter().any(|p| p.signed_distance(outside) < 0.0));
    }

    #[test]
    fn render_space_plane_matches_world_plane() {
        let origin = Point3::new(1000.0, 2000.0, 10.0);
        let plane = Plane::new(Point3::new(1001.0, 2000.0, 0.0), Vec3::new(1.0, 1.0, 0.0)).unwrap();
        let [nx, ny, nz, d] = plane_to_render_space(&plane, origin);
        for p in [Point3::new(1003.0, 2001.0, 4.0), Point3::new(995.0, 1990.0, 12.0)] {
            let rel = p - origin;
            let ours = f64::from(nx) * rel.x + f64::from(ny) * rel.y + f64::from(nz) * rel.z + f64::from(d);
            assert!((ours - plane.signed_distance(p)).abs() < 1e-4);
        }
    }

    #[test]
    fn srgb_conversion_endpoints() {
        assert!(srgb_to_linear(0).abs() < 1e-9);
        assert!((srgb_to_linear(255) - 1.0).abs() < 1e-6);
        assert!((srgb_to_linear(188) - 0.5).abs() < 0.01);
    }
}

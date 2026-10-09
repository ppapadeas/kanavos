//! Solid modelling façade (extrude, sweep, CSG) over swappable kernels.
//!
//! Layer L2. See `docs/architecture.md` and ADR-0003.
//!
//! The first backend is a plain indexed triangle mesh ([`Mesh`]). Element code builds solids only through the
//! functions in this crate, so the kernel can change without touching it.
//!
//! Conventions: metres, Z up, counter-clockwise triangles seen from outside (outward normals), closed meshes for
//! anything that represents a physical volume.
//!
//! Builders: [`cuboid`], [`extrude`] (prisms with through-holes in plan), [`extrude_between`] and
//! [`extrude_planar`] (sloped slabs), [`column_solid`] and [`prism_with_through_holes`] (walls with door and window
//! holes and sloped tops, as one closed mesh), [`sweep`] (rails along a polyline), [`cylinder`].
//! Operations: [`Mesh::welded`], normals ([`Mesh::vertex_normals`], [`Mesh::split_creases`]), [`section`] of a
//! mesh by a plane (polygons with holes in plane coordinates) and [`clip`] (the part behind a plane, capped).

mod column;
mod section;
mod sweep;
mod triangulate;
mod weld;

pub use column::{ColumnPiece, ThroughHole, ZPlane, column_solid, prism_with_through_holes};
pub use section::{PlaneFrame, clip, section};
pub use sweep::{rect_profile, sweep};
pub use triangulate::triangulate;

use kanavos_geom::{Bbox3, Point2, Point3, Polygon2, PolygonWithHoles, Vec3};
use serde::{Deserialize, Serialize};

/// Error from a solid operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolidError {
    /// The input profile or path is degenerate.
    Degenerate(&'static str),
    /// Triangulation failed (self-intersecting or invalid polygon).
    Triangulation(String),
}

impl std::fmt::Display for SolidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolidError::Degenerate(what) => write!(f, "degenerate solid: {what}"),
            SolidError::Triangulation(why) => write!(f, "could not triangulate: {why}"),
        }
    }
}

impl std::error::Error for SolidError {}

/// An indexed triangle mesh.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mesh {
    /// Vertex positions, metres.
    pub positions: Vec<Point3>,
    /// Triangles as indices into `positions`, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// An empty mesh.
    pub fn new() -> Self {
        Mesh::default()
    }

    /// Whether the mesh has no triangles.
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// Adds a vertex and returns its index.
    pub fn push_vertex(&mut self, p: Point3) -> u32 {
        self.positions.push(p);
        (self.positions.len() - 1) as u32
    }

    /// Adds a triangle by vertex indices.
    pub fn push_triangle(&mut self, a: u32, b: u32, c: u32) {
        self.triangles.push([a, b, c]);
    }

    /// Adds a planar convex quad `a b c d` (counter-clockwise seen from outside) as two triangles.
    pub fn push_quad(&mut self, a: Point3, b: Point3, c: Point3, d: Point3) {
        let i = self.push_vertex(a);
        let j = self.push_vertex(b);
        let k = self.push_vertex(c);
        let l = self.push_vertex(d);
        self.push_triangle(i, j, k);
        self.push_triangle(i, k, l);
    }

    /// Appends another mesh.
    pub fn append(&mut self, other: &Mesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.triangles.extend(other.triangles.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
    }

    /// Triangle corner positions; triangles with out-of-range indices are skipped.
    pub fn triangle_points(&self) -> impl Iterator<Item = [Point3; 3]> + '_ {
        self.triangles.iter().filter_map(|t| {
            Some([
                *self.positions.get(t[0] as usize)?,
                *self.positions.get(t[1] as usize)?,
                *self.positions.get(t[2] as usize)?,
            ])
        })
    }

    /// Bounding box, or `None` for an empty mesh.
    pub fn bbox(&self) -> Option<Bbox3> {
        Bbox3::from_points(self.positions.iter().copied())
    }

    /// Enclosed volume (divergence theorem); meaningful only for closed, consistently oriented meshes.
    pub fn volume(&self) -> f64 {
        self.triangle_points().map(|[a, b, c]| a.to_vec().dot(b.to_vec().cross(c.to_vec())) / 6.0).sum()
    }

    /// Total surface area.
    pub fn surface_area(&self) -> f64 {
        self.triangle_points().map(|[a, b, c]| (b - a).cross(c - a).length() / 2.0).sum()
    }

    /// Applies a function to every vertex.
    pub fn map_positions(&self, f: impl Fn(Point3) -> Point3) -> Mesh {
        Mesh { positions: self.positions.iter().map(|&p| f(p)).collect(), triangles: self.triangles.clone() }
    }

    /// Moved by a vector.
    pub fn translated(&self, v: Vec3) -> Mesh {
        self.map_positions(|p| p + v)
    }

    /// Rotated about the vertical axis through `(cx, cy)`.
    pub fn rotated_z(&self, cx: f64, cy: f64, angle_rad: f64) -> Mesh {
        let (s, c) = angle_rad.sin_cos();
        self.map_positions(|p| {
            let (x, y) = (p.x - cx, p.y - cy);
            Point3::new(cx + x * c - y * s, cy + x * s + y * c, p.z)
        })
    }

    /// Per-triangle unit normals (zero for degenerate triangles).
    pub fn face_normals(&self) -> Vec<Vec3> {
        self.triangle_points()
            .map(|[a, b, c]| (b - a).cross(c - a).normalized().unwrap_or(Vec3::ZERO))
            .collect()
    }

    /// The same surface with every triangle flipped.
    pub fn flipped(&self) -> Mesh {
        Mesh {
            positions: self.positions.clone(),
            triangles: self.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect(),
        }
    }
}

/// An axis-aligned box from two opposite corners.
pub fn cuboid(min: Point3, max: Point3) -> Mesh {
    let (x0, y0, z0) = (min.x.min(max.x), min.y.min(max.y), min.z.min(max.z));
    let (x1, y1, z1) = (min.x.max(max.x), min.y.max(max.y), min.z.max(max.z));
    let p = |x, y, z| Point3::new(x, y, z);
    let mut m = Mesh::new();
    m.push_quad(p(x0, y0, z0), p(x0, y1, z0), p(x1, y1, z0), p(x1, y0, z0)); // bottom
    m.push_quad(p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)); // top
    m.push_quad(p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)); // south
    m.push_quad(p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1)); // east
    m.push_quad(p(x1, y1, z0), p(x0, y1, z0), p(x0, y1, z1), p(x1, y1, z1)); // north
    m.push_quad(p(x0, y1, z0), p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1)); // west
    m
}

/// A vertical prism: the profile extruded from `z0` to `z1` (`z1 > z0`). Holes become through-holes.
///
/// Failure modes: degenerate profile (fewer than three vertices or zero area), zero height, or a profile that
/// cannot be triangulated (self-intersecting).
pub fn extrude(profile: &PolygonWithHoles, z0: f64, z1: f64) -> Result<Mesh, SolidError> {
    if !(z1 - z0).is_finite() || (z1 - z0).abs() < 1e-9 {
        return Err(SolidError::Degenerate("zero extrusion height"));
    }
    let (z0, z1) = (z0.min(z1), z0.max(z1));
    let shape = profile.oriented();
    if shape.outer.len() < 3 {
        return Err(SolidError::Degenerate("profile needs at least three vertices"));
    }
    let (points, tris) = triangulate(&shape)?;
    let mut m = Mesh::new();
    // Caps.
    let base_bottom = m.positions.len() as u32;
    for p in &points {
        m.push_vertex(Point3::from_plan(*p, z0));
    }
    let base_top = m.positions.len() as u32;
    for p in &points {
        m.push_vertex(Point3::from_plan(*p, z1));
    }
    for t in &tris {
        m.push_triangle(base_bottom + t[0], base_bottom + t[2], base_bottom + t[1]);
        m.push_triangle(base_top + t[0], base_top + t[1], base_top + t[2]);
    }
    // Sides: outer ring is counter-clockwise and holes clockwise, so every edge's outside is on its right.
    for ring in shape.rings() {
        for e in ring.edges() {
            let a0 = Point3::from_plan(e.start, z0);
            let b0 = Point3::from_plan(e.end, z0);
            let b1 = Point3::from_plan(e.end, z1);
            let a1 = Point3::from_plan(e.start, z1);
            m.push_quad(a0, b0, b1, a1);
        }
    }
    Ok(m)
}

/// A vertical prism from a simple polygon.
pub fn extrude_polygon(profile: &Polygon2, z0: f64, z1: f64) -> Result<Mesh, SolidError> {
    extrude(&PolygonWithHoles::from(profile.clone()), z0, z1)
}

/// A vertical cylinder approximated by `segments` facets.
pub fn cylinder(centre: Point2, radius: f64, z0: f64, z1: f64, segments: usize) -> Result<Mesh, SolidError> {
    if radius <= 0.0 || !radius.is_finite() {
        return Err(SolidError::Degenerate("cylinder radius must be positive"));
    }
    let n = segments.max(6);
    let [cx, cy] = centre.xy();
    let ring: Vec<Point2> = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            Point2::new(cx + radius * a.cos(), cy + radius * a.sin())
        })
        .collect();
    extrude_polygon(&Polygon2::new(ring), z0, z1)
}

/// A slanted prism: the plan profile with a bottom face and a top face given by height functions.
///
/// Used for elements whose top or bottom follows a plane (pitched roof slabs, stair flights). Both faces are
/// triangulated identically; `bottom(p) < top(p)` must hold at every vertex.
pub fn extrude_between(
    profile: &PolygonWithHoles,
    bottom: impl Fn(Point2) -> f64,
    top: impl Fn(Point2) -> f64,
) -> Result<Mesh, SolidError> {
    let shape = profile.oriented();
    if shape.outer.len() < 3 {
        return Err(SolidError::Degenerate("profile needs at least three vertices"));
    }
    let (points, tris) = triangulate(&shape)?;
    let mut m = Mesh::new();
    let base_bottom = m.positions.len() as u32;
    for p in &points {
        m.push_vertex(Point3::from_plan(*p, bottom(*p)));
    }
    let base_top = m.positions.len() as u32;
    for p in &points {
        m.push_vertex(Point3::from_plan(*p, top(*p)));
    }
    for t in &tris {
        m.push_triangle(base_bottom + t[0], base_bottom + t[2], base_bottom + t[1]);
        m.push_triangle(base_top + t[0], base_top + t[1], base_top + t[2]);
    }
    for ring in shape.rings() {
        for e in ring.edges() {
            m.push_quad(
                Point3::from_plan(e.start, bottom(e.start)),
                Point3::from_plan(e.end, bottom(e.end)),
                Point3::from_plan(e.end, top(e.end)),
                Point3::from_plan(e.start, top(e.start)),
            );
        }
    }
    Ok(m)
}

/// A slanted prism between two planes over a plan profile (sloped slabs, roof planes, stair waists).
///
/// Failure modes: as for [`extrude_between`]; `bottom` must be below `top` over the whole profile.
pub fn extrude_planar(profile: &PolygonWithHoles, bottom: ZPlane, top: ZPlane) -> Result<Mesh, SolidError> {
    extrude_between(profile, |p| bottom.at(p), |p| top.at(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planar_extrusion_volume() {
        let sq = PolygonWithHoles::from(Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)]));
        let bottom = ZPlane::through(Point2::ORIGIN, 3.0, kanavos_geom::Vec2::new(0.0, 0.5));
        let m = extrude_planar(&sq, bottom, bottom.shifted(0.2)).unwrap();
        assert!((m.volume() - 8.0 * 0.2).abs() < 1e-9);
        assert!(m.is_closed(1e-9));
    }

    #[test]
    fn cuboid_volume_and_orientation() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 3.0, 4.0));
        assert!((m.volume() - 24.0).abs() < 1e-9);
        assert!((m.surface_area() - 2.0 * (6.0 + 8.0 + 12.0)).abs() < 1e-9);
    }

    #[test]
    fn extrusion_with_hole_has_net_volume() {
        let shape = PolygonWithHoles {
            outer: Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
            holes: vec![Polygon2::from_m(&[(2.0, 2.0), (4.0, 2.0), (4.0, 4.0), (2.0, 4.0)])],
        };
        let m = extrude(&shape, 0.0, 0.25).unwrap();
        assert!((m.volume() - 96.0 * 0.25).abs() < 1e-9, "volume {}", m.volume());
    }

    #[test]
    fn concave_extrusion_volume() {
        let l = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let m = extrude_polygon(&l, 0.0, 3.0).unwrap();
        assert!((m.volume() - 12.0 * 3.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_flat_extrusions() {
        let sq = Polygon2::from_m(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        assert!(extrude_polygon(&sq, 1.0, 1.0).is_err());
    }
}

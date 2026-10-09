//! Scene description handed to the renderer: triangle meshes with an object id, a colour and a category.

use kanavos_drawing::Rgba;
use kanavos_geom::{Bbox3, Point3};
use kanavos_solid::Mesh;

/// One coloured triangle mesh belonging to a pickable object.
///
/// Several meshes may share an `id` (a window's frame and its glass, the layers of a wall): they are picked,
/// selected, hidden and ghosted together. Positions are `f32` metres **relative to the scene origin** (see
/// [`Scene::origin`]), which keeps full precision near the model even when world coordinates are large.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneMesh {
    /// Object id reported by picking and used for selection (usually the element id).
    pub id: u64,
    /// Free category number (element kind, layer…) used by [`crate::Renderer::set_category_hidden`].
    pub category: u32,
    /// Surface colour, sRGB. Alpha below 255 makes the mesh transparent (glass); it is then drawn after the
    /// opaque geometry, back to front, without writing depth.
    pub color: Rgba,
    /// Vertex positions, metres relative to the scene origin.
    pub positions: Vec<[f32; 3]>,
    /// Optional per-vertex normals (same length as `positions`). When absent, normals are computed per face and
    /// smoothed across edges flatter than the renderer's crease angle, splitting vertices where needed.
    pub normals: Option<Vec<[f32; 3]>>,
    /// Triangles as vertex indices, three per triangle, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
}

impl Default for SceneMesh {
    /// An empty white mesh with id 0.
    fn default() -> Self {
        SceneMesh {
            id: 0,
            category: 0,
            color: Rgba::WHITE,
            positions: Vec::new(),
            normals: None,
            indices: Vec::new(),
        }
    }
}

impl SceneMesh {
    /// Converts a solid mesh (world metres, `f64`) into a scene mesh relative to `origin`.
    ///
    /// The subtraction happens in `f64` before rounding to `f32`, so precision depends only on the distance
    /// from the origin, not on the size of the world coordinates. Triangles with out-of-range indices are
    /// dropped.
    pub fn from_solid_mesh(id: u64, mesh: &Mesh, origin: Point3, color: Rgba, category: u32) -> SceneMesh {
        let positions = mesh
            .positions
            .iter()
            .map(|p| [(p.x - origin.x) as f32, (p.y - origin.y) as f32, (p.z - origin.z) as f32])
            .collect::<Vec<_>>();
        let n = positions.len() as u64;
        let indices = mesh
            .triangles
            .iter()
            .filter(|t| t.iter().all(|&i| u64::from(i) < n))
            .flat_map(|t| t.iter().copied())
            .collect();
        SceneMesh { id, category, color, positions, normals: None, indices }
    }

    /// Whether the mesh is drawn as transparent.
    pub fn is_transparent(&self) -> bool {
        self.color.a < 255
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Bounding box in scene (origin-relative) coordinates, or `None` when empty or not finite.
    pub fn local_bounds(&self) -> Option<Bbox3> {
        Bbox3::from_points(
            self.positions
                .iter()
                .filter(|p| p.iter().all(|c| c.is_finite()))
                .map(|p| Point3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))),
        )
    }
}

/// A complete scene: an origin and the meshes around it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    /// World position (metres) that mesh coordinates are relative to. Choose a point near the model, for
    /// example the project base point or the centre of the site.
    pub origin: Point3,
    /// Meshes; meshes with the same id form one object.
    pub meshes: Vec<SceneMesh>,
}

impl Scene {
    /// An empty scene around `origin`.
    pub fn new(origin: Point3) -> Scene {
        Scene { origin, meshes: Vec::new() }
    }

    /// Adds a solid mesh in world coordinates.
    pub fn push_solid(&mut self, id: u64, mesh: &Mesh, color: Rgba, category: u32) {
        self.meshes.push(SceneMesh::from_solid_mesh(id, mesh, self.origin, color, category));
    }

    /// Bounding box of all meshes in world coordinates.
    pub fn bounds(&self) -> Option<Bbox3> {
        let local = self.meshes.iter().filter_map(SceneMesh::local_bounds).reduce(|a, b| a.union(&b))?;
        let o = self.origin.to_vec();
        Some(Bbox3 { min: local.min + o, max: local.max + o })
    }
}

/// A sensible scene origin for world-space meshes: the centre of their bounding box, rounded to whole metres so
/// that grid lines and coordinates stay readable.
pub fn suggest_origin<'a>(meshes: impl IntoIterator<Item = &'a Mesh>) -> Point3 {
    match meshes.into_iter().filter_map(Mesh::bbox).reduce(|a, b| a.union(&b)) {
        Some(b) => {
            let c = b.centre();
            if c.x.is_finite() && c.y.is_finite() && c.z.is_finite() {
                Point3::new(c.x.round(), c.y.round(), c.z.round())
            } else {
                Point3::ORIGIN
            }
        }
        None => Point3::ORIGIN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_solid::cuboid;

    #[test]
    fn solid_mesh_conversion_is_relative_to_origin() {
        let origin = Point3::new(476_500.0, 4_097_300.0, 100.0);
        let m =
            cuboid(Point3::new(476_501.25, 4_097_302.5, 100.0), Point3::new(476_502.25, 4_097_303.5, 103.0));
        let s = SceneMesh::from_solid_mesh(7, &m, origin, Rgba::rgb(200, 190, 180), 3);
        assert_eq!(s.id, 7);
        assert_eq!(s.triangle_count(), 12);
        let b = s.local_bounds().unwrap();
        assert!((b.min.x - 1.25).abs() < 1e-6 && (b.max.z - 3.0).abs() < 1e-6);
        assert!(!s.is_transparent());
    }

    #[test]
    fn invalid_triangles_are_dropped() {
        let mut m = cuboid(Point3::ORIGIN, Point3::new(1.0, 1.0, 1.0));
        m.triangles.push([0, 1, 99]);
        let s = SceneMesh::from_solid_mesh(1, &m, Point3::ORIGIN, Rgba::WHITE, 0);
        assert_eq!(s.triangle_count(), 12);
    }

    #[test]
    fn scene_bounds_are_world() {
        let mut scene = Scene::new(Point3::new(1000.0, 2000.0, 0.0));
        scene.push_solid(
            1,
            &cuboid(Point3::new(1001.0, 2001.0, 0.0), Point3::new(1002.0, 2003.0, 3.0)),
            Rgba::WHITE,
            0,
        );
        let b = scene.bounds().unwrap();
        assert!((b.min.x - 1001.0).abs() < 1e-6 && (b.max.y - 2003.0).abs() < 1e-6);
    }

    #[test]
    fn origin_suggestion_rounds_bbox_centre() {
        let a = cuboid(Point3::new(10.2, 20.0, 0.0), Point3::new(12.2, 22.0, 3.0));
        let o = suggest_origin([&a]);
        assert!((o.x - 11.0).abs() < 1e-12 && (o.y - 21.0).abs() < 1e-12 && (o.z - 2.0).abs() < 1e-12);
    }
}

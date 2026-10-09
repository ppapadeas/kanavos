//! The 3D scene: every element as coloured meshes in world coordinates, with a per-element cache and clipping
//! helpers for section boxes.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kanavos_drawing::Rgba;
use kanavos_elements::{opening_placement, solids};
use kanavos_geom::{Bbox3, Plane, Point3, Vec3};
use kanavos_model::{BuildUp, Category, Document, Element, ElementKind, Id, Material, Storey, Structure};
use kanavos_objects::{OpeningContext, object_parts, opening_parts};
use kanavos_solid::Mesh;

use crate::holes::wall_solids_with_openings;
use crate::meshops::{V3, clip_polygon};

/// One coloured mesh of an element.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneItem {
    /// Element it belongs to.
    pub element: Id,
    /// Element category.
    pub category: Category,
    /// Storey of the element.
    pub storey: Option<Id>,
    /// Material, if known (for section hatching and exports).
    pub material: Option<Id>,
    /// Mesh in world coordinates (metres, Z absolute).
    pub mesh: Mesh,
    /// Display colour (alpha < 255 for glass).
    pub color: Rgba,
}

/// All 3D items.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    /// Items.
    pub items: Vec<SceneItem>,
}

impl Scene {
    /// Bounding box of every item, or `None` for an empty scene.
    pub fn bbox(&self) -> Option<Bbox3> {
        self.items.iter().filter_map(|i| i.mesh.bbox()).reduce(|a, b| a.union(&b))
    }

    /// Total number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.items.iter().map(|i| i.mesh.triangles.len()).sum()
    }
}

/// What to include in a scene.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SceneOptions {
    /// Only these storeys; `None` includes all.
    pub storeys: Option<BTreeSet<Id>>,
    /// Categories left out.
    pub hidden: BTreeSet<Category>,
}

impl SceneOptions {
    fn includes(&self, e: &Element) -> bool {
        if !e.category().is_building_part() || self.hidden.contains(&e.category()) {
            return false;
        }
        match &self.storeys {
            Some(filter) => e.storey.is_some_and(|s| filter.contains(&s)),
            None => true,
        }
    }
}

const DEFAULT_COLOR: Rgba = Rgba::rgb(210, 205, 195);

/// Items of one element (empty for annotation and spaces).
pub fn element_scene_items(doc: &Document, id: Id) -> Vec<SceneItem> {
    let Some(e) = doc.element(id) else { return Vec::new() };
    let category = e.category();
    let mut items = Vec::new();
    let color_of = |m: Option<Id>| m.and_then(|m| doc.material(m)).map(|m| m.color).unwrap_or(DEFAULT_COLOR);
    let item = |mesh: Mesh, material: Option<Id>, color: Rgba| SceneItem {
        element: id,
        category,
        storey: e.storey,
        material,
        mesh,
        color,
    };
    match &e.kind {
        ElementKind::Opening(o) => {
            if let Ok(pl) = opening_placement(doc, id) {
                let ctx = OpeningContext { opening: o, left_face: pl.left_face, right_face: pl.right_face };
                let [cx, cy] = pl.centre.xy();
                for part in opening_parts(&ctx) {
                    let mesh = part.mesh.map_positions(|q| {
                        let x = cx + pl.along.x * q.x + pl.across.x * q.y;
                        let y = cy + pl.along.y * q.x + pl.across.y * q.y;
                        Point3::new(x, y, pl.sill_elevation + q.z)
                    });
                    items.push(item(mesh, None, part.color));
                }
            }
        }
        ElementKind::Object(o) => {
            let floor = e.storey.and_then(|s| doc.storey(s)).map(|s| s.elevation.as_m()).unwrap_or(0.0);
            let z0 = floor + o.elevation.as_m();
            let [px, py] = o.position.xy();
            let (s, c) = o.rotation.as_rad().sin_cos();
            let mirror = if o.mirrored { -1.0 } else { 1.0 };
            for part in object_parts(o) {
                let mut mesh = part.mesh.map_positions(|q| {
                    let lx = q.x * mirror;
                    Point3::new(px + lx * c - q.y * s, py + lx * s + q.y * c, z0 + q.z)
                });
                if o.mirrored {
                    mesh = mesh.flipped();
                }
                items.push(item(mesh, None, part.color));
            }
        }
        ElementKind::Wall(_) => {
            if let Some(layers) = wall_solids_with_openings(doc, id) {
                for (mesh, material) in layers {
                    items.push(item(mesh, material, color_of(material)));
                }
            } else if let Ok(solids) = solids(doc, id) {
                for s in solids {
                    items.push(item(s.mesh, s.material, color_of(s.material)));
                }
            }
        }
        _ => {
            if let Ok(solids) = solids(doc, id) {
                for s in solids {
                    items.push(item(s.mesh, s.material, color_of(s.material)));
                }
            }
        }
    }
    items
}

/// The scene of the whole model, filtered by the options.
pub fn scene(doc: &Document, opts: &SceneOptions) -> Scene {
    let mut s = Scene::default();
    for e in doc.elements() {
        if opts.includes(e) {
            s.items.extend(element_scene_items(doc, e.id));
        }
    }
    s
}

/// Everything an element's meshes depend on. Two equal values give equal meshes.
#[derive(Debug, Clone, PartialEq)]
struct Deps {
    element: Element,
    storey: Option<Storey>,
    materials: Vec<Option<Material>>,
    build_ups: Vec<Option<BuildUp>>,
    related: Vec<Element>,
    related_storeys: Vec<Option<Storey>>,
}

fn structure_deps(doc: &Document, s: &Structure, materials: &mut Vec<Id>, build_ups: &mut Vec<Id>) {
    match s {
        Structure::Basic { material, .. } => materials.push(*material),
        Structure::Layered { build_up } => {
            build_ups.push(*build_up);
            if let Some(b) = doc.build_up(*build_up) {
                materials.extend(b.layers.iter().map(|l| l.material));
            }
        }
    }
}

fn wall_bbox(w: &kanavos_model::Wall) -> [f64; 4] {
    let [x0, y0] = w.start.xy();
    let [x1, y1] = w.end.xy();
    [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
}

fn deps_of(doc: &Document, e: &Element) -> Deps {
    let mut materials = Vec::new();
    let mut build_ups = Vec::new();
    let mut related = Vec::new();
    let mut related_storeys = Vec::new();
    match &e.kind {
        ElementKind::Wall(w) => {
            structure_deps(doc, &w.structure, &mut materials, &mut build_ups);
            // Hosted openings cut holes; nearby walls may change the junctions.
            related.extend(doc.openings_in(e.id).filter_map(|(id, _)| doc.element(id).cloned()));
            let b = wall_bbox(w);
            if let Some(s) = e.storey {
                for (oid, other) in doc.walls_on(s) {
                    if oid == e.id {
                        continue;
                    }
                    let ob = wall_bbox(other);
                    let m = 1.0;
                    if b[0] - m <= ob[2] && ob[0] <= b[2] + m && b[1] - m <= ob[3] && ob[1] <= b[3] + m {
                        if let Some(oe) = doc.element(oid) {
                            related.push(oe.clone());
                        }
                        structure_deps(doc, &other.structure, &mut materials, &mut build_ups);
                    }
                }
            }
        }
        ElementKind::Slab(s) => structure_deps(doc, &s.structure, &mut materials, &mut build_ups),
        ElementKind::Roof(r) => structure_deps(doc, &r.structure, &mut materials, &mut build_ups),
        ElementKind::Column(c) => materials.push(c.material),
        ElementKind::Beam(b) => materials.push(b.material),
        ElementKind::Stair(s) => materials.push(s.material),
        ElementKind::Railing(r) => materials.push(r.material),
        ElementKind::Opening(o) => {
            if let Some(host) = doc.element(o.host) {
                related.push(host.clone());
                related_storeys.push(host.storey.and_then(|s| doc.storey(s)).cloned());
                if let ElementKind::Wall(w) = &host.kind {
                    structure_deps(doc, &w.structure, &mut materials, &mut build_ups);
                }
            }
        }
        _ => {}
    }
    Deps {
        element: e.clone(),
        storey: e.storey.and_then(|s| doc.storey(s)).cloned(),
        materials: materials.iter().map(|m| doc.material(*m).cloned()).collect(),
        build_ups: build_ups.iter().map(|b| doc.build_up(*b).cloned()).collect(),
        related,
        related_storeys,
    }
}

#[derive(Debug, Clone)]
struct CacheEntry {
    deps: Deps,
    items: Arc<Vec<SceneItem>>,
}

/// Hit and miss counts of a [`SceneCache`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    /// Elements whose meshes were reused.
    pub hits: u64,
    /// Elements whose meshes were computed.
    pub misses: u64,
    /// Elements currently cached.
    pub entries: usize,
}

/// Reuses the meshes of elements that did not change between document versions.
///
/// Documents are persistent values, so an element's meshes can be reused whenever the element and everything
/// its geometry reads (storey, materials, build-ups, host wall, hosted openings, neighbouring walls) are equal
/// to what they were when the meshes were built.
#[derive(Debug, Clone, Default)]
pub struct SceneCache {
    entries: HashMap<Id, CacheEntry>,
    hits: u64,
    misses: u64,
}

impl SceneCache {
    /// An empty cache.
    pub fn new() -> SceneCache {
        SceneCache::default()
    }

    /// The scene items of one element, reused when nothing they depend on changed.
    pub fn element_items(&mut self, doc: &Document, id: Id) -> Arc<Vec<SceneItem>> {
        let Some(e) = doc.element(id) else { return Arc::new(Vec::new()) };
        let deps = deps_of(doc, e);
        if let Some(entry) = self.entries.get(&id)
            && entry.deps == deps
        {
            self.hits += 1;
            return entry.items.clone();
        }
        self.misses += 1;
        let items = Arc::new(element_scene_items(doc, id));
        self.entries.insert(id, CacheEntry { deps, items: items.clone() });
        items
    }

    /// The scene of a document, filtered by the options, reusing cached meshes. Entries of deleted elements are
    /// dropped.
    pub fn scene(&mut self, doc: &Document, opts: &SceneOptions) -> Scene {
        let mut s = Scene::default();
        let mut seen = HashSet::new();
        for e in doc.elements() {
            if !opts.includes(e) {
                continue;
            }
            seen.insert(e.id);
            s.items.extend(self.element_items(doc, e.id).iter().cloned());
        }
        if opts.storeys.is_none() && opts.hidden.is_empty() {
            self.entries.retain(|id, _| seen.contains(id));
        } else {
            self.entries.retain(|id, _| doc.element(*id).is_some());
        }
        s
    }

    /// Hit and miss counts.
    pub fn stats(&self) -> CacheStats {
        CacheStats { hits: self.hits, misses: self.misses, entries: self.entries.len() }
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Clips a mesh to the region in front of every plane (`signed_distance ≥ 0` for all planes). The result is open
/// where it was cut.
pub fn clip_mesh(mesh: &Mesh, planes: &[Plane]) -> Mesh {
    let mut out = Mesh::new();
    for [a, b, c] in mesh.triangle_points() {
        let mut poly: Vec<V3> = vec![a.to_array(), b.to_array(), c.to_array()];
        for pl in planes {
            let o = pl.origin.to_array();
            let n = pl.normal.to_array();
            poly =
                clip_polygon(&poly, |p| (p[0] - o[0]) * n[0] + (p[1] - o[1]) * n[1] + (p[2] - o[2]) * n[2]);
            if poly.len() < 3 {
                break;
            }
        }
        if poly.len() < 3 {
            continue;
        }
        let base = out.positions.len() as u32;
        out.positions.extend(poly.iter().map(|p| Point3::new(p[0], p[1], p[2])));
        for i in 1..poly.len() as u32 - 1 {
            out.push_triangle(base, base + i, base + i + 1);
        }
    }
    out
}

/// The six inward-facing planes of a box.
pub fn box_planes(b: &Bbox3) -> [Plane; 6] {
    let mk = |o: Point3, n: Vec3| Plane { origin: o, normal: n };
    [
        mk(b.min, Vec3::X),
        mk(b.min, Vec3::Y),
        mk(b.min, Vec3::Z),
        mk(b.max, -Vec3::X),
        mk(b.max, -Vec3::Y),
        mk(b.max, -Vec3::Z),
    ]
}

/// The scene clipped to a section box: items outside are dropped, items crossing a face are cut open.
pub fn clip_scene_to_box(scene: &Scene, b: &Bbox3) -> Scene {
    let planes = box_planes(b);
    let mut out = Scene::default();
    for it in &scene.items {
        let Some(ib) = it.mesh.bbox() else { continue };
        let outside = ib.max.x < b.min.x
            || ib.min.x > b.max.x
            || ib.max.y < b.min.y
            || ib.min.y > b.max.y
            || ib.max.z < b.min.z
            || ib.min.z > b.max.z;
        if outside {
            continue;
        }
        let inside = ib.min.x >= b.min.x
            && ib.max.x <= b.max.x
            && ib.min.y >= b.min.y
            && ib.max.y <= b.max.y
            && ib.min.z >= b.min.z
            && ib.max.z <= b.max.z;
        let mesh = if inside { it.mesh.clone() } else { clip_mesh(&it.mesh, &planes) };
        if !mesh.is_empty() {
            out.items.push(SceneItem { mesh, ..it.clone() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_solid::cuboid;

    #[test]
    fn box_clipping_keeps_the_inside() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 4.0, 4.0));
        let b = Bbox3 { min: Point3::new(1.0, -1.0, -1.0), max: Point3::new(5.0, 5.0, 2.0) };
        let c = clip_mesh(&m, &box_planes(&b));
        let bb = c.bbox().unwrap();
        assert!((bb.min.x - 1.0).abs() < 1e-12 && (bb.max.z - 2.0).abs() < 1e-12);
        // Kept: the bottom (3×4), the sides x = 4 (4×2), y = 0 and y = 4 (3×2 each); the cut faces stay open.
        assert!((c.surface_area() - (12.0 + 8.0 + 6.0 + 6.0)).abs() < 1e-9);
    }
}

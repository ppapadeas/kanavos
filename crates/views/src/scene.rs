//! The 3D scene: every element as coloured meshes in world coordinates.

use std::collections::BTreeSet;

use kanavos_drawing::Rgba;
use kanavos_elements::{opening_placement, solids};
use kanavos_geom::Point3;
use kanavos_model::{Category, Document, ElementKind, Id};
use kanavos_objects::{OpeningContext, object_parts, opening_parts};
use kanavos_solid::Mesh;

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

/// What to include in a scene.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SceneOptions {
    /// Only these storeys; `None` includes all.
    pub storeys: Option<BTreeSet<Id>>,
    /// Categories left out.
    pub hidden: BTreeSet<Category>,
}

const DEFAULT_COLOR: Rgba = Rgba::rgb(210, 205, 195);

/// Items of one element (empty for annotation and spaces).
pub fn element_scene_items(doc: &Document, id: Id) -> Vec<SceneItem> {
    let Some(e) = doc.element(id) else { return Vec::new() };
    let category = e.category();
    let mut items = Vec::new();
    let color_of = |m: Option<Id>| m.and_then(|m| doc.material(m)).map(|m| m.color).unwrap_or(DEFAULT_COLOR);
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
                    items.push(SceneItem {
                        element: id,
                        category,
                        storey: e.storey,
                        material: None,
                        mesh,
                        color: part.color,
                    });
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
                items.push(SceneItem {
                    element: id,
                    category,
                    storey: e.storey,
                    material: None,
                    mesh,
                    color: part.color,
                });
            }
        }
        _ => {
            if let Ok(solids) = solids(doc, id) {
                for s in solids {
                    items.push(SceneItem {
                        element: id,
                        category,
                        storey: e.storey,
                        material: s.material,
                        color: color_of(s.material),
                        mesh: s.mesh,
                    });
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
        if !e.category().is_building_part() || opts.hidden.contains(&e.category()) {
            continue;
        }
        if let Some(filter) = &opts.storeys
            && !e.storey.is_some_and(|st| filter.contains(&st))
        {
            continue;
        }
        s.items.extend(element_scene_items(doc, e.id));
    }
    s
}

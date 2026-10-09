//! Parametric object system for doors, windows and fixtures.
//!
//! Layer L4. See `docs/architecture.md` and ADR-0006.
//!
//! Objects are defined in Rust for now (a catalogue of parametric definitions); a sandboxed authoring language is
//! a later phase (ADR-0006). Every definition produces a 2D plan symbol and 3D parts from its parameters.
//!
//! Local coordinates:
//! - **Openings** (doors, windows): `x` along the host wall (the opening spans `−width/2 … +width/2`), `y` across
//!   the wall (positive towards the wall's left face; the faces are at `left_face` and `right_face`), `z` up from
//!   the sill. Views place them with the opening's world placement.
//! - **Free objects** (furniture, fixtures): origin at the insertion point, `x` right, `y` back/up in plan, `z` up
//!   from the object's base. Views apply the object's position, rotation and mirroring.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use kanavos_drawing::{Item, LinePattern, Rgba, Shape, Stroke, pt, weights};
use kanavos_geom::Point3;
use kanavos_model::{Object, Opening, OpeningKind};
use kanavos_solid::{Mesh, cuboid};

/// What an object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectCategory {
    /// Doors (hosted in walls).
    Door,
    /// Windows (hosted in walls).
    Window,
    /// Empty openings (hosted in walls).
    Void,
    /// Furniture.
    Furniture,
    /// Sanitary fixtures.
    Sanitary,
    /// Kitchen units and appliances.
    Kitchen,
    /// Equipment.
    Equipment,
    /// Trees and plants.
    Vegetation,
    /// Cars.
    Vehicle,
}

impl ObjectCategory {
    /// Catalogue key of the category name.
    pub fn name_key(self) -> &'static str {
        match self {
            ObjectCategory::Door => "objects.category.door",
            ObjectCategory::Window => "objects.category.window",
            ObjectCategory::Void => "objects.category.void",
            ObjectCategory::Furniture => "objects.category.furniture",
            ObjectCategory::Sanitary => "objects.category.sanitary",
            ObjectCategory::Kitchen => "objects.category.kitchen",
            ObjectCategory::Equipment => "objects.category.equipment",
            ObjectCategory::Vegetation => "objects.category.vegetation",
            ObjectCategory::Vehicle => "objects.category.vehicle",
        }
    }

    /// Whether objects of this category are hosted in walls.
    pub fn is_opening(self) -> bool {
        matches!(self, ObjectCategory::Door | ObjectCategory::Window | ObjectCategory::Void)
    }
}

/// Kind of a parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// A length in metres.
    Length,
    /// A whole number.
    Count,
    /// On/off (0 or 1).
    Toggle,
    /// An angle in degrees.
    Angle,
}

/// A parameter of an object definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamDef {
    /// Key used in `params` maps, e.g. `"frame"`.
    pub key: &'static str,
    /// Catalogue key of the label.
    pub label: &'static str,
    /// Kind.
    pub kind: ParamKind,
    /// Default value.
    pub default: f64,
    /// Minimum value.
    pub min: f64,
    /// Maximum value.
    pub max: f64,
}

/// A catalogue entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDef {
    /// Stable identifier, stored in documents, e.g. `"door.single_swing"`.
    pub id: &'static str,
    /// Catalogue key of the display name.
    pub name: &'static str,
    /// Category.
    pub category: ObjectCategory,
    /// Default width (openings: clear width; objects: x size), metres.
    pub width: f64,
    /// Default depth (objects: y size), metres. Openings ignore it.
    pub depth: f64,
    /// Default height, metres.
    pub height: f64,
    /// Default sill height for openings, metres.
    pub sill: f64,
    /// Parameters beyond width/depth/height.
    pub params: Vec<ParamDef>,
}

fn p(key: &'static str, label: &'static str, kind: ParamKind, default: f64, min: f64, max: f64) -> ParamDef {
    ParamDef { key, label, kind, default, min, max }
}

fn def(
    id: &'static str,
    name: &'static str,
    category: ObjectCategory,
    size: (f64, f64, f64),
    sill: f64,
    params: Vec<ParamDef>,
) -> ObjectDef {
    ObjectDef { id, name, category, width: size.0, depth: size.1, height: size.2, sill, params }
}

/// The built-in catalogue.
pub fn catalogue() -> &'static [ObjectDef] {
    static CATALOGUE: OnceLock<Vec<ObjectDef>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        use ObjectCategory::*;
        use ParamKind::*;
        let frame = || p("frame", "objects.param.frame", Length, 0.06, 0.02, 0.2);
        vec![
            def("door.single_swing", "objects.door.single_swing", Door, (0.9, 0.0, 2.1), 0.0, vec![frame()]),
            def("door.double_swing", "objects.door.double_swing", Door, (1.4, 0.0, 2.1), 0.0, vec![frame()]),
            def("door.sliding", "objects.door.sliding", Door, (1.6, 0.0, 2.2), 0.0, vec![frame()]),
            def(
                "window.casement_1",
                "objects.window.casement_1",
                Window,
                (0.8, 0.0, 1.2),
                0.9,
                vec![frame()],
            ),
            def(
                "window.casement_2",
                "objects.window.casement_2",
                Window,
                (1.2, 0.0, 1.4),
                0.9,
                vec![frame()],
            ),
            def("window.sliding", "objects.window.sliding", Window, (1.8, 0.0, 1.4), 0.9, vec![frame()]),
            def("window.fixed", "objects.window.fixed", Window, (1.0, 0.0, 1.0), 1.0, vec![frame()]),
            def("void.rectangular", "objects.void.rectangular", Void, (1.0, 0.0, 2.2), 0.0, vec![]),
            def(
                "furniture.bed_double",
                "objects.furniture.bed_double",
                Furniture,
                (1.6, 2.0, 0.45),
                0.0,
                vec![],
            ),
            def(
                "furniture.bed_single",
                "objects.furniture.bed_single",
                Furniture,
                (0.9, 2.0, 0.45),
                0.0,
                vec![],
            ),
            def("furniture.table", "objects.furniture.table", Furniture, (1.6, 0.9, 0.75), 0.0, vec![]),
            def("furniture.chair", "objects.furniture.chair", Furniture, (0.45, 0.5, 0.9), 0.0, vec![]),
            def("furniture.sofa", "objects.furniture.sofa", Furniture, (2.2, 0.9, 0.8), 0.0, vec![]),
            def("furniture.wardrobe", "objects.furniture.wardrobe", Furniture, (1.2, 0.6, 2.4), 0.0, vec![]),
            def("sanitary.wc", "objects.sanitary.wc", Sanitary, (0.38, 0.65, 0.4), 0.0, vec![]),
            def("sanitary.basin", "objects.sanitary.basin", Sanitary, (0.6, 0.45, 0.85), 0.0, vec![]),
            def("sanitary.shower", "objects.sanitary.shower", Sanitary, (0.9, 0.9, 0.05), 0.0, vec![]),
            def("sanitary.bath", "objects.sanitary.bath", Sanitary, (1.7, 0.75, 0.55), 0.0, vec![]),
            def("kitchen.counter", "objects.kitchen.counter", Kitchen, (2.4, 0.6, 0.9), 0.0, vec![]),
            def("kitchen.sink", "objects.kitchen.sink", Kitchen, (0.8, 0.6, 0.9), 0.0, vec![]),
            def("kitchen.hob", "objects.kitchen.hob", Kitchen, (0.6, 0.6, 0.9), 0.0, vec![]),
            def("kitchen.fridge", "objects.kitchen.fridge", Kitchen, (0.6, 0.65, 1.85), 0.0, vec![]),
            def("vegetation.tree", "objects.vegetation.tree", Vegetation, (4.0, 4.0, 6.0), 0.0, vec![]),
            def("vehicle.car", "objects.vehicle.car", Vehicle, (1.8, 4.5, 1.5), 0.0, vec![]),
        ]
    })
}

/// Finds a definition by id.
pub fn find(id: &str) -> Option<&'static ObjectDef> {
    catalogue().iter().find(|d| d.id == id)
}

/// Definitions of a category.
pub fn of_category(category: ObjectCategory) -> impl Iterator<Item = &'static ObjectDef> {
    catalogue().iter().filter(move |d| d.category == category)
}

/// The default style for an opening kind.
pub fn default_style(kind: OpeningKind) -> &'static str {
    match kind {
        OpeningKind::Door => "door.single_swing",
        OpeningKind::Window => "window.casement_2",
        OpeningKind::Void => "void.rectangular",
    }
}

/// A parameter value from a map, falling back to the definition's default (and to `fallback` for unknown keys).
pub fn param(def: Option<&ObjectDef>, params: &BTreeMap<String, f64>, key: &str, fallback: f64) -> f64 {
    if let Some(v) = params.get(key) {
        return *v;
    }
    def.and_then(|d| d.params.iter().find(|p| p.key == key)).map(|p| p.default).unwrap_or(fallback)
}

/// An opening in its wall, for drawing its symbol and parts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpeningContext<'a> {
    /// The opening.
    pub opening: &'a Opening,
    /// Offset of the host's left face from its reference line (local `y`), metres.
    pub left_face: f64,
    /// Offset of the host's right face from its reference line (local `y`), metres.
    pub right_face: f64,
}

/// A coloured 3D part.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Mesh in local coordinates.
    pub mesh: Mesh,
    /// Colour (alpha < 255 for glass).
    pub color: Rgba,
}

const FRAME: Rgba = Rgba::rgb(235, 235, 232);
const LEAF: Rgba = Rgba::rgb(160, 120, 80);
const GLASS: Rgba = Rgba::new(160, 200, 225, 110);

/// Plan symbol of a door or window in local coordinates (layers `door.symbol` / `window.symbol`).
pub fn opening_symbol(ctx: &OpeningContext<'_>) -> Vec<Item> {
    let o = ctx.opening;
    let w = o.width.as_m();
    let (yl, yr) = (ctx.left_face, ctx.right_face);
    let mut items = Vec::new();
    let thin = Stroke::solid(weights::FINE);
    let medium = Stroke::solid(weights::THIN);
    match o.kind {
        OpeningKind::Door => {
            let layer = "door.symbol";
            // Jamb lines.
            items.push(Item::line(pt(-w / 2.0, yl), pt(-w / 2.0, yr), thin.clone(), layer));
            items.push(Item::line(pt(w / 2.0, yl), pt(w / 2.0, yr), thin.clone(), layer));
            // Leaf and swing on the side given by flip_side (default: left face side).
            let face = if o.flip_side { yr } else { yl };
            let sign = if o.flip_side { -1.0 } else { 1.0 };
            let hinge_x = if o.flip_hand { w / 2.0 } else { -w / 2.0 };
            let free_x = -hinge_x;
            let leaf_end = pt(hinge_x, face + sign * w);
            items.push(Item::line(pt(hinge_x, face), leaf_end, medium, layer));
            let start = (pt(free_x, face).y - face).atan2(free_x - hinge_x);
            let end = sign * std::f64::consts::FRAC_PI_2;
            let sweep = {
                let s = end - start;
                if s > std::f64::consts::PI {
                    s - std::f64::consts::TAU
                } else if s < -std::f64::consts::PI {
                    s + std::f64::consts::TAU
                } else {
                    s
                }
            };
            items.push(Item {
                shape: Shape::Arc { centre: pt(hinge_x, face), radius: w, start, sweep },
                stroke: Some(Stroke::patterned(weights::FINE, LinePattern::Solid)),
                fill: None,
                layer: layer.into(),
                source: None,
            });
        }
        OpeningKind::Window => {
            let layer = "window.symbol";
            let mid = (yl + yr) / 2.0;
            items.push(Item::line(pt(-w / 2.0, yl), pt(-w / 2.0, yr), thin.clone(), layer));
            items.push(Item::line(pt(w / 2.0, yl), pt(w / 2.0, yr), thin.clone(), layer));
            items.push(Item::line(pt(-w / 2.0, mid + 0.02), pt(w / 2.0, mid + 0.02), medium.clone(), layer));
            items.push(Item::line(pt(-w / 2.0, mid - 0.02), pt(w / 2.0, mid - 0.02), medium, layer));
        }
        OpeningKind::Void => {
            let layer = "opening.symbol";
            items.push(Item::line(pt(-w / 2.0, yl), pt(w / 2.0, yr), thin.clone(), layer));
            items.push(Item::line(pt(-w / 2.0, yr), pt(w / 2.0, yl), thin, layer));
        }
    }
    items
}

/// 3D parts of a door or window in local coordinates.
pub fn opening_parts(ctx: &OpeningContext<'_>) -> Vec<Part> {
    let o = ctx.opening;
    let w = o.width.as_m();
    let h = o.height.as_m();
    let def = find(&o.style);
    let f = param(def, &o.params, "frame", 0.06);
    let mid = (ctx.left_face + ctx.right_face) / 2.0;
    let depth = 0.07;
    let (y0, y1) = (mid - depth / 2.0, mid + depth / 2.0);
    let mut parts = Vec::new();
    if o.kind == OpeningKind::Void {
        return parts;
    }
    let mut frame = Mesh::new();
    frame.append(&cuboid(Point3::new(-w / 2.0, y0, 0.0), Point3::new(-w / 2.0 + f, y1, h)));
    frame.append(&cuboid(Point3::new(w / 2.0 - f, y0, 0.0), Point3::new(w / 2.0, y1, h)));
    frame.append(&cuboid(Point3::new(-w / 2.0, y0, h - f), Point3::new(w / 2.0, y1, h)));
    if o.kind == OpeningKind::Window {
        frame.append(&cuboid(Point3::new(-w / 2.0, y0, 0.0), Point3::new(w / 2.0, y1, f)));
    }
    parts.push(Part { mesh: frame, color: FRAME });
    let inner = cuboid(
        Point3::new(-w / 2.0 + f, mid - 0.01, if o.kind == OpeningKind::Window { f } else { 0.0 }),
        Point3::new(w / 2.0 - f, mid + 0.01, h - f),
    );
    parts.push(Part { mesh: inner, color: if o.kind == OpeningKind::Door { LEAF } else { GLASS } });
    parts
}

fn object_size(obj: &Object) -> (f64, f64, f64) {
    let def = find(&obj.definition);
    let w = param(def, &obj.params, "width", def.map(|d| d.width).unwrap_or(1.0));
    let d = param(def, &obj.params, "depth", def.map(|d| d.depth).unwrap_or(1.0));
    let h = param(def, &obj.params, "height", def.map(|d| d.height).unwrap_or(1.0));
    (w, d, h)
}

/// Plan symbol of a free object in local coordinates (before its placement is applied), layer `object.symbol`.
pub fn object_symbol(obj: &Object) -> Vec<Item> {
    let (w, d, _) = object_size(obj);
    let stroke = Stroke::solid(weights::FINE);
    vec![Item::polyline(
        vec![pt(-w / 2.0, 0.0), pt(w / 2.0, 0.0), pt(w / 2.0, d), pt(-w / 2.0, d)],
        true,
        stroke,
        "object.symbol",
    )]
}

/// 3D parts of a free object in local coordinates.
pub fn object_parts(obj: &Object) -> Vec<Part> {
    let (w, d, h) = object_size(obj);
    vec![Part {
        mesh: cuboid(Point3::new(-w / 2.0, 0.0, 0.0), Point3::new(w / 2.0, d, h)),
        color: Rgba::rgb(200, 195, 185),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_ids_are_unique_and_named() {
        let mut ids: Vec<&str> = catalogue().iter().map(|d| d.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
        for kind in [OpeningKind::Door, OpeningKind::Window, OpeningKind::Void] {
            assert!(find(default_style(kind)).is_some());
        }
    }
}

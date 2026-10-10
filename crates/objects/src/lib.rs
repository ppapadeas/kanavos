//! Parametric object system for doors, windows, voids, furniture and fixtures.
//!
//! Layer L4. See `docs/architecture.md`, ADR-0006 and the specs in `specs/objects/`.
//!
//! Objects are built-in parametric definitions written in Rust (ADR-0006 phase 1); a sandboxed authoring language
//! is a later phase. Every definition produces a 2D plan symbol and coloured 3D parts from its parameters. Every
//! object was drawn from scratch from general drafting conventions (ISO 128, ΕΛΟΤ practice) and common product
//! dimensions.
//!
//! # Local coordinates
//!
//! - **Openings** (doors, windows, voids): `x` along the host wall (the opening spans `−width/2 … +width/2`),
//!   `y` across the wall (positive towards the wall's left side; the faces are at `left_face` and `right_face`),
//!   `z` up from the sill. The opening's `width` and `height` are the hole in the wall; frames sit inside it.
//! - **Free objects** (furniture, fixtures): origin at the insertion point, `x` right, `y` back in plan, `z` up
//!   from the object's base. Objects that stand against a wall have their origin at the middle of their front
//!   edge and extend over `y = 0 … depth` ([`Origin::FrontCentre`]); free-standing objects (tables, trees, cars)
//!   are centred on the origin ([`Origin::Centre`]). Views apply position, rotation and mirroring.
//!
//! # Orientation of openings
//!
//! Openings follow the wall build-up convention (`specs/objects/orientation.md`). A build-up lists its layers from
//! the wall's left face to its right face, and the template's exterior build-ups list the outside layer first, so
//! an exterior wall drawn clockwise around a building has its left face outside. Accordingly, by default a
//! window's **outer face** (frame position, external sill, shutters, roller box) is the wall's **left face**, and
//! door leaves and window sashes open towards the **right face**, into the building. `flip_side` swaps the two
//! faces; `flip_hand` swaps the hinge jamb (single hinges sit at `x = −width/2` by default) and the side sliding
//! leaves park on. [`OpeningContext::outer_face`] is the one place the rule is applied.
//!
//! # Plan symbols
//!
//! Layers are `door.symbol`, `window.symbol`, `opening.symbol` and `object.symbol`, with suffixes for parts a
//! view may hide: `.swing` (swing arcs and sliding arrows), `.above` (parts above the cut plane, dashed),
//! `.sill`/`.threshold` and `.shutter`. Leaves are drawn at 0.35 mm, frames at 0.25 mm, glass, swing arcs and
//! furniture at 0.18 mm and details at 0.13 mm. [`opening_symbol_at`] and [`object_symbol_at`] take the drawing
//! scale: at 1:50 and larger symbols show full detail, at smaller scales they are simplified.

mod bathroom;
mod catalogue;
mod doors;
mod furniture;
mod host;
mod kitchen;
mod outdoor;
mod params;
mod parts;
mod shape;
mod sym;
mod voids;
mod windows;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use kanavos_drawing::{Drawing, Item, Rect, Rgba, pt};
use kanavos_geom::Point3;
use kanavos_model::{Object, Opening, OpeningKind};
use kanavos_solid::{Mesh, cuboid};

use crate::host::Host;
use crate::params::P;
use crate::parts::Parts;
use crate::sym::{Pen, Sym};

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
    /// Every category, in catalogue order.
    pub const ALL: [ObjectCategory; 9] = [
        ObjectCategory::Door,
        ObjectCategory::Window,
        ObjectCategory::Void,
        ObjectCategory::Furniture,
        ObjectCategory::Kitchen,
        ObjectCategory::Sanitary,
        ObjectCategory::Equipment,
        ObjectCategory::Vegetation,
        ObjectCategory::Vehicle,
    ];

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

    /// The opening kind of an opening category.
    pub fn opening_kind(self) -> Option<OpeningKind> {
        match self {
            ObjectCategory::Door => Some(OpeningKind::Door),
            ObjectCategory::Window => Some(OpeningKind::Window),
            ObjectCategory::Void => Some(OpeningKind::Void),
            _ => None,
        }
    }
}

/// Kind of a parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// A length in metres.
    Length,
    /// A whole number (counts, and percentages such as `glazing`).
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

impl ParamDef {
    /// A stored value made valid: non-finite values give the default, others are limited to `min … max`; counts
    /// and toggles are rounded.
    pub fn clamp(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return self.default;
        }
        let v = value.max(self.min).min(self.max.max(self.min));
        match self.kind {
            ParamKind::Count | ParamKind::Toggle => v.round(),
            ParamKind::Length | ParamKind::Angle => v,
        }
    }
}

/// Where a free object's insertion point sits in its footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// The middle of the front edge; the object extends over `y = 0 … depth` (objects against a wall).
    #[default]
    FrontCentre,
    /// The centre of the footprint (tables, trees, cars).
    Centre,
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
    /// Default width (openings: width of the hole; objects: x size), metres.
    pub width: f64,
    /// Default depth (objects: y size), metres. Openings ignore it.
    pub depth: f64,
    /// Default height, metres.
    pub height: f64,
    /// Default sill height for openings, metres.
    pub sill: f64,
    /// Parameters beyond width/depth/height.
    pub params: Vec<ParamDef>,
    /// Where the insertion point of a free object sits.
    pub origin: Origin,
    /// IFC entity the object exports as (ISO 16739-1), e.g. `"IfcDoor"`, `"IfcSanitaryTerminal"`.
    pub ifc_class: &'static str,
    /// IFC predefined type, e.g. `"DOOR"`, `"TOILETPAN"`, `"NOTDEFINED"`.
    pub ifc_type: &'static str,
    /// Smallest accepted width, depth and height, metres.
    pub min_size: [f64; 3],
    /// Largest accepted width, depth and height, metres.
    pub max_size: [f64; 3],
}

impl ObjectDef {
    /// The definition of a parameter.
    pub fn param_def(&self, key: &str) -> Option<&ParamDef> {
        self.params.iter().find(|p| p.key == key)
    }

    /// Every parameter at its default value.
    pub fn default_params(&self) -> BTreeMap<String, f64> {
        self.params.iter().map(|p| (p.key.to_owned(), p.default)).collect()
    }
}

/// The built-in catalogue.
pub fn catalogue() -> &'static [ObjectDef] {
    static CATALOGUE: OnceLock<Vec<ObjectDef>> = OnceLock::new();
    CATALOGUE.get_or_init(catalogue::build)
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
/// The value is returned as stored; generators use it limited to the definition's range.
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

impl OpeningContext<'_> {
    /// Offset (local `y`) of the opening's outer face: the wall's left face, or its right face when the opening's
    /// `flip_side` is set.
    ///
    /// A window's frame is set back from this face, and its external sill, shutters and roller box lie on it. With
    /// the template's build-ups this is the outside of an exterior wall drawn clockwise around a building
    /// (`specs/objects/orientation.md`).
    pub fn outer_face(&self) -> f64 {
        if self.opening.flip_side { self.right_face } else { self.left_face }
    }

    /// Offset (local `y`) of the face that door leaves and window sashes open towards: the face opposite
    /// [`outer_face`](Self::outer_face), the wall's right face unless `flip_side` is set.
    pub fn inner_face(&self) -> f64 {
        if self.opening.flip_side { self.left_face } else { self.right_face }
    }
}

/// A coloured 3D part.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Mesh in local coordinates.
    pub mesh: Mesh,
    /// Colour (alpha < 255 for glass).
    pub color: Rgba,
}

/// The definition an opening is drawn with: its style when that is a style of the opening's kind, else the
/// default style of the kind.
pub fn opening_def(o: &Opening) -> Option<&'static ObjectDef> {
    find(&o.style)
        .filter(|d| d.category.opening_kind() == Some(o.kind))
        .or_else(|| find(default_style(o.kind)))
}

/// Plan symbol of a door, window or void in local coordinates at 1:50.
pub fn opening_symbol(ctx: &OpeningContext<'_>) -> Vec<Item> {
    opening_symbol_at(ctx, 50.0)
}

/// Plan symbol of a door, window or void in local coordinates for a drawing at scale 1:`scale`.
///
/// Layers: `door.symbol*`, `window.symbol*` or `opening.symbol*` (see the crate documentation).
pub fn opening_symbol_at(ctx: &OpeningContext<'_>, scale: f64) -> Vec<Item> {
    let o = ctx.opening;
    let (Some(def), Some(host)) = (opening_def(o), Host::new(ctx)) else {
        return degenerate_opening_symbol(ctx, scale);
    };
    let p = P::new(def, &o.params);
    let mut sym = Sym::new("opening.symbol", scale);
    match def.category {
        ObjectCategory::Door => doors::symbol(def.id, &mut sym, &host, &p),
        ObjectCategory::Window => windows::symbol(def.id, &mut sym, &host, &p),
        _ => voids::symbol(def.id, &mut sym, &host, &p),
    }
    host.finish_symbol(sym)
}

/// Jamb lines only, for openings too small to draw.
fn degenerate_opening_symbol(ctx: &OpeningContext<'_>, scale: f64) -> Vec<Item> {
    let w = ctx.opening.width.as_m();
    let mut sym = Sym::new("opening.symbol", scale);
    if w.is_finite() && w > 0.0 {
        sym.line((-w / 2.0, ctx.left_face), (-w / 2.0, ctx.right_face), Pen::Fine);
        sym.line((w / 2.0, ctx.left_face), (w / 2.0, ctx.right_face), Pen::Fine);
    }
    sym.into_items()
}

/// Plan symbol of an opening that lies wholly above the cut plane (a high window seen from below): the frame
/// and glass, or for voids the wall faces, dashed on the `<kind>.symbol.above` layer.
pub fn opening_symbol_overhead(ctx: &OpeningContext<'_>, scale: f64) -> Vec<Item> {
    let o = ctx.opening;
    let (Some(def), Some(host)) = (opening_def(o), Host::new(ctx)) else { return Vec::new() };
    let p = P::new(def, &o.params);
    let mut sym = Sym::new("opening.symbol.above", scale);
    match def.category {
        ObjectCategory::Door => windows::overhead_symbol(&mut sym, &host, &p, &windows::DOOR),
        ObjectCategory::Window => windows::overhead_symbol(&mut sym, &host, &p, &windows::WINDOW),
        _ => voids::overhead_symbol(&mut sym, &host),
    }
    host.finish_symbol(sym)
}

/// 3D parts of a door, window or void in local coordinates.
pub fn opening_parts(ctx: &OpeningContext<'_>) -> Vec<Part> {
    let o = ctx.opening;
    let (Some(def), Some(host)) = (opening_def(o), Host::new(ctx)) else { return Vec::new() };
    let p = P::new(def, &o.params);
    let mut parts = Parts::new();
    match def.category {
        ObjectCategory::Door => doors::parts(def.id, &mut parts, &host, &p),
        ObjectCategory::Window => windows::parts(def.id, &mut parts, &host, &p),
        _ => voids::parts(def.id, &mut parts, &host, &p),
    }
    host.finish_parts(parts)
}

/// The definition of a free object, if its id is in the catalogue.
pub fn object_def(obj: &Object) -> Option<&'static ObjectDef> {
    find(&obj.definition)
}

/// Width, depth and height of a free object: its `width`, `depth` and `height` parameters or the definition's
/// defaults, limited to the definition's size range (1 m cube for unknown definitions).
pub fn object_size(obj: &Object) -> (f64, f64, f64) {
    let def = object_def(obj);
    let get = |key: &str, i: usize, default: f64| {
        let v = param(def, &obj.params, key, def.map(|_| default).unwrap_or(1.0));
        let (lo, hi) = def
            .map(|d| {
                (d.min_size.get(i).copied().unwrap_or(0.01), d.max_size.get(i).copied().unwrap_or(100.0))
            })
            .unwrap_or((0.01, 100.0));
        if v.is_finite() { v.max(lo).min(hi.max(lo)) } else { default }
    };
    match def {
        Some(d) => (get("width", 0, d.width), get("depth", 1, d.depth), get("height", 2, d.height)),
        None => (get("width", 0, 1.0), get("depth", 1, 1.0), get("height", 2, 1.0)),
    }
}

/// The local footprint box of a free object, `(x0, y0, x1, y1)` and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bx {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub h: f64,
}

impl Bx {
    fn of(obj: &Object, def: Option<&ObjectDef>) -> Bx {
        let (w, d, h) = object_size(obj);
        match def.map(|d| d.origin).unwrap_or_default() {
            Origin::FrontCentre => Bx { x0: -w / 2.0, y0: 0.0, x1: w / 2.0, y1: d, h },
            Origin::Centre => Bx { x0: -w / 2.0, y0: -d / 2.0, x1: w / 2.0, y1: d / 2.0, h },
        }
    }
    /// Width.
    pub(crate) fn w(&self) -> f64 {
        self.x1 - self.x0
    }
    /// Depth.
    pub(crate) fn d(&self) -> f64 {
        self.y1 - self.y0
    }
    /// Centre x.
    pub(crate) fn cx(&self) -> f64 {
        (self.x0 + self.x1) / 2.0
    }
    /// Centre y.
    pub(crate) fn cy(&self) -> f64 {
        (self.y0 + self.y1) / 2.0
    }
}

/// Plan symbol of a free object at 1:50, in local coordinates (before its placement is applied), on layers
/// `object.symbol*`.
pub fn object_symbol(obj: &Object) -> Vec<Item> {
    object_symbol_at(obj, 50.0)
}

/// Plan symbol of a free object for a drawing at scale 1:`scale`, in local coordinates. Parts above the cut
/// plane (wall cabinets, a water heater, a parasol) are dashed on `object.symbol.above`.
pub fn object_symbol_at(obj: &Object, scale: f64) -> Vec<Item> {
    let def = object_def(obj);
    let b = Bx::of(obj, def);
    let mut sym = Sym::new("object.symbol", scale);
    let drawn = def.is_some_and(|def| {
        let p = P::new(def, &obj.params);
        furniture::symbol(def.id, &mut sym, &b, &p)
            || kitchen::symbol(def.id, &mut sym, &b, &p)
            || bathroom::symbol(def.id, &mut sym, &b, &p)
            || outdoor::symbol(def.id, &mut sym, &b, &p)
    });
    if !drawn {
        // Unknown definition: its box with a cross.
        sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
        sym.line((b.x0, b.y0), (b.x1, b.y1), Pen::Detail);
        sym.line((b.x0, b.y1), (b.x1, b.y0), Pen::Detail);
    }
    sym.into_items()
}

/// 3D parts of a free object in local coordinates.
pub fn object_parts(obj: &Object) -> Vec<Part> {
    let def = object_def(obj);
    let b = Bx::of(obj, def);
    let mut parts = Parts::new();
    let built = def.is_some_and(|def| {
        let p = P::new(def, &obj.params);
        furniture::parts(def.id, &mut parts, &b, &p)
            || kitchen::parts(def.id, &mut parts, &b, &p)
            || bathroom::parts(def.id, &mut parts, &b, &p)
            || outdoor::parts(def.id, &mut parts, &b, &p)
    });
    if !built {
        parts.add(
            cuboid(Point3::new(b.x0, b.y0, 0.0), Point3::new(b.x1, b.y1, b.h)),
            Rgba::rgb(200, 195, 185),
        );
    }
    parts.into_parts()
}

/// The local plan extent of a free object's symbol, including parts beyond its nominal size (chairs around a
/// table, the swing of a shower door, a parasol).
pub fn object_footprint(obj: &Object) -> Rect {
    let b = Bx::of(obj, object_def(obj));
    let mut r = Rect { min: pt(b.x0, b.y0), max: pt(b.x1, b.y1) };
    let d = Drawing { items: object_symbol(obj), ..Drawing::model(50.0) };
    if let Some(s) = d.bounds() {
        r = r.union(&s);
    }
    r
}

#[cfg(test)]
mod tests;

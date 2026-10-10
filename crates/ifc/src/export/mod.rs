//! IFC 4.3 export (`IFC4X3_ADD2`). The mapping is documented in `docs/formats/ifc.md` and specified in
//! `specs/files/ifc-export.md`.

mod shapes;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::Path;

use kanavos_drawing::Rgba;
use kanavos_geom::{Length, Point2, Point3, Polygon2, PolygonWithHoles, Vec2, Vec3};
use kanavos_model::{
    Beam, Column, Document, Element, ElementKind, Id, Object, Opening, OpeningKind, Phase, Profile, Railing,
    Roof, RoofShape, Slab, Space, SpaceCategory, Stair, Structure, Wall,
};
use kanavos_solid::Mesh;

pub(crate) use shapes::{Polyhedron, WallShape, wall_shapes};

use crate::IfcError;
use crate::guid::{Guid, document_namespace};
use crate::math::Frame3;
use crate::step::{P, StepWriter};

/// Schema identifier written to `FILE_SCHEMA`.
pub const EXPORT_SCHEMA: &str = "IFC4X3_ADD2";

/// User property in which imports before `Element::external_id` existed kept the IFC `GlobalId`. Files are
/// upgraded when they are read (see `kanavos_format::legacy`); export still honours the property for documents
/// built in memory the old way, and never writes it as a user property.
pub const GLOBAL_ID_PROPERTY: &str = "ifc:GlobalId";

/// Callback giving an element's body as a triangle mesh in project coordinates (metres, `z` above ±0.00), or
/// `None` to use the exporter's own geometry.
pub type GeometryFn<'a> = &'a dyn Fn(Id) -> Option<Mesh>;

/// How a document is exported.
#[derive(Clone)]
pub struct ExportOptions<'a> {
    /// Namespace for derived `GlobalId`s. `None` uses the document's project id, or, for a document without one,
    /// a namespace derived from the project's name, location, municipality and owner (see [`crate::guid`]).
    pub project_guid: Option<Guid>,
    /// Time stamp for the file header, ISO 8601 (`YYYY-MM-DDThh:mm:ss`). Supplied by the caller so that output
    /// is reproducible.
    pub timestamp: String,
    /// Author for the file header.
    pub author: String,
    /// Organisation for the file header.
    pub organization: String,
    /// Originating application for the file header, e.g. `Kanavos 0.1`.
    pub application: String,
    /// File name for the file header.
    pub file_name: String,
    /// Optional body geometry per element; when it returns a mesh, that mesh replaces the exporter's geometry.
    pub geometry: Option<GeometryFn<'a>>,
}

impl Default for ExportOptions<'_> {
    fn default() -> Self {
        ExportOptions {
            project_guid: None,
            timestamp: "1970-01-01T00:00:00".to_owned(),
            author: String::new(),
            organization: String::new(),
            application: format!("Kanavos {}", env!("CARGO_PKG_VERSION")),
            file_name: String::new(),
            geometry: None,
        }
    }
}

impl fmt::Debug for ExportOptions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExportOptions")
            .field("project_guid", &self.project_guid)
            .field("timestamp", &self.timestamp)
            .field("author", &self.author)
            .field("organization", &self.organization)
            .field("application", &self.application)
            .field("file_name", &self.file_name)
            .field("geometry", &self.geometry.is_some())
            .finish()
    }
}

/// What an export wrote and left out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportReport {
    /// Products written, per IFC entity type (e.g. `IfcWall` → 4).
    pub products: BTreeMap<String, usize>,
    /// Elements not exported, per Kanavos element type (annotation has no IFC product).
    pub skipped: BTreeMap<String, usize>,
    /// Problems with individual elements (degenerate geometry, fallbacks).
    pub warnings: Vec<String>,
    /// Number of entity instances in the file.
    pub instances: u64,
}

/// The result of an export.
#[derive(Debug, Clone, PartialEq)]
pub struct Exported {
    /// The IFC file, ISO 10303-21 clear text (pure ASCII).
    pub text: String,
    /// Report.
    pub report: ExportReport,
}

/// Exports a document as IFC 4.3.
pub fn export(doc: &Document, options: &ExportOptions<'_>) -> Exported {
    let mut x = Exporter::new(doc, options);
    x.run();
    x.finish()
}

/// Exports a document to a file (written to `<path>.tmp` first, then renamed).
pub fn export_to_file(
    path: &Path,
    doc: &Document,
    options: &ExportOptions<'_>,
) -> Result<ExportReport, IfcError> {
    let out = export(doc, options);
    let mut tmp_name = path.file_name().map(std::ffi::OsString::from).unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, out.text.as_bytes()).map_err(|e| IfcError::Io {
        action: "writing",
        path: tmp.clone(),
        source: e,
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        IfcError::Io { action: "replacing", path: path.to_path_buf(), source: e }
    })?;
    Ok(out.report)
}

/// A property value.
enum Pv {
    Label(String),
    Identifier(String),
    Text(String),
    Bool(bool),
    Logical(bool),
    Real(f64),
    Length(f64),
    Count(i64),
    /// An enumerated value (written as `IfcPropertyEnumeratedValue`).
    Enum(&'static str),
}

/// A quantity.
enum Q {
    Length(&'static str, f64),
    Area(&'static str, f64),
    Volume(&'static str, f64),
}

/// Where an element sits in the spatial structure.
#[derive(Clone, Copy)]
struct Host {
    /// Placement entity of the storey (or building).
    placement: u64,
    /// Storey (or building) frame in project coordinates.
    world: Frame3,
    /// Storey id, or `None` for the building.
    storey: Option<Id>,
}

/// An exported wall, for placing its openings.
struct WallHost {
    placement: u64,
    /// Wall frame relative to its host.
    local: Frame3,
    host: Host,
    shape: WallShape,
    product: u64,
    external: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum StyleKey {
    Material(Id),
    Door,
    Window,
    Space,
    Default,
}

struct Exporter<'a> {
    doc: &'a Document,
    opts: &'a ExportOptions<'a>,
    w: StepWriter,
    ns: Guid,
    used_guids: HashSet<Guid>,
    report: ExportReport,
    // Shared instances.
    origin: u64,
    model_ctx: u64,
    body_ctx: u64,
    axis_ctx: u64,
    footprint_ctx: u64,
    metre: u64,
    project: u64,
    building: u64,
    building_placement: u64,
    dirs: HashMap<(i64, i64, i64), u64>,
    // Caches.
    materials: BTreeMap<Id, u64>,
    layer_sets: BTreeMap<String, u64>,
    usages: BTreeMap<String, u64>,
    styles: BTreeMap<StyleKey, u64>,
    // Relationships collected while writing products.
    storeys: BTreeMap<Id, (u64, Host)>,
    contained: BTreeMap<Option<Id>, Vec<u64>>,
    spaces: BTreeMap<Id, Vec<u64>>,
    material_rels: BTreeMap<String, (u64, Vec<u64>)>,
    walls: BTreeMap<Id, WallHost>,
    wall_shapes: BTreeMap<Id, WallShape>,
}

fn status(phase: Phase) -> &'static str {
    match phase {
        Phase::New => "NEW",
        Phase::Existing => "EXISTING",
        Phase::Demolished => "DEMOLISH",
    }
}

fn snap(v: f64) -> f64 {
    if v.abs() < 1e-12 { 0.0 } else { v }
}

/// Inverse of a plan frame applied to a plan point.
fn to_local(frame: &Frame3, p: Point2) -> Point2 {
    let [x, y] = p.xy();
    let d = Vec2::new(x - frame.origin.x, y - frame.origin.y);
    let ax = Vec2::new(frame.x.x, frame.x.y);
    let ay = Vec2::new(frame.y.x, frame.y.y);
    Point2::new(d.dot(ax), d.dot(ay))
}

fn space_is_external(c: SpaceCategory) -> bool {
    matches!(c, SpaceCategory::Balcony | SpaceCategory::SemiOpen | SpaceCategory::Pilotis)
}

/// Stable snake_case key of a space category (as in the native format).
pub fn space_category_key(c: SpaceCategory) -> &'static str {
    match c {
        SpaceCategory::MainUse => "main_use",
        SpaceCategory::Auxiliary => "auxiliary",
        SpaceCategory::Stairwell => "stairwell",
        SpaceCategory::LiftShaft => "lift_shaft",
        SpaceCategory::SemiOpen => "semi_open",
        SpaceCategory::Balcony => "balcony",
        SpaceCategory::Pilotis => "pilotis",
        SpaceCategory::Parking => "parking",
        SpaceCategory::Attic => "attic",
        SpaceCategory::Technical => "technical",
        SpaceCategory::Shaft => "shaft",
        SpaceCategory::Other => "other",
    }
}

fn material_category_key(c: kanavos_model::MaterialCategory) -> &'static str {
    use kanavos_model::MaterialCategory as M;
    match c {
        M::Concrete => "concrete",
        M::Masonry => "masonry",
        M::Insulation => "insulation",
        M::Plaster => "plaster",
        M::Stone => "stone",
        M::Tile => "tile",
        M::Wood => "wood",
        M::Metal => "metal",
        M::Glass => "glass",
        M::Membrane => "membrane",
        M::Screed => "screed",
        M::Gravel => "gravel",
        M::Soil => "soil",
        M::Air => "air",
        M::Board => "board",
        M::Other => "other",
    }
}

impl<'a> Exporter<'a> {
    fn new(doc: &'a Document, opts: &'a ExportOptions<'a>) -> Exporter<'a> {
        let ns = opts.project_guid.unwrap_or_else(|| document_namespace(doc));
        Exporter {
            doc,
            opts,
            w: StepWriter::new(),
            ns,
            used_guids: HashSet::new(),
            report: ExportReport::default(),
            origin: 0,
            model_ctx: 0,
            body_ctx: 0,
            axis_ctx: 0,
            footprint_ctx: 0,
            metre: 0,
            project: 0,
            building: 0,
            building_placement: 0,
            dirs: HashMap::new(),
            materials: BTreeMap::new(),
            layer_sets: BTreeMap::new(),
            usages: BTreeMap::new(),
            styles: BTreeMap::new(),
            storeys: BTreeMap::new(),
            contained: BTreeMap::new(),
            spaces: BTreeMap::new(),
            material_rels: BTreeMap::new(),
            walls: BTreeMap::new(),
            wall_shapes: BTreeMap::new(),
        }
    }

    // ----- Identifiers -----------------------------------------------------------------------------------------

    /// A derived GlobalId for a stable name, e.g. `rel:contains:storey:7`.
    fn guid(&mut self, name: &str) -> String {
        let g = Guid::v5(&self.ns, name.as_bytes());
        self.used_guids.insert(g);
        g.to_ifc()
    }

    /// The GlobalId of an element: its external id (the one it was imported with; the legacy property for
    /// documents without the field), if it is a valid IFC identifier not used yet in this file, else a derived
    /// one.
    fn element_guid(&mut self, e: &Element) -> String {
        let stored =
            e.external_id.as_deref().or_else(|| e.properties.get(GLOBAL_ID_PROPERTY).map(String::as_str));
        if let Some(g) = stored.and_then(Guid::from_ifc)
            && self.used_guids.insert(g)
        {
            return g.to_ifc();
        }
        self.guid(&format!("element:{}", e.id.0))
    }

    fn warn(&mut self, message: String) {
        self.report.warnings.push(message);
    }

    fn skip(&mut self, e: &Element) {
        *self.report.skipped.entry(e.kind.type_name().to_owned()).or_default() += 1;
    }

    fn count(&mut self, ifc_type: &str) {
        *self.report.products.entry(ifc_type.to_owned()).or_default() += 1;
    }

    // ----- Geometry primitives ---------------------------------------------------------------------------------

    fn point3(&mut self, p: Point3) -> u64 {
        self.w.add("IFCCARTESIANPOINT", vec![P::reals([snap(p.x), snap(p.y), snap(p.z)])])
    }

    fn point2(&mut self, p: Point2) -> u64 {
        let [x, y] = p.xy();
        self.w.add("IFCCARTESIANPOINT", vec![P::reals([snap(x), snap(y)])])
    }

    fn dir3(&mut self, v: Vec3) -> u64 {
        let v = v.normalized().unwrap_or(Vec3::Z);
        let q = |c: f64| (snap(c) * 1e9).round() as i64;
        let key = (q(v.x), q(v.y), q(v.z));
        if let Some(&id) = self.dirs.get(&key) {
            return id;
        }
        let id = self.w.add("IFCDIRECTION", vec![P::reals([snap(v.x), snap(v.y), snap(v.z)])]);
        self.dirs.insert(key, id);
        id
    }

    fn dir2(&mut self, v: Vec2) -> u64 {
        let v = v.normalized().unwrap_or(Vec2::X);
        self.w.add("IFCDIRECTION", vec![P::reals([snap(v.x), snap(v.y)])])
    }

    /// `IfcAxis2Placement3D` for a frame (axes written only when they differ from the defaults).
    fn axis3(&mut self, f: &Frame3) -> u64 {
        let location = if f.origin.to_vec().length() < 1e-12 { self.origin } else { self.point3(f.origin) };
        let default_axes = (f.z - Vec3::Z).length() < 1e-12 && (f.x - Vec3::X).length() < 1e-12;
        if default_axes {
            self.w.add("IFCAXIS2PLACEMENT3D", vec![P::Ref(location), P::Null, P::Null])
        } else {
            let z = self.dir3(f.z);
            let x = self.dir3(f.x);
            self.w.add("IFCAXIS2PLACEMENT3D", vec![P::Ref(location), P::Ref(z), P::Ref(x)])
        }
    }

    fn axis2(&mut self, origin: Point2, x: Option<Vec2>) -> u64 {
        let loc = self.point2(origin);
        let dir = match x {
            Some(v) if (v - Vec2::X).length() > 1e-12 => P::Ref(self.dir2(v)),
            _ => P::Null,
        };
        self.w.add("IFCAXIS2PLACEMENT2D", vec![P::Ref(loc), dir])
    }

    fn placement(&mut self, relative_to: Option<u64>, f: &Frame3) -> u64 {
        let rel = relative_to.map(P::Ref).unwrap_or(P::Null);
        let axes = self.axis3(f);
        self.w.add("IFCLOCALPLACEMENT", vec![rel, P::Ref(axes)])
    }

    fn polyline(&mut self, pts: &[Point2], closed: bool) -> u64 {
        let mut ids: Vec<u64> = pts.iter().map(|&p| self.point2(p)).collect();
        if closed && let Some(&first) = ids.first() {
            ids.push(first);
        }
        self.w.add("IFCPOLYLINE", vec![P::refs(ids)])
    }

    /// An area profile from a polygon with holes (outer counter-clockwise, holes clockwise).
    fn profile(&mut self, shape: &PolygonWithHoles) -> u64 {
        let shape = shape.oriented();
        let outer = self.polyline(&shape.outer.vertices, true);
        if shape.holes.is_empty() {
            self.w.add("IFCARBITRARYCLOSEDPROFILEDEF", vec![P::e("AREA"), P::Null, P::Ref(outer)])
        } else {
            let inner: Vec<u64> = shape.holes.iter().map(|h| self.polyline(&h.vertices, true)).collect();
            self.w.add(
                "IFCARBITRARYPROFILEDEFWITHVOIDS",
                vec![P::e("AREA"), P::Null, P::Ref(outer), P::refs(inner)],
            )
        }
    }

    fn rect_profile(&mut self, centre: Point2, x_dir: Option<Vec2>, xdim: f64, ydim: f64) -> u64 {
        let pos = self.axis2(centre, x_dir);
        self.w.add(
            "IFCRECTANGLEPROFILEDEF",
            vec![P::e("AREA"), P::Null, P::Ref(pos), P::Real(xdim), P::Real(ydim)],
        )
    }

    fn circle_profile(&mut self, centre: Point2, radius: f64) -> u64 {
        let pos = self.axis2(centre, None);
        self.w.add("IFCCIRCLEPROFILEDEF", vec![P::e("AREA"), P::Null, P::Ref(pos), P::Real(radius)])
    }

    fn extrusion(&mut self, profile: u64, position: &Frame3, direction: Vec3, depth: f64) -> u64 {
        let pos = self.axis3(position);
        let dir = self.dir3(direction);
        self.w.add("IFCEXTRUDEDAREASOLID", vec![P::Ref(profile), P::Ref(pos), P::Ref(dir), P::Real(depth)])
    }

    /// A closed polyhedron as `IfcFacetedBrep`.
    fn brep(&mut self, poly: &Polyhedron) -> Option<u64> {
        if poly.faces.is_empty() {
            return None;
        }
        let points: Vec<u64> = poly.vertices.iter().map(|&p| self.point3(p)).collect();
        let mut faces = Vec::with_capacity(poly.faces.len());
        for f in &poly.faces {
            let ids: Vec<u64> = f.iter().filter_map(|&i| points.get(i).copied()).collect();
            let lp = self.w.add("IFCPOLYLOOP", vec![P::refs(ids)]);
            let bound = self.w.add("IFCFACEOUTERBOUND", vec![P::Ref(lp), P::Bool(true)]);
            faces.push(self.w.add("IFCFACE", vec![P::refs([bound])]));
        }
        let shell = self.w.add("IFCCLOSEDSHELL", vec![P::refs(faces)]);
        Some(self.w.add("IFCFACETEDBREP", vec![P::Ref(shell)]))
    }

    fn shape_rep(&mut self, ctx: u64, ident: &str, kind: &str, items: Vec<u64>) -> u64 {
        self.w.add("IFCSHAPEREPRESENTATION", vec![P::Ref(ctx), P::s(ident), P::s(kind), P::refs(items)])
    }

    fn product_shape(&mut self, reps: Vec<u64>) -> u64 {
        self.w.add("IFCPRODUCTDEFINITIONSHAPE", vec![P::Null, P::Null, P::refs(reps)])
    }

    /// The body from the geometry callback, if it supplies one: the mesh mapped into the element's frame.
    fn callback_body(&mut self, id: Id, world: &Frame3) -> Option<u64> {
        let f = self.opts.geometry?;
        let mesh = f(id)?;
        if mesh.is_empty() {
            return None;
        }
        let inv = world.inverse()?;
        let local = mesh.map_positions(|p| inv.apply(p));
        let poly = Polyhedron::from_mesh(&local);
        let item = self.brep(&poly)?;
        Some(self.shape_rep(self.body_ctx, "Body", "Brep", vec![item]))
    }

    // ----- Styles and materials --------------------------------------------------------------------------------

    fn style(&mut self, key: StyleKey) -> u64 {
        if let Some(&s) = self.styles.get(&key) {
            return s;
        }
        let (name, color) = match key {
            StyleKey::Material(id) => match self.doc.material(id) {
                Some(m) => (m.name.clone(), m.color),
                None => ("Default".to_owned(), Rgba::LIGHT_GREY),
            },
            StyleKey::Door => ("Door".to_owned(), Rgba::rgb(150, 110, 70)),
            StyleKey::Window => ("Window".to_owned(), Rgba::new(150, 200, 230, 110)),
            StyleKey::Space => ("Space".to_owned(), Rgba::new(160, 200, 255, 50)),
            StyleKey::Default => ("Default".to_owned(), Rgba::LIGHT_GREY),
        };
        let c = |v: u8| f64::from(v) / 255.0;
        let rgb = self.w.add(
            "IFCCOLOURRGB",
            vec![P::Null, P::Real(c(color.r)), P::Real(c(color.g)), P::Real(c(color.b))],
        );
        let transparency = 1.0 - c(color.a);
        let shading = self.w.add("IFCSURFACESTYLESHADING", vec![P::Ref(rgb), P::Real(transparency)]);
        let style = self.w.add("IFCSURFACESTYLE", vec![P::s(name), P::e("BOTH"), P::refs([shading])]);
        self.styles.insert(key, style);
        style
    }

    fn styled(&mut self, items: &[u64], key: StyleKey) {
        let style = self.style(key);
        for &item in items {
            self.w.add("IFCSTYLEDITEM", vec![P::Ref(item), P::refs([style]), P::Null]);
        }
    }

    fn material(&mut self, id: Id) -> u64 {
        if let Some(&m) = self.materials.get(&id) {
            return m;
        }
        let (name, category) = match self.doc.material(id) {
            Some(m) => (m.name.clone(), Some(material_category_key(m.category))),
            None => (format!("material {}", id.0), None),
        };
        let m = self.w.add("IFCMATERIAL", vec![P::s(name), P::Null, category.map(P::s).unwrap_or(P::Null)]);
        self.materials.insert(id, m);
        m
    }

    /// The layer set of a structure: the build-up's layers, or one layer for a single material.
    fn layer_set(&mut self, s: &Structure) -> Option<u64> {
        let (key, name, layers) = match s {
            Structure::Layered { build_up } => {
                let b = self.doc.build_up(*build_up)?;
                (
                    format!("b{}", build_up.0),
                    b.name.clone(),
                    b.layers.iter().map(|l| (l.material, l.thickness.as_m(), l.core)).collect::<Vec<_>>(),
                )
            }
            Structure::Basic { material, thickness } => {
                let name = self.doc.material(*material).map(|m| m.name.clone()).unwrap_or_default();
                (
                    format!("m{}:{}", material.0, (thickness.as_m() * 1e6).round() as i64),
                    name,
                    vec![(*material, thickness.as_m(), true)],
                )
            }
        };
        if let Some(&id) = self.layer_sets.get(&key) {
            return Some(id);
        }
        let mut ids = Vec::with_capacity(layers.len());
        for (material, thickness, core) in layers {
            let m = self.material(material);
            let priority = self.doc.material(material).map(|m| i64::from(m.priority.min(100)));
            ids.push(self.w.add(
                "IFCMATERIALLAYER",
                vec![
                    P::Ref(m),
                    P::Real(thickness),
                    P::Null,
                    P::Null,
                    P::Null,
                    if core { P::s("LoadBearing") } else { P::Null },
                    priority.map(P::Int).unwrap_or(P::Null),
                ],
            ));
        }
        let set = self.w.add("IFCMATERIALLAYERSET", vec![P::refs(ids), P::opt_s(&name), P::Null]);
        self.layer_sets.insert(key, set);
        Some(set)
    }

    /// Associates a structure's layer set with a product through a usage (walls: axis 2; slabs: axis 3).
    fn associate_layers(&mut self, product: u64, s: &Structure, axis: &'static str, offset: f64) {
        let Some(set) = self.layer_set(s) else { return };
        let key = format!("usage:{set}:{axis}:{}", (offset * 1e6).round() as i64);
        let usage = match self.usages.get(&key) {
            Some(&u) => u,
            None => {
                let u = self.w.add(
                    "IFCMATERIALLAYERSETUSAGE",
                    vec![P::Ref(set), P::e(axis), P::e("NEGATIVE"), P::Real(offset), P::Null],
                );
                self.usages.insert(key.clone(), u);
                u
            }
        };
        self.material_rels.entry(key).or_insert((usage, Vec::new())).1.push(product);
    }

    fn associate_layer_set(&mut self, product: u64, s: &Structure) {
        let Some(set) = self.layer_set(s) else { return };
        self.material_rels.entry(format!("set:{set}")).or_insert((set, Vec::new())).1.push(product);
    }

    fn associate_material(&mut self, product: u64, material: Id) {
        let m = self.material(material);
        self.material_rels
            .entry(format!("material:{}", material.0))
            .or_insert((m, Vec::new()))
            .1
            .push(product);
    }

    // ----- Properties and quantities ---------------------------------------------------------------------------

    fn pset(&mut self, owner: &str, product: u64, name: &str, props: Vec<(&str, Pv)>) {
        if props.is_empty() {
            return;
        }
        let mut ids = Vec::with_capacity(props.len());
        for (pname, value) in props {
            let id = match value {
                Pv::Enum(v) => self.w.add(
                    "IFCPROPERTYENUMERATEDVALUE",
                    vec![P::s(pname), P::Null, P::List(vec![P::typed("IFCLABEL", P::s(v))]), P::Null],
                ),
                other => {
                    let v = match other {
                        Pv::Label(s) => P::typed("IFCLABEL", P::s(s)),
                        Pv::Identifier(s) => P::typed("IFCIDENTIFIER", P::s(s)),
                        Pv::Text(s) => P::typed("IFCTEXT", P::s(s)),
                        Pv::Bool(b) => P::typed("IFCBOOLEAN", P::Bool(b)),
                        Pv::Logical(b) => P::typed("IFCLOGICAL", P::Bool(b)),
                        Pv::Real(r) => P::typed("IFCREAL", P::Real(r)),
                        Pv::Length(r) => P::typed("IFCPOSITIVELENGTHMEASURE", P::Real(r)),
                        Pv::Count(c) => P::typed("IFCCOUNTMEASURE", P::Int(c)),
                        Pv::Enum(_) => P::Null,
                    };
                    self.w.add("IFCPROPERTYSINGLEVALUE", vec![P::s(pname), P::Null, v, P::Null])
                }
            };
            ids.push(id);
        }
        let guid = self.guid(&format!("pset:{owner}:{name}"));
        let set = self.w.add("IFCPROPERTYSET", vec![P::s(guid), P::Null, P::s(name), P::Null, P::refs(ids)]);
        let rel_guid = self.guid(&format!("rel:pset:{owner}:{name}"));
        self.w.add(
            "IFCRELDEFINESBYPROPERTIES",
            vec![P::s(rel_guid), P::Null, P::Null, P::Null, P::refs([product]), P::Ref(set)],
        );
    }

    fn qto(&mut self, owner: &str, product: u64, name: &str, quantities: Vec<Q>) {
        let mut ids = Vec::with_capacity(quantities.len());
        for q in quantities {
            let (entity, qname, value) = match q {
                Q::Length(n, v) => ("IFCQUANTITYLENGTH", n, v),
                Q::Area(n, v) => ("IFCQUANTITYAREA", n, v),
                Q::Volume(n, v) => ("IFCQUANTITYVOLUME", n, v),
            };
            if !(value.is_finite() && value >= 0.0) {
                continue;
            }
            ids.push(self.w.add(entity, vec![P::s(qname), P::Null, P::Null, P::Real(value), P::Null]));
        }
        if ids.is_empty() {
            return;
        }
        let guid = self.guid(&format!("qto:{owner}:{name}"));
        let set = self
            .w
            .add("IFCELEMENTQUANTITY", vec![P::s(guid), P::Null, P::s(name), P::Null, P::Null, P::refs(ids)]);
        let rel_guid = self.guid(&format!("rel:qto:{owner}:{name}"));
        self.w.add(
            "IFCRELDEFINESBYPROPERTIES",
            vec![P::s(rel_guid), P::Null, P::Null, P::Null, P::refs([product]), P::Ref(set)],
        );
    }

    /// The user properties of an element as `Kanavos_Properties`.
    fn user_properties(&mut self, e: &Element, product: u64) {
        let props: Vec<(&str, Pv)> = e
            .properties
            .iter()
            .filter(|(k, _)| k.as_str() != GLOBAL_ID_PROPERTY)
            .map(|(k, v)| (k.as_str(), Pv::Text(v.clone())))
            .collect();
        self.pset(&format!("element:{}", e.id.0), product, "Kanavos_Properties", props);
    }

    // ----- Spatial structure -----------------------------------------------------------------------------------

    fn header_and_project(&mut self) {
        let doc = self.doc;
        let info = doc.info();
        self.origin = self.point3(Point3::ORIGIN);
        let wcs = self.w.add("IFCAXIS2PLACEMENT3D", vec![P::Ref(self.origin), P::Null, P::Null]);
        let georef = &doc.site().georef;
        let rot = georef.rotation.as_rad();
        let north = self.dir2(Vec2::new(-rot.sin(), rot.cos()));
        let precision = (doc.tolerance().length.as_m() / 10.0).max(1e-9);
        self.model_ctx = self.w.add(
            "IFCGEOMETRICREPRESENTATIONCONTEXT",
            vec![P::Null, P::s("Model"), P::Int(3), P::Real(precision), P::Ref(wcs), P::Ref(north)],
        );
        let sub = |x: &mut Exporter<'_>, ident: &str, view: &'static str| {
            let parent = x.model_ctx;
            x.w.add(
                "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
                vec![
                    P::s(ident),
                    P::s("Model"),
                    P::Derived,
                    P::Derived,
                    P::Derived,
                    P::Derived,
                    P::Ref(parent),
                    P::Null,
                    P::e(view),
                    P::Null,
                ],
            )
        };
        self.body_ctx = sub(self, "Body", "MODEL_VIEW");
        self.axis_ctx = sub(self, "Axis", "GRAPH_VIEW");
        self.footprint_ctx = sub(self, "FootPrint", "PLAN_VIEW");
        let origin2 = self.point2(Point2::ORIGIN);
        let wcs2 = self.w.add("IFCAXIS2PLACEMENT2D", vec![P::Ref(origin2), P::Null]);
        let plan_ctx = self.w.add(
            "IFCGEOMETRICREPRESENTATIONCONTEXT",
            vec![P::Null, P::s("Plan"), P::Int(2), P::Real(precision), P::Ref(wcs2), P::Null],
        );

        let si = |x: &mut Exporter<'_>, kind: &'static str, name: &'static str| {
            x.w.add("IFCSIUNIT", vec![P::Derived, P::e(kind), P::Null, P::e(name)])
        };
        self.metre = si(self, "LENGTHUNIT", "METRE");
        let area = si(self, "AREAUNIT", "SQUARE_METRE");
        let volume = si(self, "VOLUMEUNIT", "CUBIC_METRE");
        let angle = si(self, "PLANEANGLEUNIT", "RADIAN");
        let units = self.w.add("IFCUNITASSIGNMENT", vec![P::refs([self.metre, area, volume, angle])]);

        let guid = self.guid("project");
        let name = if info.name.is_empty() { "Project".to_owned() } else { info.name.clone() };
        self.project = self.w.add(
            "IFCPROJECT",
            vec![
                P::s(guid),
                P::Null,
                P::s(name),
                P::opt_s(&info.notes),
                P::Null,
                P::opt_s(&info.location),
                P::Null,
                P::refs([self.model_ctx, plan_ctx]),
                P::Ref(units),
            ],
        );

        // Georeferencing (when the project origin has grid coordinates).
        let set = georef.easting.abs() > 1e-9
            || georef.northing.abs() > 1e-9
            || georef.altitude.abs() > 1e-9
            || rot.abs() > 1e-12;
        if set {
            let (name, description, datum) = if georef.epsg == 2100 {
                (format!("EPSG:{}", georef.epsg), P::s("GGRS87 / Greek Grid"), P::s("GGRS87"))
            } else {
                (format!("EPSG:{}", georef.epsg), P::Null, P::Null)
            };
            let crs = self.w.add(
                "IFCPROJECTEDCRS",
                vec![P::s(name), description, datum, P::Null, P::Null, P::Null, P::Ref(self.metre)],
            );
            self.w.add(
                "IFCMAPCONVERSION",
                vec![
                    P::Ref(self.model_ctx),
                    P::Ref(crs),
                    P::Real(georef.easting),
                    P::Real(georef.northing),
                    P::Real(georef.altitude),
                    P::Real(rot.cos()),
                    P::Real(-rot.sin()),
                    P::Real(1.0),
                ],
            );
        }
    }

    fn spatial_structure(&mut self) {
        let doc = self.doc;
        let info = doc.info();
        let site_placement = self.placement(None, &Frame3::IDENTITY);
        let site_shape = match &doc.site().plot {
            Some(plot) if plot.len() >= 3 => {
                let curve = self.polyline(&plot.vertices, true);
                let rep = self.shape_rep(self.footprint_ctx, "FootPrint", "Curve2D", vec![curve]);
                P::Ref(self.product_shape(vec![rep]))
            }
            _ => P::Null,
        };
        let georef = &doc.site().georef;
        let site_guid = self.guid("site");
        let site = self.w.add(
            "IFCSITE",
            vec![
                P::s(site_guid),
                P::Null,
                P::s(if info.location.is_empty() { "Site".to_owned() } else { info.location.clone() }),
                P::Null,
                P::Null,
                P::Ref(site_placement),
                site_shape,
                P::Null,
                P::e("ELEMENT"),
                P::Null,
                P::Null,
                if georef.altitude.abs() > 1e-9 { P::Real(georef.altitude) } else { P::Null },
                P::Null,
                P::Null,
            ],
        );
        self.building_placement = self.placement(Some(site_placement), &Frame3::IDENTITY);
        let address = if info.location.is_empty() && info.municipality.is_empty() {
            P::Null
        } else {
            let lines =
                if info.location.is_empty() { P::Null } else { P::List(vec![P::s(info.location.clone())]) };
            P::Ref(self.w.add(
                "IFCPOSTALADDRESS",
                vec![
                    P::Null,
                    P::Null,
                    P::Null,
                    P::Null,
                    lines,
                    P::Null,
                    P::opt_s(&info.municipality),
                    P::Null,
                    P::Null,
                    P::Null,
                ],
            ))
        };
        let building_guid = self.guid("building");
        self.building = self.w.add(
            "IFCBUILDING",
            vec![
                P::s(building_guid),
                P::Null,
                P::s(if info.name.is_empty() { "Building".to_owned() } else { info.name.clone() }),
                P::Null,
                P::Null,
                P::Ref(self.building_placement),
                P::Null,
                P::Null,
                P::e("ELEMENT"),
                P::Real(georef.altitude),
                P::Real(doc.site().ground_level.as_m()),
                address,
            ],
        );
        let g = self.guid("rel:aggregates:project");
        self.w.add(
            "IFCRELAGGREGATES",
            vec![P::s(g), P::Null, P::Null, P::Null, P::Ref(self.project), P::refs([site])],
        );
        let g = self.guid("rel:aggregates:site");
        self.w.add(
            "IFCRELAGGREGATES",
            vec![P::s(g), P::Null, P::Null, P::Null, P::Ref(site), P::refs([self.building])],
        );

        let mut storey_ids = Vec::new();
        for s in doc.storeys() {
            let local = Frame3::translation(Vec3::new(0.0, 0.0, s.elevation.as_m()));
            let placement = self.placement(Some(self.building_placement), &local);
            let guid = self.guid(&format!("storey:{}", s.id.0));
            let id = self.w.add(
                "IFCBUILDINGSTOREY",
                vec![
                    P::s(guid),
                    P::Null,
                    P::s(s.name.clone()),
                    P::Null,
                    P::Null,
                    P::Ref(placement),
                    P::Null,
                    P::Null,
                    P::e("ELEMENT"),
                    P::Real(s.elevation.as_m()),
                ],
            );
            let key = format!("storey:{}", s.id.0);
            self.pset(
                &key,
                id,
                "Pset_BuildingStoreyCommon",
                vec![("AboveGround", Pv::Logical(!s.below_ground))],
            );
            self.qto(
                &key,
                id,
                "Qto_BuildingStoreyBaseQuantities",
                vec![Q::Length("GrossHeight", s.height.as_m())],
            );
            self.storeys.insert(s.id, (id, Host { placement, world: local, storey: Some(s.id) }));
            storey_ids.push(id);
            self.count("IfcBuildingStorey");
        }
        if !storey_ids.is_empty() {
            let g = self.guid("rel:aggregates:building");
            self.w.add(
                "IFCRELAGGREGATES",
                vec![P::s(g), P::Null, P::Null, P::Null, P::Ref(self.building), P::refs(storey_ids)],
            );
        }
    }

    fn host(&self, e: &Element) -> Host {
        e.storey.and_then(|s| self.storeys.get(&s)).map(|(_, h)| *h).unwrap_or(Host {
            placement: self.building_placement,
            world: Frame3::IDENTITY,
            storey: None,
        })
    }

    fn contain(&mut self, host: &Host, product: u64) {
        self.contained.entry(host.storey).or_default().push(product);
    }

    // ----- Elements --------------------------------------------------------------------------------------------

    fn run(&mut self) {
        self.header_and_project();
        self.spatial_structure();
        self.wall_shapes = wall_shapes(self.doc);
        let doc = self.doc;
        for e in doc.elements() {
            match &e.kind {
                ElementKind::Wall(w) => self.wall(e, w),
                ElementKind::Slab(s) => self.slab(e, s),
                ElementKind::Column(c) => self.column(e, c),
                ElementKind::Beam(b) => self.beam(e, b),
                ElementKind::Opening(_) => {}
                ElementKind::Stair(s) => self.stair(e, s),
                ElementKind::Roof(r) => self.roof(e, r),
                ElementKind::Railing(r) => self.railing(e, r),
                ElementKind::Space(s) => self.space(e, s),
                ElementKind::Object(o) => self.object(e, o),
                _ => self.skip(e),
            }
        }
        for e in doc.elements() {
            if let ElementKind::Opening(o) = &e.kind {
                self.opening(e, o);
            }
        }
        self.relationships();
    }

    fn name_of(e: &Element) -> String {
        if e.tag.is_empty() { format!("{} {}", e.category().key(), e.id.0) } else { e.tag.clone() }
    }

    fn product(
        &mut self,
        entity: &str,
        e: &Element,
        object_type: P,
        placement: u64,
        shape: Option<u64>,
        extra: Vec<P>,
    ) -> u64 {
        let guid = self.element_guid(e);
        let mut params = vec![
            P::s(guid),
            P::Null,
            P::s(Self::name_of(e)),
            P::Null,
            object_type,
            P::Ref(placement),
            shape.map(P::Ref).unwrap_or(P::Null),
            P::opt_s(&e.tag),
        ];
        params.extend(extra);
        self.w.add(entity, params)
    }

    fn wall(&mut self, e: &Element, w: &Wall) {
        let Some(shape) = self.wall_shapes.get(&e.id).cloned() else {
            self.warn(format!("wall {} has no length or thickness and was not exported", e.id.0));
            self.skip(e);
            return;
        };
        let Some(dir) = w.direction() else {
            self.skip(e);
            return;
        };
        let height = w.height.as_m();
        if height.is_nan() || height <= 0.0 {
            self.warn(format!("wall {} has no height and was not exported", e.id.0));
            self.skip(e);
            return;
        }
        let host = self.host(e);
        let local = Frame3::plan(w.start, w.base_offset.as_m(), dir.angle().as_rad());
        let world = host.world.then(&local);
        let placement = self.placement(Some(host.placement), &local);
        let length = w.length().as_m();

        let axis_item = match shape.arc {
            None => self.polyline(&[Point2::ORIGIN, Point2::new(w.start.distance(w.end).as_m(), 0.0)], false),
            Some(arc) => {
                let c = to_local(&local, arc.centre);
                let pos = self.axis2(c, None);
                let circle = self.w.add("IFCCIRCLE", vec![P::Ref(pos), P::Real(arc.radius.as_m())]);
                let a = self.point2(Point2::ORIGIN);
                let b = self.point2(to_local(&local, w.end));
                self.w.add(
                    "IFCTRIMMEDCURVE",
                    vec![
                        P::Ref(circle),
                        P::refs([a]),
                        P::refs([b]),
                        P::Bool(arc.sweep.as_rad() > 0.0),
                        P::e("CARTESIAN"),
                    ],
                )
            }
        };
        let axis_rep = self.shape_rep(self.axis_ctx, "Axis", "Curve2D", vec![axis_item]);
        let body_rep = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                let outline =
                    Polygon2::new(shape.outline.vertices.iter().map(|&p| to_local(&local, p)).collect());
                let chord = w.start.distance(w.end).as_m();
                let square = Polygon2::from_m(&[
                    (0.0, shape.right),
                    (chord, shape.right),
                    (chord, shape.left),
                    (0.0, shape.left),
                ]);
                let is_rect = shape.arc.is_none()
                    && outline.len() == 4
                    && (outline.area().as_m2() - square.area().as_m2()).abs() < 1e-9
                    && outline
                        .vertices
                        .iter()
                        .all(|p| square.vertices.iter().any(|q| p.distance(*q).as_m() < 1e-9));
                let profile = if is_rect {
                    self.rect_profile(
                        Point2::new(chord / 2.0, shape.centre_offset()),
                        None,
                        chord,
                        shape.thickness,
                    )
                } else {
                    self.profile(&PolygonWithHoles::from(outline))
                };
                let solid = self.extrusion(profile, &Frame3::IDENTITY, Vec3::Z, height);
                let core = self.doc.structure_core_material(&w.structure);
                self.styled(&[solid], core.map(StyleKey::Material).unwrap_or(StyleKey::Default));
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid])
            }
        };
        let shape_id = self.product_shape(vec![axis_rep, body_rep]);
        let product =
            self.product("IFCWALL", e, P::Null, placement, Some(shape_id), vec![P::e("NOTDEFINED")]);
        self.count("IfcWall");
        self.associate_layers(product, &w.structure, "AXIS2", shape.left);
        self.contain(&host, product);

        let external = matches!(w.structure, Structure::Layered { build_up } if Some(build_up) == self.doc.defaults().exterior_wall);
        let reference = match &w.structure {
            Structure::Layered { build_up } => self.doc.build_up(*build_up).map(|b| b.name.clone()),
            Structure::Basic { material, .. } => self.doc.material(*material).map(|m| m.name.clone()),
        };
        let key = format!("element:{}", e.id.0);
        let mut props = vec![("IsExternal", Pv::Bool(external)), ("Status", Pv::Enum(status(e.phase)))];
        if let Some(r) = reference {
            props.insert(0, ("Reference", Pv::Identifier(r)));
        }
        self.pset(&key, product, "Pset_WallCommon", props);

        let openings: Vec<(f64, f64)> = self
            .doc
            .openings_in(e.id)
            .map(|(_, o)| (o.width.as_m().max(0.0), o.height.as_m().max(0.0)))
            .collect();
        let opening_area: f64 = openings.iter().map(|(wd, h)| wd * h).sum();
        let footprint = shape.outline.area().as_m2();
        self.qto(
            &key,
            product,
            "Qto_WallBaseQuantities",
            vec![
                Q::Length("Length", length),
                Q::Length("Width", shape.thickness),
                Q::Length("Height", height),
                Q::Area("GrossFootprintArea", footprint),
                Q::Area("GrossSideArea", length * height),
                Q::Area("NetSideArea", (length * height - opening_area).max(0.0)),
                Q::Volume("GrossVolume", footprint * height),
                Q::Volume("NetVolume", (footprint * height - opening_area * shape.thickness).max(0.0)),
            ],
        );
        self.user_properties(e, product);
        self.walls.insert(e.id, WallHost { placement, local, host, shape, product, external });
    }

    fn opening(&mut self, e: &Element, o: &Opening) {
        let Some(wall_el) = self.doc.element(o.host) else {
            self.skip(e);
            return;
        };
        let Some(wall) = wall_el.as_wall().cloned() else {
            self.skip(e);
            return;
        };
        let (width, height) = (o.width.as_m(), o.height.as_m());
        let Some(host) = self.walls.get(&o.host) else {
            self.warn(format!("opening {} is in wall {}, which was not exported", e.id.0, o.host.0));
            self.skip(e);
            return;
        };
        if !(width > 0.0 && height > 0.0) {
            self.warn(format!("opening {} has no size and was not exported", e.id.0));
            self.skip(e);
            return;
        }
        let (wall_placement, wall_local, wall_host, shape, wall_product, external) =
            (host.placement, host.local, host.host, host.shape.clone(), host.product, host.external);
        let z = o.sill.as_m() - wall.base_offset.as_m();
        // Opening frame relative to the wall: at the opening centre on the reference line, X along the wall.
        let (rel, extra_depth) = match shape.arc {
            None => (Frame3::translation(Vec3::new(o.offset.as_m(), 0.0, z)), 0.0),
            Some(arc) => {
                let len = arc.length().as_m().max(1e-9);
                let t = o.offset.as_m() / len;
                let p = arc.at(t);
                let a = arc.start.as_rad() + arc.sweep.as_rad() * t;
                let radial = Vec2::new(a.cos(), a.sin());
                let tangent = radial.perp() * arc.sweep.as_rad().signum();
                let at = Frame3::plan(p, wall.base_offset.as_m() + z, tangent.angle().as_rad());
                let rel = wall_local.inverse().map(|inv| inv.then(&at)).unwrap_or(Frame3::IDENTITY);
                let r = arc.radius.as_m().max(1e-9);
                let sagitta = r - (r * r - (width / 2.0).powi(2)).max(0.0).sqrt();
                (rel, 2.0 * sagitta)
            }
        };
        let opening_placement = self.placement(Some(wall_placement), &rel);
        let margin = 0.05;
        let depth = shape.thickness + 2.0 * margin + extra_depth;
        let profile = self.rect_profile(Point2::new(0.0, shape.centre_offset()), None, width, depth);
        let solid = self.extrusion(profile, &Frame3::IDENTITY, Vec3::Z, height);
        let rep = self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid]);
        let opening_shape = self.product_shape(vec![rep]);
        let key = format!("element:{}", e.id.0);

        let filling = match o.kind {
            OpeningKind::Door => Some(("IFCDOOR", "IfcDoor", "DOOR")),
            OpeningKind::Window => Some(("IFCWINDOW", "IfcWindow", "WINDOW")),
            OpeningKind::Void => None,
        };
        let opening_guid =
            if filling.is_some() { self.guid(&format!("opening:{}", e.id.0)) } else { self.element_guid(e) };
        let opening_name = if filling.is_some() { format!("opening {}", e.id.0) } else { Self::name_of(e) };
        let opening = self.w.add(
            "IFCOPENINGELEMENT",
            vec![
                P::s(opening_guid),
                P::Null,
                P::s(opening_name),
                P::Null,
                P::Null,
                P::Ref(opening_placement),
                P::Ref(opening_shape),
                P::Null,
                P::e("OPENING"),
            ],
        );
        self.count("IfcOpeningElement");
        let g = self.guid(&format!("rel:voids:{}", e.id.0));
        self.w.add(
            "IFCRELVOIDSELEMENT",
            vec![P::s(g), P::Null, P::Null, P::Null, P::Ref(wall_product), P::Ref(opening)],
        );
        let wall_area_qto = vec![
            Q::Length("Width", width),
            Q::Length("Height", height),
            Q::Length("Depth", depth),
            Q::Area("Area", width * height),
            Q::Volume("Volume", width * height * depth),
        ];
        let opening_key = if filling.is_some() { format!("opening:{}", e.id.0) } else { key.clone() };
        self.qto(&opening_key, opening, "Qto_OpeningElementBaseQuantities", wall_area_qto);

        let mut kanavos_props = vec![
            ("Style", Pv::Label(o.style.clone())),
            ("FlipSide", Pv::Bool(o.flip_side)),
            ("FlipHand", Pv::Bool(o.flip_hand)),
        ];
        for (k, v) in &o.params {
            kanavos_props.push((k.as_str(), Pv::Real(*v)));
        }

        let Some((entity, ifc_name, predefined)) = filling else {
            self.pset(&key, opening, "Kanavos_Opening", kanavos_props);
            self.user_properties(e, opening);
            return;
        };
        let local = Frame3::IDENTITY;
        let world = wall_host.world.then(&wall_local).then(&rel);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                let panel = shape.thickness.clamp(0.02, 0.08);
                let profile = self.rect_profile(Point2::new(0.0, shape.centre_offset()), None, width, panel);
                let solid = self.extrusion(profile, &Frame3::IDENTITY, Vec3::Z, height);
                let style = if o.kind == OpeningKind::Door { StyleKey::Door } else { StyleKey::Window };
                self.styled(&[solid], style);
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid])
            }
        };
        let filling_placement = self.placement(Some(opening_placement), &local);
        let filling_shape = self.product_shape(vec![body]);
        let extra = vec![P::Real(height), P::Real(width), P::e(predefined), P::e("NOTDEFINED"), P::Null];
        let product =
            self.product(entity, e, P::s(o.style.clone()), filling_placement, Some(filling_shape), extra);
        self.count(ifc_name);
        let g = self.guid(&format!("rel:fills:{}", e.id.0));
        self.w.add(
            "IFCRELFILLSELEMENT",
            vec![P::s(g), P::Null, P::Null, P::Null, P::Ref(opening), P::Ref(product)],
        );
        self.contain(&wall_host, product);
        let (pset, qto) = if o.kind == OpeningKind::Door {
            ("Pset_DoorCommon", "Qto_DoorBaseQuantities")
        } else {
            ("Pset_WindowCommon", "Qto_WindowBaseQuantities")
        };
        let mut common = vec![("IsExternal", Pv::Bool(external)), ("Status", Pv::Enum(status(e.phase)))];
        if !e.tag.is_empty() {
            common.insert(0, ("Reference", Pv::Identifier(e.tag.clone())));
        }
        self.pset(&key, product, pset, common);
        self.pset(&key, product, "Kanavos_Opening", kanavos_props);
        self.qto(
            &key,
            product,
            qto,
            vec![Q::Length("Width", width), Q::Length("Height", height), Q::Area("Area", width * height)],
        );
        self.user_properties(e, product);
    }

    fn slab(&mut self, e: &Element, s: &Slab) {
        let thickness = self.doc.structure_thickness(&s.structure).map(|t| t.as_m()).unwrap_or(0.0);
        let tol = self.doc.tolerance();
        if (thickness.is_nan() || thickness <= 0.0) || !s.outline.outer.is_valid(&tol) {
            self.warn(format!(
                "slab {} has no thickness or a degenerate outline and was not exported",
                e.id.0
            ));
            self.skip(e);
            return;
        }
        let host = self.host(e);
        let local = Frame3::translation(Vec3::new(0.0, 0.0, s.top_offset.as_m()));
        let world = host.world.then(&local);
        let placement = self.placement(Some(host.placement), &local);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                let profile = self.profile(&s.outline);
                let solid = self.extrusion(profile, &Frame3::IDENTITY, Vec3::new(0.0, 0.0, -1.0), thickness);
                let core = self.doc.structure_core_material(&s.structure);
                self.styled(&[solid], core.map(StyleKey::Material).unwrap_or(StyleKey::Default));
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid])
            }
        };
        let shape = self.product_shape(vec![body]);
        let product = self.product("IFCSLAB", e, P::Null, placement, Some(shape), vec![P::e("FLOOR")]);
        self.count("IfcSlab");
        self.associate_layers(product, &s.structure, "AXIS3", 0.0);
        self.contain(&host, product);
        let key = format!("element:{}", e.id.0);
        self.pset(
            &key,
            product,
            "Pset_SlabCommon",
            vec![("IsExternal", Pv::Bool(false)), ("Status", Pv::Enum(status(e.phase)))],
        );
        let gross = s.outline.outer.area().as_m2();
        let net = s.outline.area().as_m2();
        self.qto(
            &key,
            product,
            "Qto_SlabBaseQuantities",
            vec![
                Q::Length("Width", thickness),
                Q::Length("Perimeter", s.outline.outer.perimeter().as_m()),
                Q::Area("GrossArea", gross),
                Q::Area("NetArea", net),
                Q::Volume("GrossVolume", gross * thickness),
                Q::Volume("NetVolume", net * thickness),
            ],
        );
        self.user_properties(e, product);
    }

    fn column(&mut self, e: &Element, c: &Column) {
        let height = c.height.as_m();
        let area = match c.profile {
            Profile::Rectangle { width, depth } => width.as_m() * depth.as_m(),
            Profile::Circle { diameter } => std::f64::consts::PI * (diameter.as_m() / 2.0).powi(2),
        };
        if !(height > 0.0 && area > 0.0) {
            self.warn(format!("column {} is degenerate and was not exported", e.id.0));
            self.skip(e);
            return;
        }
        let host = self.host(e);
        let local = Frame3::plan(c.position, c.base_offset.as_m(), c.rotation.as_rad());
        let world = host.world.then(&local);
        let placement = self.placement(Some(host.placement), &local);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                let profile = match c.profile {
                    Profile::Rectangle { width, depth } => {
                        self.rect_profile(Point2::ORIGIN, None, width.as_m(), depth.as_m())
                    }
                    Profile::Circle { diameter } => {
                        self.circle_profile(Point2::ORIGIN, diameter.as_m() / 2.0)
                    }
                };
                let solid = self.extrusion(profile, &Frame3::IDENTITY, Vec3::Z, height);
                self.styled(&[solid], StyleKey::Material(c.material));
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid])
            }
        };
        let shape = self.product_shape(vec![body]);
        let product = self.product("IFCCOLUMN", e, P::Null, placement, Some(shape), vec![P::e("COLUMN")]);
        self.count("IfcColumn");
        self.associate_material(product, c.material);
        self.contain(&host, product);
        let key = format!("element:{}", e.id.0);
        self.pset(
            &key,
            product,
            "Pset_ColumnCommon",
            vec![("LoadBearing", Pv::Bool(true)), ("Status", Pv::Enum(status(e.phase)))],
        );
        self.qto(
            &key,
            product,
            "Qto_ColumnBaseQuantities",
            vec![
                Q::Length("Length", height),
                Q::Area("CrossSectionArea", area),
                Q::Volume("GrossVolume", area * height),
                Q::Volume("NetVolume", area * height),
            ],
        );
        self.user_properties(e, product);
    }

    fn beam(&mut self, e: &Element, b: &Beam) {
        let length = b.start.distance(b.end).as_m();
        let area = match b.profile {
            Profile::Rectangle { width, depth } => width.as_m() * depth.as_m(),
            Profile::Circle { diameter } => std::f64::consts::PI * (diameter.as_m() / 2.0).powi(2),
        };
        let Some(dir) = (b.end - b.start).normalized() else {
            self.skip(e);
            return;
        };
        if !(length > 0.0 && area > 0.0) {
            self.warn(format!("beam {} is degenerate and was not exported", e.id.0));
            self.skip(e);
            return;
        }
        let host = self.host(e);
        let local = Frame3::plan(b.start, b.top_offset.as_m(), dir.angle().as_rad());
        let world = host.world.then(&local);
        let placement = self.placement(Some(host.placement), &local);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                // Profile in the beam's cross-section: profile x = beam Y (width), profile y = up; top at z = 0.
                let profile = match b.profile {
                    Profile::Rectangle { width, depth } => self.rect_profile(
                        Point2::new(0.0, -depth.as_m() / 2.0),
                        None,
                        width.as_m(),
                        depth.as_m(),
                    ),
                    Profile::Circle { diameter } => {
                        self.circle_profile(Point2::new(0.0, -diameter.as_m() / 2.0), diameter.as_m() / 2.0)
                    }
                };
                let position = Frame3::from_axes(Point3::ORIGIN, Some(Vec3::X), Some(Vec3::Y));
                let solid = self.extrusion(profile, &position, Vec3::Z, length);
                self.styled(&[solid], StyleKey::Material(b.material));
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid])
            }
        };
        let axis = self.polyline(&[Point2::ORIGIN, Point2::new(length, 0.0)], false);
        let axis_rep = self.shape_rep(self.axis_ctx, "Axis", "Curve2D", vec![axis]);
        let shape = self.product_shape(vec![axis_rep, body]);
        let product = self.product("IFCBEAM", e, P::Null, placement, Some(shape), vec![P::e("BEAM")]);
        self.count("IfcBeam");
        self.associate_material(product, b.material);
        self.contain(&host, product);
        let key = format!("element:{}", e.id.0);
        self.pset(
            &key,
            product,
            "Pset_BeamCommon",
            vec![("LoadBearing", Pv::Bool(true)), ("Status", Pv::Enum(status(e.phase)))],
        );
        self.qto(
            &key,
            product,
            "Qto_BeamBaseQuantities",
            vec![
                Q::Length("Length", length),
                Q::Area("CrossSectionArea", area),
                Q::Volume("GrossVolume", area * length),
                Q::Volume("NetVolume", area * length),
            ],
        );
        self.user_properties(e, product);
    }

    fn space(&mut self, e: &Element, s: &Space) {
        let tol = self.doc.tolerance();
        let storey_height = e.storey.and_then(|id| self.doc.storey(id)).map(|st| st.height.as_m());
        let height = s.height.map(Length::as_m).or(storey_height).unwrap_or(3.0);
        if !s.boundary.outer.is_valid(&tol) || (height.is_nan() || height <= 0.0) {
            self.warn(format!("space {} has a degenerate boundary and was not exported", e.id.0));
            self.skip(e);
            return;
        }
        let host = self.host(e);
        let local = Frame3::IDENTITY;
        let world = host.world;
        let placement = self.placement(Some(host.placement), &local);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                let profile = self.profile(&s.boundary);
                let solid = self.extrusion(profile, &Frame3::IDENTITY, Vec3::Z, height);
                self.styled(&[solid], StyleKey::Space);
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid])
            }
        };
        let footprint = self.polyline(&s.boundary.outer.to_ccw().vertices, true);
        let footprint_rep = self.shape_rep(self.footprint_ctx, "FootPrint", "Curve2D", vec![footprint]);
        let shape = self.product_shape(vec![footprint_rep, body]);
        let external = space_is_external(s.category);
        let predefined = if s.category == SpaceCategory::Parking {
            "PARKING"
        } else if external {
            "EXTERNAL"
        } else {
            "INTERNAL"
        };
        let guid = self.element_guid(e);
        let name = if s.number.is_empty() { s.name.clone() } else { s.number.clone() };
        let product = self.w.add(
            "IFCSPACE",
            vec![
                P::s(guid),
                P::Null,
                P::s(name),
                P::Null,
                P::Null,
                P::Ref(placement),
                P::Ref(shape),
                P::s(s.name.clone()),
                P::e("ELEMENT"),
                P::e(predefined),
                P::Null,
            ],
        );
        self.count("IfcSpace");
        if let Some(st) = host.storey {
            self.spaces.entry(st).or_default().push(product);
        } else {
            self.contain(&host, product);
        }
        let key = format!("element:{}", e.id.0);
        let mut common = vec![("IsExternal", Pv::Bool(external))];
        if !s.number.is_empty() {
            common.insert(0, ("Reference", Pv::Identifier(s.number.clone())));
        }
        self.pset(&key, product, "Pset_SpaceCommon", common);
        let mut own = vec![("Category", Pv::Label(space_category_key(s.category).to_owned()))];
        if !s.finish.is_empty() {
            own.push(("Finish", Pv::Label(s.finish.clone())));
        }
        self.pset(&key, product, "Kanavos_Space", own);
        let area = s.boundary.area().as_m2();
        let perimeter = s.boundary.outer.perimeter().as_m();
        self.qto(
            &key,
            product,
            "Qto_SpaceBaseQuantities",
            vec![
                Q::Length("Height", height),
                Q::Length("GrossPerimeter", perimeter),
                Q::Length("NetPerimeter", perimeter),
                Q::Area("GrossFloorArea", area),
                Q::Area("NetFloorArea", area),
                Q::Volume("GrossVolume", area * height),
                Q::Volume("NetVolume", area * height),
            ],
        );
        self.user_properties(e, product);
    }

    fn stair(&mut self, e: &Element, s: &Stair) {
        let host = self.host(e);
        let local = Frame3::plan(s.origin, s.base_offset.as_m(), s.direction.as_rad());
        let world = host.world.then(&local);
        let placement = self.placement(Some(host.placement), &local);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => Some(rep),
            None => match shapes::stair_parts(s) {
                None => None,
                Some((flights, landings)) => {
                    let mut items = Vec::new();
                    for f in &flights {
                        // Extrude the side profile across the flight, from its left edge to its right.
                        let left = f.direction.perp();
                        let right = left * -1.0;
                        let edge = f.start + left * (f.width / 2.0);
                        let position = Frame3::from_axes(
                            Point3::from_plan(edge, 0.0),
                            Some(Vec3::new(right.x, right.y, 0.0)),
                            Some(Vec3::new(f.direction.x, f.direction.y, 0.0)),
                        );
                        let profile = self.profile(&PolygonWithHoles::from(f.profile.clone()));
                        items.push(self.extrusion(profile, &position, Vec3::Z, f.width));
                    }
                    for l in &landings {
                        let profile = self.profile(&PolygonWithHoles::from(l.outline.clone()));
                        let position = Frame3::translation(Vec3::new(0.0, 0.0, l.top));
                        items.push(self.extrusion(
                            profile,
                            &position,
                            Vec3::new(0.0, 0.0, -1.0),
                            l.thickness,
                        ));
                    }
                    self.styled(&items, StyleKey::Material(s.material));
                    Some(self.shape_rep(self.body_ctx, "Body", "SweptSolid", items))
                }
            },
        };
        if body.is_none() {
            self.warn(format!("stair {} has degenerate parameters; exported without geometry", e.id.0));
        }
        let shape = body.map(|b| self.product_shape(vec![b]));
        let predefined = match s.shape {
            kanavos_model::StairShape::Straight => "STRAIGHT_RUN_STAIR",
            kanavos_model::StairShape::QuarterTurnLeft => "QUARTER_TURN_STAIR",
            kanavos_model::StairShape::QuarterTurnRight => "QUARTER_TURN_STAIR",
            kanavos_model::StairShape::HalfTurnLeft => "HALF_TURN_STAIR",
            kanavos_model::StairShape::HalfTurnRight => "HALF_TURN_STAIR",
        };
        let product = self.product("IFCSTAIR", e, P::Null, placement, shape, vec![P::e(predefined)]);
        self.count("IfcStair");
        self.associate_material(product, s.material);
        self.contain(&host, product);
        let key = format!("element:{}", e.id.0);
        self.pset(
            &key,
            product,
            "Pset_StairCommon",
            vec![
                ("NumberOfRiser", Pv::Count(i64::from(s.risers))),
                ("NumberOfTreads", Pv::Count(i64::from(s.risers.saturating_sub(1)))),
                ("RiserHeight", Pv::Length(s.riser_height().as_m())),
                ("TreadLength", Pv::Length(s.going.as_m())),
                ("Status", Pv::Enum(status(e.phase))),
            ],
        );
        self.user_properties(e, product);
    }

    fn roof(&mut self, e: &Element, r: &Roof) {
        let thickness = self.doc.structure_thickness(&r.structure).map(|t| t.as_m()).unwrap_or(0.0);
        let tol = self.doc.tolerance();
        if (thickness.is_nan() || thickness <= 0.0) || !r.outline.is_valid(&tol) {
            self.warn(format!(
                "roof {} has no thickness or a degenerate outline and was not exported",
                e.id.0
            ));
            self.skip(e);
            return;
        }
        let host = self.host(e);
        let local = Frame3::IDENTITY;
        let world = host.world;
        let placement = self.placement(Some(host.placement), &local);
        let core = self.doc.structure_core_material(&r.structure);
        let style = core.map(StyleKey::Material).unwrap_or(StyleKey::Default);
        let mut flat = matches!(r.shape, RoofShape::Flat { .. });
        let mut projected = r.outline.area().as_m2();
        let mut surface = projected;
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                let mut rep = None;
                if !flat {
                    match shapes::pitched_roof(r, thickness) {
                        Ok(poly) => {
                            if let Some(item) = self.brep(&poly) {
                                self.styled(&[item], style);
                                surface = roof_top_area(&poly);
                                rep = Some(self.shape_rep(self.body_ctx, "Body", "Brep", vec![item]));
                            }
                        }
                        Err(why) => {
                            self.warn(format!("roof {} exported as flat: {why:?} outline", e.id.0));
                            flat = true;
                        }
                    }
                }
                match rep {
                    Some(rep) => rep,
                    None => {
                        let mut items = Vec::new();
                        let profile = self.profile(&PolygonWithHoles::from(r.outline.clone()));
                        let position = Frame3::translation(Vec3::new(0.0, 0.0, r.base.as_m()));
                        items.push(self.extrusion(profile, &position, Vec3::new(0.0, 0.0, -1.0), thickness));
                        if let RoofShape::Flat { parapet: Some(p) } = &r.shape {
                            match shapes::inset(&r.outline, p.thickness.as_m()) {
                                Some(inner) if p.height.as_m() > 0.0 => {
                                    let ring = PolygonWithHoles {
                                        outer: r.outline.clone(),
                                        holes: vec![inner.reversed()],
                                    };
                                    let profile = self.profile(&ring);
                                    items.push(self.extrusion(profile, &position, Vec3::Z, p.height.as_m()));
                                }
                                _ => self.warn(format!("roof {}: parapet could not be built", e.id.0)),
                            }
                        }
                        self.styled(&items, style);
                        projected = r.outline.area().as_m2();
                        surface = projected;
                        self.shape_rep(self.body_ctx, "Body", "SweptSolid", items)
                    }
                }
            }
        };
        let shape = self.product_shape(vec![body]);
        let predefined = match (&r.shape, flat) {
            (_, true) => "FLAT_ROOF",
            (RoofShape::Pitched { slopes }, false) => {
                let sloped = slopes.iter().filter(|s| s.is_some()).count();
                if sloped == 1 {
                    "SHED_ROOF"
                } else if sloped == slopes.len() {
                    "HIP_ROOF"
                } else if sloped == 2 {
                    "GABLE_ROOF"
                } else {
                    "NOTDEFINED"
                }
            }
            _ => "NOTDEFINED",
        };
        let product = self.product("IFCROOF", e, P::Null, placement, Some(shape), vec![P::e(predefined)]);
        self.count("IfcRoof");
        if flat {
            // The roof slab hangs below its top surface at `base`, like a floor slab below its top.
            self.associate_layers(product, &r.structure, "AXIS3", r.base.as_m());
        } else {
            self.associate_layer_set(product, &r.structure);
        }
        self.contain(&host, product);
        let key = format!("element:{}", e.id.0);
        self.pset(
            &key,
            product,
            "Pset_RoofCommon",
            vec![("IsExternal", Pv::Bool(true)), ("Status", Pv::Enum(status(e.phase)))],
        );
        self.qto(
            &key,
            product,
            "Qto_RoofBaseQuantities",
            vec![
                Q::Area("GrossArea", surface),
                Q::Area("NetArea", surface),
                Q::Area("ProjectedArea", projected),
            ],
        );
        self.user_properties(e, product);
    }

    fn railing(&mut self, e: &Element, r: &Railing) {
        let height = r.height.as_m();
        let segments: Vec<(Point2, Point2)> = r
            .path
            .windows(2)
            .filter_map(|w| match w {
                [a, b] if a.distance(*b).as_m() > 1e-6 => Some((*a, *b)),
                _ => None,
            })
            .collect();
        if segments.is_empty() || (height.is_nan() || height <= 0.1) {
            self.warn(format!("railing {} is degenerate and was not exported", e.id.0));
            self.skip(e);
            return;
        }
        let host = self.host(e);
        let local = Frame3::translation(Vec3::new(0.0, 0.0, r.base_offset.as_m()));
        let world = host.world.then(&local);
        let placement = self.placement(Some(host.placement), &local);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                // Each segment: a 60 × 40 mm top rail and a 20 mm infill panel below it.
                let mut items = Vec::new();
                for (a, b) in &segments {
                    if let Some(rail) = shapes::band(*a, *b, 0.06) {
                        let profile = self.profile(&PolygonWithHoles::from(rail));
                        let position = Frame3::translation(Vec3::new(0.0, 0.0, height - 0.04));
                        items.push(self.extrusion(profile, &position, Vec3::Z, 0.04));
                    }
                    if let Some(panel) = shapes::band(*a, *b, 0.02) {
                        let profile = self.profile(&PolygonWithHoles::from(panel));
                        let position = Frame3::translation(Vec3::new(0.0, 0.0, 0.05));
                        items.push(self.extrusion(profile, &position, Vec3::Z, (height - 0.09).max(0.01)));
                    }
                }
                self.styled(&items, StyleKey::Material(r.material));
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", items)
            }
        };
        let shape = self.product_shape(vec![body]);
        let product = self.product("IFCRAILING", e, P::Null, placement, Some(shape), vec![P::e("GUARDRAIL")]);
        self.count("IfcRailing");
        self.associate_material(product, r.material);
        self.contain(&host, product);
        let key = format!("element:{}", e.id.0);
        let length: f64 = segments.iter().map(|(a, b)| a.distance(*b).as_m()).sum();
        self.pset(
            &key,
            product,
            "Pset_RailingCommon",
            vec![("Height", Pv::Length(height)), ("Status", Pv::Enum(status(e.phase)))],
        );
        self.qto(&key, product, "Qto_RailingBaseQuantities", vec![Q::Length("Length", length)]);
        self.user_properties(e, product);
    }

    fn object(&mut self, e: &Element, o: &Object) {
        let param = |k: &str, default: f64| {
            o.params.get(k).copied().filter(|v| *v > 0.0 && v.is_finite()).unwrap_or(default)
        };
        let (w, d, h) = (param("width", 0.6), param("depth", 0.6), param("height", 0.75));
        let host = self.host(e);
        let local = Frame3::plan(o.position, o.elevation.as_m(), o.rotation.as_rad());
        let world = host.world.then(&local);
        let placement = self.placement(Some(host.placement), &local);
        let body = match self.callback_body(e.id, &world) {
            Some(rep) => rep,
            None => {
                // Placeholder: the object's bounding box, from its insertion point (mirrored to the left).
                let x0 = if o.mirrored { -w } else { 0.0 };
                let profile = self.rect_profile(Point2::new(x0 + w / 2.0, d / 2.0), None, w, d);
                let solid = self.extrusion(profile, &Frame3::IDENTITY, Vec3::Z, h);
                self.styled(&[solid], StyleKey::Default);
                self.shape_rep(self.body_ctx, "Body", "SweptSolid", vec![solid])
            }
        };
        let shape = self.product_shape(vec![body]);
        let def = o.definition.as_str();
        let family = def.split('.').next().unwrap_or("");
        let (entity, ifc_name, predefined) = match family {
            "furniture" | "kitchen" => ("IFCFURNITURE", "IfcFurniture", "NOTDEFINED"),
            "sanitary" | "bathroom" => {
                let p = if def.contains("wc") || def.contains("toilet") {
                    "TOILETPAN"
                } else if def.contains("basin") {
                    "WASHHANDBASIN"
                } else if def.contains("shower") {
                    "SHOWER"
                } else if def.contains("bath") {
                    "BATH"
                } else if def.contains("bidet") {
                    "BIDET"
                } else if def.contains("sink") {
                    "SINK"
                } else {
                    "NOTDEFINED"
                };
                ("IFCSANITARYTERMINAL", "IfcSanitaryTerminal", p)
            }
            _ => ("IFCBUILDINGELEMENTPROXY", "IfcBuildingElementProxy", "NOTDEFINED"),
        };
        let product = self.product(entity, e, P::s(def), placement, Some(shape), vec![P::e(predefined)]);
        self.count(ifc_name);
        self.contain(&host, product);
        let key = format!("element:{}", e.id.0);
        let mut props =
            vec![("Definition", Pv::Label(o.definition.clone())), ("Mirrored", Pv::Bool(o.mirrored))];
        for (k, v) in &o.params {
            props.push((k.as_str(), Pv::Real(*v)));
        }
        self.pset(&key, product, "Kanavos_Object", props);
        self.user_properties(e, product);
    }

    // ----- Relationships ---------------------------------------------------------------------------------------

    fn relationships(&mut self) {
        let contained = std::mem::take(&mut self.contained);
        for (storey, products) in contained {
            let (structure, key) = match storey.and_then(|s| self.storeys.get(&s).map(|(id, _)| (*id, s))) {
                Some((id, s)) => (id, format!("rel:contains:storey:{}", s.0)),
                None => (self.building, "rel:contains:building".to_owned()),
            };
            let g = self.guid(&key);
            self.w.add(
                "IFCRELCONTAINEDINSPATIALSTRUCTURE",
                vec![P::s(g), P::Null, P::Null, P::Null, P::refs(products), P::Ref(structure)],
            );
        }
        let spaces = std::mem::take(&mut self.spaces);
        for (storey, products) in spaces {
            let Some((id, _)) = self.storeys.get(&storey).copied() else { continue };
            let g = self.guid(&format!("rel:aggregates:storey:{}", storey.0));
            self.w.add(
                "IFCRELAGGREGATES",
                vec![P::s(g), P::Null, P::Null, P::Null, P::Ref(id), P::refs(products)],
            );
        }
        let rels = std::mem::take(&mut self.material_rels);
        for (key, (material, products)) in rels {
            let g = self.guid(&format!("rel:material:{key}"));
            self.w.add(
                "IFCRELASSOCIATESMATERIAL",
                vec![P::s(g), P::Null, P::Null, P::Null, P::refs(products), P::Ref(material)],
            );
        }
    }

    fn finish(self) -> Exported {
        let mut report = self.report;
        report.instances = self.w.count();
        let opts = self.opts;
        let header = [
            (
                "FILE_DESCRIPTION",
                vec![P::List(vec![P::s("ViewDefinition [DesignTransferView]")]), P::s("2;1")],
            ),
            (
                "FILE_NAME",
                vec![
                    P::s(opts.file_name.clone()),
                    P::s(opts.timestamp.clone()),
                    P::List(vec![P::s(opts.author.clone())]),
                    P::List(vec![P::s(opts.organization.clone())]),
                    P::s(format!("Kanavos IFC writer {}", env!("CARGO_PKG_VERSION"))),
                    P::s(opts.application.clone()),
                    P::s(""),
                ],
            ),
            ("FILE_SCHEMA", vec![P::List(vec![P::s(EXPORT_SCHEMA)])]),
        ];
        Exported { text: self.w.finish(&header), report }
    }
}

/// Area of the upward-facing faces of a polyhedron.
fn roof_top_area(p: &Polyhedron) -> f64 {
    let mut total = 0.0;
    for f in &p.faces {
        let pts: Vec<Point3> = f.iter().filter_map(|&i| p.vertices.get(i).copied()).collect();
        let mut n = Vec3::ZERO;
        for (k, a) in pts.iter().enumerate() {
            if let Some(b) = pts.get((k + 1) % pts.len()) {
                n += a.to_vec().cross(b.to_vec());
            }
        }
        if n.z > 1e-9 {
            total += n.length() / 2.0;
        }
    }
    total
}

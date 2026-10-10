//! IFC import for IFC 2x3, IFC 4 and IFC 4.3. The mapping is documented in `docs/formats/ifc.md` and specified
//! in `specs/files/ifc-import.md`.

mod graph;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use kanavos_drawing::Rgba;
use kanavos_geom::{Angle, Length, Point2, Point3, PolygonWithHoles, Vec2, Vec3};
use kanavos_model::{
    Beam, BuildUp, BuildUpLayer, BuildUpUsage, Column, Document, Element, ElementKind, Georef, Id, Material,
    MaterialCategory, Opening, OpeningKind, Phase, Profile, ProjectId, ReferenceLine, Slab, Space,
    SpaceCategory, Storey, Structure, View, ViewKind, Wall, starter_with_id,
};

pub use graph::Units;

use crate::IfcError;
use crate::math::Frame3;
use crate::step::{Entity, StepFile, Value, parse};
use graph::{Extrusion, Geometry, ProfileKind, is_curved, project_units, unit_factor};

/// IFC schema family of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schema {
    /// IFC 2x3 (`IFC2X3`).
    Ifc2x3,
    /// IFC 4 (`IFC4`, including the `IFC4X1` and `IFC4X2` drafts).
    Ifc4,
    /// IFC 4.3 (`IFC4X3`, `IFC4X3_ADD2`, …).
    Ifc4x3,
}

impl Schema {
    /// Recognises a `FILE_SCHEMA` identifier.
    pub fn from_identifier(id: &str) -> Option<Schema> {
        let id = id.trim().to_ascii_uppercase();
        if id.starts_with("IFC2X3") {
            Some(Schema::Ifc2x3)
        } else if id.starts_with("IFC4X3") {
            Some(Schema::Ifc4x3)
        } else if id.starts_with("IFC4") {
            Some(Schema::Ifc4)
        } else {
            None
        }
    }
}

/// What an import starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportBase {
    /// The starter template without its storeys: materials, build-ups, defaults, the 3D view, site plan,
    /// coverage diagram, area table and schedules are ready to use (default).
    #[default]
    Starter,
    /// An empty document.
    Empty,
}

/// How a file is imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOptions {
    /// Starting document.
    pub base: ImportBase,
    /// Name of the material created for elements without one.
    pub fallback_material: String,
    /// Keep each element's IFC `GlobalId` as its external id (`Element::external_id`) so a later export reuses
    /// it.
    pub keep_global_ids: bool,
    /// Object style for doors without a Kanavos style.
    pub door_style: String,
    /// Object style for windows without a Kanavos style.
    pub window_style: String,
    /// Object style for empty openings.
    pub void_style: String,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            base: ImportBase::Starter,
            fallback_material: "Unspecified material".to_owned(),
            keep_global_ids: true,
            door_style: "door.single_swing".to_owned(),
            window_style: "window.casement".to_owned(),
            void_style: "void".to_owned(),
        }
    }
}

/// An instance that was not imported, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportIssue {
    /// Instance id in the file (`#n`).
    pub entity: u64,
    /// Entity type as written in the file, e.g. `IFCROOF`.
    pub ifc_type: String,
    /// Its `GlobalId`, if it has one.
    pub global_id: String,
    /// Reason.
    pub reason: String,
}

/// What an import read, mapped and left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Schema identifier from the file header.
    pub schema: String,
    /// Instances per entity type in the file.
    pub entity_counts: BTreeMap<String, usize>,
    /// Instances mapped to Kanavos objects, per entity type.
    pub mapped: BTreeMap<String, usize>,
    /// Products not mapped, per entity type and reason.
    pub skipped: BTreeMap<String, BTreeMap<String, usize>>,
    /// Every product not mapped, individually.
    pub issues: Vec<ImportIssue>,
    /// Simplifications made while mapping (for example clipping ignored), with counts.
    pub simplifications: BTreeMap<String, usize>,
    /// File-level remarks (units, georeferencing, storeys created).
    pub warnings: Vec<String>,
}

impl ImportReport {
    /// Number of products not mapped.
    pub fn skipped_total(&self) -> usize {
        self.skipped.values().flat_map(BTreeMap::values).sum()
    }
}

/// The result of an import.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The new document.
    pub document: Document,
    /// Report.
    pub report: ImportReport,
}

/// Imports IFC (ISO 10303-21 clear text) into a new document.
pub fn import(bytes: &[u8], options: &ImportOptions) -> Result<Imported, IfcError> {
    let file = parse(bytes)?;
    import_parsed(&file, options)
}

/// Imports an IFC file.
pub fn import_file(path: &Path, options: &ImportOptions) -> Result<Imported, IfcError> {
    let bytes = std::fs::read(path).map_err(|e| IfcError::Io {
        action: "reading",
        path: path.to_path_buf(),
        source: e,
    })?;
    import(&bytes, options)
}

/// Imports an already parsed file.
pub fn import_parsed(file: &StepFile, options: &ImportOptions) -> Result<Imported, IfcError> {
    let schema_id = file.schemas().into_iter().next().unwrap_or_default();
    if Schema::from_identifier(&schema_id).is_none() {
        return Err(IfcError::UnsupportedSchema(schema_id));
    }
    let project = file.of_type("IFCPROJECT").first().and_then(|&p| file.get(p)).ok_or(IfcError::NoProject)?;
    let units = project_units(file, project);
    let mut im = Importer::new(file, units, options);
    im.report.schema = schema_id;
    im.report.entity_counts = file.type_counts();
    if (units.length - 1.0).abs() > 1e-12 {
        im.report.warnings.push(format!("lengths converted from file units ({} m per unit)", units.length));
    }
    im.run(project);
    Ok(Imported { document: im.doc, report: im.report })
}

const WALL_TYPES: &[&str] = &["IFCWALL", "IFCWALLSTANDARDCASE", "IFCWALLELEMENTEDCASE"];
const SLAB_TYPES: &[&str] = &["IFCSLAB", "IFCSLABSTANDARDCASE", "IFCSLABELEMENTEDCASE"];
const COLUMN_TYPES: &[&str] = &["IFCCOLUMN", "IFCCOLUMNSTANDARDCASE"];
const BEAM_TYPES: &[&str] = &["IFCBEAM", "IFCBEAMSTANDARDCASE"];
const DOOR_TYPES: &[&str] = &["IFCDOOR", "IFCDOORSTANDARDCASE"];
const WINDOW_TYPES: &[&str] = &["IFCWINDOW", "IFCWINDOWSTANDARDCASE"];
const OBJECT_TYPES: &[&str] = &[
    "IFCFURNITURE",
    "IFCFURNISHINGELEMENT",
    "IFCSANITARYTERMINAL",
    "IFCFLOWTERMINAL",
    "IFCBUILDINGELEMENTPROXY",
];
const SPATIAL_TYPES: &[&str] = &[
    "IFCPROJECT",
    "IFCSITE",
    "IFCBUILDING",
    "IFCBUILDINGSTOREY",
    "IFCSPACE",
    "IFCFACILITY",
    "IFCFACILITYPART",
];

/// A vertical or other extruded solid in world coordinates.
struct Solid {
    /// The profile mapped into the plan (for vertical solids) or into the profile plane's world position.
    area: PolygonWithHoles,
    /// Profile corner points in world coordinates, at both ends of the extrusion.
    corners: Vec<Point3>,
    zmin: f64,
    zmax: f64,
    vertical: bool,
    /// World frame of the profile plane.
    frame: Frame3,
    /// Extrusion vector, world.
    vector: Vec3,
    kind: ProfileKind,
    /// Profile position (2D) inside the plane.
    position: crate::math::Frame2,
}

struct StoreyInfo {
    ifc: Option<u64>,
    id: Id,
    name: String,
    elevation: f64,
    top: f64,
}

struct WallFrame {
    id: Id,
    start: Point2,
    dir: Vec2,
    storey: usize,
    /// For curved walls: centre, reference radius, start angle and signed sweep.
    arc: Option<(Point2, f64, f64, f64)>,
}

impl WallFrame {
    /// Distance along the reference line from the wall start to the projection of a plan point.
    fn along(&self, p: Point2) -> f64 {
        match self.arc {
            None => self.dir.dot(p - self.start),
            Some((centre, radius, a0, sweep)) => {
                let tau = std::f64::consts::TAU;
                let v = p - centre;
                let theta = v.y.atan2(v.x);
                let raw = ((theta - a0) * sweep.signum()).rem_euclid(tau);
                // Points just before the start come out near a full turn; bring them back.
                let raw = if raw > sweep.abs() + (tau - sweep.abs()) / 2.0 { raw - tau } else { raw };
                raw * radius
            }
        }
    }
}

/// A wall axis in plan.
enum Axis {
    Straight(Point2, Point2),
    Arc { centre: Point2, radius: f64, start: Point2, end: Point2, sweep: f64 },
}

struct Importer<'a> {
    f: &'a StepFile,
    g: Geometry<'a>,
    opts: &'a ImportOptions,
    doc: Document,
    report: ImportReport,
    contained: HashMap<u64, u64>,
    parent: HashMap<u64, u64>,
    voids: Vec<(u64, u64)>,
    fills: HashMap<u64, u64>,
    material_of: HashMap<u64, u64>,
    type_of: HashMap<u64, u64>,
    psets: HashMap<u64, Vec<u64>>,
    colours: HashMap<u64, Rgba>,
    storeys: Vec<StoreyInfo>,
    storey_by_ifc: HashMap<u64, usize>,
    materials_by_name: HashMap<String, Id>,
    material_cache: HashMap<u64, Id>,
    build_up_cache: HashMap<(u64, bool), Option<(Id, f64)>>,
    walls: HashMap<u64, WallFrame>,
    slabs: HashMap<u64, Id>,
    handled: HashSet<u64>,
}

fn phase_from(status: Option<&str>) -> Phase {
    match status.map(str::to_ascii_uppercase).as_deref() {
        Some("EXISTING") => Phase::Existing,
        Some("DEMOLISH" | "DEMOLISHED") => Phase::Demolished,
        _ => Phase::New,
    }
}

fn space_category(key: &str) -> Option<SpaceCategory> {
    Some(match key {
        "main_use" => SpaceCategory::MainUse,
        "auxiliary" => SpaceCategory::Auxiliary,
        "stairwell" => SpaceCategory::Stairwell,
        "lift_shaft" => SpaceCategory::LiftShaft,
        "semi_open" => SpaceCategory::SemiOpen,
        "balcony" => SpaceCategory::Balcony,
        "pilotis" => SpaceCategory::Pilotis,
        "parking" => SpaceCategory::Parking,
        "attic" => SpaceCategory::Attic,
        "technical" => SpaceCategory::Technical,
        "shaft" => SpaceCategory::Shaft,
        "other" => SpaceCategory::Other,
        _ => return None,
    })
}

/// Guesses a material family from its name or IFC category (English and Greek keywords).
fn guess_category(text: &str) -> MaterialCategory {
    let t = text.to_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| t.contains(k));
    if has(&["concrete", "beton", "σκυρόδεμα", "μπετό"]) {
        MaterialCategory::Concrete
    } else if has(&[
        "insulation",
        "eps",
        "xps",
        "mineral wool",
        "rockwool",
        "θερμομόνωση",
        "μόνωση",
        "πετροβάμβακ",
    ]) {
        MaterialCategory::Insulation
    } else if has(&["brick", "block", "masonry", "τούβλ", "οπτοπλινθ", "τοιχοποιία"])
    {
        MaterialCategory::Masonry
    } else if has(&["plasterboard", "gypsum", "drywall", "board", "γυψοσανίδα"]) {
        MaterialCategory::Board
    } else if has(&["plaster", "render", "επίχρισμα", "σοβά"]) {
        MaterialCategory::Plaster
    } else if has(&["glass", "glazing", "γυαλ", "υαλο"]) {
        MaterialCategory::Glass
    } else if has(&["timber", "wood", "ξύλ"]) {
        MaterialCategory::Wood
    } else if has(&["steel", "alumin", "metal", "iron", "χάλυβ", "αλουμίν", "μέταλ"]) {
        MaterialCategory::Metal
    } else if has(&["marble", "stone", "granite", "μάρμαρ", "πέτρ"]) {
        MaterialCategory::Stone
    } else if has(&["tile", "ceramic", "πλακίδ", "κεραμ"]) {
        MaterialCategory::Tile
    } else if has(&["membrane", "bitumen", "waterproof", "μεμβράνη", "στεγανω"]) {
        MaterialCategory::Membrane
    } else if has(&["screed", "mortar", "τσιμεντοκονία", "κονίαμα"]) {
        MaterialCategory::Screed
    } else if has(&["gravel", "χαλίκ"]) {
        MaterialCategory::Gravel
    } else if has(&["soil", "earth", "έδαφος", "χώμα"]) {
        MaterialCategory::Soil
    } else if has(&["air", "cavity", "αέρα", "διάκενο"]) {
        MaterialCategory::Air
    } else {
        MaterialCategory::Other
    }
}

fn default_colour(c: MaterialCategory) -> Rgba {
    match c {
        MaterialCategory::Concrete => Rgba::rgb(186, 186, 180),
        MaterialCategory::Masonry => Rgba::rgb(196, 122, 92),
        MaterialCategory::Insulation => Rgba::rgb(240, 238, 225),
        MaterialCategory::Plaster | MaterialCategory::Board => Rgba::rgb(236, 232, 222),
        MaterialCategory::Stone => Rgba::rgb(225, 222, 215),
        MaterialCategory::Tile => Rgba::rgb(214, 200, 180),
        MaterialCategory::Wood => Rgba::rgb(170, 125, 80),
        MaterialCategory::Metal => Rgba::rgb(150, 155, 160),
        MaterialCategory::Glass => Rgba::new(170, 205, 225, 120),
        MaterialCategory::Membrane => Rgba::rgb(60, 60, 60),
        MaterialCategory::Screed => Rgba::rgb(175, 175, 170),
        MaterialCategory::Gravel => Rgba::rgb(160, 155, 145),
        MaterialCategory::Soil => Rgba::rgb(130, 105, 75),
        MaterialCategory::Air => Rgba::new(255, 255, 255, 0),
        MaterialCategory::Other => Rgba::rgb(200, 200, 200),
    }
}

impl<'a> Importer<'a> {
    fn new(f: &'a StepFile, units: Units, opts: &'a ImportOptions) -> Importer<'a> {
        // The import is deterministic: the new document has no project id yet; the application gives it one the
        // first time it is saved, like any document without an id.
        let mut doc = match opts.base {
            ImportBase::Starter => starter_with_id(ProjectId::NIL),
            ImportBase::Empty => Document::new(),
        };
        let starter_storeys: Vec<Id> = doc.storeys().iter().map(|s| s.id).collect();
        for s in starter_storeys {
            doc = doc.without_storey(s).unwrap_or(doc);
        }
        let materials_by_name = doc.materials().map(|m| (m.name.clone(), m.id)).collect();
        Importer {
            f,
            g: Geometry::new(f, units),
            opts,
            doc,
            report: ImportReport::default(),
            contained: HashMap::new(),
            parent: HashMap::new(),
            voids: Vec::new(),
            fills: HashMap::new(),
            material_of: HashMap::new(),
            type_of: HashMap::new(),
            psets: HashMap::new(),
            colours: HashMap::new(),
            storeys: Vec::new(),
            storey_by_ifc: HashMap::new(),
            materials_by_name,
            material_cache: HashMap::new(),
            build_up_cache: HashMap::new(),
            walls: HashMap::new(),
            slabs: HashMap::new(),
            handled: HashSet::new(),
        }
    }

    fn len(&self) -> f64 {
        self.g.units.length
    }

    // ----- Report ----------------------------------------------------------------------------------------------

    fn mapped(&mut self, id: u64) {
        self.handled.insert(id);
        if let Some(t) = self.f.type_of(id) {
            *self.report.mapped.entry(t.to_owned()).or_default() += 1;
        }
    }

    fn skipped(&mut self, id: u64, reason: &str) {
        self.handled.insert(id);
        let ifc_type = self.f.type_of(id).unwrap_or("?").to_owned();
        *self.report.skipped.entry(ifc_type.clone()).or_default().entry(reason.to_owned()).or_default() += 1;
        let global_id = self.f.get(id).and_then(|e| e.str_at(0)).unwrap_or("").to_owned();
        self.report.issues.push(ImportIssue { entity: id, ifc_type, global_id, reason: reason.to_owned() });
    }

    fn simplified(&mut self, notes: Vec<String>) {
        for n in notes {
            *self.report.simplifications.entry(n).or_default() += 1;
        }
    }

    // ----- Indexes ---------------------------------------------------------------------------------------------

    fn index(&mut self) {
        let f = self.f;
        for &r in f.of_type("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
            let Some(e) = f.get(r) else { continue };
            let Some(structure) = e.ref_at(5) else { continue };
            for el in e.refs_at(4) {
                self.contained.entry(el).or_insert(structure);
            }
        }
        for &r in f.of_type("IFCRELAGGREGATES") {
            let Some(e) = f.get(r) else { continue };
            let Some(whole) = e.ref_at(4) else { continue };
            for part in e.refs_at(5) {
                self.parent.entry(part).or_insert(whole);
            }
        }
        for &r in f.of_type("IFCRELVOIDSELEMENT") {
            if let Some(e) = f.get(r)
                && let (Some(host), Some(opening)) = (e.ref_at(4), e.ref_at(5))
            {
                self.voids.push((host, opening));
            }
        }
        for &r in f.of_type("IFCRELFILLSELEMENT") {
            if let Some(e) = f.get(r)
                && let (Some(opening), Some(filling)) = (e.ref_at(4), e.ref_at(5))
            {
                self.fills.entry(opening).or_insert(filling);
            }
        }
        for &r in f.of_type("IFCRELASSOCIATESMATERIAL") {
            let Some(e) = f.get(r) else { continue };
            let Some(material) = e.ref_at(5) else { continue };
            for o in e.refs_at(4) {
                self.material_of.entry(o).or_insert(material);
            }
        }
        for &r in f.of_type("IFCRELDEFINESBYTYPE") {
            let Some(e) = f.get(r) else { continue };
            let Some(t) = e.ref_at(5) else { continue };
            for o in e.refs_at(4) {
                self.type_of.entry(o).or_insert(t);
            }
        }
        for &r in f.of_type("IFCRELDEFINESBYPROPERTIES") {
            let Some(e) = f.get(r) else { continue };
            let Some(p) = e.ref_at(5) else { continue };
            for o in e.refs_at(4) {
                self.psets.entry(o).or_default().push(p);
            }
        }
        // Material colours from material definition representations.
        for &r in f.of_type("IFCMATERIALDEFINITIONREPRESENTATION") {
            let Some(e) = f.get(r) else { continue };
            let Some(material) = e.ref_at(3) else { continue };
            if let Some(c) = e.refs_at(2).into_iter().find_map(|rep| self.styled_colour(rep)) {
                self.colours.insert(material, c);
            }
        }
    }

    fn styled_colour(&self, rep: u64) -> Option<Rgba> {
        let f = self.f;
        for item in f.get(rep)?.refs_at(3) {
            let Some(styled) = f.get(item) else { continue };
            let mut styles = styled.refs_at(1);
            // IFC 2x3 wraps styles in presentation style assignments.
            let assigned: Vec<u64> = styles
                .iter()
                .filter(|&&s| f.type_of(s) == Some("IFCPRESENTATIONSTYLEASSIGNMENT"))
                .flat_map(|&s| f.get(s).map(|a| a.refs_at(0)).unwrap_or_default())
                .collect();
            styles.extend(assigned);
            for s in styles {
                let Some(style) = f.get(s) else { continue };
                if style.name.as_ref() != "IFCSURFACESTYLE" {
                    continue;
                }
                for element in style.refs_at(2) {
                    let Some(shading) = f.get(element) else { continue };
                    if !matches!(shading.name.as_ref(), "IFCSURFACESTYLESHADING" | "IFCSURFACESTYLERENDERING")
                    {
                        continue;
                    }
                    let Some(rgb) = shading.ref_at(0).and_then(|c| f.get(c)) else { continue };
                    let ch = |i: usize| (rgb.f64_at(i).unwrap_or(0.8).clamp(0.0, 1.0) * 255.0).round() as u8;
                    let transparency = shading.f64_at(1).unwrap_or(0.0).clamp(0.0, 1.0);
                    let a = ((1.0 - transparency) * 255.0).round() as u8;
                    return Some(Rgba::new(ch(1), ch(2), ch(3), a));
                }
            }
        }
        None
    }

    // ----- Properties ------------------------------------------------------------------------------------------

    fn psets_of(&self, product: u64) -> Vec<&'a Entity> {
        let f = self.f;
        self.psets
            .get(&product)
            .map(|ps| {
                ps.iter().filter_map(|&p| f.get(p)).filter(|e| e.name.as_ref() == "IFCPROPERTYSET").collect()
            })
            .unwrap_or_default()
    }

    /// Properties of a named set (or of every set when `set` is `None`) as `(name, value)`.
    fn properties(&self, product: u64, set: Option<&str>) -> Vec<(&'a str, &'a Value)> {
        let f = self.f;
        let mut out = Vec::new();
        for ps in self.psets_of(product) {
            if set.is_some_and(|s| ps.str_at(2) != Some(s)) {
                continue;
            }
            for p in ps.refs_at(4) {
                let Some(prop) = f.get(p) else { continue };
                let Some(name) = prop.str_at(0) else { continue };
                let value = match prop.name.as_ref() {
                    "IFCPROPERTYSINGLEVALUE" => prop.attr(2),
                    "IFCPROPERTYENUMERATEDVALUE" => prop.list_at(2).and_then(<[Value]>::first),
                    _ => None,
                };
                if let Some(v) = value {
                    out.push((name, v));
                }
            }
        }
        out
    }

    fn property(&self, product: u64, set: Option<&str>, name: &str) -> Option<&'a Value> {
        self.properties(product, set).into_iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    /// A length quantity (`IfcQuantityLength`) of a named element quantity set, in metres.
    fn length_quantity(&self, product: u64, set: &str, name: &str) -> Option<f64> {
        let f = self.f;
        let sets = self.psets.get(&product)?;
        for &q in sets {
            let Some(qs) = f.get(q) else { continue };
            if qs.name.as_ref() != "IFCELEMENTQUANTITY" || qs.str_at(2) != Some(set) {
                continue;
            }
            for item in qs.refs_at(5) {
                let Some(quantity) = f.get(item) else { continue };
                if quantity.name.as_ref() == "IFCQUANTITYLENGTH" && quantity.str_at(0) == Some(name) {
                    return quantity.f64_at(3).map(|v| v * self.len());
                }
            }
        }
        None
    }

    // ----- Storeys ---------------------------------------------------------------------------------------------

    fn read_storeys(&mut self) {
        let f = self.f;
        let mut found: Vec<(f64, u64, String)> = Vec::new();
        for &id in f.of_type("IFCBUILDINGSTOREY") {
            let Some(e) = f.get(id) else { continue };
            let elevation = match e.ref_at(5) {
                Some(p) => self.g.placement(p).origin.z,
                None => e.f64_at(9).map(|z| z * self.len()).unwrap_or(0.0),
            };
            let name = e
                .str_at(2)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| e.str_at(7))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Storey {}", found.len() + 1));
            found.push((elevation, id, name));
            self.mapped(id);
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if found.is_empty() {
            self.report.warnings.push("the file has no storeys; one storey at ±0.00 was created".into());
            let (doc, id) = self.doc.with_new_id();
            self.doc = doc;
            self.storeys.push(StoreyInfo { ifc: None, id, name: "±0.00".into(), elevation: 0.0, top: 0.0 });
            return;
        }
        for (elevation, ifc, name) in found {
            let (doc, id) = self.doc.with_new_id();
            self.doc = doc;
            self.storey_by_ifc.insert(ifc, self.storeys.len());
            self.storeys.push(StoreyInfo { ifc: Some(ifc), id, name, elevation, top: elevation });
        }
    }

    /// Writes the storeys and one plan view each. The height is the storey's `GrossHeight` quantity when given,
    /// else the distance to the next storey, else (last storey) the top of its tallest element, else 3 m.
    fn finish_storeys(&mut self) {
        let n = self.storeys.len();
        for i in 0..n {
            let Some(s) = self.storeys.get(i) else { continue };
            let next = self.storeys.get(i + 1).map(|t| t.elevation);
            let gross = s
                .ifc
                .and_then(|ifc| self.length_quantity(ifc, "Qto_BuildingStoreyBaseQuantities", "GrossHeight"));
            let mut height = match (gross, next) {
                (Some(h), _) if h > 0.1 => h,
                (_, Some(z)) => z - s.elevation,
                (_, None) => s.top - s.elevation,
            };
            if height.is_nan() || height <= 0.1 {
                height = 3.0;
            }
            let mut storey = Storey::new(s.id, s.name.clone(), Length::m(s.elevation), Length::m(height));
            if let Some(ifc) = s.ifc {
                let above = self.property(ifc, None, "AboveGround").and_then(Value::as_bool);
                storey.below_ground = match above {
                    Some(a) => !a,
                    None => s.elevation + height <= 1e-3,
                };
            }
            let (doc, view) = self.doc.with_storey(storey).with_new_id();
            self.doc = doc.with_view(View {
                id: view,
                name: s.name.clone(),
                kind: ViewKind::Plan { storey: s.id },
                scale: 50,
                hidden_layers: Default::default(),
            });
        }
    }

    /// The storey of a product: through containment (directly, via a space, or via the whole it is part of), or
    /// else the highest storey at or below `z`.
    fn storey_index(&self, product: u64, z: f64) -> usize {
        let mut cur = product;
        for _ in 0..16 {
            if let Some(&i) = self.storey_by_ifc.get(&cur) {
                return i;
            }
            let next = self.contained.get(&cur).or_else(|| self.parent.get(&cur)).copied();
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        let mut best = 0;
        for (i, s) in self.storeys.iter().enumerate() {
            if s.elevation <= z + 0.01 {
                best = i;
            }
        }
        best
    }

    fn storey_elevation(&self, i: usize) -> f64 {
        self.storeys.get(i).map(|s| s.elevation).unwrap_or(0.0)
    }

    fn raise_top(&mut self, i: usize, z: f64) {
        if let Some(s) = self.storeys.get_mut(i)
            && z > s.top
        {
            s.top = z;
        }
    }

    // ----- Materials -------------------------------------------------------------------------------------------

    fn material_select(&self, product: u64) -> Option<u64> {
        self.material_of
            .get(&product)
            .or_else(|| self.type_of.get(&product).and_then(|t| self.material_of.get(t)))
            .copied()
    }

    /// The Kanavos material for an `IfcMaterial` (created by name when new), or the fallback material.
    fn material(&mut self, ifc: Option<u64>, priority: Option<u8>) -> Id {
        if let Some(id) = ifc.and_then(|m| self.material_cache.get(&m)) {
            return *id;
        }
        let f = self.f;
        let entity = ifc.and_then(|m| f.get(m)).filter(|e| e.name.as_ref() == "IFCMATERIAL");
        let name = entity
            .and_then(|e| e.str_at(0))
            .filter(|n| !n.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| self.opts.fallback_material.clone());
        let id = match self.materials_by_name.get(&name) {
            Some(&id) => id,
            None => {
                let hint = format!("{name} {}", entity.and_then(|e| e.str_at(2)).unwrap_or(""));
                let category = guess_category(&hint);
                let color =
                    ifc.and_then(|m| self.colours.get(&m)).copied().unwrap_or(default_colour(category));
                let (doc, id) = self.doc.with_new_id();
                self.doc = doc.with_material(Material {
                    id,
                    name: name.clone(),
                    category,
                    color,
                    cut_pattern: Default::default(),
                    cut_background: None,
                    priority: priority.unwrap_or(50),
                    conductivity: None,
                    density: None,
                });
                self.materials_by_name.insert(name, id);
                id
            }
        };
        if let Some(m) = ifc {
            self.material_cache.insert(m, id);
        }
        id
    }

    /// The first `IfcMaterial` inside any material select.
    fn first_material(&self, select: u64) -> Option<u64> {
        let f = self.f;
        let e = f.get(select)?;
        match e.name.as_ref() {
            "IFCMATERIAL" => Some(select),
            "IFCMATERIALLIST" => e.refs_at(0).first().copied(),
            "IFCMATERIALLAYERSETUSAGE" | "IFCMATERIALPROFILESETUSAGE" => self.first_material(e.ref_at(0)?),
            "IFCMATERIALLAYERSET" => {
                let layers = e.refs_at(0);
                let core =
                    layers.iter().find(|&&l| f.get(l).and_then(|l| l.str_at(5)) == Some("LoadBearing"));
                core.or(layers.first()).and_then(|&l| f.get(l)).and_then(|l| l.ref_at(0))
            }
            "IFCMATERIALLAYER" | "IFCMATERIALPROFILE" if e.name.as_ref() == "IFCMATERIALLAYER" => e.ref_at(0),
            "IFCMATERIALPROFILE" => e.ref_at(2),
            "IFCMATERIALPROFILESET" => e.refs_at(2).first().and_then(|&p| f.get(p)).and_then(|p| p.ref_at(2)),
            "IFCMATERIALCONSTITUENTSET" => {
                e.refs_at(2).first().and_then(|&c| f.get(c)).and_then(|c| c.ref_at(2))
            }
            _ => None,
        }
    }

    fn single_material(&mut self, product: u64) -> Id {
        let m = self.material_select(product).and_then(|s| self.first_material(s));
        self.material(m, None)
    }

    /// The structure of a wall, slab or roof from its material association and geometric thickness.
    fn structure(&mut self, product: u64, thickness: f64, usage: BuildUpUsage) -> Structure {
        let f = self.f;
        let select = self.material_select(product);
        let entity = select.and_then(|s| f.get(s));
        let (set, sense) = match entity.map(|e| e.name.as_ref()) {
            Some("IFCMATERIALLAYERSETUSAGE") => {
                (entity.and_then(|e| e.ref_at(0)), entity.and_then(|e| e.enum_at(2)))
            }
            Some("IFCMATERIALLAYERSET") => (select, None),
            _ => (None, None),
        };
        if let Some(set) = set {
            // Kanavos lists wall layers left → right and slab layers top → bottom, which is the IFC order when
            // the layers are stacked in the negative direction. Layers stacked in the positive direction are
            // reversed; a layer set without a usage gives no direction and is kept in the order written.
            let reverse = sense == Some("POSITIVE");
            if let Some((build_up, total)) = self.build_up(set, reverse, usage) {
                if (total - thickness).abs() <= (0.002_f64).max(0.02 * thickness) {
                    return Structure::Layered { build_up };
                }
                *self
                    .report
                    .simplifications
                    .entry("layer set thickness differs from the geometry; single material used".into())
                    .or_default() += 1;
            }
        }
        let material = select.and_then(|s| self.first_material(s));
        Structure::Basic { material: self.material(material, None), thickness: Length::m(thickness) }
    }

    /// A build-up for a layer set with more than one layer: an existing one with the same name and layers, or a
    /// new one. Returns it with its total thickness.
    fn build_up(&mut self, set: u64, reverse: bool, usage: BuildUpUsage) -> Option<(Id, f64)> {
        if let Some(cached) = self.build_up_cache.get(&(set, reverse)) {
            return *cached;
        }
        let f = self.f;
        let e = f.get(set)?;
        let mut layers = Vec::new();
        let mut names = Vec::new();
        for l in e.refs_at(0) {
            let Some(layer) = f.get(l) else { continue };
            let thickness = layer.f64_at(1).unwrap_or(0.0) * self.len();
            if thickness.is_nan() || thickness <= 0.0 {
                continue;
            }
            let priority =
                layer.attr(6).and_then(Value::as_i64).and_then(|p| u8::try_from(p.clamp(0, 100)).ok());
            let ifc_material = layer.ref_at(0);
            let material = self.material(ifc_material, priority);
            names.push(self.doc.material(material).map(|m| m.name.clone()).unwrap_or_default());
            let core = layer.str_at(5) == Some("LoadBearing");
            layers.push(BuildUpLayer { material, thickness: Length::m(thickness), core });
        }
        if reverse {
            layers.reverse();
            names.reverse();
        }
        let result = if layers.len() < 2 {
            None
        } else {
            let total: f64 = layers.iter().map(|l| l.thickness.as_m()).sum();
            let name = e
                .str_at(1)
                .filter(|n| !n.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| names.join(" / "));
            let same = |b: &BuildUp| {
                b.name == name
                    && b.layers.len() == layers.len()
                    && b.layers.iter().zip(&layers).all(|(x, y)| {
                        x.material == y.material && (x.thickness.as_m() - y.thickness.as_m()).abs() < 1e-4
                    })
            };
            let existing = self.doc.build_ups().find(|b| same(b)).map(|b| b.id);
            let id = match existing {
                Some(id) => id,
                None => {
                    let (doc, id) = self.doc.with_new_id();
                    self.doc = doc.with_build_up(BuildUp { id, name, usage, layers });
                    id
                }
            };
            Some((id, total))
        };
        self.build_up_cache.insert((set, reverse), result);
        result
    }

    // ----- Geometry --------------------------------------------------------------------------------------------

    fn solids(&mut self, product: &Entity, notes: &mut Vec<String>) -> Vec<Solid> {
        let world = self.g.product_frame(product);
        let Some(body) = self.g.representation(product, "Body") else {
            notes.push("no body representation".into());
            return Vec::new();
        };
        let extrusions: Vec<Extrusion> = self.g.extrusions(body, notes);
        extrusions
            .iter()
            .filter_map(|ex| {
                let frame = world.then(&ex.frame);
                let vector = frame.apply_vec(ex.vector);
                let unit = vector.normalized()?;
                let normal = frame.z.normalized()?;
                let vertical = unit.z.abs() > 0.999 && normal.z.abs() > 0.999;
                let map = |p: &kanavos_geom::Polygon2| {
                    kanavos_geom::Polygon2::new(
                        p.vertices
                            .iter()
                            .map(|q| {
                                let w = frame.apply2(q.x.as_m(), q.y.as_m());
                                Point2::new(w.x, w.y)
                            })
                            .collect(),
                    )
                };
                let area = PolygonWithHoles {
                    outer: map(&ex.profile.area.outer),
                    holes: ex.profile.area.holes.iter().map(map).collect(),
                }
                .oriented();
                let mut corners = Vec::new();
                for q in &ex.profile.area.outer.vertices {
                    let w = frame.apply2(q.x.as_m(), q.y.as_m());
                    corners.push(w);
                    corners.push(w + vector);
                }
                let zs = corners.iter().map(|p| p.z);
                let zmin = zs.clone().fold(f64::INFINITY, f64::min);
                let zmax = zs.fold(f64::NEG_INFINITY, f64::max);
                Some(Solid {
                    area,
                    corners,
                    zmin,
                    zmax,
                    vertical,
                    frame,
                    vector,
                    kind: ex.profile.kind,
                    position: ex.profile.position,
                })
            })
            .collect()
    }

    /// The axis of a product in plan, from its `Axis` representation: a straight segment, or a circular arc
    /// (`IfcTrimmedCurve` of an `IfcCircle`). `Err` for other curved axes.
    fn axis(&mut self, product: &Entity) -> Result<Option<Axis>, String> {
        let f = self.f;
        let world = self.g.product_frame(product);
        let Some(rep) = self.g.representation(product, "Axis") else { return Ok(None) };
        let Some(&curve) = f.get(rep).map(|r| r.refs_at(3)).unwrap_or_default().first() else {
            return Ok(None);
        };
        let Some(points) = self.g.curve_points(curve, 0) else { return Ok(None) };
        let plan: Vec<Point2> = points
            .iter()
            .map(|p| {
                let w = world.apply2(p.x.as_m(), p.y.as_m());
                Point2::new(w.x, w.y)
            })
            .collect();
        let (Some(&a), Some(&b)) = (plan.first(), plan.last()) else { return Ok(None) };
        if is_curved(f, curve) {
            let circle = f.get(curve).and_then(|c| c.ref_at(0)).and_then(|c| f.get(c));
            let centre_local = circle.and_then(|c| c.ref_at(0)).and_then(|p| self.g.axis_placement(p));
            let (Some(circle), Some(local)) = (circle, centre_local) else {
                return Err("curved wall axes other than circular arcs are not imported".into());
            };
            let c = world.apply(local.origin);
            let centre = Point2::new(c.x, c.y);
            let radius = circle.f64_at(1).unwrap_or(0.0) * self.len() * world.x.length();
            let mut sweep = 0.0;
            for w in plan.windows(2) {
                if let [p, q] = w {
                    sweep += (*p - centre).angle_to(*q - centre).as_rad();
                }
            }
            if !(radius > 1e-3 && sweep.abs() > 1e-6) {
                return Err("the wall arc is degenerate".into());
            }
            return Ok(Some(Axis::Arc { centre, radius, start: a, end: b, sweep }));
        }
        let seg = kanavos_geom::Segment2::new(a, b);
        if seg.length().as_m() < 1e-3 {
            return Ok(None);
        }
        if plan.iter().any(|p| seg.distance_to(*p).as_m() > 1e-3) {
            return Err("wall axis is not straight".into());
        }
        Ok(Some(Axis::Straight(a, b)))
    }

    // ----- Elements --------------------------------------------------------------------------------------------

    fn add_element(&mut self, ifc: u64, storey: usize, tag: Option<&str>, kind: ElementKind) -> Id {
        let (doc, id) = self.doc.with_new_id();
        let storey_id = self.storeys.get(storey).map(|s| s.id).unwrap_or(Id(0));
        let mut el = Element::on_storey(id, storey_id, kind);
        if let Some(t) = tag.filter(|t| !t.trim().is_empty()) {
            el.tag = t.to_owned();
        }
        let status = self.property(ifc, None, "Status").and_then(Value::as_str);
        el.phase = phase_from(status);
        if self.opts.keep_global_ids
            && let Some(g) = self.f.get(ifc).and_then(|e| e.str_at(0)).filter(|g| g.len() == 22)
        {
            el.external_id = Some(g.to_owned());
        }
        for (k, v) in self.properties(ifc, Some("Kanavos_Properties")) {
            if let Some(s) = v.as_str() {
                el.properties.insert(k.to_owned(), s.to_owned());
            }
        }
        self.doc = doc.with_element(el);
        id
    }

    fn for_types(
        &mut self,
        types: &[&str],
        map: fn(&mut Importer<'a>, u64, &'a Entity) -> Result<(), String>,
    ) {
        let f = self.f;
        let mut ids: Vec<u64> = types.iter().flat_map(|t| f.of_type(t).iter().copied()).collect();
        ids.sort_unstable();
        for id in ids {
            let Some(e) = f.get(id) else { continue };
            match map(self, id, e) {
                Ok(()) => self.mapped(id),
                Err(reason) => self.skipped(id, &reason),
            }
        }
    }

    fn map_wall(&mut self, id: u64, e: &'a Entity) -> Result<(), String> {
        let mut notes = Vec::new();
        let solids: Vec<Solid> = self.solids(e, &mut notes).into_iter().filter(|s| s.vertical).collect();
        if solids.is_empty() {
            return Err("the body is not a vertical extrusion".into());
        }
        let points: Vec<Point2> = solids.iter().flat_map(|s| s.area.outer.vertices.iter().copied()).collect();
        let axis = match self.axis(e)? {
            Some(a) => a,
            None => {
                notes.push("wall without an axis: reference line derived from the body".into());
                let (a, b) = centre_line(&points).ok_or("the wall body is degenerate")?;
                Axis::Straight(a, b)
            }
        };
        // Signed distance of each body point to the left of the reference line.
        let ys: Vec<f64> = match &axis {
            Axis::Straight(a, b) => {
                let n = (*b - *a).normalized().ok_or("the wall axis has no length")?.perp();
                points.iter().map(|p| n.dot(*p - *a)).collect()
            }
            Axis::Arc { centre, radius, sweep, .. } => {
                points.iter().map(|p| sweep.signum() * (radius - p.distance(*centre).as_m())).collect()
            }
        };
        let ymin = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let ymax = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let thickness = ymax - ymin;
        if thickness.is_nan() || thickness < 1e-3 {
            return Err("the wall has no thickness".into());
        }
        let tol = (5e-4_f64).max(thickness * 1e-3);
        let (reference, shift) = if ymax.abs() <= tol {
            (ReferenceLine::Left, 0.0)
        } else if ymin.abs() <= tol {
            (ReferenceLine::Right, 0.0)
        } else if (ymin + ymax).abs() <= tol {
            (ReferenceLine::Centre, 0.0)
        } else {
            notes.push("wall axis off the wall faces: moved to the centre".into());
            (ReferenceLine::Centre, (ymin + ymax) / 2.0)
        };
        let (start, end, bulge, arc) = match axis {
            Axis::Straight(a, b) => {
                let n = (b - a).normalized().ok_or("the wall axis has no length")?.perp();
                (a + n * shift, b + n * shift, 0.0, None)
            }
            Axis::Arc { centre, radius, start, end, sweep } => {
                let r = radius - sweep.signum() * shift;
                if r.is_nan() || r <= 1e-3 {
                    return Err("the wall arc is degenerate".into());
                }
                let k = r / radius;
                let (a, b) = (centre + (start - centre) * k, centre + (end - centre) * k);
                let a0 = (a - centre).angle().as_rad();
                (a, b, (sweep / 4.0).tan(), Some((centre, r, a0, sweep)))
            }
        };
        let dir = (end - start).normalized().ok_or("the wall axis has no length")?;
        let zmin = solids.iter().map(|s| s.zmin).fold(f64::INFINITY, f64::min);
        let zmax = solids.iter().map(|s| s.zmax).fold(f64::NEG_INFINITY, f64::max);
        let storey = self.storey_index(id, zmin);
        let elevation = self.storey_elevation(storey);
        let structure = self.structure(id, thickness, BuildUpUsage::Wall);
        let wall = Wall {
            start,
            end,
            bulge,
            reference,
            structure,
            height: Length::m(zmax - zmin),
            base_offset: Length::m(zmin - elevation),
        };
        self.raise_top(storey, zmax);
        let el = self.add_element(id, storey, e.str_at(7), ElementKind::Wall(wall));
        self.walls.insert(id, WallFrame { id: el, start, dir, storey, arc });
        self.simplified(notes);
        Ok(())
    }

    fn map_slab(&mut self, id: u64, e: &'a Entity) -> Result<(), String> {
        let mut notes = Vec::new();
        let solids: Vec<Solid> = self.solids(e, &mut notes).into_iter().filter(|s| s.vertical).collect();
        let Some(main) = solids.iter().max_by(|a, b| a.area.area().total_cmp(&b.area.area())) else {
            return Err("the body is not a vertical extrusion".into());
        };
        let zmin = solids.iter().map(|s| s.zmin).fold(f64::INFINITY, f64::min);
        let zmax = solids.iter().map(|s| s.zmax).fold(f64::NEG_INFINITY, f64::max);
        let thickness = zmax - zmin;
        if (thickness.is_nan() || thickness < 1e-3) || main.area.outer.len() < 3 {
            return Err("the slab is degenerate".into());
        }
        if e.enum_at(8) == Some("ROOF") {
            notes.push("roof slab imported as a slab".into());
        }
        let outline = main.area.clone();
        let storey = self.storey_index(id, zmax);
        let elevation = self.storey_elevation(storey);
        let structure = self.structure(id, thickness, BuildUpUsage::Slab);
        let slab = Slab { outline, structure, top_offset: Length::m(zmax - elevation) };
        let el = self.add_element(id, storey, e.str_at(7), ElementKind::Slab(slab));
        self.slabs.insert(id, el);
        self.simplified(notes);
        Ok(())
    }

    fn map_column(&mut self, id: u64, e: &'a Entity) -> Result<(), String> {
        let mut notes = Vec::new();
        let solids = self.solids(e, &mut notes);
        let Some(s) = solids.iter().find(|s| s.vertical && !matches!(s.kind, ProfileKind::Other)) else {
            return Err("the body is not a vertical extrusion of a rectangle or circle".into());
        };
        let centre = s.frame.apply2(s.position.origin.x, s.position.origin.y);
        let x = s.frame.apply_vec(Vec3::new(s.position.x.x, s.position.x.y, 0.0));
        let profile = match s.kind {
            ProfileKind::Rectangle { xdim, ydim } => {
                Profile::Rectangle { width: Length::m(xdim), depth: Length::m(ydim) }
            }
            ProfileKind::Circle { radius } => Profile::Circle { diameter: Length::m(2.0 * radius) },
            ProfileKind::Other => return Err("unsupported column profile".into()),
        };
        let storey = self.storey_index(id, s.zmin);
        let elevation = self.storey_elevation(storey);
        let column = Column {
            position: Point2::new(centre.x, centre.y),
            rotation: Angle::rad(x.y.atan2(x.x)),
            profile,
            material: self.single_material(id),
            height: Length::m(s.zmax - s.zmin),
            base_offset: Length::m(s.zmin - elevation),
        };
        self.raise_top(storey, s.zmax);
        self.add_element(id, storey, e.str_at(7), ElementKind::Column(column));
        self.simplified(notes);
        Ok(())
    }

    fn map_beam(&mut self, id: u64, e: &'a Entity) -> Result<(), String> {
        let mut notes = Vec::new();
        let solids = self.solids(e, &mut notes);
        let horizontal = |s: &Solid| s.vector.normalized().is_some_and(|v| v.z.abs() < 0.01);
        let Some(s) = solids.iter().find(|s| horizontal(s) && !matches!(s.kind, ProfileKind::Other)) else {
            return Err("the body is not a horizontal extrusion of a rectangle or circle".into());
        };
        let c0 = s.frame.apply2(s.position.origin.x, s.position.origin.y);
        let c1 = c0 + s.vector;
        let px = s.frame.apply_vec(Vec3::new(s.position.x.x, s.position.x.y, 0.0));
        let py = s.frame.apply_vec(Vec3::new(s.position.y.x, s.position.y.y, 0.0));
        let (profile, half_depth) = match s.kind {
            ProfileKind::Rectangle { xdim, ydim } => {
                let (w, d) = if px.z.abs() > py.z.abs() { (ydim, xdim) } else { (xdim, ydim) };
                (Profile::Rectangle { width: Length::m(w), depth: Length::m(d) }, d / 2.0)
            }
            ProfileKind::Circle { radius } => (Profile::Circle { diameter: Length::m(2.0 * radius) }, radius),
            ProfileKind::Other => return Err("unsupported beam profile".into()),
        };
        let top = c0.z.max(c1.z) + half_depth;
        let storey = self.storey_index(id, top - 2.0 * half_depth);
        let elevation = self.storey_elevation(storey);
        let beam = Beam {
            start: Point2::new(c0.x, c0.y),
            end: Point2::new(c1.x, c1.y),
            profile,
            material: self.single_material(id),
            top_offset: Length::m(top - elevation),
        };
        self.add_element(id, storey, e.str_at(7), ElementKind::Beam(beam));
        self.simplified(notes);
        Ok(())
    }

    fn map_space(&mut self, id: u64, e: &'a Entity) -> Result<(), String> {
        let mut notes = Vec::new();
        let solids: Vec<Solid> = self.solids(e, &mut notes).into_iter().filter(|s| s.vertical).collect();
        let Some(main) = solids.iter().max_by(|a, b| a.area.area().total_cmp(&b.area.area())) else {
            return Err("the body is not a vertical extrusion".into());
        };
        if main.area.outer.len() < 3 {
            return Err("the space boundary is degenerate".into());
        }
        let boundary = main.area.clone();
        let (zmin, zmax) = (main.zmin, main.zmax);
        let storey = self.storey_index(id, zmin);
        let name_attr = e.str_at(2).unwrap_or("").trim().to_owned();
        let long = e.str_at(7).unwrap_or("").trim().to_owned();
        let (name, number) = if long.is_empty() {
            (name_attr, String::new())
        } else if name_attr == long {
            (long, String::new())
        } else {
            (long, name_attr)
        };
        let category = self
            .property(id, Some("Kanavos_Space"), "Category")
            .and_then(Value::as_str)
            .and_then(space_category)
            .unwrap_or_default();
        let finish = self
            .property(id, Some("Kanavos_Space"), "Finish")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let space = Space {
            boundary,
            name,
            number,
            category,
            height: Some(Length::m(zmax - zmin)),
            stamp: None,
            finish,
        };
        self.add_element(id, storey, None, ElementKind::Space(space));
        self.simplified(notes);
        Ok(())
    }

    /// Roofs with their own extruded body become flat roofs; a second extrusion with a hole sitting on top of
    /// the roof slab becomes its parapet. Roofs built from parts are imported through the parts (slabs).
    fn map_roof(&mut self, id: u64, e: &'a Entity) -> Result<(), String> {
        let mut notes = Vec::new();
        if self.g.representation(e, "Body").is_none() {
            let has_parts = self.parent.values().any(|&whole| whole == id);
            return Err(if has_parts {
                "roof without its own body; its parts are imported separately".into()
            } else {
                "roof without a body".into()
            });
        }
        let solids: Vec<Solid> = self.solids(e, &mut notes).into_iter().collect();
        if solids.iter().any(|s| !s.vertical) || solids.is_empty() {
            return Err("pitched roof geometry is not imported yet".into());
        }
        let Some(main) = solids
            .iter()
            .filter(|s| s.area.holes.is_empty())
            .max_by(|a, b| a.area.area().total_cmp(&b.area.area()))
        else {
            return Err("the roof has no solid slab".into());
        };
        let thickness = main.zmax - main.zmin;
        if (thickness.is_nan() || thickness < 1e-3) || main.area.outer.len() < 3 {
            return Err("the roof is degenerate".into());
        }
        let parapet = solids
            .iter()
            .find(|s| s.area.holes.len() == 1 && (s.zmin - main.zmax).abs() < 1e-3)
            .and_then(|s| {
                let hole = s.area.holes.first()?;
                let band = s.area.outer.area().as_m2() - hole.area().as_m2();
                let mid_perimeter = (s.area.outer.perimeter().as_m() + hole.perimeter().as_m()) / 2.0;
                (mid_perimeter > 0.0).then(|| kanavos_model::Parapet {
                    height: Length::m(s.zmax - s.zmin),
                    thickness: Length::m(band / mid_perimeter),
                })
            });
        let storey = self.storey_index(id, main.zmin);
        let elevation = self.storey_elevation(storey);
        let structure = self.structure(id, thickness, BuildUpUsage::Roof);
        let roof = kanavos_model::Roof {
            outline: main.area.outer.clone(),
            base: Length::m(main.zmax - elevation),
            structure,
            shape: kanavos_model::RoofShape::Flat { parapet },
        };
        self.add_element(id, storey, e.str_at(7), ElementKind::Roof(roof));
        self.simplified(notes);
        Ok(())
    }

    /// Furniture, fixtures and proxies written by Kanavos (with a `Kanavos_Object` set) become objects again.
    fn map_object(&mut self, id: u64, e: &'a Entity) -> Result<(), String> {
        let props = self.properties(id, Some("Kanavos_Object"));
        let Some(definition) =
            props.iter().find(|(n, _)| *n == "Definition").and_then(|(_, v)| v.as_str()).map(str::to_owned)
        else {
            return Err("no Kanavos equivalent yet".into());
        };
        let mirrored =
            props.iter().find(|(n, _)| *n == "Mirrored").and_then(|(_, v)| v.as_bool()).unwrap_or(false);
        let params: BTreeMap<String, f64> = props
            .iter()
            .filter(|(n, _)| !matches!(*n, "Definition" | "Mirrored"))
            .filter_map(|(n, v)| v.as_f64().map(|x| ((*n).to_owned(), x)))
            .collect();
        let world = self.g.product_frame(e);
        let storey = self.storey_index(id, world.origin.z);
        let elevation = self.storey_elevation(storey);
        let object = kanavos_model::Object {
            definition,
            position: Point2::new(world.origin.x, world.origin.y),
            rotation: Angle::rad(world.x.y.atan2(world.x.x)),
            mirrored,
            elevation: Length::m(world.origin.z - elevation),
            params,
        };
        self.add_element(id, storey, e.str_at(7), ElementKind::Object(object));
        Ok(())
    }

    fn map_openings(&mut self) {
        let voids = std::mem::take(&mut self.voids);
        let mut seen = HashSet::new();
        for (host, opening) in voids {
            if !seen.insert(opening) {
                continue;
            }
            let filling = self.fills.get(&opening).copied();
            if self.slabs.contains_key(&host) {
                match self.map_slab_opening(host, opening) {
                    Ok(()) => self.mapped(opening),
                    Err(reason) => self.skipped(opening, &reason),
                }
                if let Some(fl) = filling {
                    self.skipped(fl, "doors and windows in slabs are not imported");
                }
                continue;
            }
            match self.map_opening(host, opening, filling) {
                Ok(()) => {
                    self.mapped(opening);
                    if let Some(fl) = filling {
                        self.mapped(fl);
                    }
                }
                Err(reason) => {
                    self.skipped(opening, &reason);
                    if let Some(fl) = filling {
                        self.skipped(fl, &reason);
                    }
                }
            }
        }
        // Doors and windows that fill no opening in a wall.
        let f = self.f;
        let mut free: Vec<u64> =
            DOOR_TYPES.iter().chain(WINDOW_TYPES).flat_map(|t| f.of_type(t).iter().copied()).collect();
        free.sort_unstable();
        for id in free {
            if !self.handled.contains(&id) {
                self.skipped(id, "door or window without an opening in an imported wall");
            }
        }
    }

    /// An opening through a slab becomes a hole in the slab's outline.
    fn map_slab_opening(&mut self, host: u64, opening: u64) -> Result<(), String> {
        let f = self.f;
        let slab_id = *self.slabs.get(&host).ok_or("the host is not an imported slab")?;
        let o = f.get(opening).ok_or("missing opening")?;
        let mut notes = Vec::new();
        let solids: Vec<Solid> = self.solids(o, &mut notes).into_iter().filter(|s| s.vertical).collect();
        let ring = solids
            .iter()
            .max_by(|a, b| a.area.outer.area().total_cmp(&b.area.outer.area()))
            .map(|s| s.area.outer.clone())
            .ok_or("the slab opening is not a vertical extrusion")?;
        let Some(ElementKind::Slab(slab)) = self.doc.element(slab_id).map(|e| e.kind.clone()) else {
            return Err("the host is not an imported slab".into());
        };
        let tol = self.doc.tolerance();
        if !ring.vertices.iter().all(|p| slab.outline.outer.contains(*p, &tol)) {
            return Err("the slab opening is not inside the slab outline".into());
        }
        let mut slab = slab;
        slab.outline.holes.push(ring.to_cw());
        if let Some(mut element) = self.doc.element(slab_id).cloned() {
            element.kind = ElementKind::Slab(slab);
            self.doc = self.doc.with_element(element);
        }
        notes.push("slab opening imported as a hole in the slab".into());
        self.simplified(notes);
        Ok(())
    }

    fn map_opening(&mut self, host: u64, opening: u64, filling: Option<u64>) -> Result<(), String> {
        let f = self.f;
        let Some(wall) = self.walls.get(&host) else {
            return Err("the host is not an imported wall".into());
        };
        let (wall_id, storey) = (wall.id, wall.storey);
        let o = f.get(opening).ok_or("missing opening")?;
        let fill = filling.and_then(|id| f.get(id));
        let kind = match fill.map(|e| e.name.as_ref()) {
            Some(t) if DOOR_TYPES.contains(&t) => OpeningKind::Door,
            Some(t) if WINDOW_TYPES.contains(&t) => OpeningKind::Window,
            Some(_) => return Err("the opening is filled by something other than a door or window".into()),
            None => OpeningKind::Void,
        };
        let mut notes = Vec::new();
        let mut corners: Vec<Point3> =
            self.solids(o, &mut notes).into_iter().flat_map(|s| s.corners).collect();
        if corners.is_empty()
            && let Some(fl) = fill
        {
            notes.push("opening without geometry: door or window body used".into());
            corners = self.solids(fl, &mut Vec::new()).into_iter().flat_map(|s| s.corners).collect();
        }
        if corners.is_empty() {
            return Err("the opening has no geometry".into());
        }
        let along: Vec<f64> = match self.walls.get(&host) {
            Some(w) => corners.iter().map(|p| w.along(Point2::new(p.x, p.y))).collect(),
            None => return Err("the host is not an imported wall".into()),
        };
        let amin = along.iter().copied().fold(f64::INFINITY, f64::min);
        let amax = along.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let zmin = corners.iter().map(|p| p.z).fold(f64::INFINITY, f64::min);
        let zmax = corners.iter().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max);
        let s = self.len();
        let overall = |i: usize| fill.and_then(|e| e.f64_at(i)).map(|v| v * s).filter(|v| *v > 0.0);
        let width = overall(9).unwrap_or(amax - amin);
        let height = overall(8).unwrap_or(zmax - zmin);
        if !(width > 1e-3 && height > 1e-3) {
            return Err("the opening has no size".into());
        }
        let source = filling.unwrap_or(opening);
        let props = self.properties(source, Some("Kanavos_Opening"));
        let get = |name: &str| props.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        let default_style = match kind {
            OpeningKind::Door => &self.opts.door_style,
            OpeningKind::Window => &self.opts.window_style,
            OpeningKind::Void => &self.opts.void_style,
        };
        let style =
            get("Style").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| default_style.clone());
        let params: BTreeMap<String, f64> = props
            .iter()
            .filter(|(n, _)| !matches!(*n, "Style" | "FlipSide" | "FlipHand"))
            .filter_map(|(n, v)| v.as_f64().map(|x| ((*n).to_owned(), x)))
            .collect();
        let value = Opening {
            host: wall_id,
            kind,
            style,
            offset: Length::m((amin + amax) / 2.0),
            width: Length::m(width),
            height: Length::m(height),
            sill: Length::m(zmin - self.storey_elevation(storey)),
            flip_side: get("FlipSide").and_then(Value::as_bool).unwrap_or(false),
            flip_hand: get("FlipHand").and_then(Value::as_bool).unwrap_or(false),
            params,
        };
        let tag = fill.and_then(|e| e.str_at(7)).or_else(|| o.str_at(7));
        self.add_element(source, storey, tag, ElementKind::Opening(value));
        self.simplified(notes);
        Ok(())
    }

    /// Reports every product that was neither mapped nor skipped with a specific reason.
    fn report_unmapped_products(&mut self) {
        let f = self.f;
        let placement_types = ["IFCLOCALPLACEMENT", "IFCGRIDPLACEMENT", "IFCLINEARPLACEMENT"];
        let mut ids = Vec::new();
        for (&id, record) in &f.data {
            let Some(e) = record.simple() else { continue };
            if self.handled.contains(&id) || SPATIAL_TYPES.contains(&e.name.as_ref()) {
                continue;
            }
            let is_product = e.str_at(0).is_some_and(|g| g.len() == 22)
                && e.ref_at(5).and_then(|p| f.type_of(p)).is_some_and(|t| placement_types.contains(&t));
            if is_product {
                ids.push((id, e.name.as_ref()));
            }
        }
        for (id, name) in ids {
            let reason = match name {
                "IFCOPENINGELEMENT" | "IFCOPENINGSTANDARDCASE" => "opening not in an imported wall",
                "IFCANNOTATION" | "IFCGRID" => "annotation and grids are not imported",
                _ => "no Kanavos equivalent yet",
            };
            self.skipped(id, reason);
        }
    }

    fn georeference(&mut self) {
        let f = self.f;
        let Some(map) = f.of_type("IFCMAPCONVERSION").first().and_then(|&m| f.get(m)) else { return };
        let crs = map.ref_at(1).and_then(|c| f.get(c));
        let unit =
            crs.and_then(|c| c.ref_at(6)).and_then(|u| unit_factor(f, u, 0)).map(|(_, k)| k).unwrap_or(1.0);
        let epsg = crs
            .and_then(|c| c.str_at(0))
            .and_then(|n| n.rsplit(':').next())
            .and_then(|code| code.trim().parse::<u32>().ok())
            .unwrap_or(2100);
        let abscissa = map.f64_at(5).unwrap_or(1.0);
        let ordinate = map.f64_at(6).unwrap_or(0.0);
        let rotation = (-ordinate).atan2(abscissa);
        if let Some(scale) = map.f64_at(7)
            && (scale - 1.0).abs() > 1e-9
        {
            self.report.warnings.push(format!("map conversion scale {scale} ignored"));
        }
        let georef = Georef {
            epsg,
            easting: map.f64_at(2).unwrap_or(0.0) * unit,
            northing: map.f64_at(3).unwrap_or(0.0) * unit,
            rotation: Angle::rad(rotation),
            altitude: map.f64_at(4).unwrap_or(0.0) * unit,
        };
        let mut site = self.doc.site().clone();
        site.georef = georef;
        self.doc = self.doc.with_site(site);
    }

    fn project_info(&mut self, project: &Entity) {
        let f = self.f;
        let mut info = self.doc.info().clone();
        if let Some(name) = project.str_at(2).filter(|s| !s.trim().is_empty()) {
            info.name = name.to_owned();
        }
        if let Some(long) = project.str_at(5).filter(|s| !s.trim().is_empty()) {
            info.location = long.to_owned();
        }
        if let Some(address) = f
            .of_type("IFCBUILDING")
            .first()
            .and_then(|&b| f.get(b))
            .and_then(|b| b.ref_at(11))
            .and_then(|a| f.get(a))
        {
            if let Some(town) = address.str_at(6).filter(|s| !s.trim().is_empty()) {
                info.municipality = town.to_owned();
            }
            if info.location.is_empty()
                && let Some(line) = address.list_at(4).and_then(<[Value]>::first).and_then(Value::as_str)
            {
                info.location = line.to_owned();
            }
        }
        self.doc = self.doc.with_info(info);
    }

    fn run(&mut self, project: &'a Entity) {
        self.index();
        self.read_storeys();
        self.for_types(WALL_TYPES, Importer::map_wall);
        self.for_types(SLAB_TYPES, Importer::map_slab);
        self.for_types(COLUMN_TYPES, Importer::map_column);
        self.for_types(BEAM_TYPES, Importer::map_beam);
        self.for_types(&["IFCSPACE"], Importer::map_space);
        self.for_types(&["IFCROOF"], Importer::map_roof);
        self.for_types(OBJECT_TYPES, Importer::map_object);
        self.map_openings();
        self.report_unmapped_products();
        self.georeference();
        self.project_info(project);
        self.finish_storeys();
        if let Err(e) = self.doc.validate() {
            self.report.warnings.push(format!("the imported document is inconsistent: {}", e.0));
        }
    }
}

/// The centre line of a band-shaped point set: along its longest edge direction, through the middle of its
/// thickness.
fn centre_line(points: &[Point2]) -> Option<(Point2, Point2)> {
    let n = points.len();
    let mut best: Option<(f64, Vec2)> = None;
    for i in 0..n {
        let (Some(&a), Some(&b)) = (points.get(i), points.get((i + 1) % n)) else { continue };
        let v = b - a;
        if best.is_none_or(|(l, _)| v.length() > l) {
            best = Some((v.length(), v));
        }
    }
    let (_, v) = best?;
    let d = v.normalized()?;
    let normal = d.perp();
    let origin = *points.first()?;
    let along: Vec<f64> = points.iter().map(|p| d.dot(*p - origin)).collect();
    let across: Vec<f64> = points.iter().map(|p| normal.dot(*p - origin)).collect();
    let (smin, smax) = (
        along.iter().copied().fold(f64::INFINITY, f64::min),
        along.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let (ymin, ymax) = (
        across.iter().copied().fold(f64::INFINITY, f64::min),
        across.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let mid = (ymin + ymax) / 2.0;
    Some((origin + d * smin + normal * mid, origin + d * smax + normal * mid))
}

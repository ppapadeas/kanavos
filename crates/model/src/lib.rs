//! The project document: storeys, elements, attributes and relations.
//!
//! Layer L3. See `docs/architecture.md`.
//!
//! A [`Document`] is an immutable value. Changes produce a new document (see `kanavos-engine`), and unchanged parts
//! are shared between versions through reference counting, so keeping every version for undo is cheap.
//!
//! The document contains no geometry rules: how a wall's outline or a roof's planes are computed lives in
//! `kanavos-elements`. This crate holds the parameters users edit and does no I/O.

mod attributes;
mod element;
mod project_id;
mod site;
mod template;
mod view;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use kanavos_geom::{Length, Tolerance};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use attributes::{BuildUp, BuildUpLayer, BuildUpUsage, Material, MaterialCategory};
pub use element::{
    Beam, Category, Column, Dimension, DraftShape, DraftStyle, Drafting, Element, ElementKind, GridLine,
    LevelMark, Object, Opening, OpeningKind, Parapet, Phase, Profile, Railing, RailingInfill, ReferenceLine,
    Roof, RoofShape, Slab, Space, SpaceCategory, Stair, StairShape, StairUse, Structure, TextAlign, TextNote,
    Underlay, Wall,
};
pub use project_id::{InvalidProjectId, ProjectId};
pub use site::{BuildingTerms, Georef, ProjectInfo, Site};
pub use template::{starter, starter_with_id};
pub use view::{Camera, Paper, PaperSize, ScheduleKind, Sheet, View, ViewKind, Viewport};

/// Stable identifier of anything in a document. Never reused within a document.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct Id(pub u64);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// A horizontal level of the building.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Storey {
    /// Identifier.
    pub id: Id,
    /// Display name, e.g. "Ισόγειο".
    pub name: String,
    /// Elevation of the finished floor above project zero (±0.00).
    pub elevation: Length,
    /// Floor-to-floor height.
    pub height: Length,
    /// Height of the plan cut plane above the floor.
    #[serde(default = "default_cut_height")]
    pub cut_height: Length,
    /// Whether the storey is below ground (υπόγειο), which changes how ΝΟΚ counts its areas.
    #[serde(default)]
    pub below_ground: bool,
}

fn default_cut_height() -> Length {
    Length::m(1.10)
}

impl Storey {
    /// A storey with the default cut height, above ground.
    pub fn new(id: Id, name: impl Into<String>, elevation: Length, height: Length) -> Storey {
        Storey {
            id,
            name: name.into(),
            elevation,
            height,
            cut_height: default_cut_height(),
            below_ground: false,
        }
    }
    /// Absolute elevation of the plan cut plane.
    pub fn cut_elevation(&self) -> Length {
        self.elevation + self.cut_height
    }
    /// Elevation of the top of the storey (floor of the next one).
    pub fn top(&self) -> Length {
        self.elevation + self.height
    }
}

/// Default attributes used by tools when creating new elements.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Defaults {
    /// Build-up for exterior walls.
    pub exterior_wall: Option<Id>,
    /// Build-up for interior walls.
    pub interior_wall: Option<Id>,
    /// Build-up for floor slabs.
    pub floor_slab: Option<Id>,
    /// Build-up for ground-bearing slabs.
    pub ground_slab: Option<Id>,
    /// Build-up for flat roofs.
    pub flat_roof: Option<Id>,
    /// Build-up for pitched roofs.
    pub pitched_roof: Option<Id>,
    /// Material for columns, beams and stairs.
    pub structural_material: Option<Id>,
    /// Material for railings.
    pub railing_material: Option<Id>,
}

/// Error when a document fails its consistency check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidDocument(pub String);

impl fmt::Display for InvalidDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid document: {}", self.0)
    }
}

impl std::error::Error for InvalidDocument {}

/// The project document.
///
/// Serialises to a JSON object with deterministic key order (maps are ordered by id).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    next_id: u64,
    /// Permanent project id; nil until one is assigned (see [`ProjectId`]).
    #[serde(default, skip_serializing_if = "ProjectId::is_nil")]
    project_id: ProjectId,
    #[serde(default)]
    tolerance: Tolerance,
    #[serde(default)]
    info: Arc<ProjectInfo>,
    #[serde(default)]
    site: Arc<Site>,
    #[serde(default)]
    defaults: Arc<Defaults>,
    #[serde(default)]
    storeys: Arc<BTreeMap<Id, Storey>>,
    #[serde(default)]
    materials: Arc<BTreeMap<Id, Material>>,
    #[serde(default)]
    build_ups: Arc<BTreeMap<Id, BuildUp>>,
    #[serde(default)]
    elements: Arc<BTreeMap<Id, Element>>,
    #[serde(default)]
    views: Arc<BTreeMap<Id, View>>,
    #[serde(default)]
    sheets: Arc<BTreeMap<Id, Sheet>>,
}

impl Document {
    /// An empty document.
    pub fn new() -> Self {
        Document::default()
    }

    // ----- Reading -------------------------------------------------------------------------------------------

    /// The project's permanent id. Nil for empty documents and for files written before ids existed; the
    /// application assigns one the first time such a document is saved.
    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Project tolerance.
    pub fn tolerance(&self) -> Tolerance {
        self.tolerance
    }

    /// Project information.
    pub fn info(&self) -> &ProjectInfo {
        &self.info
    }

    /// Site, plot and building terms.
    pub fn site(&self) -> &Site {
        &self.site
    }

    /// Default attributes for new elements.
    pub fn defaults(&self) -> &Defaults {
        &self.defaults
    }

    /// The next identifier that [`Document::with_new_id`] will hand out.
    pub fn peek_next_id(&self) -> Id {
        Id(self.next_id + 1)
    }

    /// All storeys ordered by elevation (lowest first).
    pub fn storeys(&self) -> Vec<&Storey> {
        let mut v: Vec<&Storey> = self.storeys.values().collect();
        v.sort_by(|a, b| a.elevation.total_cmp(&b.elevation).then(a.id.cmp(&b.id)));
        v
    }

    /// Looks up a storey.
    pub fn storey(&self, id: Id) -> Option<&Storey> {
        self.storeys.get(&id)
    }

    /// The storey directly above, by elevation.
    pub fn storey_above(&self, id: Id) -> Option<&Storey> {
        let all = self.storeys();
        let i = all.iter().position(|s| s.id == id)?;
        all.get(i + 1).copied()
    }

    /// The storey directly below, by elevation.
    pub fn storey_below(&self, id: Id) -> Option<&Storey> {
        let all = self.storeys();
        let i = all.iter().position(|s| s.id == id)?;
        i.checked_sub(1).and_then(|j| all.get(j).copied())
    }

    /// Looks up an element.
    pub fn element(&self, id: Id) -> Option<&Element> {
        self.elements.get(&id)
    }

    /// All elements in id order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.values()
    }

    /// Number of elements.
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Elements placed on a storey.
    pub fn elements_on(&self, storey: Id) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.storey == Some(storey))
    }

    /// Elements of a category, anywhere.
    pub fn elements_of(&self, category: Category) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.category() == category)
    }

    /// Annotation elements that belong to a non-plan view.
    pub fn elements_in_view(&self, view: Id) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.view == Some(view))
    }

    /// All walls on a storey, with their element ids.
    pub fn walls_on(&self, storey: Id) -> impl Iterator<Item = (Id, &Wall)> {
        self.elements_on(storey).filter_map(|e| e.as_wall().map(|w| (e.id, w)))
    }

    /// Doors, windows and voids hosted by a wall.
    pub fn openings_in(&self, wall: Id) -> impl Iterator<Item = (Id, &Opening)> {
        self.elements.values().filter_map(move |e| match &e.kind {
            ElementKind::Opening(o) if o.host == wall => Some((e.id, o)),
            _ => None,
        })
    }

    /// Looks up a material.
    pub fn material(&self, id: Id) -> Option<&Material> {
        self.materials.get(&id)
    }

    /// All materials in id order.
    pub fn materials(&self) -> impl Iterator<Item = &Material> {
        self.materials.values()
    }

    /// Looks up a build-up.
    pub fn build_up(&self, id: Id) -> Option<&BuildUp> {
        self.build_ups.get(&id)
    }

    /// All build-ups in id order.
    pub fn build_ups(&self) -> impl Iterator<Item = &BuildUp> {
        self.build_ups.values()
    }

    /// Looks up a view.
    pub fn view(&self, id: Id) -> Option<&View> {
        self.views.get(&id)
    }

    /// All views in id order.
    pub fn views(&self) -> impl Iterator<Item = &View> {
        self.views.values()
    }

    /// The plan view of a storey, if one exists.
    pub fn plan_view_of(&self, storey: Id) -> Option<&View> {
        self.views.values().find(|v| matches!(v.kind, ViewKind::Plan { storey: s } if s == storey))
    }

    /// Looks up a sheet.
    pub fn sheet(&self, id: Id) -> Option<&Sheet> {
        self.sheets.get(&id)
    }

    /// All sheets, ordered by number then id.
    pub fn sheets(&self) -> Vec<&Sheet> {
        let mut v: Vec<&Sheet> = self.sheets.values().collect();
        v.sort_by(|a, b| a.number.cmp(&b.number).then(a.id.cmp(&b.id)));
        v
    }

    /// Total thickness of a structure, or `None` if it refers to a missing build-up.
    pub fn structure_thickness(&self, s: &Structure) -> Option<Length> {
        match s {
            Structure::Basic { thickness, .. } => Some(*thickness),
            Structure::Layered { build_up } => self.build_up(*build_up).map(BuildUp::thickness),
        }
    }

    /// Layers of a structure (a basic structure is one layer), or `None` if the build-up is missing.
    pub fn structure_layers(&self, s: &Structure) -> Option<Vec<BuildUpLayer>> {
        match s {
            Structure::Basic { material, thickness } => {
                Some(vec![BuildUpLayer { material: *material, thickness: *thickness, core: true }])
            }
            Structure::Layered { build_up } => self.build_up(*build_up).map(|b| b.layers.clone()),
        }
    }

    /// The material with the highest junction priority in a structure (its core), if any.
    pub fn structure_core_material(&self, s: &Structure) -> Option<Id> {
        let layers = self.structure_layers(s)?;
        if let Some(core) = layers.iter().find(|l| l.core) {
            return Some(core.material);
        }
        layers
            .iter()
            .max_by_key(|l| self.material(l.material).map(|m| m.priority).unwrap_or(0))
            .map(|l| l.material)
    }

    /// Whether any element, build-up or default refers to a material.
    pub fn material_in_use(&self, id: Id) -> bool {
        let in_structure = |s: &Structure| match s {
            Structure::Basic { material, .. } => *material == id,
            Structure::Layered { .. } => false,
        };
        self.build_ups.values().any(|b| b.layers.iter().any(|l| l.material == id))
            || self.elements.values().any(|e| match &e.kind {
                ElementKind::Wall(w) => in_structure(&w.structure),
                ElementKind::Slab(s) => in_structure(&s.structure),
                ElementKind::Roof(r) => in_structure(&r.structure),
                ElementKind::Column(c) => c.material == id,
                ElementKind::Beam(b) => b.material == id,
                ElementKind::Stair(s) => s.material == id,
                ElementKind::Railing(r) => r.material == id,
                _ => false,
            })
    }

    /// Whether any element refers to a build-up.
    pub fn build_up_in_use(&self, id: Id) -> bool {
        let uses = |s: &Structure| matches!(s, Structure::Layered { build_up } if *build_up == id);
        self.elements.values().any(|e| match &e.kind {
            ElementKind::Wall(w) => uses(&w.structure),
            ElementKind::Slab(s) => uses(&s.structure),
            ElementKind::Roof(r) => uses(&r.structure),
            _ => false,
        })
    }

    // ----- Changing (each returns a new document) ------------------------------------------------------------

    /// Reserves a fresh identifier, returning it with the updated document.
    pub fn with_new_id(&self) -> (Document, Id) {
        let mut next = self.clone();
        next.next_id += 1;
        let id = Id(next.next_id);
        (next, id)
    }

    /// Returns a document with a different project id. Only for creating, importing or first saving a project:
    /// the id identifies the project for its whole life.
    pub fn with_project_id(&self, id: ProjectId) -> Document {
        Document { project_id: id, ..self.clone() }
    }

    /// Returns a document with a different tolerance.
    pub fn with_tolerance(&self, tolerance: Tolerance) -> Document {
        Document { tolerance, ..self.clone() }
    }

    /// Returns a document with new project information.
    pub fn with_info(&self, info: ProjectInfo) -> Document {
        Document { info: Arc::new(info), ..self.clone() }
    }

    /// Returns a document with a new site.
    pub fn with_site(&self, site: Site) -> Document {
        Document { site: Arc::new(site), ..self.clone() }
    }

    /// Returns a document with new defaults.
    pub fn with_defaults(&self, defaults: Defaults) -> Document {
        Document { defaults: Arc::new(defaults), ..self.clone() }
    }

    /// Returns a document with the storey inserted or replaced.
    pub fn with_storey(&self, storey: Storey) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.storeys).insert(storey.id, storey);
        next
    }

    /// Returns a document without the storey, its elements and its plan view, or `None` if it does not exist.
    pub fn without_storey(&self, id: Id) -> Option<Document> {
        if !self.storeys.contains_key(&id) {
            return None;
        }
        let mut next = self.clone();
        Arc::make_mut(&mut next.storeys).remove(&id);
        let doomed: Vec<Id> = self.elements_on(id).map(|e| e.id).collect();
        for e in doomed {
            next = next.without_element(e).unwrap_or(next);
        }
        let plan_views: Vec<Id> = self
            .views
            .values()
            .filter(|v| matches!(v.kind, ViewKind::Plan { storey } if storey == id))
            .map(|v| v.id)
            .collect();
        for v in plan_views {
            next = next.without_view(v).unwrap_or(next);
        }
        Some(next)
    }

    /// Returns a document with the element inserted or replaced.
    pub fn with_element(&self, element: Element) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.elements).insert(element.id, element);
        next
    }

    /// Returns a document without the element and the openings it hosts, or `None` if it does not exist.
    pub fn without_element(&self, id: Id) -> Option<Document> {
        if !self.elements.contains_key(&id) {
            return None;
        }
        let hosted: Vec<Id> = self.openings_in(id).map(|(oid, _)| oid).collect();
        let mut next = self.clone();
        let map = Arc::make_mut(&mut next.elements);
        map.remove(&id);
        for h in hosted {
            map.remove(&h);
        }
        Some(next)
    }

    /// Returns a document with the material inserted or replaced.
    pub fn with_material(&self, material: Material) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.materials).insert(material.id, material);
        next
    }

    /// Returns a document without the material, or `None` if it does not exist. Callers check
    /// [`Document::material_in_use`] first.
    pub fn without_material(&self, id: Id) -> Option<Document> {
        if !self.materials.contains_key(&id) {
            return None;
        }
        let mut next = self.clone();
        Arc::make_mut(&mut next.materials).remove(&id);
        Some(next)
    }

    /// Returns a document with the build-up inserted or replaced.
    pub fn with_build_up(&self, build_up: BuildUp) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.build_ups).insert(build_up.id, build_up);
        next
    }

    /// Returns a document without the build-up, or `None` if it does not exist. Callers check
    /// [`Document::build_up_in_use`] first.
    pub fn without_build_up(&self, id: Id) -> Option<Document> {
        if !self.build_ups.contains_key(&id) {
            return None;
        }
        let mut next = self.clone();
        Arc::make_mut(&mut next.build_ups).remove(&id);
        Some(next)
    }

    /// Returns a document with the view inserted or replaced.
    pub fn with_view(&self, view: View) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.views).insert(view.id, view);
        next
    }

    /// Returns a document without the view, its annotations and any viewports showing it, or `None` if it does
    /// not exist.
    pub fn without_view(&self, id: Id) -> Option<Document> {
        if !self.views.contains_key(&id) {
            return None;
        }
        let mut next = self.clone();
        Arc::make_mut(&mut next.views).remove(&id);
        let owned: Vec<Id> = self.elements_in_view(id).map(|e| e.id).collect();
        if !owned.is_empty() {
            let map = Arc::make_mut(&mut next.elements);
            for e in owned {
                map.remove(&e);
            }
        }
        if self.sheets.values().any(|s| s.viewports.iter().any(|vp| vp.view == id)) {
            for sheet in Arc::make_mut(&mut next.sheets).values_mut() {
                sheet.viewports.retain(|vp| vp.view != id);
            }
        }
        Some(next)
    }

    /// Returns a document with the sheet inserted or replaced.
    pub fn with_sheet(&self, sheet: Sheet) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.sheets).insert(sheet.id, sheet);
        next
    }

    /// Returns a document without the sheet, or `None` if it does not exist.
    pub fn without_sheet(&self, id: Id) -> Option<Document> {
        if !self.sheets.contains_key(&id) {
            return None;
        }
        let mut next = self.clone();
        Arc::make_mut(&mut next.sheets).remove(&id);
        Some(next)
    }

    /// Whether two documents share the same element storage (no element changed between them).
    pub fn shares_elements_with(&self, other: &Document) -> bool {
        Arc::ptr_eq(&self.elements, &other.elements)
    }

    /// Checks internal consistency after loading: ids below the id counter and unique, references resolvable.
    pub fn validate(&self) -> Result<(), InvalidDocument> {
        let max = |it: &mut dyn Iterator<Item = Id>| it.map(|i| i.0).max().unwrap_or(0);
        let highest = [
            max(&mut self.storeys.keys().copied()),
            max(&mut self.elements.keys().copied()),
            max(&mut self.materials.keys().copied()),
            max(&mut self.build_ups.keys().copied()),
            max(&mut self.views.keys().copied()),
            max(&mut self.sheets.keys().copied()),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        if highest > self.next_id {
            return Err(InvalidDocument(format!("id {highest} is above the id counter {}", self.next_id)));
        }
        for (k, e) in self.elements.iter() {
            if *k != e.id {
                return Err(InvalidDocument(format!("element key {k} holds element {}", e.id)));
            }
            if let Some(s) = e.storey
                && !self.storeys.contains_key(&s)
            {
                return Err(InvalidDocument(format!("element {} is on missing storey {s}", e.id)));
            }
            if let ElementKind::Opening(o) = &e.kind
                && self.element(o.host).and_then(Element::as_wall).is_none()
            {
                return Err(InvalidDocument(format!("opening {} has no host wall {}", e.id, o.host)));
            }
        }
        for b in self.build_ups.values() {
            for l in &b.layers {
                if !self.materials.contains_key(&l.material) {
                    return Err(InvalidDocument(format!(
                        "build-up {} uses missing material {}",
                        b.id, l.material
                    )));
                }
            }
        }
        for v in self.views.values() {
            if let ViewKind::Plan { storey } = v.kind
                && !self.storeys.contains_key(&storey)
            {
                return Err(InvalidDocument(format!("plan view {} shows missing storey {storey}", v.id)));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Point2;

    fn ground(doc: &Document) -> (Document, Id) {
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_storey(Storey::new(id, "Ισόγειο", Length::ZERO, Length::m(3.2)));
        (doc, id)
    }

    fn wall(storey: Id, id: Id) -> Element {
        Element::on_storey(
            id,
            storey,
            ElementKind::Wall(Wall {
                start: Point2::new(0.0, 0.0),
                end: Point2::new(5.0, 0.0),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Basic { material: Id(999), thickness: Length::cm(25.0) },
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        )
    }

    #[test]
    fn ids_are_unique_and_increasing() {
        let d = Document::new();
        let (d, a) = d.with_new_id();
        let (_, b) = d.with_new_id();
        assert!(b > a);
    }

    #[test]
    fn versions_share_unchanged_data() {
        let (d1, s) = ground(&Document::new());
        let d2 = d1.with_storey(Storey::new(s, "Ισόγειο", Length::ZERO, Length::m(3.0)));
        assert!(d1.shares_elements_with(&d2));
        assert!(d1.storey(s).map(|x| x.height) != d2.storey(s).map(|x| x.height));
    }

    #[test]
    fn walls_are_filtered_by_storey() {
        let (d, s) = ground(&Document::new());
        let (d, w) = d.with_new_id();
        let d = d.with_element(wall(s, w));
        assert_eq!(d.walls_on(s).count(), 1);
        assert_eq!(d.walls_on(Id(999)).count(), 0);
    }

    #[test]
    fn deleting_a_wall_deletes_its_openings() {
        let (d, s) = ground(&Document::new());
        let (d, w) = d.with_new_id();
        let d = d.with_element(wall(s, w));
        let (d, o) = d.with_new_id();
        let d = d.with_element(Element::on_storey(
            o,
            s,
            ElementKind::Opening(Opening {
                host: w,
                kind: OpeningKind::Door,
                style: "door.single_swing".into(),
                offset: Length::m(2.0),
                width: Length::m(0.9),
                height: Length::m(2.1),
                sill: Length::ZERO,
                flip_side: false,
                flip_hand: false,
                params: Default::default(),
            }),
        ));
        assert_eq!(d.openings_in(w).count(), 1);
        let d = d.without_element(w).unwrap();
        assert!(d.element(o).is_none());
    }

    #[test]
    fn deleting_a_storey_cascades() {
        let doc = starter();
        let ground = doc.storeys()[0].id;
        assert!(doc.plan_view_of(ground).is_some());
        let doc = doc.without_storey(ground).unwrap();
        assert!(doc.plan_view_of(ground).is_none());
        doc.validate().unwrap();
    }

    #[test]
    fn starter_round_trips_through_json() {
        let doc = starter();
        doc.validate().unwrap();
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
        // Deterministic: serialising again gives the same text.
        assert_eq!(json, serde_json::to_string_pretty(&back).unwrap());
    }

    #[test]
    fn project_id_is_written_as_a_uuid_and_optional_when_reading() {
        let id = ProjectId::from_u128(0x1234_5678_9abc_4def_8123_4567_89ab_cdef);
        let doc = Document::new().with_project_id(id);
        let v = serde_json::to_value(&doc).unwrap();
        assert_eq!(v["project_id"], "12345678-9abc-4def-8123-456789abcdef");
        assert_eq!(serde_json::from_value::<Document>(v).unwrap().project_id(), id);
        // Documents without an id (older files, empty documents) read and write without the key.
        let old: Document = serde_json::from_str(r#"{"next_id": 0}"#).unwrap();
        assert!(old.project_id().is_nil());
        assert!(serde_json::to_value(&old).unwrap().get("project_id").is_none());
    }
}

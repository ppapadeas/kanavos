//! Built-in commands. Each command's `name()` is part of the public control-protocol API: the JSON form is
//! `{"name": "<command name>", "args": {...}}` (see [`AnyCommand`]).
//!
//! The original commands live in this file; commands added later are grouped by area in submodules and
//! re-exported here, so every command is `kanavos_engine::commands::<Type>`.

mod build;
mod dup;
mod edit;
pub mod examples;
mod sheet;
mod site;

pub use build::{AutoDimensions, DetectAllSpaces, PlaceOpening, RoofFromWalls, RoofKind, SlabFromWalls};
pub use dup::{DuplicateBuildUp, DuplicateMaterial, DuplicateStorey, DuplicateView};
pub use edit::{AlignEdge, AlignElements, JoinWalls};
pub use sheet::{MoveViewport, PlaceView, RemoveViewport};
pub use site::{SetBuildingTerms, SetPlot};

pub use crate::geo::OutlineFace;

use std::collections::{BTreeMap, BTreeSet};

use kanavos_geom::{Angle, Point2, Transform2, Vec2};
use kanavos_model::{
    BuildUp, Defaults, Document, Element, ElementKind, Id, Material, Paper, Phase, ProjectInfo, Sheet, Site,
    SpaceCategory, Storey, View, ViewKind, Viewport,
};
use kanavos_units::{Length, Locale};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::labels::{counted, new_element, noun_key};
use crate::{Command, CommandError, Outcome};

pub(crate) fn geometry(e: kanavos_elements::GeometryError) -> CommandError {
    match e {
        kanavos_elements::GeometryError::Invalid { field, reason, .. } => {
            CommandError::InvalidArgument { field, reason }
        }
        kanavos_elements::GeometryError::NotFound(id) => CommandError::NotFound(id),
        other => CommandError::Geometry(other.to_string()),
    }
}

pub(crate) fn positive(v: Length, field: &'static str, doc: &Document) -> Result<(), CommandError> {
    if v.is_positive(&doc.tolerance()) {
        Ok(())
    } else {
        Err(CommandError::InvalidArgument { field, reason: "must be a positive length".into() })
    }
}

pub(crate) fn non_empty(s: &str, field: &'static str) -> Result<(), CommandError> {
    if s.trim().is_empty() {
        Err(CommandError::InvalidArgument { field, reason: "must not be empty".into() })
    } else {
        Ok(())
    }
}

// ----- Project, site, defaults ---------------------------------------------------------------------------------

/// `project.set_info` — replaces the project information (title block data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetProjectInfo {
    /// New project information.
    pub info: ProjectInfo,
}

impl Command for SetProjectInfo {
    fn name(&self) -> &'static str {
        "project.set_info"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        Ok((doc.with_info(self.info.clone()), Outcome::default()))
    }
}

/// `site.set` — replaces the site: plot, street and building lines, building terms, georeferencing, terrain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetSite {
    /// New site.
    pub site: Site,
}

impl Command for SetSite {
    fn name(&self) -> &'static str {
        "site.set"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let t = &self.site.terms;
        if !(t.building_coefficient >= 0.0 && t.building_coefficient.is_finite()) {
            return Err(CommandError::InvalidArgument {
                field: "building_coefficient",
                reason: "must be zero or more".into(),
            });
        }
        if !(0.0..=1.0).contains(&t.coverage_ratio) {
            return Err(CommandError::InvalidArgument {
                field: "coverage_ratio",
                reason: "must be between 0 and 1".into(),
            });
        }
        if let Some(plot) = &self.site.plot
            && !plot.is_valid(&doc.tolerance())
        {
            return Err(CommandError::InvalidArgument {
                field: "plot",
                reason: "needs at least three points enclosing an area".into(),
            });
        }
        Ok((doc.with_site(self.site.clone()), Outcome::default()))
    }
}

/// `defaults.set` — sets the default build-ups and materials used by tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetDefaults {
    /// New defaults.
    pub defaults: Defaults,
}

impl Command for SetDefaults {
    fn name(&self) -> &'static str {
        "defaults.set"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        Ok((doc.with_defaults(self.defaults.clone()), Outcome::default()))
    }
}

// ----- Storeys -------------------------------------------------------------------------------------------------

/// `storey.create` — adds a storey and its plan view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateStorey {
    /// Display name.
    pub name: String,
    /// Finished floor elevation above project zero.
    pub elevation: Length,
    /// Floor-to-floor height.
    pub height: Length,
    /// Plan cut height above the floor (default 1.10 m).
    #[serde(default)]
    pub cut_height: Option<Length>,
    /// Below ground.
    #[serde(default)]
    pub below_ground: bool,
    /// Name of the plan view; defaults to "Κάτοψη <name>".
    #[serde(default)]
    pub plan_name: Option<String>,
}

impl CreateStorey {
    /// A storey with default cut height, above ground.
    pub fn new(name: impl Into<String>, elevation: Length, height: Length) -> Self {
        CreateStorey {
            name: name.into(),
            elevation,
            height,
            cut_height: None,
            below_ground: false,
            plan_name: None,
        }
    }
}

impl Command for CreateStorey {
    fn name(&self) -> &'static str {
        "storey.create"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        non_empty(&self.name, "name")?;
        positive(self.height, "height", doc)?;
        if !self.elevation.is_finite() {
            return Err(CommandError::InvalidArgument {
                field: "elevation",
                reason: "must be finite".into(),
            });
        }
        let (doc, id) = doc.with_new_id();
        let mut storey = Storey::new(id, self.name.trim(), self.elevation, self.height);
        if let Some(c) = self.cut_height {
            positive(c, "cut_height", &doc)?;
            storey.cut_height = c;
        }
        storey.below_ground = self.below_ground;
        let doc = doc.with_storey(storey);
        let (doc, vid) = doc.with_new_id();
        let plan_name = self.plan_name.clone().unwrap_or_else(|| format!("Κάτοψη {}", self.name.trim()));
        let doc = doc.with_view(View {
            id: vid,
            name: plan_name,
            kind: ViewKind::Plan { storey: id },
            scale: 50,
            hidden_layers: BTreeSet::new(),
        });
        Ok((doc, Outcome { created: vec![id, vid], ..Outcome::default() }))
    }
}

/// `storey.update` — changes a storey. Only the given fields change.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateStorey {
    /// Storey.
    pub id: Id,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New elevation.
    #[serde(default)]
    pub elevation: Option<Length>,
    /// New height.
    #[serde(default)]
    pub height: Option<Length>,
    /// New cut height.
    #[serde(default)]
    pub cut_height: Option<Length>,
    /// New below-ground flag.
    #[serde(default)]
    pub below_ground: Option<bool>,
    /// When the height changes, move the storeys above by the same amount.
    #[serde(default)]
    pub shift_above: bool,
}

impl Command for UpdateStorey {
    fn name(&self) -> &'static str {
        "storey.update"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let old = doc.storey(self.id).ok_or(CommandError::NotFound(self.id))?.clone();
        let mut s = old.clone();
        if let Some(n) = &self.name {
            non_empty(n, "name")?;
            s.name = n.trim().to_owned();
        }
        if let Some(e) = self.elevation {
            if !e.is_finite() {
                return Err(CommandError::InvalidArgument {
                    field: "elevation",
                    reason: "must be finite".into(),
                });
            }
            s.elevation = e;
        }
        if let Some(h) = self.height {
            positive(h, "height", doc)?;
            s.height = h;
        }
        if let Some(c) = self.cut_height {
            positive(c, "cut_height", doc)?;
            s.cut_height = c;
        }
        if let Some(b) = self.below_ground {
            s.below_ground = b;
        }
        let mut next = doc.with_storey(s.clone());
        let mut changed = vec![self.id];
        if self.shift_above {
            let delta = s.top() - old.top();
            if !doc.tolerance().is_zero(delta) {
                for above in
                    doc.storeys().into_iter().filter(|o| o.elevation > old.elevation && o.id != old.id)
                {
                    let mut a = above.clone();
                    a.elevation += delta;
                    next = next.with_storey(a);
                    changed.push(above.id);
                }
            }
        }
        Ok((next, Outcome { changed, ..Outcome::default() }))
    }
}

/// `storey.delete` — removes a storey with all its elements and its plan view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteStorey {
    /// Storey.
    pub id: Id,
}

impl Command for DeleteStorey {
    fn name(&self) -> &'static str {
        "storey.delete"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if doc.storeys().len() <= 1 {
            return Err(CommandError::InvalidArgument {
                field: "id",
                reason: "the last storey cannot be deleted".into(),
            });
        }
        let deleted: Vec<Id> = doc.elements_on(self.id).map(|e| e.id).collect();
        let next = doc.without_storey(self.id).ok_or(CommandError::NotFound(self.id))?;
        Ok((next, Outcome { deleted, ..Outcome::default() }))
    }
}

// ----- Materials and build-ups ---------------------------------------------------------------------------------

/// `material.create` — adds a material (its `id` is ignored and assigned).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateMaterial {
    /// The material.
    pub material: Material,
}

impl Command for CreateMaterial {
    fn name(&self) -> &'static str {
        "material.create"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        non_empty(&self.material.name, "name")?;
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_material(Material { id, ..self.material.clone() });
        Ok((doc, Outcome { created: vec![id], ..Outcome::default() }))
    }
}

/// `material.update` — replaces a material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateMaterial {
    /// The material, with the id of the one to replace.
    pub material: Material,
}

impl Command for UpdateMaterial {
    fn name(&self) -> &'static str {
        "material.update"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let id = self.material.id;
        doc.material(id).ok_or(CommandError::NotFound(id))?;
        non_empty(&self.material.name, "name")?;
        Ok((doc.with_material(self.material.clone()), Outcome { changed: vec![id], ..Outcome::default() }))
    }
}

/// `material.delete` — removes an unused material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteMaterial {
    /// Material.
    pub id: Id,
}

impl Command for DeleteMaterial {
    fn name(&self) -> &'static str {
        "material.delete"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if doc.material_in_use(self.id) {
            return Err(CommandError::InUse(self.id));
        }
        let next = doc.without_material(self.id).ok_or(CommandError::NotFound(self.id))?;
        Ok((next, Outcome { deleted: vec![self.id], ..Outcome::default() }))
    }
}

fn check_build_up(doc: &Document, b: &BuildUp) -> Result<(), CommandError> {
    non_empty(&b.name, "name")?;
    if b.layers.is_empty() {
        return Err(CommandError::InvalidArgument {
            field: "layers",
            reason: "needs at least one layer".into(),
        });
    }
    for l in &b.layers {
        doc.material(l.material).ok_or(CommandError::NotFound(l.material))?;
        positive(l.thickness, "thickness", doc)?;
    }
    Ok(())
}

/// `build_up.create` — adds a build-up (its `id` is ignored and assigned).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateBuildUp {
    /// The build-up.
    pub build_up: BuildUp,
}

impl Command for CreateBuildUp {
    fn name(&self) -> &'static str {
        "build_up.create"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        check_build_up(doc, &self.build_up)?;
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_build_up(BuildUp { id, ..self.build_up.clone() });
        Ok((doc, Outcome { created: vec![id], ..Outcome::default() }))
    }
}

/// `build_up.update` — replaces a build-up; elements using it update automatically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateBuildUp {
    /// The build-up, with the id of the one to replace.
    pub build_up: BuildUp,
}

impl Command for UpdateBuildUp {
    fn name(&self) -> &'static str {
        "build_up.update"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let id = self.build_up.id;
        doc.build_up(id).ok_or(CommandError::NotFound(id))?;
        check_build_up(doc, &self.build_up)?;
        Ok((doc.with_build_up(self.build_up.clone()), Outcome { changed: vec![id], ..Outcome::default() }))
    }
}

/// `build_up.delete` — removes an unused build-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteBuildUp {
    /// Build-up.
    pub id: Id,
}

impl Command for DeleteBuildUp {
    fn name(&self) -> &'static str {
        "build_up.delete"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if doc.build_up_in_use(self.id) {
            return Err(CommandError::InUse(self.id));
        }
        let next = doc.without_build_up(self.id).ok_or(CommandError::NotFound(self.id))?;
        Ok((next, Outcome { deleted: vec![self.id], ..Outcome::default() }))
    }
}

// ----- Elements ------------------------------------------------------------------------------------------------

/// `element.create` — adds any element: walls, slabs, openings, stairs, annotation …
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateElement {
    /// Storey (required for building parts and plan annotation).
    #[serde(default)]
    pub storey: Option<Id>,
    /// Owning section/elevation view, for annotation drawn there.
    #[serde(default)]
    pub view: Option<Id>,
    /// Type-specific data.
    pub kind: ElementKind,
    /// User mark.
    #[serde(default)]
    pub tag: String,
    /// Construction status.
    #[serde(default)]
    pub phase: Phase,
    /// Free user properties.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

impl CreateElement {
    /// An element on a storey with no tag or properties.
    pub fn on(storey: Id, kind: ElementKind) -> Self {
        CreateElement {
            storey: Some(storey),
            view: None,
            kind,
            tag: String::new(),
            phase: Phase::New,
            properties: BTreeMap::new(),
        }
    }
}

impl Command for CreateElement {
    fn name(&self) -> &'static str {
        "element.create"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if let Some(v) = self.view {
            doc.view(v).ok_or(CommandError::NotFound(v))?;
        }
        let (next, id) = doc.with_new_id();
        let e = Element {
            id,
            storey: self.storey,
            view: self.view,
            phase: self.phase,
            tag: self.tag.clone(),
            properties: self.properties.clone(),
            kind: self.kind.clone(),
        };
        kanavos_elements::validate(&next, &e).map_err(geometry)?;
        Ok((next.with_element(e), Outcome { created: vec![id], ..Outcome::default() }))
    }
    fn label(&self, locale: Locale) -> String {
        new_element(locale, self.kind.category())
    }
}

/// `element.update` — changes an element. Only the given fields change; `kind` replaces all type data.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateElement {
    /// Element.
    pub id: Id,
    /// New type data (must be the same type).
    #[serde(default)]
    pub kind: Option<ElementKind>,
    /// New storey.
    #[serde(default)]
    pub storey: Option<Id>,
    /// New tag.
    #[serde(default)]
    pub tag: Option<String>,
    /// New phase.
    #[serde(default)]
    pub phase: Option<Phase>,
    /// New properties.
    #[serde(default)]
    pub properties: Option<BTreeMap<String, String>>,
}

impl Command for UpdateElement {
    fn name(&self) -> &'static str {
        "element.update"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let mut e = doc.element(self.id).ok_or(CommandError::NotFound(self.id))?.clone();
        if let Some(k) = &self.kind {
            if k.type_name() != e.kind.type_name() {
                return Err(CommandError::InvalidArgument {
                    field: "kind",
                    reason: format!("must stay a {}", e.kind.type_name()),
                });
            }
            e.kind = k.clone();
        }
        if let Some(s) = self.storey {
            e.storey = Some(s);
        }
        if let Some(t) = &self.tag {
            e.tag = t.clone();
        }
        if let Some(p) = self.phase {
            e.phase = p;
        }
        if let Some(p) = &self.properties {
            e.properties = p.clone();
        }
        kanavos_elements::validate(doc, &e).map_err(geometry)?;
        let mut next = doc.with_element(e.clone());
        // Hosted openings follow their wall to a new storey.
        if self.storey.is_some() {
            let hosted: Vec<Element> =
                doc.openings_in(self.id).filter_map(|(oid, _)| doc.element(oid).cloned()).collect();
            for mut o in hosted {
                o.storey = e.storey;
                next = next.with_element(o);
            }
        }
        Ok((next, Outcome { changed: vec![self.id], ..Outcome::default() }))
    }
    fn label(&self, locale: Locale) -> String {
        let key = self.kind.as_ref().map(|k| k.category().key()).unwrap_or("element");
        counted(locale, "update", 1, key)
    }
    fn describe(&self, doc: &Document, locale: Locale) -> String {
        counted(locale, "update", 1, noun_key(Some(doc), &[self.id]))
    }
}

/// `element.delete` — removes elements (and the openings their walls host).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteElements {
    /// Elements.
    pub ids: Vec<Id>,
}

impl Command for DeleteElements {
    fn name(&self) -> &'static str {
        "element.delete"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if self.ids.is_empty() {
            return Err(CommandError::InvalidArgument { field: "ids", reason: "must not be empty".into() });
        }
        let mut next = doc.clone();
        let mut deleted = Vec::new();
        for id in &self.ids {
            if next.element(*id).is_none() {
                if doc.element(*id).is_some() {
                    continue; // Already removed with its host.
                }
                return Err(CommandError::NotFound(*id));
            }
            deleted.extend(next.openings_in(*id).map(|(o, _)| o));
            deleted.push(*id);
            next = next.without_element(*id).ok_or(CommandError::NotFound(*id))?;
        }
        Ok((next, Outcome { deleted, ..Outcome::default() }))
    }
    fn label(&self, locale: Locale) -> String {
        counted(locale, "delete", self.ids.len(), "element")
    }
    fn describe(&self, doc: &Document, locale: Locale) -> String {
        counted(locale, "delete", self.ids.len(), noun_key(Some(doc), &self.ids))
    }
}

/// A geometric transformation in plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformSpec {
    /// Move by a vector (metres).
    Move {
        /// X displacement.
        dx: f64,
        /// Y displacement.
        dy: f64,
    },
    /// Rotate about a centre by an angle in degrees (counter-clockwise).
    Rotate {
        /// Centre.
        centre: Point2,
        /// Angle in degrees.
        degrees: f64,
    },
    /// Mirror about the line through two points.
    Mirror {
        /// First point on the axis.
        a: Point2,
        /// Second point on the axis.
        b: Point2,
    },
}

impl TransformSpec {
    /// The affine transform.
    pub fn to_transform(&self) -> Transform2 {
        match self {
            TransformSpec::Move { dx, dy } => Transform2::translation(Vec2::new(*dx, *dy)),
            TransformSpec::Rotate { centre, degrees } => {
                Transform2::rotation_about(*centre, Angle::deg(*degrees))
            }
            TransformSpec::Mirror { a, b } => Transform2::mirror(*a, *b),
        }
    }
    /// The transform applied `n` times (for array copies).
    pub fn repeated(&self, n: u32) -> Transform2 {
        let one = self.to_transform();
        let mut t = Transform2::IDENTITY;
        for _ in 0..n {
            t = one.then(&t);
        }
        t
    }
}

/// `element.transform` — moves, rotates or mirrors elements, optionally as copies (`count` copies form an array).
///
/// Openings move with their host wall. An opening selected without its host slides along the wall by the move's
/// component along the wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransformElements {
    /// Elements.
    pub ids: Vec<Id>,
    /// Transformation.
    pub transform: TransformSpec,
    /// Make copies instead of moving.
    #[serde(default)]
    pub copy: bool,
    /// Number of copies when `copy` is set (each one transformed once more than the previous).
    #[serde(default = "one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

impl Command for TransformElements {
    fn name(&self) -> &'static str {
        "element.transform"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if self.ids.is_empty() {
            return Err(CommandError::InvalidArgument { field: "ids", reason: "must not be empty".into() });
        }
        for id in &self.ids {
            doc.element(*id).ok_or(CommandError::NotFound(*id))?;
        }
        let selected: BTreeSet<Id> = self.ids.iter().copied().collect();
        let mut next = doc.clone();
        let mut out = Outcome::default();
        if !self.copy {
            let t = self.transform.to_transform();
            for id in &self.ids {
                let Some(e) = doc.element(*id) else { continue };
                let mut moved = e.clone();
                if let ElementKind::Opening(o) = &e.kind {
                    if selected.contains(&o.host) {
                        continue;
                    }
                    if let TransformSpec::Move { dx, dy } = self.transform
                        && let Some(w) = doc.element(o.host).and_then(Element::as_wall)
                        && let Some(d) = w.direction()
                    {
                        let mut o2 = o.clone();
                        o2.offset += Length::m(Vec2::new(dx, dy).dot(d));
                        moved.kind = ElementKind::Opening(o2);
                    } else {
                        continue;
                    }
                } else {
                    moved.kind = e.kind.transformed(&t);
                }
                kanavos_elements::validate(&next, &moved).map_err(geometry)?;
                next = next.with_element(moved);
                out.changed.push(*id);
            }
            return Ok((next, out));
        }
        let count = self.count.clamp(1, 1000);
        for n in 1..=count {
            let t = self.transform.repeated(n);
            let mut new_ids: BTreeMap<Id, Id> = BTreeMap::new();
            // Hosts first, so copied openings can point at copied walls.
            let mut order: Vec<&Element> = self.ids.iter().filter_map(|id| doc.element(*id)).collect();
            order.sort_by_key(|e| matches!(e.kind, ElementKind::Opening(_)));
            for e in order {
                let mut c = e.clone();
                let (d, nid) = next.with_new_id();
                next = d;
                c.id = nid;
                match &e.kind {
                    ElementKind::Opening(o) => {
                        let Some(host) = new_ids.get(&o.host) else { continue };
                        let mut o2 = o.clone();
                        o2.host = *host;
                        c.kind = ElementKind::Opening(o2);
                    }
                    k => c.kind = k.transformed(&t),
                }
                new_ids.insert(e.id, nid);
                next = next.with_element(c);
                out.created.push(nid);
            }
            // Openings hosted by copied walls but not selected come along too.
            for (old_wall, new_wall) in new_ids.clone() {
                let hosted: Vec<Element> = doc
                    .openings_in(old_wall)
                    .filter(|(oid, _)| !selected.contains(oid))
                    .filter_map(|(oid, _)| doc.element(oid).cloned())
                    .collect();
                for o in hosted {
                    let ElementKind::Opening(op) = &o.kind else { continue };
                    let (d, nid) = next.with_new_id();
                    next = d;
                    let mut op2 = op.clone();
                    op2.host = new_wall;
                    next =
                        next.with_element(Element { id: nid, kind: ElementKind::Opening(op2), ..o.clone() });
                    out.created.push(nid);
                }
            }
        }
        Ok((next, out))
    }
    fn label(&self, locale: Locale) -> String {
        self.label_for(locale, "element")
    }
    fn describe(&self, doc: &Document, locale: Locale) -> String {
        self.label_for(locale, noun_key(Some(doc), &self.ids))
    }
}

impl TransformElements {
    fn label_for(&self, locale: Locale, key: &str) -> String {
        let n = self.ids.len();
        if self.copy {
            let base = counted(locale, "copy", n, key);
            if self.count > 1 { format!("{base} ×{}", self.count) } else { base }
        } else {
            let verb = match self.transform {
                TransformSpec::Move { .. } => "move",
                TransformSpec::Rotate { .. } => "rotate",
                TransformSpec::Mirror { .. } => "mirror",
            };
            counted(locale, verb, n, key)
        }
    }
}

/// `element.copy_to_storey` — copies elements to another storey at the same plan position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CopyToStorey {
    /// Elements.
    pub ids: Vec<Id>,
    /// Target storey.
    pub storey: Id,
}

impl Command for CopyToStorey {
    fn name(&self) -> &'static str {
        "element.copy_to_storey"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        doc.storey(self.storey).ok_or(CommandError::NotFound(self.storey))?;
        let mut next = doc.clone();
        let mut out = Outcome::default();
        let mut map: BTreeMap<Id, Id> = BTreeMap::new();
        let mut order: Vec<&Element> = self.ids.iter().filter_map(|id| doc.element(*id)).collect();
        if order.len() != self.ids.len() {
            let missing = self.ids.iter().find(|id| doc.element(**id).is_none()).copied().unwrap_or(Id(0));
            return Err(CommandError::NotFound(missing));
        }
        order.sort_by_key(|e| matches!(e.kind, ElementKind::Opening(_)));
        let selected: BTreeSet<Id> = self.ids.iter().copied().collect();
        for e in order {
            let mut c = e.clone();
            let (d, nid) = next.with_new_id();
            next = d;
            c.id = nid;
            c.storey = Some(self.storey);
            if let ElementKind::Opening(o) = &e.kind {
                let Some(host) = map.get(&o.host) else { continue };
                let mut o2 = o.clone();
                o2.host = *host;
                c.kind = ElementKind::Opening(o2);
            }
            map.insert(e.id, nid);
            next = next.with_element(c);
            out.created.push(nid);
        }
        for (old_wall, new_wall) in map.clone() {
            let hosted: Vec<Element> = doc
                .openings_in(old_wall)
                .filter(|(oid, _)| !selected.contains(oid))
                .filter_map(|(oid, _)| doc.element(oid).cloned())
                .collect();
            for o in hosted {
                let ElementKind::Opening(op) = &o.kind else { continue };
                let (d, nid) = next.with_new_id();
                next = d;
                let mut op2 = op.clone();
                op2.host = new_wall;
                next = next.with_element(Element {
                    id: nid,
                    storey: Some(self.storey),
                    kind: ElementKind::Opening(op2),
                    ..o.clone()
                });
                out.created.push(nid);
            }
        }
        Ok((next, out))
    }
    fn label(&self, locale: Locale) -> String {
        counted(locale, "copy_to_storey", self.ids.len(), "element")
    }
    fn describe(&self, doc: &Document, locale: Locale) -> String {
        let label = counted(locale, "copy_to_storey", self.ids.len(), noun_key(Some(doc), &self.ids));
        match doc.storey(self.storey) {
            Some(s) => format!("{label} «{}»", s.name),
            None => label,
        }
    }
}

/// `wall.split` — splits a straight wall in two at the point closest to `at`. Openings stay with the part that
/// contains their centre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SplitWall {
    /// Wall.
    pub id: Id,
    /// Split point (projected onto the wall's reference line).
    pub at: Point2,
}

impl Command for SplitWall {
    fn name(&self) -> &'static str {
        "wall.split"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let e = doc.element(self.id).ok_or(CommandError::NotFound(self.id))?;
        let w = e
            .as_wall()
            .ok_or(CommandError::InvalidArgument { field: "id", reason: "is not a wall".into() })?;
        if w.bulge.abs() > 1e-9 {
            return Err(CommandError::InvalidArgument {
                field: "id",
                reason: "curved walls cannot be split yet".into(),
            });
        }
        let seg = w.segment();
        let t = seg.project_param(self.at);
        let len = w.length().as_m();
        let tol = doc.tolerance().length.as_m();
        if t * len <= tol || (1.0 - t) * len <= tol {
            return Err(CommandError::InvalidArgument {
                field: "at",
                reason: "must be inside the wall".into(),
            });
        }
        let split = seg.at(t);
        let mut first = e.clone();
        let mut w1 = w.clone();
        w1.end = split;
        first.kind = ElementKind::Wall(w1);
        let (next, nid) = doc.with_new_id();
        let mut w2 = w.clone();
        w2.start = split;
        let second = Element { id: nid, kind: ElementKind::Wall(w2), ..e.clone() };
        let mut next = next.with_element(first).with_element(second);
        let cut = Length::m(t * len);
        let hosted: Vec<Element> =
            doc.openings_in(self.id).filter_map(|(oid, _)| doc.element(oid).cloned()).collect();
        for mut o in hosted {
            if let ElementKind::Opening(op) = &mut o.kind
                && op.offset > cut
            {
                op.host = nid;
                op.offset -= cut;
                next = next.with_element(o);
            }
        }
        Ok((next, Outcome { created: vec![nid], changed: vec![self.id], ..Outcome::default() }))
    }
}

/// `space.detect` — creates a space from the walls enclosing a point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DetectSpace {
    /// Storey.
    pub storey: Id,
    /// A point inside the room.
    pub point: Point2,
    /// Name.
    pub name: String,
    /// Number.
    #[serde(default)]
    pub number: String,
    /// Category.
    #[serde(default)]
    pub category: SpaceCategory,
}

impl Command for DetectSpace {
    fn name(&self) -> &'static str {
        "space.detect"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        doc.storey(self.storey).ok_or(CommandError::NotFound(self.storey))?;
        let boundary = crate::geo::detect_space(doc, self.storey, self.point)?;
        CreateElement::on(
            self.storey,
            ElementKind::Space(kanavos_model::Space {
                boundary,
                name: self.name.clone(),
                number: self.number.clone(),
                category: self.category,
                height: None,
                stamp: Some(self.point),
                finish: String::new(),
            }),
        )
        .apply(doc)
    }
}

// ----- Views and sheets ----------------------------------------------------------------------------------------

/// `view.create` — adds a view (section, elevation, 3D, schedule …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateView {
    /// Name.
    pub name: String,
    /// What it shows.
    pub kind: ViewKind,
    /// Scale denominator.
    pub scale: u32,
}

impl Command for CreateView {
    fn name(&self) -> &'static str {
        "view.create"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        non_empty(&self.name, "name")?;
        check_view_kind(doc, &self.kind)?;
        if self.scale == 0 {
            return Err(CommandError::InvalidArgument {
                field: "scale",
                reason: "must be at least 1".into(),
            });
        }
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_view(View {
            id,
            name: self.name.clone(),
            kind: self.kind.clone(),
            scale: self.scale,
            hidden_layers: BTreeSet::new(),
        });
        Ok((doc, Outcome { created: vec![id], ..Outcome::default() }))
    }
}

fn check_view_kind(doc: &Document, kind: &ViewKind) -> Result<(), CommandError> {
    match kind {
        ViewKind::Plan { storey } => {
            doc.storey(*storey).ok_or(CommandError::NotFound(*storey))?;
        }
        ViewKind::Section { start, end, .. } | ViewKind::Elevation { start, end, .. }
            if start.coincides(*end, &doc.tolerance()) =>
        {
            return Err(CommandError::InvalidArgument {
                field: "end",
                reason: "coincides with the start".into(),
            });
        }
        _ => {}
    }
    Ok(())
}

/// `view.update` — changes a view. Only the given fields change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateView {
    /// View.
    pub id: Id,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New kind data.
    #[serde(default)]
    pub kind: Option<ViewKind>,
    /// New scale.
    #[serde(default)]
    pub scale: Option<u32>,
    /// New hidden layer prefixes.
    #[serde(default)]
    pub hidden_layers: Option<BTreeSet<String>>,
}

impl Command for UpdateView {
    fn name(&self) -> &'static str {
        "view.update"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let mut v = doc.view(self.id).ok_or(CommandError::NotFound(self.id))?.clone();
        if let Some(n) = &self.name {
            non_empty(n, "name")?;
            v.name = n.clone();
        }
        if let Some(k) = &self.kind {
            check_view_kind(doc, k)?;
            v.kind = k.clone();
        }
        if let Some(s) = self.scale {
            if s == 0 {
                return Err(CommandError::InvalidArgument {
                    field: "scale",
                    reason: "must be at least 1".into(),
                });
            }
            v.scale = s;
        }
        if let Some(h) = &self.hidden_layers {
            v.hidden_layers = h.clone();
        }
        Ok((doc.with_view(v), Outcome { changed: vec![self.id], ..Outcome::default() }))
    }
}

/// `view.delete` — removes a view, its annotation and its viewports. Plan views of existing storeys stay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteView {
    /// View.
    pub id: Id,
}

impl Command for DeleteView {
    fn name(&self) -> &'static str {
        "view.delete"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let v = doc.view(self.id).ok_or(CommandError::NotFound(self.id))?;
        // The last plan view of an existing storey goes with the storey; extra plan views (duplicates) may go.
        if let ViewKind::Plan { storey } = v.kind
            && doc.storey(storey).is_some()
            && doc.views().filter(|o| matches!(o.kind, ViewKind::Plan { storey: s } if s == storey)).count()
                <= 1
        {
            return Err(CommandError::InvalidArgument {
                field: "id",
                reason: "a storey's plan view is deleted with the storey".into(),
            });
        }
        let next = doc.without_view(self.id).ok_or(CommandError::NotFound(self.id))?;
        Ok((next, Outcome { deleted: vec![self.id], ..Outcome::default() }))
    }
}

/// `sheet.create` — adds a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateSheet {
    /// Sheet number.
    pub number: String,
    /// Title.
    pub name: String,
    /// Paper.
    pub paper: Paper,
    /// Viewports.
    #[serde(default)]
    pub viewports: Vec<Viewport>,
}

impl Command for CreateSheet {
    fn name(&self) -> &'static str {
        "sheet.create"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        non_empty(&self.name, "name")?;
        for vp in &self.viewports {
            doc.view(vp.view).ok_or(CommandError::NotFound(vp.view))?;
        }
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_sheet(Sheet {
            id,
            number: self.number.clone(),
            name: self.name.clone(),
            paper: self.paper,
            viewports: self.viewports.clone(),
            title_block: true,
        });
        Ok((doc, Outcome { created: vec![id], ..Outcome::default() }))
    }
}

/// `sheet.update` — replaces a sheet (number, name, paper, viewports, title block).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateSheet {
    /// The sheet, with the id of the one to replace.
    pub sheet: Sheet,
}

impl Command for UpdateSheet {
    fn name(&self) -> &'static str {
        "sheet.update"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        doc.sheet(self.sheet.id).ok_or(CommandError::NotFound(self.sheet.id))?;
        non_empty(&self.sheet.name, "name")?;
        for vp in &self.sheet.viewports {
            doc.view(vp.view).ok_or(CommandError::NotFound(vp.view))?;
        }
        Ok((
            doc.with_sheet(self.sheet.clone()),
            Outcome { changed: vec![self.sheet.id], ..Outcome::default() },
        ))
    }
}

/// `sheet.delete` — removes a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSheet {
    /// Sheet.
    pub id: Id,
}

impl Command for DeleteSheet {
    fn name(&self) -> &'static str {
        "sheet.delete"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let next = doc.without_sheet(self.id).ok_or(CommandError::NotFound(self.id))?;
        Ok((next, Outcome { deleted: vec![self.id], ..Outcome::default() }))
    }
}

/// `sheet.permit_set` — generates the permit drawing set as sheets.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct PermitSet {
    /// Language of generated titles.
    #[serde(default)]
    pub locale: Locale,
}

impl Command for PermitSet {
    fn name(&self) -> &'static str {
        "sheet.permit_set"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let before: BTreeSet<Id> = doc.sheets().iter().map(|s| s.id).collect();
        let next = kanavos_sheets::permit_set(doc, self.locale)
            .map_err(|e| CommandError::Geometry(e.to_string()))?;
        let created = next.sheets().iter().map(|s| s.id).filter(|id| !before.contains(id)).collect();
        Ok((next, Outcome { created, ..Outcome::default() }))
    }
}

// ----- Batch ---------------------------------------------------------------------------------------------------

/// `batch` — runs several commands as one undo step. Fails as a whole if any command fails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Batch {
    /// Label shown in the history, e.g. "Περίγραμμα τοίχων".
    #[serde(default)]
    pub label: String,
    /// Commands in order.
    pub commands: Vec<AnyCommand>,
}

impl Command for Batch {
    fn name(&self) -> &'static str {
        "batch"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let mut current = doc.clone();
        let mut out = Outcome::default();
        for c in &self.commands {
            let (next, o) = c.apply(&current)?;
            current = next;
            out.created.extend(o.created);
            out.changed.extend(o.changed);
            out.deleted.extend(o.deleted);
        }
        Ok((current, out))
    }
    fn label(&self, locale: Locale) -> String {
        if !self.label.is_empty() {
            return self.label.clone();
        }
        match self.commands.as_slice() {
            [one] => one.label(locale),
            many => kanavos_i18n::tf(locale, "engine.label.batch", &[("n", &many.len().to_string())]),
        }
    }
    fn describe(&self, doc: &Document, locale: Locale) -> String {
        match self.commands.as_slice() {
            [one] if self.label.is_empty() => one.describe(doc, locale),
            _ => self.label(locale),
        }
    }
}

/// Why a command could not be read from JSON. `path` points at the failing value, e.g. `args.kind.start`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Path of the failing value (`name`, `args`, `args.kind.start` …).
    pub path: String,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ParseError {}

fn parse_args<T: serde::de::DeserializeOwned>(args: serde_json::Value) -> Result<T, ParseError> {
    use serde_path_to_error::Segment;
    let e = match serde_path_to_error::deserialize::<_, T>(&args) {
        Ok(v) => return Ok(v),
        Err(e) => e,
    };
    let mut path = e.path().to_string();
    let mut message = e.inner().to_string();
    // Element kinds are tagged by a `type` field, which hides the failing field from the path: read the kind's
    // own data again to find it.
    let mut at = Some(&args);
    for seg in e.path().iter() {
        at = match seg {
            Segment::Map { key } => at.and_then(|v| v.get(key)),
            Segment::Seq { index } => at.and_then(|v| v.get(*index)),
            _ => None,
        };
    }
    if let Some((inner, msg)) = at.and_then(refine_kind) {
        if inner != "." {
            path = if path == "." { inner } else { format!("{path}.{inner}") };
        }
        message = msg;
    }
    let path = if path == "." { "args".to_owned() } else { format!("args.{path}") };
    Err(ParseError { path, message })
}

/// The path and message of the error inside an element kind's data, if the value is one.
fn refine_kind(v: &serde_json::Value) -> Option<(String, String)> {
    fn deep<T: serde::de::DeserializeOwned>(v: &serde_json::Value) -> Option<(String, String)> {
        serde_path_to_error::deserialize::<_, T>(v)
            .err()
            .map(|e| (e.path().to_string(), e.inner().to_string()))
    }
    use kanavos_model as m;
    let ty = v.get("type")?.as_str()?;
    let mut obj = v.as_object()?.clone();
    obj.remove("type");
    let inner = serde_json::Value::Object(obj);
    match ty {
        "wall" => deep::<m::Wall>(&inner),
        "slab" => deep::<m::Slab>(&inner),
        "column" => deep::<m::Column>(&inner),
        "beam" => deep::<m::Beam>(&inner),
        "opening" => deep::<m::Opening>(&inner),
        "stair" => deep::<m::Stair>(&inner),
        "roof" => deep::<m::Roof>(&inner),
        "railing" => deep::<m::Railing>(&inner),
        "space" => deep::<m::Space>(&inner),
        "object" => deep::<m::Object>(&inner),
        "dimension" => deep::<m::Dimension>(&inner),
        "text" => deep::<m::TextNote>(&inner),
        "level" => deep::<m::LevelMark>(&inner),
        "drafting" => deep::<m::Drafting>(&inner),
        "grid" => deep::<m::GridLine>(&inner),
        "underlay" => deep::<m::Underlay>(&inner),
        _ => None,
    }
}

macro_rules! any_command {
    ($( $name:literal => $variant:ident ),* $(,)?) => {
        /// Any command, as sent over the control protocol: `{"name": "...", "args": {...}}`.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(tag = "name", content = "args")]
        pub enum AnyCommand {
            $(
                #[doc = concat!("`", $name, "`")]
                #[serde(rename = $name)]
                $variant($variant),
            )*
        }

        impl AnyCommand {
            /// Every command name, in declaration order.
            pub const NAMES: &'static [&'static str] = &[$($name),*];

            /// Reads a command from its name and JSON arguments. Errors carry the path of the failing value.
            /// Missing or `null` arguments read as `{}`, so commands whose arguments all have defaults need none.
            pub fn from_parts(name: &str, args: serde_json::Value) -> Result<AnyCommand, ParseError> {
                let args = if args.is_null() { serde_json::Value::Object(Default::default()) } else { args };
                match name {
                    $($name => parse_args::<$variant>(args).map(AnyCommand::$variant),)*
                    other => Err(ParseError {
                        path: "name".into(),
                        message: format!("unknown command '{other}'"),
                    }),
                }
            }

            /// The JSON schema of one command's arguments, or `None` for an unknown name.
            pub fn args_schema(name: &str) -> Option<serde_json::Value> {
                match name {
                    $($name => serde_json::to_value(schemars::schema_for!($variant)).ok(),)*
                    _ => None,
                }
            }

            /// The command's arguments as JSON (the `args` member of its JSON form).
            pub fn args(&self) -> serde_json::Value {
                match self {
                    $(AnyCommand::$variant(c) => serde_json::to_value(c).unwrap_or(serde_json::Value::Null),)*
                }
            }
        }

        impl Command for AnyCommand {
            fn name(&self) -> &'static str {
                match self {
                    $(AnyCommand::$variant(c) => c.name(),)*
                }
            }
            fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
                match self {
                    $(AnyCommand::$variant(c) => c.apply(doc),)*
                }
            }
            fn label(&self, locale: Locale) -> String {
                match self {
                    $(AnyCommand::$variant(c) => c.label(locale),)*
                }
            }
            fn describe(&self, doc: &Document, locale: Locale) -> String {
                match self {
                    $(AnyCommand::$variant(c) => c.describe(doc, locale),)*
                }
            }
        }

        $(
            impl From<$variant> for AnyCommand {
                fn from(c: $variant) -> Self {
                    AnyCommand::$variant(c)
                }
            }
        )*
    };
}

any_command! {
    "project.set_info" => SetProjectInfo,
    "site.set" => SetSite,
    "site.set_plot" => SetPlot,
    "site.set_terms" => SetBuildingTerms,
    "defaults.set" => SetDefaults,
    "storey.create" => CreateStorey,
    "storey.update" => UpdateStorey,
    "storey.delete" => DeleteStorey,
    "storey.duplicate" => DuplicateStorey,
    "material.create" => CreateMaterial,
    "material.update" => UpdateMaterial,
    "material.delete" => DeleteMaterial,
    "material.duplicate" => DuplicateMaterial,
    "build_up.create" => CreateBuildUp,
    "build_up.update" => UpdateBuildUp,
    "build_up.delete" => DeleteBuildUp,
    "build_up.duplicate" => DuplicateBuildUp,
    "element.create" => CreateElement,
    "element.update" => UpdateElement,
    "element.delete" => DeleteElements,
    "element.transform" => TransformElements,
    "element.copy_to_storey" => CopyToStorey,
    "element.align" => AlignElements,
    "wall.split" => SplitWall,
    "wall.join" => JoinWalls,
    "opening.place" => PlaceOpening,
    "slab.from_walls" => SlabFromWalls,
    "roof.from_walls" => RoofFromWalls,
    "space.detect" => DetectSpace,
    "space.detect_all" => DetectAllSpaces,
    "dimension.auto" => AutoDimensions,
    "view.create" => CreateView,
    "view.update" => UpdateView,
    "view.delete" => DeleteView,
    "view.duplicate" => DuplicateView,
    "sheet.create" => CreateSheet,
    "sheet.update" => UpdateSheet,
    "sheet.delete" => DeleteSheet,
    "sheet.place_view" => PlaceView,
    "sheet.move_viewport" => MoveViewport,
    "sheet.remove_viewport" => RemoveViewport,
    "sheet.permit_set" => PermitSet,
    "batch" => Batch,
}

/// JSON schema of [`AnyCommand`] (all commands with their arguments).
pub fn command_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(AnyCommand)).unwrap_or(serde_json::Value::Null)
}

/// Whether a command name exists.
pub fn is_command(name: &str) -> bool {
    AnyCommand::NAMES.contains(&name)
}

/// The generic label of a command name in a language (as shown in menus and command lists).
pub fn command_label(name: &str, locale: Locale) -> String {
    kanavos_i18n::t(locale, &format!("engine.cmd.{name}")).into_owned()
}

/// One-line description of what a command does, in a language.
pub fn command_help(name: &str, locale: Locale) -> String {
    kanavos_i18n::t(locale, &format!("engine.help.{name}")).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::{ReferenceLine, Structure, Wall, starter};

    fn ground(doc: &Document) -> Id {
        doc.storeys()[0].id
    }

    fn wall(doc: &Document, a: (f64, f64), b: (f64, f64)) -> ElementKind {
        ElementKind::Wall(Wall {
            start: Point2::new(a.0, a.1),
            end: Point2::new(b.0, b.1),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Layered { build_up: doc.defaults().exterior_wall.unwrap() },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        })
    }

    #[test]
    fn rejects_zero_length_walls() {
        let doc = starter();
        let err =
            CreateElement::on(ground(&doc), wall(&doc, (1.0, 1.0), (1.0, 1.0))).apply(&doc).unwrap_err();
        assert!(matches!(err, CommandError::InvalidArgument { field: "end", .. }));
    }

    #[test]
    fn creating_a_storey_adds_its_plan() {
        let (doc, out) = CreateStorey::new("Δώμα", Length::m(6.4), Length::m(3.0)).apply(&starter()).unwrap();
        assert!(doc.plan_view_of(out.created[0]).is_some());
    }

    #[test]
    fn delete_missing_element_is_an_error() {
        assert_eq!(
            DeleteElements { ids: vec![Id(9999)] }.apply(&starter()).unwrap_err(),
            CommandError::NotFound(Id(9999))
        );
    }

    #[test]
    fn array_copy_creates_count_copies() {
        let doc = starter();
        let (doc, out) =
            CreateElement::on(ground(&doc), wall(&doc, (0.0, 0.0), (4.0, 0.0))).apply(&doc).unwrap();
        let w = out.created[0];
        let (doc, out) = TransformElements {
            ids: vec![w],
            transform: TransformSpec::Move { dx: 0.0, dy: 3.0 },
            copy: true,
            count: 3,
        }
        .apply(&doc)
        .unwrap();
        assert_eq!(out.created.len(), 3);
        let last = doc.element(out.created[2]).and_then(Element::as_wall).unwrap();
        assert!(doc.tolerance().equal(last.start.y, Length::m(9.0)));
    }

    #[test]
    fn split_wall_keeps_openings_on_the_right_part() {
        let doc = starter();
        let st = ground(&doc);
        let (doc, out) = CreateElement::on(st, wall(&doc, (0.0, 0.0), (8.0, 0.0))).apply(&doc).unwrap();
        let w = out.created[0];
        let door = ElementKind::Opening(kanavos_model::Opening {
            host: w,
            kind: kanavos_model::OpeningKind::Door,
            style: "door.single_swing".into(),
            offset: Length::m(6.0),
            width: Length::m(0.9),
            height: Length::m(2.1),
            sill: Length::ZERO,
            flip_side: false,
            flip_hand: false,
            params: BTreeMap::new(),
        });
        let (doc, out) = CreateElement::on(st, door).apply(&doc).unwrap();
        let d = out.created[0];
        let (doc, out) = SplitWall { id: w, at: Point2::new(4.0, 0.3) }.apply(&doc).unwrap();
        let o = doc.element(d).and_then(Element::as_opening).unwrap();
        assert_eq!(o.host, out.created[0]);
        assert!(doc.tolerance().equal(o.offset, Length::m(2.0)));
    }

    #[test]
    fn commands_round_trip_through_json() {
        let c: AnyCommand = TransformElements {
            ids: vec![Id(3)],
            transform: TransformSpec::Rotate { centre: Point2::new(1.0, 2.0), degrees: 90.0 },
            copy: false,
            count: 1,
        }
        .into();
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["name"], "element.transform");
        let back: AnyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, c);
        assert!(command_schema().is_object());
    }

    #[test]
    fn batch_is_atomic() {
        let doc = starter();
        let st = ground(&doc);
        let good: AnyCommand = CreateElement::on(st, wall(&doc, (0.0, 0.0), (4.0, 0.0))).into();
        let bad: AnyCommand = CreateElement::on(st, wall(&doc, (1.0, 1.0), (1.0, 1.0))).into();
        let err = Batch { label: String::new(), commands: vec![good, bad] }.apply(&doc);
        assert!(err.is_err());
    }
}

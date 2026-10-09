//! All wall junctions of a storey in one pass.

use std::collections::BTreeMap;

use kanavos_geom::{PolygonWithHoles, union_all};
use kanavos_model::{Document, ElementKind, Id};
use kanavos_units::Length;

use crate::junction::{Junction, WallInfo, resolve};
use crate::wall::{WallLayout, column_cuts, layout_from, wall_plan_parts, wall_solids};
use crate::{ElementSolid, GeometryError, PlanParts, element};

/// The walls of a storey with all their junctions resolved together.
///
/// Computing a storey at once costs about as much as a few single walls: junctions are found through a spatial
/// grid and every wall is laid out once. Views, schedules and exports that need many walls of a storey should use
/// this instead of calling [`crate::wall_layout`] per wall; the results are identical.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreyGeometry {
    storey: Id,
    infos: BTreeMap<Id, WallInfo>,
    layouts: BTreeMap<Id, WallLayout>,
    errors: BTreeMap<Id, GeometryError>,
    junctions: Vec<Junction>,
}

impl StoreyGeometry {
    /// Resolves every wall junction of a storey.
    ///
    /// Failure modes: the storey does not exist ([`GeometryError::NotFound`]). Walls whose own data is invalid
    /// are reported by [`StoreyGeometry::wall`] and left out of the junctions.
    pub fn compute(doc: &Document, storey: Id) -> Result<StoreyGeometry, GeometryError> {
        doc.storey(storey).ok_or(GeometryError::NotFound(storey))?;
        let mut infos = Vec::new();
        let mut errors = BTreeMap::new();
        for e in doc.elements_on(storey) {
            if !matches!(e.kind, ElementKind::Wall(_)) {
                continue;
            }
            match WallInfo::new(doc, e) {
                Ok(i) => infos.push(i),
                Err(err) => {
                    errors.insert(e.id, err);
                }
            }
        }
        let (mut rules, junctions) = resolve(&infos);
        let mut layouts = BTreeMap::new();
        for (w, r) in infos.iter().zip(rules.iter_mut()) {
            r.cuts.extend(column_cuts(doc, w));
            r.canonicalize();
            layouts.insert(w.id, layout_from(w, r));
        }
        let infos = infos.into_iter().map(|i| (i.id, i)).collect();
        Ok(StoreyGeometry { storey, infos, layouts, errors, junctions })
    }

    /// The storey.
    pub fn storey(&self) -> Id {
        self.storey
    }

    /// The layout of a wall of this storey.
    pub fn wall(&self, id: Id) -> Result<&WallLayout, GeometryError> {
        if let Some(l) = self.layouts.get(&id) {
            return Ok(l);
        }
        Err(self.errors.get(&id).cloned().unwrap_or(GeometryError::NotFound(id)))
    }

    /// Every wall layout, in id order.
    pub fn walls(&self) -> impl Iterator<Item = (Id, &WallLayout)> {
        self.layouts.iter().map(|(id, l)| (*id, l))
    }

    /// Every junction.
    pub fn junctions(&self) -> &[Junction] {
        &self.junctions
    }

    /// Plan parts of an element of this storey at an absolute cut elevation, using the resolved walls.
    pub fn plan_parts(&self, doc: &Document, id: Id, cut: Length) -> Result<PlanParts, GeometryError> {
        if let (Some(info), Some(layout)) = (self.infos.get(&id), self.layouts.get(&id)) {
            return Ok(wall_plan_parts(doc, info, layout, cut.as_m()));
        }
        let e = element(doc, id)?;
        if matches!(e.kind, ElementKind::Wall(_)) {
            return Err(self.errors.get(&id).cloned().unwrap_or(GeometryError::NotFound(id)));
        }
        crate::plan_parts(doc, id, cut)
    }

    /// The outer contour of the walls and columns cut at an absolute elevation, all unioned: drawn with the heavy
    /// line, joined walls show no line between them (the layer boundaries inside stay thin).
    pub fn cut_outline(&self, doc: &Document, cut: Length) -> Vec<PolygonWithHoles> {
        let mut regions: Vec<PolygonWithHoles> = Vec::new();
        for (id, info) in &self.infos {
            if let Some(layout) = self.layouts.get(id) {
                regions
                    .extend(wall_plan_parts(doc, info, layout, cut.as_m()).cut.into_iter().map(|c| c.region));
            }
        }
        for e in doc.elements_on(self.storey) {
            if matches!(e.kind, ElementKind::Column(_))
                && let Ok(p) = crate::plan_parts(doc, e.id, cut)
            {
                regions.extend(p.cut.into_iter().map(|c| c.region));
            }
        }
        union_all(&regions).unwrap_or(regions)
    }

    /// 3D solids of an element of this storey, using the resolved walls.
    pub fn solids(&self, doc: &Document, id: Id) -> Result<Vec<ElementSolid>, GeometryError> {
        if let (Some(info), Some(layout)) = (self.infos.get(&id), self.layouts.get(&id)) {
            return wall_solids(doc, info, layout);
        }
        crate::solids(doc, id)
    }
}

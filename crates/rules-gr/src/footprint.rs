//! Storey outlines and the building footprint used for κάλυψη and δόμηση.
//!
//! **Local stand-in.** `kanavos_elements::storey_footprint` will compute storey outlines once polygon booleans
//! land in `kanavos-geom`; until then [`storey_outline`] builds them here from element outlines with the
//! boolean stand-ins in `ops`. Switching is local to this module.
//!
//! Outline of a storey (see `specs/rules-gr/coverage.md`):
//! - walls (full thickness) and enclosed spaces are united and the holes of the union filled, so rooms
//!   without a space element still count; columns add to coverage only (free-standing columns of porches and
//!   pilotis are not floor area, and columns within walls are inside the outline anyway);
//! - walls lower than [`NokRules::low_wall_height`] are fences, parapets or low partitions and are left out;
//! - when [`NokRules::exclude_external_insulation`] is set, the external insulation of every wall — the run of
//!   layers from a face up to and including an insulation layer, with no core layer in that run, provided the
//!   layers outside the insulation are no thicker than [`NokRules::cladding_allowance`] — is cut away from the
//!   result.
//!
//! Tolerance: element outlines come from `kanavos-elements` exactly; the union snaps vertices to a grid far
//! below the project tolerance (see `ops`).

use kanavos_elements::{footprint, wall_layout};
use kanavos_geom::PolygonWithHoles;
use kanavos_model::{Document, ElementKind, Id, MaterialCategory, SpaceCategory};

use crate::ops;
use crate::rules::NokRules;

/// Plan outlines of one storey.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StoreyOutline {
    /// Storey.
    pub storey: Id,
    /// Enclosed floor outline: walls, columns and enclosed spaces with holes filled, external insulation cut
    /// away.
    pub enclosed: Vec<PolygonWithHoles>,
    /// What the storey adds to coverage: the enclosed outline with semi-open spaces and pilotis.
    pub covered: Vec<PolygonWithHoles>,
}

/// Whether a space category is enclosed by walls (part of the storey's floor area).
pub fn is_enclosed(c: SpaceCategory) -> bool {
    !matches!(c, SpaceCategory::SemiOpen | SpaceCategory::Balcony | SpaceCategory::Pilotis)
}

/// Whether a space category adds to coverage (κάλυψη) on an above-ground storey: everything but balconies
/// (ΝΟΚ 12 §3–4 β' [ΚΧΠ 207]).
pub fn adds_to_coverage(c: SpaceCategory) -> bool {
    !matches!(c, SpaceCategory::Balcony)
}

/// Plan regions of a wall's external insulation (with what lies outside it), per the rule in the module
/// documentation. Empty when the wall has none or the rule is off.
fn insulation_bands(doc: &Document, id: Id, rules: &NokRules) -> Vec<PolygonWithHoles> {
    if !rules.exclude_external_insulation {
        return Vec::new();
    }
    let Ok(layout) = wall_layout(doc, id) else { return Vec::new() };
    let layers = &layout.layers;
    let insulation = |m: Id| doc.material(m).is_some_and(|m| m.category == MaterialCategory::Insulation);
    // Layer thicknesses in the same order as the layout's bands.
    let structure = doc.element(id).and_then(|e| e.as_wall()).map(|w| w.structure.clone());
    let widths: Vec<f64> = structure
        .and_then(|s| doc.structure_layers(&s))
        .map(|ls| ls.iter().map(|l| l.thickness.as_m()).collect())
        .unwrap_or_default();
    let thickness = |i: usize| widths.get(i).copied().unwrap_or(0.0);
    let n = layers.len();
    let first_core = layers.iter().position(|l| l.core).unwrap_or(0);
    let last_core = layers.iter().rposition(|l| l.core).unwrap_or(n.saturating_sub(1));
    let mut out = Vec::new();
    // Left face: the last insulation layer before the first core layer.
    if let Some(k) = layers.iter().take(first_core).rposition(|l| insulation(l.material)) {
        let outside: f64 = (0..k).map(thickness).sum();
        if outside <= rules.cladding_allowance + 1e-6 {
            out.extend(layers.iter().take(k + 1).map(|l| PolygonWithHoles::from(l.region.clone())));
        }
    }
    // Right face: the first insulation layer after the last core layer.
    if let Some(k) = (last_core + 1..n).find(|&i| layers.get(i).is_some_and(|l| insulation(l.material))) {
        let outside: f64 = (k + 1..n).map(thickness).sum();
        if outside <= rules.cladding_allowance + 1e-6 {
            out.extend(layers.iter().skip(k).map(|l| PolygonWithHoles::from(l.region.clone())));
        }
    }
    out
}

/// Outlines of a storey (see the module documentation for the rules).
pub fn storey_outline(doc: &Document, storey: Id, rules: &NokRules) -> StoreyOutline {
    let mut solid: Vec<PolygonWithHoles> = Vec::new();
    let mut open: Vec<PolygonWithHoles> = Vec::new();
    let mut bands: Vec<PolygonWithHoles> = Vec::new();
    for e in doc.elements_on(storey) {
        match &e.kind {
            ElementKind::Wall(w) => {
                if w.height.as_m() < rules.low_wall_height {
                    continue;
                }
                if let Ok(l) = wall_layout(doc, e.id) {
                    solid.push(l.outline.into());
                    bands.extend(insulation_bands(doc, e.id, rules));
                }
            }
            ElementKind::Column(_) => {
                // Free-standing columns (porches, pilotis) are covered but not floor area; columns inside the
                // walls are already within the enclosed outline.
                if let Ok(Some(f)) = footprint(doc, e.id) {
                    open.push(f);
                }
            }
            ElementKind::Space(s) => {
                if is_enclosed(s.category) {
                    solid.push(s.boundary.clone());
                } else if adds_to_coverage(s.category) {
                    open.push(s.boundary.clone());
                }
            }
            _ => {}
        }
    }
    let filled = ops::fill_holes(&ops::union(&solid));
    let enclosed = ops::difference(&filled, &bands);
    let mut all = filled;
    all.extend(open);
    let covered = ops::difference(&ops::union(&all), &bands);
    StoreyOutline { storey, enclosed, covered }
}

/// The building footprint for coverage with the given rules: the union of the covered outlines of every
/// above-ground storey (the plan projection of the building), less the light shafts that start on the lowest
/// above-ground storey (ΝΟΚ 12 §4 ε' [ΚΧΠ 207 §4 ε']).
pub fn building_footprint_with(doc: &Document, rules: &NokRules) -> Vec<PolygonWithHoles> {
    let above: Vec<Id> = doc.storeys().iter().filter(|s| !s.below_ground).map(|s| s.id).collect();
    let parts: Vec<PolygonWithHoles> =
        above.iter().flat_map(|s| storey_outline(doc, *s, rules).covered).collect();
    let shafts: Vec<PolygonWithHoles> = above
        .first()
        .map(|s| {
            doc.elements_on(*s)
                .filter_map(|e| e.as_space())
                .filter(|sp| sp.category == SpaceCategory::Shaft)
                .map(|sp| sp.boundary.clone())
                .collect()
        })
        .unwrap_or_default();
    ops::difference(&ops::union(&parts), &shafts)
}

/// The building footprint for coverage with the default rules (see [`building_footprint_with`]).
pub fn building_footprint(doc: &Document) -> Vec<PolygonWithHoles> {
    building_footprint_with(doc, &NokRules::default())
}

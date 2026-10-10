//! Element quantities for schedules, measured on the junction-clean geometry.

use kanavos_geom::Angle;
use kanavos_model::{Document, ElementKind, Id, Profile};
use kanavos_units::{Area, Length, Volume};

use crate::junction::WallInfo;
use crate::wall::{face_areas, openings_of, wall_layout, wall_solids};
use crate::{GeometryError, Quantities, SolidPart, element, solids};

/// Volume of one layer (or part) of an element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerVolume {
    /// Layer index in the build-up, if the solid is a layer.
    pub layer: Option<usize>,
    /// Material.
    pub material: Option<Id>,
    /// Volume.
    pub volume: Volume,
}

/// Area of one roof plane.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneArea {
    /// Outline edges the plane rises from.
    pub edges: Vec<usize>,
    /// Pitch.
    pub pitch: Angle,
    /// Plan (projected) area.
    pub plan_area: Area,
    /// True (sloped) area.
    pub true_area: Area,
}

/// Quantities with their breakdown: volumes per layer or part, roof planes, openings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DetailedQuantities {
    /// The summary.
    pub summary: Quantities,
    /// Volume of each solid (per build-up layer for walls, slabs and roofs; per part for stairs and railings).
    pub layers: Vec<LayerVolume>,
    /// Roof planes.
    pub planes: Vec<PlaneArea>,
    /// Openings hosted by a wall, with their area.
    pub openings: Vec<(Id, Area)>,
}

fn layer_volumes(sol: &[crate::ElementSolid]) -> Vec<LayerVolume> {
    sol.iter()
        .map(|s| LayerVolume {
            layer: match s.part {
                SolidPart::Layer(i) => Some(i),
                _ => None,
            },
            material: s.material,
            volume: Volume::m3(s.mesh.volume().abs()),
        })
        .collect()
}

/// Quantities of an element with their breakdown.
///
/// Walls: length along the reference line, height, thickness, gross and net side area (reference line × height,
/// minus openings), net area of each face after junction clean-up, and the exact volume of each layer (junctions,
/// openings and roof clipping included). Roofs: plan area (`gross_area`), true sloped area (`area`) per plane and
/// in total, volume of each layer. Stairs: walking-line length, rise, plan area of treads and landings, volume.
/// Railings: length along the rail (true length on a slope), height, volume.
pub fn detailed_quantities(doc: &Document, id: Id) -> Result<DetailedQuantities, GeometryError> {
    let e = element(doc, id)?;
    let mut d = DetailedQuantities::default();
    let q = &mut d.summary;
    match &e.kind {
        ElementKind::Wall(w) => {
            let info = WallInfo::new(doc, e)?;
            let layout = wall_layout(doc, id)?;
            let length = w.length();
            let gross = length * w.height;
            let openings = openings_of(doc, &info);
            let open: f64 = openings
                .iter()
                .map(|o| (o.u1 - o.u0) * (o.head.min(info.top) - o.sill.max(info.base)).max(0.0))
                .sum();
            q.length = Some(length);
            q.height = Some(w.height);
            q.thickness = Some(layout.thickness);
            q.gross_area = Some(gross);
            q.area = Some(gross - Area::m2(open));
            let sol = wall_solids(doc, &info, &layout)?;
            let (nl, nr) = face_areas(&info, &sol);
            q.face_area_left = Some(Area::m2(nl));
            q.face_area_right = Some(Area::m2(nr));
            d.layers = layer_volumes(&sol);
            q.volume = Some(d.layers.iter().map(|l| l.volume).sum());
            d.openings =
                openings.iter().map(|o| (o.id, Area::m2((o.u1 - o.u0) * (o.head - o.sill)))).collect();
        }
        ElementKind::Slab(s) => {
            let t = doc.structure_thickness(&s.structure).unwrap_or(Length::ZERO);
            let a = s.outline.area();
            q.area = Some(a);
            q.gross_area = Some(s.outline.outer.area());
            q.thickness = Some(t);
            q.perimeter = Some(s.outline.perimeter());
            q.volume = Some(a * t);
            d.layers = layer_volumes(&solids(doc, id)?);
        }
        ElementKind::Roof(r) => {
            let g = crate::roof::roof_geometry(doc, e)?;
            let plan: f64 = g.planes.iter().map(|p| p.plan_area()).sum();
            let true_area: f64 = g.planes.iter().map(|p| p.true_area()).sum();
            q.area = Some(Area::m2(true_area));
            q.gross_area = Some(Area::m2(plan));
            q.thickness = Some(Length::m(g.thickness));
            // Eaves and verges plus the edges of the holes (where flashings run).
            q.perimeter = Some(r.outline.perimeter() + r.holes.iter().map(|h| h.perimeter()).sum());
            d.planes = g
                .planes
                .iter()
                .map(|p| PlaneArea {
                    edges: p.edges.clone(),
                    pitch: p.pitch,
                    plan_area: Area::m2(p.plan_area()),
                    true_area: Area::m2(p.true_area()),
                })
                .collect();
            d.layers = layer_volumes(&solids(doc, id)?);
            q.volume = Some(d.layers.iter().map(|l| l.volume).sum());
        }
        ElementKind::Column(c) => {
            let a = match c.profile {
                Profile::Rectangle { width, depth } => width * depth,
                Profile::Circle { diameter } => {
                    Area::m2(std::f64::consts::PI * (diameter.as_m() / 2.0).powi(2))
                }
            };
            q.height = Some(c.height);
            q.area = Some(a);
            q.volume = Some(a * c.height);
        }
        ElementKind::Beam(b) => {
            let length = b.start.distance(b.end);
            let a = match b.profile {
                Profile::Rectangle { width, depth } => width * depth,
                Profile::Circle { diameter } => {
                    Area::m2(std::f64::consts::PI * (diameter.as_m() / 2.0).powi(2))
                }
            };
            q.length = Some(length);
            q.area = Some(a);
            q.volume = Some(a * length);
        }
        ElementKind::Space(s) => {
            q.area = Some(s.boundary.area());
            q.gross_area = Some(s.boundary.outer.area());
            q.perimeter = Some(s.boundary.perimeter());
            if let Some(h) = s.height {
                q.height = Some(h);
                q.volume = Some(s.boundary.area() * h);
            }
        }
        ElementKind::Opening(o) => {
            q.area = Some(o.width * o.height);
            q.height = Some(o.height);
            q.length = Some(o.width);
        }
        ElementKind::Railing(r) => {
            // Along the rail: a railing that follows a slope is longer than its plan.
            q.length = Some(crate::railing::railing_length(r));
            q.height = Some(r.height);
            d.layers = layer_volumes(&solids(doc, id)?);
            q.volume = Some(d.layers.iter().map(|l| l.volume).sum());
        }
        ElementKind::Stair(s) => {
            let layout = crate::stair::stair_layout(doc, id)?;
            q.height = Some(s.rise);
            q.length = Some(layout.walking_length());
            q.thickness = Some(s.thickness);
            let treads: f64 =
                layout.flights.iter().flat_map(|f| f.treads.iter()).map(|t| t.area().as_m2()).sum();
            let landings: f64 = layout.landings.iter().map(|l| l.area().as_m2()).sum();
            q.area = Some(Area::m2(treads + landings));
            d.layers = layer_volumes(&solids(doc, id)?);
            q.volume = Some(d.layers.iter().map(|l| l.volume).sum());
        }
        _ => {}
    }
    Ok(d)
}

/// Quantities of an element (see [`detailed_quantities`] for what is measured).
pub fn quantities(doc: &Document, id: Id) -> Result<Quantities, GeometryError> {
    Ok(detailed_quantities(doc, id)?.summary)
}

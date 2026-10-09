//! Validation, plan parts and 3D solids for every element kind.

use kanavos_geom::{Point2, Polygon2, PolygonWithHoles, Segment2, Vec2};
use kanavos_model::{Beam, Column, Document, Element, ElementKind, Id, Profile, RoofShape};
use kanavos_solid::{cylinder, extrude, extrude_polygon};
use kanavos_units::Length;

use crate::junction::WallInfo;
use crate::wall::{wall_layout, wall_plan_parts, wall_solids};
use crate::{CutArea, ElementSolid, GeometryError, PlanParts, SolidPart, element};

/// Absolute elevation of an element's base: its storey's floor plus the element's own base offset (walls,
/// columns, stairs, railings), or the storey floor for other kinds.
pub fn base_elevation(doc: &Document, e: &Element) -> Result<Length, GeometryError> {
    let storey = match e.storey {
        Some(s) => doc
            .storey(s)
            .ok_or_else(|| GeometryError::MissingReference { id: e.id, what: "storey".into() })?,
        None => return Ok(Length::ZERO),
    };
    let offset = match &e.kind {
        ElementKind::Wall(w) => w.base_offset,
        ElementKind::Column(c) => c.base_offset,
        ElementKind::Stair(s) => s.base_offset,
        ElementKind::Railing(r) => r.base_offset,
        ElementKind::Object(o) => o.elevation,
        _ => Length::ZERO,
    };
    Ok(storey.elevation + offset)
}

fn invalid(id: Option<Id>, field: &'static str, reason: impl Into<String>) -> GeometryError {
    GeometryError::Invalid { id, field, reason: reason.into() }
}

/// Checks that an element's parameters describe buildable geometry in this document. Used by every command
/// that creates or changes an element. The element's own id may not exist yet.
pub fn validate(doc: &Document, e: &Element) -> Result<(), GeometryError> {
    let tol = doc.tolerance();
    let id = doc.element(e.id).map(|_| e.id);
    if let Some(s) = e.storey
        && doc.storey(s).is_none()
    {
        return Err(GeometryError::MissingReference { id: e.id, what: format!("storey {s}") });
    }
    let positive = |v: Length, field: &'static str| {
        if v.is_positive(&tol) { Ok(()) } else { Err(invalid(id, field, "must be a positive length")) }
    };
    let structure = |s: &kanavos_model::Structure| -> Result<(), GeometryError> {
        let t = doc
            .structure_thickness(s)
            .ok_or_else(|| GeometryError::MissingReference { id: e.id, what: "build-up".into() })?;
        positive(t, "structure")
    };
    let material = |m: Id| -> Result<(), GeometryError> {
        if doc.material(m).is_some() {
            Ok(())
        } else {
            Err(GeometryError::MissingReference { id: e.id, what: format!("material {m}") })
        }
    };
    let needs_storey = e.category().is_building_part() || matches!(e.kind, ElementKind::Space(_));
    if needs_storey && e.storey.is_none() {
        return Err(invalid(id, "storey", "is required for building parts"));
    }
    match &e.kind {
        ElementKind::Wall(w) => {
            if w.start.coincides(w.end, &tol) {
                return Err(invalid(id, "end", "coincides with the start"));
            }
            if !w.bulge.is_finite() || w.bulge.abs() > 1.0 + 1e-9 {
                return Err(invalid(id, "bulge", "must be between -1 and 1 (at most a half circle)"));
            }
            structure(&w.structure)?;
            positive(w.height, "height")?;
            // An arc must be wider than the wall on its concave side.
            if let (Some(arc), Some(t)) = (w.arc(), doc.structure_thickness(&w.structure)) {
                let (left, right) = w.face_offsets(t);
                let inner = if arc.sweep.as_rad() > 0.0 { left } else { -right };
                if arc.radius.as_m() <= inner + 0.01 {
                    return Err(invalid(id, "bulge", "bends the wall tighter than its thickness"));
                }
            }
        }
        ElementKind::Slab(s) => {
            if !s.outline.outer.is_valid(&tol) {
                return Err(invalid(id, "outline", "needs at least three points enclosing an area"));
            }
            structure(&s.structure)?;
        }
        ElementKind::Column(c) => {
            material(c.material)?;
            positive(c.height, "height")?;
            match c.profile {
                Profile::Rectangle { width, depth } => {
                    positive(width, "width")?;
                    positive(depth, "depth")?;
                }
                Profile::Circle { diameter } => positive(diameter, "diameter")?,
            }
        }
        ElementKind::Beam(b) => {
            material(b.material)?;
            if b.start.coincides(b.end, &tol) {
                return Err(invalid(id, "end", "coincides with the start"));
            }
            match b.profile {
                Profile::Rectangle { width, depth } => {
                    positive(width, "width")?;
                    positive(depth, "depth")?;
                }
                Profile::Circle { diameter } => positive(diameter, "diameter")?,
            }
        }
        ElementKind::Opening(o) => {
            let host = doc
                .element(o.host)
                .ok_or_else(|| GeometryError::MissingReference { id: e.id, what: "host wall".into() })?;
            let ElementKind::Wall(w) = &host.kind else {
                return Err(invalid(id, "host", "must be a wall"));
            };
            if host.storey != e.storey {
                return Err(invalid(id, "storey", "must match the host wall's storey"));
            }
            positive(o.width, "width")?;
            positive(o.height, "height")?;
            let half = o.width.as_m() / 2.0;
            let len = w.length().as_m();
            if o.offset.as_m() - half < -tol.length.as_m() || o.offset.as_m() + half > len + tol.length.as_m()
            {
                return Err(invalid(id, "offset", "places the opening beyond the wall's ends"));
            }
            let wall_top = w.base_offset + w.height;
            if o.sill.as_m() < w.base_offset.as_m() - tol.length.as_m()
                || (o.sill + o.height).as_m() > wall_top.as_m() + tol.length.as_m()
            {
                return Err(invalid(id, "height", "places the opening outside the wall's height"));
            }
        }
        ElementKind::Stair(s) => {
            material(s.material)?;
            positive(s.width, "width")?;
            positive(s.rise, "rise")?;
            positive(s.going, "going")?;
            positive(s.thickness, "thickness")?;
            if s.risers < 2 {
                return Err(invalid(id, "risers", "must be at least 2"));
            }
            if let Some(first) = s.first_flight_risers
                && (first == 0 || first >= s.risers)
            {
                return Err(invalid(id, "first_flight_risers", "must leave risers for both flights"));
            }
        }
        ElementKind::Roof(r) => {
            if !r.outline.is_valid(&tol) {
                return Err(invalid(id, "outline", "needs at least three points enclosing an area"));
            }
            structure(&r.structure)?;
            if let RoofShape::Pitched { slopes } = &r.shape {
                if slopes.len() != r.outline.len() {
                    return Err(invalid(id, "slopes", "needs one entry per outline edge"));
                }
                if slopes.iter().all(Option::is_none) {
                    return Err(invalid(id, "slopes", "needs at least one sloped edge"));
                }
                for a in slopes.iter().flatten() {
                    if !(a.as_deg() > 0.0 && a.as_deg() < 89.0) {
                        return Err(invalid(id, "slopes", "pitches must be between 0° and 89°"));
                    }
                }
                // The planes must resolve (e.g. gables must not enclose a part no plane can reach).
                crate::roof::roof_geometry(doc, e).map_err(|err| match err {
                    GeometryError::Invalid { field, reason, .. } => invalid(id, field, reason),
                    other => other,
                })?;
            }
        }
        ElementKind::Railing(r) => {
            material(r.material)?;
            if r.path.len() < 2 {
                return Err(invalid(id, "path", "needs at least two points"));
            }
            positive(r.height, "height")?;
            positive(r.post_spacing, "post_spacing")?;
        }
        ElementKind::Space(s) => {
            if !s.boundary.outer.is_valid(&tol) {
                return Err(invalid(id, "boundary", "needs at least three points enclosing an area"));
            }
        }
        ElementKind::Dimension(d) => {
            if d.points.len() < 2 {
                return Err(invalid(id, "points", "needs at least two points"));
            }
        }
        ElementKind::Text(t) => {
            if t.content.trim().is_empty() {
                return Err(invalid(id, "content", "must not be empty"));
            }
            if !(t.size_mm > 0.0 && t.size_mm.is_finite()) {
                return Err(invalid(id, "size_mm", "must be positive"));
            }
        }
        ElementKind::Grid(g) => {
            if g.start.coincides(g.end, &tol) {
                return Err(invalid(id, "end", "coincides with the start"));
            }
        }
        ElementKind::Object(_)
        | ElementKind::Level(_)
        | ElementKind::Drafting(_)
        | ElementKind::Underlay(_) => {}
    }
    Ok(())
}

/// Plan outline of a column (profile rotated and placed).
pub(crate) fn column_profile(c: &Column) -> Polygon2 {
    match c.profile {
        Profile::Rectangle { width, depth } => {
            let (hw, hd) = (width.as_m() / 2.0, depth.as_m() / 2.0);
            let u = Vec2::from_angle(c.rotation);
            let v = u.perp();
            let p = c.position;
            Polygon2::new(vec![
                p - u * hw - v * hd,
                p + u * hw - v * hd,
                p + u * hw + v * hd,
                p - u * hw + v * hd,
            ])
        }
        Profile::Circle { diameter } => {
            let r = diameter.as_m() / 2.0;
            let [cx, cy] = c.position.xy();
            Polygon2::new(
                (0..32)
                    .map(|i| {
                        let a = std::f64::consts::TAU * f64::from(i) / 32.0;
                        Point2::new(cx + r * a.cos(), cy + r * a.sin())
                    })
                    .collect(),
            )
        }
    }
}

/// Plan footprint of a beam.
pub(crate) fn beam_footprint(b: &Beam) -> Option<Polygon2> {
    let w = match b.profile {
        Profile::Rectangle { width, .. } => width.as_m(),
        Profile::Circle { diameter } => diameter.as_m(),
    };
    let d = (b.end - b.start).normalized()?;
    let n = d.perp() * (w / 2.0);
    Some(Polygon2::new(vec![b.start - n, b.end - n, b.end + n, b.start + n]))
}

fn beam_depth(b: &Beam) -> f64 {
    match b.profile {
        Profile::Rectangle { depth, .. } => depth.as_m(),
        Profile::Circle { diameter } => diameter.as_m(),
    }
}

fn ring_edges(p: &Polygon2) -> Vec<Segment2> {
    p.edges().collect()
}

fn region_edges(p: &PolygonWithHoles) -> Vec<Segment2> {
    p.rings().flat_map(|r| r.edges().collect::<Vec<_>>()).collect()
}

fn storey_elevation(doc: &Document, e: &Element) -> f64 {
    e.storey.and_then(|s| doc.storey(s)).map(|s| s.elevation.as_m()).unwrap_or(0.0)
}

/// How an element appears in a plan cut at `cut` (absolute elevation).
///
/// Walls use their junction-clean layer bands, with gaps where doors and windows span the cut plane, the wall
/// seen below window sills, and openings above the cut (high windows) in `above`. Roofs give their cut band and
/// their ridge, hip, valley, eave and verge lines below or above the cut. Stairs give treads, landings and the
/// cut section of their flights; railings a thin band or their edge lines.
pub fn plan_parts(doc: &Document, id: Id, cut: Length) -> Result<PlanParts, GeometryError> {
    let e = element(doc, id)?;
    let cut = cut.as_m();
    let mut parts = PlanParts::default();
    let floor = storey_elevation(doc, e);
    match &e.kind {
        ElementKind::Wall(_) => {
            let info = WallInfo::new(doc, e)?;
            let layout = wall_layout(doc, id)?;
            return Ok(wall_plan_parts(doc, &info, &layout, cut));
        }
        ElementKind::Slab(s) => {
            let top = floor + s.top_offset.as_m();
            let t = doc.structure_thickness(&s.structure).map(|t| t.as_m()).unwrap_or(0.0);
            if top - t < cut && cut < top {
                parts.cut.push(CutArea {
                    region: s.outline.clone(),
                    material: doc.structure_core_material(&s.structure),
                });
                parts.outline = vec![s.outline.clone()];
            } else if top <= cut {
                parts.below.extend(region_edges(&s.outline));
            } else {
                parts.above.extend(region_edges(&s.outline));
            }
        }
        ElementKind::Column(c) => {
            let base = floor + c.base_offset.as_m();
            let profile = column_profile(c);
            if base < cut && cut < base + c.height.as_m() {
                parts.outline = vec![profile.clone().into()];
                parts.cut.push(CutArea { region: profile.into(), material: Some(c.material) });
            } else if base + c.height.as_m() <= cut {
                parts.below.extend(ring_edges(&profile));
            } else {
                parts.above.extend(ring_edges(&profile));
            }
        }
        ElementKind::Beam(b) => {
            if let Some(fp) = beam_footprint(b) {
                let top = floor + b.top_offset.as_m();
                let bottom = top - beam_depth(b);
                if bottom < cut && cut < top {
                    parts.outline = vec![fp.clone().into()];
                    parts.cut.push(CutArea { region: fp.into(), material: Some(b.material) });
                } else if top <= cut {
                    parts.below.extend(ring_edges(&fp));
                } else {
                    parts.above.extend(ring_edges(&fp));
                }
            }
        }
        ElementKind::Stair(_) => return crate::stair::stair_plan_parts(doc, e, cut),
        ElementKind::Roof(_) => return crate::roof::roof_plan_parts(doc, e, cut),
        ElementKind::Railing(_) => return crate::railing::railing_plan_parts(doc, e, cut),
        _ => {}
    }
    Ok(parts)
}

fn solid_err(id: Id) -> impl Fn(kanavos_solid::SolidError) -> GeometryError {
    move |err| GeometryError::Invalid { id: Some(id), field: "geometry", reason: err.to_string() }
}

/// The 3D solids of an element in world coordinates (closed meshes). Annotation, spaces and objects have none
/// here (objects come from `kanavos-objects`).
///
/// Walls give one solid per layer with true holes for their openings, clipped to the underside of roofs on their
/// storey; slabs and roofs one per layer (roofs with thickness perpendicular to their planes); stairs one per
/// flight (steps on a sloped waist) and per landing; railings their posts, rails and infill.
pub fn solids(doc: &Document, id: Id) -> Result<Vec<ElementSolid>, GeometryError> {
    let e = element(doc, id)?;
    let floor = storey_elevation(doc, e);
    let err = solid_err(id);
    let mut out = Vec::new();
    match &e.kind {
        ElementKind::Wall(_) => {
            let info = WallInfo::new(doc, e)?;
            let layout = wall_layout(doc, id)?;
            return wall_solids(doc, &info, &layout);
        }
        ElementKind::Slab(s) => {
            let top = floor + s.top_offset.as_m();
            let layers = doc
                .structure_layers(&s.structure)
                .ok_or_else(|| GeometryError::MissingReference { id, what: "build-up".into() })?;
            let mut z = top;
            for (i, l) in layers.iter().enumerate() {
                let next = z - l.thickness.as_m();
                if l.thickness.as_m() > 1e-6 {
                    out.push(ElementSolid {
                        mesh: extrude(&s.outline, next, z).map_err(&err)?,
                        material: Some(l.material),
                        part: SolidPart::Layer(i),
                    });
                }
                z = next;
            }
        }
        ElementKind::Column(c) => {
            let base = floor + c.base_offset.as_m();
            let mesh = match c.profile {
                Profile::Circle { diameter } => {
                    cylinder(c.position, diameter.as_m() / 2.0, base, base + c.height.as_m(), 32)
                        .map_err(&err)?
                }
                Profile::Rectangle { .. } => {
                    extrude_polygon(&column_profile(c), base, base + c.height.as_m()).map_err(&err)?
                }
            };
            out.push(ElementSolid { mesh, material: Some(c.material), part: SolidPart::Body });
        }
        ElementKind::Beam(b) => {
            if let Some(fp) = beam_footprint(b) {
                let top = floor + b.top_offset.as_m();
                out.push(ElementSolid {
                    mesh: extrude_polygon(&fp, top - beam_depth(b), top).map_err(&err)?,
                    material: Some(b.material),
                    part: SolidPart::Body,
                });
            }
        }
        ElementKind::Stair(_) => return crate::stair::stair_solids(doc, e),
        ElementKind::Roof(_) => return crate::roof::roof_solids(doc, e),
        ElementKind::Railing(_) => return crate::railing::railing_solids(doc, e),
        _ => {}
    }
    Ok(out)
}

//! Validation, plan parts and 3D solids for every element kind.

use kanavos_geom::{Point2, Point3, Polygon2, PolygonWithHoles, Segment2, Vec2};
use kanavos_model::{Beam, Column, Document, Element, ElementKind, Id, Profile, RoofShape};
use kanavos_solid::{Mesh, cuboid, cylinder, extrude, extrude_polygon};
use kanavos_units::Length;

use crate::wall::{band_piece, face_segments, solid_intervals, wall_layout};
use crate::{CutArea, ElementSolid, GeometryError, PlanParts, SolidPart, dir, element, stair_layout};

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
pub fn plan_parts(doc: &Document, id: Id, cut: Length) -> Result<PlanParts, GeometryError> {
    let e = element(doc, id)?;
    let cut = cut.as_m();
    let mut parts = PlanParts::default();
    let floor = storey_elevation(doc, e);
    match &e.kind {
        ElementKind::Wall(w) => {
            let layout = wall_layout(doc, id)?;
            if layout.base < cut && cut < layout.top {
                if w.arc().is_some() {
                    for l in &layout.layers {
                        parts
                            .cut
                            .push(CutArea { region: l.region.clone().into(), material: Some(l.material) });
                    }
                } else {
                    let (left, right) = w.face_offsets(layout.thickness);
                    let intervals = solid_intervals(doc, id, w, cut);
                    for (t0, t1) in &intervals {
                        let mut d = left;
                        for l in doc.structure_layers(&w.structure).unwrap_or_default() {
                            let next = d - l.thickness.as_m();
                            parts.cut.push(CutArea {
                                region: band_piece(w, d, next, *t0, *t1).into(),
                                material: Some(l.material),
                            });
                            d = next;
                        }
                    }
                    // Through window openings the wall below the sill is seen from above.
                    for (_, o) in doc.openings_in(id) {
                        let sill = floor + o.sill.as_m();
                        if sill < cut && sill > layout.base + 1e-6 && cut < sill + o.height.as_m() {
                            let c = o.offset.as_m();
                            let h = o.width.as_m() / 2.0;
                            parts.below.extend(face_segments(w, left, right, c - h, c + h));
                        }
                    }
                }
            } else if layout.top <= cut {
                parts.below.extend(ring_edges(&layout.outline));
            } else {
                parts.above.extend(ring_edges(&layout.outline));
            }
        }
        ElementKind::Slab(s) => {
            let top = floor + s.top_offset.as_m();
            let t = doc.structure_thickness(&s.structure).map(|t| t.as_m()).unwrap_or(0.0);
            if top - t < cut && cut < top {
                parts.cut.push(CutArea {
                    region: s.outline.clone(),
                    material: doc.structure_core_material(&s.structure),
                });
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
                    parts.cut.push(CutArea { region: fp.into(), material: Some(b.material) });
                } else if top <= cut {
                    parts.below.extend(ring_edges(&fp));
                } else {
                    parts.above.extend(ring_edges(&fp));
                }
            }
        }
        ElementKind::Stair(_) => {
            let layout = stair_layout(doc, id)?;
            for f in &layout.flights {
                for (i, tread) in f.treads.iter().enumerate() {
                    let z = layout.tread_elevations.get(f.first_riser + i).copied().unwrap_or(floor);
                    let target = if z <= cut { &mut parts.below } else { &mut parts.above };
                    target.extend(ring_edges(tread));
                }
            }
            for l in &layout.landings {
                parts.below.extend(ring_edges(l));
            }
        }
        ElementKind::Roof(r) => {
            let base = floor + r.base.as_m();
            let target = if base <= cut { &mut parts.below } else { &mut parts.above };
            target.extend(ring_edges(&r.outline));
        }
        ElementKind::Railing(r) => {
            for w in r.path.windows(2) {
                if let [a, b] = w {
                    parts.below.push(Segment2::new(*a, *b));
                }
            }
        }
        _ => {}
    }
    Ok(parts)
}

fn solid_err(id: Id) -> impl Fn(kanavos_solid::SolidError) -> GeometryError {
    move |err| GeometryError::Invalid { id: Some(id), field: "geometry", reason: err.to_string() }
}

/// The 3D solids of an element in world coordinates. Annotation and spaces have none.
pub fn solids(doc: &Document, id: Id) -> Result<Vec<ElementSolid>, GeometryError> {
    let e = element(doc, id)?;
    let floor = storey_elevation(doc, e);
    let err = solid_err(id);
    let mut out = Vec::new();
    match &e.kind {
        ElementKind::Wall(_) => {
            let layout = wall_layout(doc, id)?;
            for (i, l) in layout.layers.iter().enumerate() {
                out.push(ElementSolid {
                    mesh: extrude_polygon(&l.region, layout.base, layout.top).map_err(&err)?,
                    material: Some(l.material),
                    part: SolidPart::Layer(i),
                });
            }
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
        ElementKind::Stair(s) => {
            let layout = stair_layout(doc, id)?;
            let base = floor + s.base_offset.as_m();
            for f in &layout.flights {
                for (i, tread) in f.treads.iter().enumerate() {
                    let top = layout.tread_elevations.get(f.first_riser + i).copied().unwrap_or(base);
                    let bottom = (top - layout.riser_height.as_m() - s.thickness.as_m()).max(base);
                    if top - bottom > 1e-6 {
                        out.push(ElementSolid {
                            mesh: extrude_polygon(tread, bottom, top).map_err(&err)?,
                            material: Some(s.material),
                            part: SolidPart::Flight,
                        });
                    }
                }
            }
            for (l, z) in layout.landings.iter().zip(&layout.landing_elevations) {
                out.push(ElementSolid {
                    mesh: extrude_polygon(l, z - s.thickness.as_m(), *z).map_err(&err)?,
                    material: Some(s.material),
                    part: SolidPart::Landing,
                });
            }
        }
        ElementKind::Roof(r) => {
            let top = floor + r.base.as_m();
            let t = doc
                .structure_thickness(&r.structure)
                .ok_or_else(|| GeometryError::MissingReference { id, what: "build-up".into() })?
                .as_m();
            let material = doc.structure_core_material(&r.structure);
            out.push(ElementSolid {
                mesh: extrude_polygon(&r.outline, top - t, top).map_err(&err)?,
                material,
                part: SolidPart::Body,
            });
            if let RoofShape::Flat { parapet: Some(p) } = &r.shape {
                let ring = r.outline.to_ccw();
                for edge in ring.edges() {
                    let d = dir(edge.start, edge.end);
                    let n = d.perp() * p.thickness.as_m();
                    let poly = Polygon2::new(vec![edge.start, edge.end, edge.end + n, edge.start + n]);
                    out.push(ElementSolid {
                        mesh: extrude_polygon(&poly, top, top + p.height.as_m()).map_err(&err)?,
                        material,
                        part: SolidPart::Parapet,
                    });
                }
            }
        }
        ElementKind::Railing(r) => {
            let base = floor + r.base_offset.as_m();
            let top = base + r.height.as_m();
            let post = 0.04;
            let mut mesh = Mesh::new();
            for w in r.path.windows(2) {
                let [a, b] = w else { continue };
                let len = a.distance(*b).as_m();
                let n = ((len / r.post_spacing.as_m().max(0.1)).ceil() as usize).max(1);
                let d = dir(*a, *b);
                for i in 0..=n {
                    let p = *a + d * (len * i as f64 / n as f64);
                    let [x, y] = p.xy();
                    mesh.append(&cuboid(
                        Point3::new(x - post / 2.0, y - post / 2.0, base),
                        Point3::new(x + post / 2.0, y + post / 2.0, top),
                    ));
                }
                let n2 = d.perp() * 0.025;
                let rail = Polygon2::new(vec![*a - n2, *b - n2, *b + n2, *a + n2]);
                mesh.append(&extrude_polygon(&rail, top - 0.05, top).map_err(&err)?);
            }
            out.push(ElementSolid { mesh, material: Some(r.material), part: SolidPart::Rail });
        }
        _ => {}
    }
    Ok(out)
}

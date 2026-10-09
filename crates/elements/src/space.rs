//! Spaces from walls, the storey footprint and building outlines for slabs and roofs.

use kanavos_geom::{JoinStyle, Point2, Polygon2, PolygonWithHoles, Tolerance, offset_polygon, union_all};
use kanavos_model::{Document, ElementKind, Id};
use kanavos_units::Length;

use crate::build::column_profile;
use crate::storey::StoreyGeometry;
use crate::{GeometryError, base_elevation};

/// Which face of the walls an outline follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutlineFace {
    /// The outer face of the walls (the finished building outline).
    Outer,
    /// The outer face of the walls' structural core layers (for slabs bearing on the walls).
    Core,
}

fn geom_err(id: Id, e: kanavos_geom::GeomError) -> GeometryError {
    GeometryError::Invalid { id: Some(id), field: "geometry", reason: e.to_string() }
}

/// Plan regions of the walls and columns that bound spaces on a storey: walls and columns standing at the
/// storey's floor (their base below the cut plane and their top above the floor).
fn obstacles(doc: &Document, storey: Id, geo: &StoreyGeometry) -> Vec<PolygonWithHoles> {
    let Some(st) = doc.storey(storey) else { return Vec::new() };
    let floor = st.elevation.as_m();
    let cut = st.cut_elevation().as_m();
    let mut out: Vec<PolygonWithHoles> = Vec::new();
    for (_, l) in geo.walls() {
        if l.base < cut && l.top > floor + 0.01 {
            out.push(l.outline.clone().into());
        }
    }
    for e in doc.elements_on(storey) {
        if let ElementKind::Column(c) = &e.kind {
            let base = base_elevation(doc, e).map(|b| b.as_m()).unwrap_or(floor);
            if base < cut && base + c.height.as_m() > floor + 0.01 {
                out.push(column_profile(c).into());
            }
        }
    }
    out
}

/// Every closed region bounded by walls and columns on a storey, each with the columns and free-standing walls
/// inside it as holes. Doors and windows do not break rooms: whole wall outlines are used.
pub fn detect_all_spaces(doc: &Document, storey: Id) -> Result<Vec<PolygonWithHoles>, GeometryError> {
    let geo = StoreyGeometry::compute(doc, storey)?;
    let solid = union_all(&obstacles(doc, storey, &geo)).map_err(|e| geom_err(storey, e))?;
    Ok(rooms(&solid))
}

/// The rooms enclosed by a set of disjoint solid regions: each hole of a region, minus the regions inside it.
fn rooms(solid: &[PolygonWithHoles]) -> Vec<PolygonWithHoles> {
    let tol = Tolerance::default();
    let mut out = Vec::new();
    for s in solid {
        for hole in &s.holes {
            let room = hole.to_ccw();
            // Islands: regions inside this hole that are not inside another island's hole.
            let inside: Vec<&PolygonWithHoles> = solid
                .iter()
                .filter(|o| {
                    o.outer.vertices.first().is_some_and(|p| room.contains(*p, &tol)) && !std::ptr::eq(*o, s)
                })
                .filter(|o| o.outer.area().as_m2() < room.area().as_m2())
                .collect();
            let islands: Vec<Polygon2> = inside
                .iter()
                .filter(|o| {
                    !inside.iter().any(|other| {
                        !std::ptr::eq(*other, **o)
                            && other
                                .holes
                                .iter()
                                .any(|h| o.outer.vertices.first().is_some_and(|p| h.contains(*p, &tol)))
                    })
                })
                .map(|o| o.outer.to_cw())
                .collect();
            out.push(PolygonWithHoles { outer: room, holes: islands });
        }
    }
    out
}

/// Finds the region bounded by walls (and columns) around `point` on a storey: the boundary of a new space,
/// following the inner faces of the walls, with columns and free-standing walls inside it as holes.
///
/// Failure modes: the storey does not exist ([`GeometryError::NotFound`]); the point lies inside a wall or column,
/// or the region around it is not closed by walls ([`GeometryError::Invalid`]).
pub fn detect_space(doc: &Document, storey: Id, point: Point2) -> Result<PolygonWithHoles, GeometryError> {
    let geo = StoreyGeometry::compute(doc, storey)?;
    let solid = union_all(&obstacles(doc, storey, &geo)).map_err(|e| geom_err(storey, e))?;
    let tol = Tolerance::default();
    if solid.iter().any(|s| s.contains(point, &tol)) {
        return Err(GeometryError::Invalid {
            id: None,
            field: "point",
            reason: "lies inside a wall or column".into(),
        });
    }
    rooms(&solid)
        .into_iter()
        .filter(|r| r.contains(point, &tol))
        .min_by(|a, b| a.outer.area().as_m2().total_cmp(&b.outer.area().as_m2()))
        .ok_or(GeometryError::Invalid { id: None, field: "point", reason: "is not enclosed by walls".into() })
}

/// The storey's footprint: walls, columns and slabs on the storey, unioned (their outer faces), as used for the
/// building coverage. Rooms enclosed by walls are filled; openings in floor slabs (light wells, courtyards)
/// stay as holes when the storey has slabs.
pub fn storey_footprint(doc: &Document, storey: Id) -> Result<Vec<PolygonWithHoles>, GeometryError> {
    let geo = StoreyGeometry::compute(doc, storey)?;
    let mut parts: Vec<PolygonWithHoles> = geo.walls().map(|(_, l)| l.outline.clone().into()).collect();
    let mut slabs: Vec<PolygonWithHoles> = Vec::new();
    for e in doc.elements_on(storey) {
        match &e.kind {
            ElementKind::Column(c) => parts.push(column_profile(c).into()),
            ElementKind::Slab(s) => slabs.push(s.outline.clone()),
            _ => {}
        }
    }
    if slabs.is_empty() {
        // Fill the rooms.
        let u = union_all(&parts).map_err(|e| geom_err(storey, e))?;
        let filled: Vec<PolygonWithHoles> = u.into_iter().map(|p| p.outer.into()).collect();
        return union_all(&filled).map_err(|e| geom_err(storey, e));
    }
    // Walls without the rooms they enclose, plus the slabs with their openings.
    let u = union_all(&parts).map_err(|e| geom_err(storey, e))?;
    let filled: Vec<PolygonWithHoles> = u.into_iter().map(|p| p.outer.into()).collect();
    // Holes of slabs that are not covered by wall-enclosed area are kept: subtract them after the union.
    let slab_holes: Vec<PolygonWithHoles> =
        slabs.iter().flat_map(|s| s.holes.iter().map(|h| PolygonWithHoles::from(h.to_ccw()))).collect();
    let slab_outers: Vec<PolygonWithHoles> = slabs.iter().map(|s| s.outer.clone().into()).collect();
    let all = kanavos_geom::union(&filled, &slab_outers).map_err(|e| geom_err(storey, e))?;
    // Keep wall material inside slab openings (walls around a light well), remove the open part.
    let walls_only = union_all(&parts).map_err(|e| geom_err(storey, e))?;
    let open = kanavos_geom::difference(&slab_holes, &walls_only).map_err(|e| geom_err(storey, e))?;
    kanavos_geom::difference(&all, &open).map_err(|e| geom_err(storey, e))
}

/// The outline of the building on a storey, traced along the walls' outer faces or the outer face of their
/// cores. Use it to create slabs and roofs (grow it with [`outline_with_overhang`] for eaves). Free-standing walls
/// inside the building do not appear; separate buildings give separate outlines.
pub fn outline_from_walls(
    doc: &Document,
    storey: Id,
    face: OutlineFace,
) -> Result<Vec<Polygon2>, GeometryError> {
    let geo = StoreyGeometry::compute(doc, storey)?;
    let mut parts: Vec<PolygonWithHoles> = Vec::new();
    for (_, l) in geo.walls() {
        match face {
            OutlineFace::Outer => parts.push(l.outline.clone().into()),
            OutlineFace::Core => {
                let core: Vec<PolygonWithHoles> =
                    l.layers.iter().filter(|b| b.core).map(|b| b.region.clone().into()).collect();
                if core.is_empty() {
                    parts.extend(l.layers.iter().map(|b| PolygonWithHoles::from(b.region.clone())));
                } else {
                    parts.extend(core);
                }
            }
        }
    }
    // Columns belong to the structure: they cut the lower-priority wall cores they stand in, so without them the
    // core ring breaks at every column.
    for e in doc.elements_on(storey) {
        if let ElementKind::Column(c) = &e.kind {
            parts.push(column_profile(c).into());
        }
    }
    let u = union_all(&parts).map_err(|e| geom_err(storey, e))?;
    let tol = Tolerance::default();
    let outers: Vec<Polygon2> = u.iter().map(|p| p.outer.clone()).collect();
    // Drop outlines lying inside another one (free-standing walls in rooms).
    let mut out: Vec<Polygon2> = Vec::new();
    for (i, o) in outers.iter().enumerate() {
        let inside = outers.iter().enumerate().any(|(j, other)| {
            i != j
                && other.area().as_m2() > o.area().as_m2()
                && o.vertices.first().is_some_and(|p| other.contains(*p, &tol))
        });
        if !inside {
            out.push(o.to_ccw());
        }
    }
    Ok(out)
}

/// An outline grown by an overhang with mitred corners (slab edges, roof eaves). A negative overhang shrinks it.
pub fn outline_with_overhang(outline: &[Polygon2], overhang: Length) -> Result<Vec<Polygon2>, GeometryError> {
    let mut out = Vec::new();
    for p in outline {
        let grown = offset_polygon(p, overhang.as_m(), JoinStyle::Miter { limit: 4.0 })
            .map_err(|e| GeometryError::Invalid { id: None, field: "overhang", reason: e.to_string() })?;
        out.extend(grown.into_iter().map(|g| g.outer));
    }
    Ok(out)
}

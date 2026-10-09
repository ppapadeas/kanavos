//! Plan geometry derived from walls for commands: room boundaries, building outlines and exterior dimension
//! chains.
//!
//! Rooms and outlines come from `kanavos-elements` ([`detect_space`], [`detect_all_spaces`],
//! [`outline_from_walls`] delegate to its junction-aware implementations, so they follow the same wall faces
//! as plans and 3D). Exterior dimension chains are computed here from the outline and the openings.
//!
//! Tolerances: rooms and outlines smaller than 0.2 m² are ignored. Failure modes: no walls on the storey, no
//! closed room around a point, or a point inside a wall.

use kanavos_geom::{Angle, Containment, Point2, Polygon2, PolygonWithHoles, Vec2};
use kanavos_model::{Dimension, Document, ElementKind, Id};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::CommandError;

/// Ends closer than this are one vertex (metres).
const MERGE: f64 = 1e-3;
/// Smallest area of a room or cell (m²).
const MIN_AREA: f64 = 0.2;

/// Which wall face an outline follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutlineFace {
    /// The outer face of the walls (finish included).
    #[default]
    Outer,
    /// The outer face of the walls' structural core (layers marked `core`).
    Core,
}

fn v(p: Point2) -> Vec2 {
    p.to_vec()
}

fn pt(p: Vec2) -> Point2 {
    Point2::new(p.x, p.y)
}

/// Offsets a closed ring: edge `i` (from vertex `i` to `i + 1`) moves to its left by `dists[i]`. Corners are
/// mitred; collinear edges with different offsets get a step. Returns `None` for degenerate input.
pub(crate) fn offset_ring(ring: &[Vec2], dists: &[f64]) -> Option<Vec<Vec2>> {
    let n = ring.len();
    if n < 3 || dists.len() != n {
        return None;
    }
    let mut out = Vec::with_capacity(n + 4);
    // Where each offset edge starts and ends, to detect edges turned around by a too-large offset.
    let mut starts = vec![Vec2::ZERO; n];
    let mut ends = vec![Vec2::ZERO; n];
    let mut dirs = vec![Vec2::ZERO; n];
    for i in 0..n {
        let j = (i + 1) % n;
        let (p0, p1, p2) = (*ring.get(i)?, *ring.get(j)?, *ring.get((i + 2) % n)?);
        let (d0, d1) = (*dists.get(i)?, *dists.get(j)?);
        let u0 = (p1 - p0).normalized()?;
        let u1 = (p2 - p1).normalized()?;
        let (n0, n1) = (u0.perp(), u1.perp());
        *dirs.get_mut(i)? = u0;
        let a = p0 + n0 * d0;
        let b = p1 + n1 * d1;
        let cross = u0.cross(u1);
        let (end, start) = if cross.abs() < 1e-9 {
            if u0.dot(u1) > 0.0 && (d0 - d1).abs() < 1e-9 {
                // Collinear continuation: no corner.
                let q = p1 + n0 * d0;
                (q, q)
            } else {
                let (e, s) = (p1 + n0 * d0, p1 + n1 * d1);
                out.push(e);
                out.push(s);
                (e, s)
            }
        } else {
            let t = (b - a).cross(u1) / cross;
            let q = a + u0 * t;
            out.push(q);
            (q, q)
        };
        *ends.get_mut(i)? = end;
        *starts.get_mut(j)? = start;
    }
    for ((s, e), u) in starts.iter().zip(&ends).zip(&dirs) {
        if (*e - *s).dot(*u) < -1e-9 {
            return None;
        }
    }
    // Drop repeated points.
    let mut clean: Vec<Vec2> = Vec::with_capacity(out.len());
    for p in out {
        if clean.last().is_none_or(|q: &Vec2| (*q - p).length() > 1e-9) {
            clean.push(p);
        }
    }
    while clean.len() > 1 && clean.first().zip(clean.last()).is_some_and(|(a, b)| (*a - *b).length() <= 1e-9)
    {
        clean.pop();
    }
    (clean.len() >= 3).then_some(clean)
}

/// Offsets a polygon by the same distance everywhere: positive grows it, negative shrinks it (mitred corners).
pub fn offset_polygon(poly: &Polygon2, distance: f64) -> Option<Polygon2> {
    let ccw = poly.to_ccw();
    let ring: Vec<Vec2> = ccw.vertices.iter().map(|p| v(*p)).collect();
    // For a counter-clockwise ring the outside is on the right: grow by moving edges right (negative left).
    let dists = vec![-distance; ring.len()];
    let out = offset_ring(&ring, &dists)?;
    let p = Polygon2::new(out.into_iter().map(pt).collect());
    (p.signed_area() > 0.0).then_some(p)
}

/// A point strictly inside a polygon: the centroid when it is inside, otherwise the middle of the widest span
/// on a few horizontal scan lines. Used for space stamps and "is this room already a space" tests.
pub fn interior_point(poly: &Polygon2) -> Option<Point2> {
    let tol = kanavos_geom::Tolerance::default();
    if let Some(c) = poly.centroid()
        && poly.classify(c, &tol) == Containment::Inside
    {
        return Some(c);
    }
    let bbox = poly.bbox()?;
    let (y0, y1) = (bbox.min.y.as_m(), bbox.max.y.as_m());
    for f in [0.5, 0.25, 0.75, 0.125, 0.375, 0.625, 0.875, 0.0625, 0.9375] {
        let y = y0 + (y1 - y0) * f;
        let mut xs: Vec<f64> = poly
            .edges()
            .filter_map(|e| {
                let ([ax, ay], [bx, by]) = (e.start.xy(), e.end.xy());
                ((ay > y) != (by > y)).then(|| ax + (y - ay) / (by - ay) * (bx - ax))
            })
            .collect();
        xs.sort_by(f64::total_cmp);
        let best = xs
            .chunks_exact(2)
            .filter_map(|c| match c {
                [a, b] => Some((*a, *b)),
                _ => None,
            })
            .max_by(|p, q| (p.1 - p.0).total_cmp(&(q.1 - q.0)));
        if let Some((a, b)) = best {
            let p = Point2::new((a + b) / 2.0, y);
            if poly.classify(p, &tol) == Containment::Inside {
                return Some(p);
            }
        }
    }
    None
}

fn no_walls(storey: Id) -> CommandError {
    CommandError::InvalidArgument { field: "storey", reason: format!("storey {storey} has no closed walls") }
}

/// Maps an element geometry error to a command error, keeping the failing field.
fn geometry_error(e: kanavos_elements::GeometryError) -> CommandError {
    match e {
        kanavos_elements::GeometryError::Invalid { field, reason, .. } => {
            CommandError::InvalidArgument { field, reason }
        }
        kanavos_elements::GeometryError::NotFound(id) => CommandError::NotFound(id),
        other => CommandError::Geometry(other.to_string()),
    }
}

/// Room boundaries (inside faces of the enclosing walls, columns as holes) of every closed cell on a storey,
/// largest first. Delegates to `kanavos_elements::detect_all_spaces`, which follows junction-clean walls.
pub fn detect_all_spaces(doc: &Document, storey: Id) -> Result<Vec<PolygonWithHoles>, CommandError> {
    doc.storey(storey).ok_or(CommandError::NotFound(storey))?;
    let mut rooms = kanavos_elements::detect_all_spaces(doc, storey).map_err(geometry_error)?;
    rooms.retain(|r| r.area().as_m2() > MIN_AREA);
    rooms.sort_by(|a, b| b.area().total_cmp(&a.area()));
    Ok(rooms)
}

/// The room boundary around `point` on a storey. Fails when the point is not inside a closed cell or lies inside
/// a wall.
pub fn detect_space(doc: &Document, storey: Id, point: Point2) -> Result<PolygonWithHoles, CommandError> {
    doc.storey(storey).ok_or(CommandError::NotFound(storey))?;
    kanavos_elements::detect_space(doc, storey, point).map_err(geometry_error)
}

/// The outline of the walls on a storey at their outer (or core) faces, counter-clockwise. With several separate
/// buildings, the largest one.
pub fn outline_from_walls(doc: &Document, storey: Id, face: OutlineFace) -> Result<Polygon2, CommandError> {
    doc.storey(storey).ok_or(CommandError::NotFound(storey))?;
    let face = match face {
        OutlineFace::Outer => kanavos_elements::OutlineFace::Outer,
        OutlineFace::Core => kanavos_elements::OutlineFace::Core,
    };
    kanavos_elements::outline_from_walls(doc, storey, face)
        .map_err(geometry_error)?
        .into_iter()
        .filter(|p| p.area().as_m2() > MIN_AREA)
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .map(|p| p.to_ccw())
        .ok_or_else(|| no_walls(storey))
}

/// Options for [`exterior_dimensions`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimensionOptions {
    /// Distance from the outline to the first chain, metres.
    pub offset: f64,
    /// Distance between chains, metres.
    pub spacing: f64,
    /// Include door and window jambs in the first chain.
    pub openings: bool,
    /// Add a chain with the overall size.
    pub overall: bool,
}

impl Default for DimensionOptions {
    fn default() -> Self {
        DimensionOptions { offset: 1.0, spacing: 0.7, openings: true, overall: true }
    }
}

/// Exterior dimension chains of a storey on its four sides (south, east, north, west): one chain through the
/// outline corners and the door and window jambs facing that side, and one with the overall size.
pub fn exterior_dimensions(
    doc: &Document,
    storey: Id,
    opts: &DimensionOptions,
) -> Result<Vec<Dimension>, CommandError> {
    let outline = outline_from_walls(doc, storey, OutlineFace::Outer)?;
    let bbox = outline.bbox().ok_or_else(|| no_walls(storey))?;
    let tol = doc.tolerance();
    let (x0, y0, x1, y1) = (bbox.min.x.as_m(), bbox.min.y.as_m(), bbox.max.x.as_m(), bbox.max.y.as_m());
    // Jambs on the outer faces, with the outward normal of their wall.
    let mut jambs: Vec<(Vec2, Vec2)> = Vec::new();
    if opts.openings {
        for e in doc.elements_on(storey) {
            let ElementKind::Opening(_) = &e.kind else { continue };
            let Ok(pl) = kanavos_elements::opening_placement(doc, e.id) else { continue };
            let c = v(pl.centre);
            let probe = |d: f64| outline.contains(pt(c + pl.across * d), &tol);
            let (left_in, right_in) = (probe(pl.left_face + 0.05), probe(pl.right_face - 0.05));
            let (normal, face) = match (left_in, right_in) {
                (false, true) => (pl.across, pl.left_face),
                (true, false) => (-pl.across, -pl.right_face),
                _ => continue,
            };
            let half = pl.width.as_m() / 2.0;
            for s in [-half, half] {
                jambs.push((c + pl.along * s + normal * face, normal));
            }
        }
    }
    // Outline edges with their outward normals (the outline is counter-clockwise: outside is on the right).
    let edges: Vec<(Vec2, Vec2, Vec2)> = outline
        .edges()
        .filter_map(|e| {
            let (a, b) = (v(e.start), v(e.end));
            (b - a).normalized().map(|d| (a, b, Vec2::new(d.y, -d.x)))
        })
        .collect();
    // (outward normal, measured axis, line coordinate of the first chain, direction)
    let sides = [
        (Vec2::new(0.0, -1.0), 0usize, y0 - opts.offset, Angle::ZERO),
        (Vec2::new(1.0, 0.0), 1usize, x1 + opts.offset, Angle::RIGHT),
        (Vec2::new(0.0, 1.0), 0usize, y1 + opts.offset, Angle::ZERO),
        (Vec2::new(-1.0, 0.0), 1usize, x0 - opts.offset, Angle::RIGHT),
    ];
    let coord = |p: Vec2, axis: usize| if axis == 0 { p.x } else { p.y };
    let mut out = Vec::new();
    for (normal, axis, line, direction) in sides {
        // Corners seen from this side: the ends of the edges that face it.
        let mut pts: Vec<Vec2> =
            edges.iter().filter(|(_, _, n)| n.dot(normal) > 0.7).flat_map(|(a, b, _)| [*a, *b]).collect();
        pts.extend(jambs.iter().filter(|(_, n)| n.dot(normal) > 0.7).map(|(p, _)| *p));
        // One point per measured position: the one nearest this side.
        pts.sort_by(|a, b| {
            coord(*a, axis).total_cmp(&coord(*b, axis)).then(b.dot(normal).total_cmp(&a.dot(normal)))
        });
        let mut chain: Vec<Vec2> = Vec::new();
        for p in pts {
            if chain.last().is_none_or(|q| (coord(p, axis) - coord(*q, axis)).abs() > MERGE) {
                chain.push(p);
            }
        }
        let place = |d: f64| if axis == 0 { Point2::new(x0, d) } else { Point2::new(d, y0) };
        let sign = normal.x + normal.y; // +1 for north/east, -1 for south/west
        if chain.len() >= 2 {
            out.push(Dimension {
                points: chain.iter().map(|p| pt(*p)).collect(),
                line_point: place(line),
                direction,
            });
        }
        if opts.overall && chain.len() > 2 {
            let (Some(first), Some(last)) = (chain.first(), chain.last()) else { continue };
            out.push(Dimension {
                points: vec![pt(*first), pt(*last)],
                line_point: place(line + sign * opts.spacing),
                direction,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::commands::CreateElement;
    use kanavos_model::{ReferenceLine, Structure, Wall, starter};
    use kanavos_units::Length;

    fn wall(
        doc: &Document,
        a: (f64, f64),
        b: (f64, f64),
        exterior: bool,
        reference: ReferenceLine,
    ) -> ElementKind {
        let d = doc.defaults();
        let bu = if exterior { d.exterior_wall } else { d.interior_wall };
        ElementKind::Wall(Wall {
            start: Point2::new(a.0, a.1),
            end: Point2::new(b.0, b.1),
            bulge: 0.0,
            reference,
            structure: Structure::Layered { build_up: bu.unwrap_or_default() },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        })
    }

    /// A 10 × 8 box with outer faces on the reference lines (drawn clockwise, exterior layers outside) and one
    /// partition at x = 4 drawn to the inner faces.
    fn boxed() -> (Session, Id) {
        let mut s = Session::new(starter());
        let st = s.document().storeys()[0].id;
        let ring = [(0.0, 0.0), (0.0, 8.0), (10.0, 8.0), (10.0, 0.0)];
        for i in 0..4 {
            let (a, b) = (ring[i], ring[(i + 1) % 4]);
            let k = wall(s.document(), a, b, true, ReferenceLine::Left);
            s.run(CreateElement::on(st, k)).unwrap();
        }
        let k = wall(s.document(), (4.0, 0.31), (4.0, 7.69), false, ReferenceLine::Centre);
        s.run(CreateElement::on(st, k)).unwrap();
        (s, st)
    }

    #[test]
    fn finds_both_rooms_at_inner_faces() {
        let (s, st) = boxed();
        let rooms = detect_all_spaces(s.document(), st).unwrap();
        assert_eq!(rooms.len(), 2);
        // Large room: x 4.065..9.69, y 0.31..7.69.
        let big = rooms[0].area().as_m2();
        assert!((big - (9.69 - 4.065) * 7.38).abs() < 1e-6, "{big}");
        let small = rooms[1].area().as_m2();
        assert!((small - (3.935 - 0.31) * 7.38).abs() < 1e-6, "{small}");
    }

    #[test]
    fn detects_the_room_around_a_point() {
        let (s, st) = boxed();
        let room = detect_space(s.document(), st, Point2::new(1.0, 1.0)).unwrap();
        assert!((room.area().as_m2() - (3.935 - 0.31) * 7.38).abs() < 1e-6);
        let err = detect_space(s.document(), st, Point2::new(4.0, 4.0)).unwrap_err();
        assert!(matches!(err, CommandError::InvalidArgument { field: "point", .. }));
        assert!(detect_space(s.document(), st, Point2::new(20.0, 4.0)).is_err());
    }

    #[test]
    fn outlines_follow_outer_and_core_faces() {
        let (s, st) = boxed();
        let outer = outline_from_walls(s.document(), st, OutlineFace::Outer).unwrap();
        assert!((outer.area().as_m2() - 80.0).abs() < 1e-6);
        let core = outline_from_walls(s.document(), st, OutlineFace::Core).unwrap();
        // Core starts behind 1 cm plaster and 8 cm insulation.
        assert!((core.area().as_m2() - (10.0 - 0.18) * (8.0 - 0.18)).abs() < 1e-6);
        assert!(core.is_ccw());
    }

    #[test]
    fn offsets_grow_and_shrink() {
        let sq = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        let grown = offset_polygon(&sq, 0.5).unwrap();
        assert!((grown.area().as_m2() - 25.0).abs() < 1e-9);
        let shrunk = offset_polygon(&sq.reversed(), -1.0).unwrap();
        assert!((shrunk.area().as_m2() - 4.0).abs() < 1e-9);
        assert!(offset_polygon(&sq, -3.0).is_none());
    }

    #[test]
    fn dimension_chains_cover_four_sides() {
        let (s, st) = boxed();
        let dims = exterior_dimensions(s.document(), st, &DimensionOptions::default()).unwrap();
        assert_eq!(dims.len(), 4); // corners only: no overall chain needed
        let south = &dims[0];
        assert_eq!(south.points.len(), 2);
        assert!((south.line_point.y.as_m() + 1.0).abs() < 1e-9);
    }

    #[test]
    fn empty_storey_has_no_outline() {
        let doc = starter();
        let st = doc.storeys()[0].id;
        assert!(outline_from_walls(&doc, st, OutlineFace::Outer).is_err());
        assert!(detect_all_spaces(&doc, st).unwrap().is_empty());
    }
}

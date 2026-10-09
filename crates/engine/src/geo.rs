//! Plan geometry derived from walls: room boundaries, building outlines and exterior dimension chains.
//!
//! **Adapter.** Commands get this geometry only through the functions in this module. They are simple local
//! implementations until `kanavos-elements` and `kanavos-views` provide their own (`detect_all_spaces`,
//! `outline_from_walls`, `storey_footprint`, `auto_dimensions`); switching is a change to the bodies of
//! [`detect_space`], [`detect_all_spaces`], [`outline_from_walls`] and [`exterior_dimensions`] only.
//!
//! Method (written for this project from textbook planar-graph techniques):
//! 1. The reference lines of the walls on a storey form a planar graph. A wall end that stops inside another
//!    wall's thickness (a partition drawn to the inner face of an exterior wall) is extended onto that wall's
//!    reference line, as a junction would join them.
//! 2. Segments are split where they cross or touch, ends within 1 mm are merged, and dangling branches are
//!    pruned, so only closed cells remain.
//! 3. Cells are traced with the face on the left of every half-edge: bounded cells run counter-clockwise, the
//!    outside of each building runs clockwise.
//! 4. A cell offset inwards by the distance from each wall's reference line to its face on the cell side is a
//!    room boundary. The outside ring offset outwards to the outer (or core) faces is the building outline.
//!
//! Tolerances: points closer than 1 mm are the same graph vertex; cells and rooms smaller than 0.2 m² are
//! ignored. Failure modes: no walls on the storey, no closed cell, or a point inside a wall. Free-standing
//! columns inside rooms are not subtracted, and curved walls are followed by chords of at most 2 mm deviation.

use std::collections::{BTreeMap, BTreeSet};

use kanavos_geom::{Angle, Containment, Point2, Polygon2, PolygonWithHoles, Vec2};
use kanavos_model::{Dimension, Document, ElementKind, Id, Wall};
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

/// Offsets of two faces from a reference line, positive to the left.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sides {
    left: f64,
    right: f64,
}

/// A straight piece of a wall's reference line.
#[derive(Debug, Clone, Copy)]
struct Piece {
    a: Vec2,
    b: Vec2,
    faces: Sides,
    core: Sides,
}

impl Piece {
    fn thickness(&self) -> f64 {
        self.faces.left - self.faces.right
    }
}

fn v(p: Point2) -> Vec2 {
    p.to_vec()
}

fn pt(p: Vec2) -> Point2 {
    Point2::new(p.x, p.y)
}

fn sides_of(doc: &Document, w: &Wall) -> Option<(Sides, Sides)> {
    let layers = doc.structure_layers(&w.structure)?;
    let thickness: f64 = layers.iter().map(|l| l.thickness.as_m()).sum();
    let (left, right) = w.face_offsets(kanavos_units::Length::m(thickness));
    let mut d = left;
    let mut core: Option<Sides> = None;
    for l in &layers {
        let next = d - l.thickness.as_m();
        if l.core {
            core = Some(match core {
                None => Sides { left: d, right: next },
                Some(c) => Sides { left: c.left, right: next },
            });
        }
        d = next;
    }
    let faces = Sides { left, right };
    Some((faces, core.unwrap_or(faces)))
}

/// Straight pieces of every wall on a storey (curved walls as chords).
fn pieces(doc: &Document, storey: Id) -> Vec<Piece> {
    let mut out = Vec::new();
    for (_, w) in doc.walls_on(storey) {
        let Some((faces, core)) = sides_of(doc, w) else { continue };
        let points: Vec<Vec2> = match w.arc() {
            Some(arc) => arc.tessellate(0.002, 8).into_iter().map(v).collect(),
            None => vec![v(w.start), v(w.end)],
        };
        for pair in points.windows(2) {
            if let [a, b] = pair
                && (*b - *a).length() > MERGE
            {
                out.push(Piece { a: *a, b: *b, faces, core });
            }
        }
    }
    out
}

/// Extends wall ends that stop inside another wall onto that wall's reference line.
fn extend_into_bands(pieces: &mut [Piece]) {
    let snapshot: Vec<Piece> = pieces.to_vec();
    for (i, p) in pieces.iter_mut().enumerate() {
        for end in [false, true] {
            let q = if end { p.b } else { p.a };
            let on_a_line =
                snapshot.iter().enumerate().any(|(j, o)| j != i && distance_to_segment(q, o) < MERGE);
            if on_a_line {
                continue;
            }
            let mut best: Option<(f64, Vec2)> = None;
            for (j, o) in snapshot.iter().enumerate() {
                if j == i {
                    continue;
                }
                let d = o.b - o.a;
                let len2 = d.length_squared();
                if len2 <= 0.0 {
                    continue;
                }
                let t = (q - o.a).dot(d) / len2;
                let len = len2.sqrt();
                if t < -MERGE / len || t > 1.0 + MERGE / len {
                    continue;
                }
                let s = d.cross(q - o.a) / len;
                if s <= o.faces.left + MERGE && s >= o.faces.right - MERGE {
                    let proj = o.a + d * t.clamp(0.0, 1.0);
                    if best.is_none_or(|(bd, _)| s.abs() < bd) {
                        best = Some((s.abs(), proj));
                    }
                }
            }
            if let Some((_, proj)) = best {
                // Only extend along the piece's own direction, never fold it back.
                let dir = p.b - p.a;
                let moved = if end { proj - p.a } else { p.b - proj };
                if moved.dot(dir) > 0.0 {
                    if end {
                        p.b = proj;
                    } else {
                        p.a = proj;
                    }
                }
            }
        }
    }
}

fn distance_to_segment(q: Vec2, p: &Piece) -> f64 {
    let d = p.b - p.a;
    let len2 = d.length_squared();
    if len2 <= 0.0 {
        return (q - p.a).length();
    }
    let t = ((q - p.a).dot(d) / len2).clamp(0.0, 1.0);
    (q - (p.a + d * t)).length()
}

/// An undirected graph edge: the piece it came from and whether the piece runs from the lower to the higher
/// vertex index.
#[derive(Debug, Clone, Copy)]
struct EdgeInfo {
    piece: usize,
    forward: bool,
}

struct Graph {
    points: Vec<Vec2>,
    pieces: Vec<Piece>,
    edges: BTreeMap<(usize, usize), EdgeInfo>,
}

fn vertex(points: &mut Vec<Vec2>, p: Vec2) -> usize {
    if let Some(i) = points.iter().position(|q| (*q - p).length() < MERGE) {
        return i;
    }
    points.push(p);
    points.len() - 1
}

impl Graph {
    fn build(mut pieces: Vec<Piece>) -> Graph {
        extend_into_bands(&mut pieces);
        let n = pieces.len();
        let mut splits: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let (Some(p), Some(q)) = (pieces.get(i), pieces.get(j)) else { continue };
                let (dp, dq) = (p.b - p.a, q.b - q.a);
                let (lp, lq) = (dp.length(), dq.length());
                if lp <= 0.0 || lq <= 0.0 {
                    continue;
                }
                let cross = dp.cross(dq);
                if cross.abs() <= 1e-9 * lp * lq {
                    // Parallel: if collinear, split each at the other's ends.
                    if (dp.cross(q.a - p.a) / lp).abs() < MERGE {
                        for (target, other, d, l, idx) in [(p, q, dp, lp, i), (q, p, dq, lq, j)] {
                            for e in [other.a, other.b] {
                                let t = (e - target.a).dot(d) / (l * l);
                                if t > MERGE / l
                                    && t < 1.0 - MERGE / l
                                    && let Some(s) = splits.get_mut(idx)
                                {
                                    s.push(t);
                                }
                            }
                        }
                    }
                    continue;
                }
                let w = q.a - p.a;
                let t = w.cross(dq) / cross;
                let u = w.cross(dp) / cross;
                let (et, eu) = (MERGE / lp, MERGE / lq);
                if t >= -et && t <= 1.0 + et && u >= -eu && u <= 1.0 + eu {
                    if let Some(s) = splits.get_mut(i) {
                        s.push(t.clamp(0.0, 1.0));
                    }
                    if let Some(s) = splits.get_mut(j) {
                        s.push(u.clamp(0.0, 1.0));
                    }
                }
            }
        }
        let mut points = Vec::new();
        let mut edges: BTreeMap<(usize, usize), EdgeInfo> = BTreeMap::new();
        for (idx, (p, ts)) in pieces.iter().zip(splits.iter_mut()).enumerate() {
            ts.sort_by(f64::total_cmp);
            let d = p.b - p.a;
            for pair in ts.windows(2) {
                let [t0, t1] = pair else { continue };
                let a = vertex(&mut points, p.a + d * *t0);
                let b = vertex(&mut points, p.a + d * *t1);
                if a == b {
                    continue;
                }
                let key = (a.min(b), a.max(b));
                let info = EdgeInfo { piece: idx, forward: a < b };
                match edges.get(&key) {
                    Some(old)
                        if pieces.get(old.piece).map(Piece::thickness).unwrap_or(0.0) >= p.thickness() => {}
                    _ => {
                        edges.insert(key, info);
                    }
                }
            }
        }
        let mut g = Graph { points, pieces, edges };
        g.prune();
        g
    }

    /// Removes dangling branches (vertices with one edge), repeatedly.
    fn prune(&mut self) {
        loop {
            let mut degree: BTreeMap<usize, usize> = BTreeMap::new();
            for (a, b) in self.edges.keys() {
                *degree.entry(*a).or_default() += 1;
                *degree.entry(*b).or_default() += 1;
            }
            let before = self.edges.len();
            self.edges.retain(|(a, b), _| {
                degree.get(a).copied().unwrap_or(0) > 1 && degree.get(b).copied().unwrap_or(0) > 1
            });
            if self.edges.len() == before {
                break;
            }
        }
    }

    fn point(&self, i: usize) -> Vec2 {
        self.points.get(i).copied().unwrap_or(Vec2::ZERO)
    }

    /// Distance from the reference line to the wall face (or core face) on the left of the half-edge `a → b`.
    fn left_distance(&self, a: usize, b: usize, core: bool) -> f64 {
        let Some(info) = self.edges.get(&(a.min(b), a.max(b))) else { return 0.0 };
        let Some(p) = self.pieces.get(info.piece) else { return 0.0 };
        let same = (a < b) == info.forward;
        let s = if core { p.core } else { p.faces };
        if same { s.left } else { -s.right }
    }

    /// Closed cycles with the face on the left of every half-edge.
    fn cycles(&self) -> Vec<Vec<usize>> {
        let mut adj: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (a, b) in self.edges.keys() {
            adj.entry(*a).or_default().push(*b);
            adj.entry(*b).or_default().push(*a);
        }
        for (u, list) in adj.iter_mut() {
            let c = self.point(*u);
            list.sort_by(|x, y| {
                let ax = (self.point(*x) - c).angle().as_rad();
                let ay = (self.point(*y) - c).angle().as_rad();
                ax.total_cmp(&ay)
            });
        }
        let mut seen: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut out = Vec::new();
        let limit = self.edges.len() * 2 + 2;
        for (a, b) in self.edges.keys() {
            for start in [(*a, *b), (*b, *a)] {
                if seen.contains(&start) {
                    continue;
                }
                let mut cycle = Vec::new();
                let mut he = start;
                let mut closed = false;
                for _ in 0..limit {
                    if !seen.insert(he) {
                        closed = he == start;
                        break;
                    }
                    cycle.push(he.0);
                    let (u, w) = he;
                    let Some(list) = adj.get(&w) else { break };
                    let Some(i) = list.iter().position(|x| *x == u) else { break };
                    let k = list.len();
                    let Some(next) = list.get((i + k - 1) % k) else { break };
                    he = (w, *next);
                }
                if closed && cycle.len() >= 3 {
                    out.push(cycle);
                }
            }
        }
        out
    }

    fn signed_area(&self, cycle: &[usize]) -> f64 {
        let n = cycle.len();
        let mut s = 0.0;
        for (i, a) in cycle.iter().enumerate() {
            let b = cycle.get((i + 1) % n).copied().unwrap_or(*a);
            let (p, q) = (self.point(*a), self.point(b));
            s += p.x * q.y - q.x * p.y;
        }
        s / 2.0
    }

    /// The cycle offset to the left of each edge by the wall face distance.
    fn offset_cycle(&self, cycle: &[usize], core: bool) -> Option<Polygon2> {
        let n = cycle.len();
        let mut ring = Vec::with_capacity(n);
        let mut dists = Vec::with_capacity(n);
        for (i, a) in cycle.iter().enumerate() {
            let b = *cycle.get((i + 1) % n)?;
            ring.push(self.point(*a));
            dists.push(self.left_distance(*a, b, core));
        }
        offset_ring(&ring, &dists).map(|r| Polygon2::new(r.into_iter().map(pt).collect()))
    }
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

fn clean(poly: Polygon2, doc: &Document) -> Option<Polygon2> {
    let p = poly.simplified(&doc.tolerance());
    (p.signed_area() > MIN_AREA && p.is_valid(&doc.tolerance())).then_some(p)
}

fn no_walls(storey: Id) -> CommandError {
    CommandError::InvalidArgument { field: "storey", reason: format!("storey {storey} has no closed walls") }
}

/// Room boundaries (inside faces of the enclosing walls) of every closed cell on a storey, largest first.
pub fn detect_all_spaces(doc: &Document, storey: Id) -> Result<Vec<PolygonWithHoles>, CommandError> {
    doc.storey(storey).ok_or(CommandError::NotFound(storey))?;
    let g = Graph::build(pieces(doc, storey));
    let mut rooms: Vec<Polygon2> = g
        .cycles()
        .iter()
        .filter(|c| g.signed_area(c) > MIN_AREA)
        .filter_map(|c| g.offset_cycle(c, false))
        .filter_map(|p| clean(p, doc))
        .collect();
    rooms.sort_by(|a, b| b.signed_area().total_cmp(&a.signed_area()));
    Ok(rooms.into_iter().map(PolygonWithHoles::from).collect())
}

/// The room boundary around `point` on a storey. Fails when the point is not inside a closed cell or lies inside
/// a wall.
pub fn detect_space(doc: &Document, storey: Id, point: Point2) -> Result<PolygonWithHoles, CommandError> {
    // Prefer the element rules once they implement detection.
    match kanavos_elements::detect_space(doc, storey, point) {
        Ok(b) => return Ok(b),
        Err(kanavos_elements::GeometryError::Unsupported(_)) => {}
        Err(e) => return Err(CommandError::Geometry(e.to_string())),
    }
    doc.storey(storey).ok_or(CommandError::NotFound(storey))?;
    let tol = doc.tolerance();
    let g = Graph::build(pieces(doc, storey));
    let mut best: Option<(f64, Polygon2)> = None;
    for c in g.cycles() {
        let area = g.signed_area(&c);
        if area <= MIN_AREA {
            continue;
        }
        let cell = Polygon2::new(c.iter().map(|i| pt(g.point(*i))).collect());
        if !cell.contains(point, &tol) {
            continue;
        }
        if best.as_ref().is_none_or(|(a, _)| area < *a)
            && let Some(room) = g.offset_cycle(&c, false).and_then(|p| clean(p, doc))
        {
            best = Some((area, room));
        }
    }
    let Some((_, room)) = best else {
        return Err(CommandError::InvalidArgument {
            field: "point",
            reason: "is not inside a room closed by walls".into(),
        });
    };
    if !room.contains(point, &tol) {
        return Err(CommandError::InvalidArgument { field: "point", reason: "is inside a wall".into() });
    }
    Ok(room.into())
}

/// The outline of the walls on a storey at their outer (or core) faces, counter-clockwise. With several separate
/// buildings, the largest one.
pub fn outline_from_walls(doc: &Document, storey: Id, face: OutlineFace) -> Result<Polygon2, CommandError> {
    doc.storey(storey).ok_or(CommandError::NotFound(storey))?;
    let g = Graph::build(pieces(doc, storey));
    let outer = g
        .cycles()
        .into_iter()
        .filter(|c| g.signed_area(c) < -MIN_AREA)
        .min_by(|a, b| g.signed_area(a).total_cmp(&g.signed_area(b)))
        .ok_or_else(|| no_walls(storey))?;
    let ring = g.offset_cycle(&outer, face == OutlineFace::Core).ok_or_else(|| no_walls(storey))?;
    clean(ring.to_ccw(), doc).ok_or_else(|| no_walls(storey))
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
    let corners: Vec<Vec2> = outline.vertices.iter().map(|p| v(*p)).collect();
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
        let mut pts: Vec<Vec2> = corners.clone();
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
    use kanavos_model::{ReferenceLine, Structure, starter};
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

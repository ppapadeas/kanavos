//! Geometry computed from element parameters for export: wall outlines with corner joins, stair flights and
//! landings, roof solids, parapets.
//!
//! These are deliberately simple, self-contained rules (documented in `specs/files/ifc-export.md`); the full
//! junction and roof rules of the modelling core may differ.

use std::collections::{BTreeMap, HashMap};

use kanavos_geom::{Arc2, Line2, Point2, Point3, Polygon2, Vec2};
use kanavos_model::{Document, Id, Roof, RoofShape, Stair, StairShape};
use kanavos_solid::Mesh;

/// Distance within which wall ends are considered to meet.
const JOIN_TOLERANCE: f64 = 1e-3;
/// Largest distance of a mitre corner from the meeting point, as a multiple of the thicker wall.
const MITRE_LIMIT: f64 = 4.0;
/// Maximum deviation of tessellated arcs from the true arc, metres.
pub const ARC_DEVIATION: f64 = 0.005;

/// Storey and rounded position of a wall end.
type EndKey = (Option<Id>, i64, i64);

/// Plan geometry of a wall.
#[derive(Debug, Clone, PartialEq)]
pub struct WallShape {
    /// Total thickness, metres.
    pub thickness: f64,
    /// Offset of the left face from the reference line (positive to the left), metres.
    pub left: f64,
    /// Offset of the right face from the reference line, metres.
    pub right: f64,
    /// Plan outline in project coordinates, counter-clockwise, including corner joins.
    pub outline: Polygon2,
    /// The reference arc, for curved walls.
    pub arc: Option<Arc2>,
}

impl WallShape {
    /// Offset of the middle of the thickness from the reference line.
    pub fn centre_offset(&self) -> f64 {
        (self.left + self.right) / 2.0
    }
}

struct StraightWall {
    id: Id,
    start: Point2,
    end: Point2,
    dir: Vec2,
    left: f64,
    right: f64,
    thickness: f64,
}

impl StraightWall {
    fn normal(&self) -> Vec2 {
        self.dir.perp()
    }
    fn face_line(&self, left: bool) -> Line2 {
        let o = if left { self.left } else { self.right };
        Line2 { origin: self.start + self.normal() * o, direction: self.dir }
    }
}

/// Computes plan outlines of all walls with a positive length and thickness.
///
/// Two straight walls whose ends meet (within 1 mm), with no third wall at that point and not parallel, are
/// joined with a mitre: both outlines end on the line through the intersections of their matching faces. A mitre
/// whose corner would lie more than four wall thicknesses from the meeting point is not made (very acute angles).
pub fn wall_shapes(doc: &Document) -> BTreeMap<Id, WallShape> {
    let mut straight: Vec<StraightWall> = Vec::new();
    let mut out = BTreeMap::new();
    for e in doc.elements() {
        let Some(w) = e.as_wall() else { continue };
        let Some(t) = doc.structure_thickness(&w.structure).map(|t| t.as_m()) else { continue };
        if !(t > 1e-6 && t.is_finite()) {
            continue;
        }
        let (left, right) = w.face_offsets(kanavos_geom::Length::m(t));
        if let Some(arc) = w.arc() {
            if let Some(outline) = arc_band(&arc, left, right) {
                out.insert(e.id, WallShape { thickness: t, left, right, outline, arc: Some(arc) });
            }
            continue;
        }
        let Some(dir) = w.direction() else { continue };
        if w.start.distance(w.end).as_m() < 1e-6 {
            continue;
        }
        straight.push(StraightWall { id: e.id, start: w.start, end: w.end, dir, left, right, thickness: t });
    }

    // Wall ends by storey and rounded position, to find meeting walls quickly.
    let storey_of: HashMap<Id, Option<Id>> = doc.elements().map(|e| (e.id, e.storey)).collect();
    let key = |s: Option<Id>, p: Point2| -> (Option<Id>, i64, i64) {
        let [x, y] = p.xy();
        (s, (x / JOIN_TOLERANCE).round() as i64, (y / JOIN_TOLERANCE).round() as i64)
    };
    let mut ends: HashMap<EndKey, Vec<(usize, bool)>> = HashMap::new();
    for (i, w) in straight.iter().enumerate() {
        let s = storey_of.get(&w.id).copied().flatten();
        for (p, at_end) in [(w.start, false), (w.end, true)] {
            // Lookups below also search the neighbouring cells, so points near a cell border still meet.
            ends.entry(key(s, p)).or_default().push((i, at_end));
        }
    }

    for (i, w) in straight.iter().enumerate() {
        let s = storey_of.get(&w.id).copied().flatten();
        let mut start_left = w.start + w.normal() * w.left;
        let mut start_right = w.start + w.normal() * w.right;
        let mut end_left = w.end + w.normal() * w.left;
        let mut end_right = w.end + w.normal() * w.right;
        for (p, at_end) in [(w.start, false), (w.end, true)] {
            let mut partners: Vec<(usize, bool)> = Vec::new();
            let [x, y] = p.xy();
            let (cx, cy) = ((x / JOIN_TOLERANCE).round() as i64, (y / JOIN_TOLERANCE).round() as i64);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(list) = ends.get(&(s, cx + dx, cy + dy)) {
                        for &(j, j_end) in list {
                            let Some(o) = straight.get(j) else { continue };
                            let q = if j_end { o.end } else { o.start };
                            if j != i && q.distance(p).as_m() <= JOIN_TOLERANCE {
                                partners.push((j, j_end));
                            }
                        }
                    }
                }
            }
            let [(j, j_end)] = partners.as_slice() else { continue };
            let Some(o) = straight.get(*j) else { continue };
            if let Some((l, r)) = mitre(w, at_end, o, *j_end, p) {
                if at_end {
                    end_left = l;
                    end_right = r;
                } else {
                    start_left = l;
                    start_right = r;
                }
            }
        }
        let quad = Polygon2::new(vec![start_right, end_right, end_left, start_left]);
        let square = Polygon2::new(vec![
            w.start + w.normal() * w.right,
            w.end + w.normal() * w.right,
            w.end + w.normal() * w.left,
            w.start + w.normal() * w.left,
        ]);
        let outline = if is_simple_quad(&quad) && quad.signed_area() > 0.0 { quad } else { square };
        out.insert(
            w.id,
            WallShape {
                thickness: w.thickness,
                left: w.left,
                right: w.right,
                outline: outline.to_ccw(),
                arc: None,
            },
        );
    }
    out
}

/// Mitre corners `(left, right)` for wall `a` at its end (`a_end`) or start, joined to wall `b` at `p`.
fn mitre(
    a: &StraightWall,
    a_end: bool,
    b: &StraightWall,
    b_end: bool,
    p: Point2,
) -> Option<(Point2, Point2)> {
    if a.dir.cross(b.dir).abs() < 1e-6 {
        return None;
    }
    // Walking along a into the corner and on along b, a's left face continues as b's left face when one wall
    // arrives at the corner and the other leaves it; when both arrive (or both leave), sides swap.
    let same = a_end != b_end;
    let left = a.face_line(true).intersection(&b.face_line(same))?;
    let right = a.face_line(false).intersection(&b.face_line(!same))?;
    let limit = MITRE_LIMIT * a.thickness.max(b.thickness);
    if left.distance(p).as_m() > limit || right.distance(p).as_m() > limit {
        return None;
    }
    Some((left, right))
}

fn segments_cross(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let o = |p: Point2, q: Point2, r: Point2| (q - p).cross(r - p);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0) && d1.abs() > 1e-12 && d2.abs() > 1e-12
}

fn is_simple_quad(q: &Polygon2) -> bool {
    let [a, b, c, d] = q.vertices.as_slice() else { return false };
    !segments_cross(*a, *b, *c, *d) && !segments_cross(*b, *c, *d, *a)
}

/// Radius of the face at `offset` (positive to the left of travel) for an arc reference line.
fn offset_radius(arc: &Arc2, offset: f64) -> f64 {
    // Travelling counter-clockwise the centre is on the left, so a left offset shrinks the radius.
    arc.radius.as_m() - arc.sweep.as_rad().signum() * offset
}

/// Points of a concentric arc at `offset` from the reference arc, from start to end.
pub fn offset_arc_points(arc: &Arc2, offset: f64) -> Vec<Point2> {
    let r = offset_radius(arc, offset);
    let a = Arc2 { radius: kanavos_geom::Length::m(r.max(1e-9)), ..*arc };
    a.tessellate(ARC_DEVIATION, 8)
}

fn arc_band(arc: &Arc2, left: f64, right: f64) -> Option<Polygon2> {
    if offset_radius(arc, left) <= 1e-6 || offset_radius(arc, right) <= 1e-6 {
        return None;
    }
    let mut pts = offset_arc_points(arc, right);
    let mut back = offset_arc_points(arc, left);
    back.reverse();
    pts.extend(back);
    let p = Polygon2::new(pts);
    if p.area().as_m2() > 1e-9 { Some(p.to_ccw()) } else { None }
}

// ----- Stairs ------------------------------------------------------------------------------------------------

/// A straight stair flight in the stair's local plan frame (X = walking direction of the first flight).
#[derive(Debug, Clone, PartialEq)]
pub struct Flight {
    /// Midpoint of the first riser's front edge, local plan coordinates.
    pub start: Point2,
    /// Walking direction (unit vector, local plan).
    pub direction: Vec2,
    /// Side profile in (distance along the flight, height) coordinates, counter-clockwise.
    pub profile: Polygon2,
    /// Width across the flight.
    pub width: f64,
    /// Number of risers.
    pub risers: u32,
}

/// A landing in the stair's local plan frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Landing {
    /// Plan outline, local coordinates.
    pub outline: Polygon2,
    /// Height of the top surface above the stair base.
    pub top: f64,
    /// Thickness.
    pub thickness: f64,
}

/// Flights and landings of a stair, or `None` when its parameters are degenerate.
///
/// Each flight of `k` risers has `k − 1` treads: its last riser rises onto the landing or the upper floor.
/// Two-flight shapes split the risers as `first_flight_risers`, or evenly (the first flight gets the smaller half).
/// Quarter turns use a square landing the width of the flight; half turns a landing spanning both flights and the
/// gap between them. The waist is measured perpendicular to the pitch line.
pub fn stair_parts(stair: &Stair) -> Option<(Vec<Flight>, Vec<Landing>)> {
    let n = stair.risers;
    let h = stair.riser_height().as_m();
    let g = stair.going.as_m();
    let w = stair.width.as_m();
    let t = stair.thickness.as_m();
    if n < 2 || !(h > 1e-6 && g > 1e-6 && w > 1e-6 && t > 1e-6) || ![h, g, w, t].iter().all(|v| v.is_finite())
    {
        return None;
    }
    let flight = |start: Point2, direction: Vec2, k: u32, z0: f64| -> Option<Flight> {
        Some(Flight { start, direction, profile: flight_profile(k, h, g, t, z0)?, width: w, risers: k })
    };
    let x = Vec2::X;
    let y = Vec2::Y;
    match stair.shape {
        StairShape::Straight => Some((vec![flight(Point2::ORIGIN, x, n, 0.0)?], vec![])),
        shape => {
            let n1 = stair.first_flight_risers.unwrap_or(n / 2).clamp(1, n - 1);
            let n2 = n - n1;
            let l1 = f64::from(n1.saturating_sub(1)) * g;
            let z_landing = f64::from(n1) * h;
            let gap = stair.gap.as_m().max(0.0);
            let side = match shape {
                StairShape::QuarterTurnLeft | StairShape::HalfTurnLeft => 1.0,
                _ => -1.0,
            };
            let mut flights = Vec::new();
            if n1 >= 2 {
                flights.push(flight(Point2::ORIGIN, x, n1, 0.0)?);
            }
            let (landing, start2, dir2) = match shape {
                StairShape::QuarterTurnLeft | StairShape::QuarterTurnRight => (
                    Polygon2::from_m(&[(l1, -w / 2.0), (l1 + w, -w / 2.0), (l1 + w, w / 2.0), (l1, w / 2.0)]),
                    Point2::new(l1 + w / 2.0, side * w / 2.0),
                    y * side,
                ),
                _ => {
                    let far = side * (w / 2.0 + gap + w);
                    let near = -side * w / 2.0;
                    let (y0, y1) = (near.min(far), near.max(far));
                    (
                        Polygon2::from_m(&[(l1, y0), (l1 + w, y0), (l1 + w, y1), (l1, y1)]),
                        Point2::new(l1, side * (w + gap)),
                        x * -1.0,
                    )
                }
            };
            if n2 >= 2 {
                flights.push(flight(start2, dir2, n2, z_landing)?);
            }
            Some((flights, vec![Landing { outline: landing, top: z_landing, thickness: t }]))
        }
    }
}

/// Side profile of a flight of `k` risers starting at height `z0`: sawtooth on top, soffit parallel to the pitch
/// line below, clipped at `z0`.
fn flight_profile(k: u32, h: f64, g: f64, t: f64, z0: f64) -> Option<Polygon2> {
    if k < 2 {
        return None;
    }
    let m = h / g;
    let drop = t * (1.0 + m * m).sqrt();
    let soffit = |s: f64| z0 + h + m * s - drop;
    let s_end = f64::from(k - 1) * g;
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let s_floor = (drop - h) / m; // where the soffit meets z0
    let front_bottom = if s_floor > 0.0 { (0.0, z0) } else { (0.0, soffit(0.0)) };
    pts.push(front_bottom);
    for i in 1..=k {
        let s = f64::from(i - 1) * g;
        let z = z0 + f64::from(i) * h;
        pts.push((s, z));
        if i < k {
            pts.push((s + g, z));
        }
    }
    let end_bottom = soffit(s_end).max(z0);
    pts.push((s_end, end_bottom));
    if s_floor > 0.0 && s_floor < s_end {
        pts.push((s_floor, z0));
    }
    let poly = Polygon2::from_m(&pts).simplified(&kanavos_geom::Tolerance::default());
    if poly.len() >= 3 && poly.area().as_m2() > 1e-9 { Some(poly.to_ccw()) } else { None }
}

// ----- Polyhedra and roofs -----------------------------------------------------------------------------------

/// A closed polyhedral surface: shared vertices and planar faces listed counter-clockwise seen from outside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Polyhedron {
    /// Vertex positions.
    pub vertices: Vec<Point3>,
    /// Faces as vertex indices.
    pub faces: Vec<Vec<usize>>,
}

impl Polyhedron {
    /// From a triangle mesh (each triangle becomes a face; coincident vertices are merged).
    pub fn from_mesh(mesh: &Mesh) -> Polyhedron {
        let mut b = PolyBuilder::default();
        for tri in mesh.triangle_points() {
            b.face(&tri);
        }
        b.finish()
    }

    /// Enclosed volume (divergence theorem, faces fanned from their first vertex).
    #[cfg(test)]
    pub fn volume(&self) -> f64 {
        let mut v = 0.0;
        for f in &self.faces {
            let pts: Vec<Point3> = f.iter().filter_map(|&i| self.vertices.get(i).copied()).collect();
            let Some(a) = pts.first() else { continue };
            for w in pts.windows(2).skip(1) {
                if let [b, c] = w {
                    v += a.to_vec().dot(b.to_vec().cross(c.to_vec())) / 6.0;
                }
            }
        }
        v
    }
}

/// Builds a polyhedron, merging vertices closer than 1 nm and dropping degenerate faces.
#[derive(Default)]
pub struct PolyBuilder {
    index: HashMap<(i64, i64, i64), usize>,
    poly: Polyhedron,
}

impl PolyBuilder {
    fn vertex(&mut self, p: Point3) -> usize {
        let q = 1e9;
        let key = ((p.x * q).round() as i64, (p.y * q).round() as i64, (p.z * q).round() as i64);
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        self.poly.vertices.push(p);
        let i = self.poly.vertices.len() - 1;
        self.index.insert(key, i);
        i
    }

    /// Adds a face (points counter-clockwise seen from outside).
    pub fn face(&mut self, points: &[Point3]) {
        let mut idx: Vec<usize> = Vec::with_capacity(points.len());
        for &p in points {
            let i = self.vertex(p);
            if idx.last() != Some(&i) {
                idx.push(i);
            }
        }
        while idx.len() > 1 && idx.first() == idx.last() {
            idx.pop();
        }
        if idx.len() < 3 {
            return;
        }
        // Drop faces without area (slivers left by clipping at a vertex).
        let pts: Vec<Point3> = idx.iter().filter_map(|&i| self.poly.vertices.get(i).copied()).collect();
        let mut normal = kanavos_geom::Vec3::ZERO;
        for (k, a) in pts.iter().enumerate() {
            if let Some(b) = pts.get((k + 1) % pts.len()) {
                normal += a.to_vec().cross(b.to_vec());
            }
        }
        if normal.length() > 1e-12 {
            self.poly.faces.push(idx);
        }
    }

    /// The polyhedron.
    pub fn finish(self) -> Polyhedron {
        self.poly
    }
}

/// A plane over the plan, `z = a·p + b`.
#[derive(Debug, Clone, Copy)]
struct HeightPlane {
    a: Vec2,
    b: f64,
}

impl HeightPlane {
    fn at(&self, p: Point2) -> f64 {
        self.a.dot(p.to_vec()) + self.b
    }
}

/// Clips a convex polygon to the half-plane `f(p) ≤ 0` for an affine `f`.
fn clip(poly: &[Point2], f: impl Fn(Point2) -> f64) -> Vec<Point2> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    let n = poly.len();
    for i in 0..n {
        let (Some(&p), Some(&q)) = (poly.get(i), poly.get((i + 1) % n)) else { break };
        let (fp, fq) = (f(p), f(q));
        if fp <= 0.0 {
            out.push(p);
        }
        if (fp < 0.0 && fq > 0.0) || (fp > 0.0 && fq < 0.0) {
            out.push(p.lerp(q, fp / (fp - fq)));
        }
    }
    out
}

/// Regions of the lower envelope of `planes` over a convex polygon: one (possibly empty) convex region each.
fn envelope_regions(outline: &[Point2], planes: &[HeightPlane]) -> Vec<Vec<Point2>> {
    planes
        .iter()
        .enumerate()
        .map(|(i, pi)| {
            let mut region = outline.to_vec();
            for (j, pj) in planes.iter().enumerate() {
                if i == j || region.len() < 3 {
                    continue;
                }
                let (pi, pj) = (*pi, *pj);
                region = clip(&region, |p| pi.at(p) - pj.at(p));
            }
            region
        })
        .collect()
}

fn lower(planes: &[HeightPlane], p: Point2) -> f64 {
    planes.iter().map(|pl| pl.at(p)).fold(f64::INFINITY, f64::min)
}

/// Why a roof could not be built as pitched planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofFallback {
    /// The outline is not convex.
    Concave,
    /// No edge has a usable pitch.
    NoSlopes,
    /// The outline is degenerate.
    Degenerate,
}

/// The solid of a pitched roof: the lower envelope of one plane per sloped outline edge, rising inwards at the
/// edge's pitch from height `base` at the edge, with the given thickness measured perpendicular to each plane.
/// Gable edges (no pitch) become vertical faces. Coordinates are plan coordinates with `z` relative to the
/// storey floor.
///
/// Only convex outlines are supported (for concave outlines the edge planes cross the interior); those return
/// [`RoofFallback::Concave`].
pub fn pitched_roof(roof: &Roof, thickness: f64) -> Result<Polyhedron, RoofFallback> {
    let RoofShape::Pitched { slopes } = &roof.shape else { return Err(RoofFallback::NoSlopes) };
    let tol = kanavos_geom::Tolerance::default();
    let ring = &roof.outline;
    if !ring.is_valid(&tol) || (thickness.is_nan() || thickness <= 0.0) {
        return Err(RoofFallback::Degenerate);
    }
    if !ring.is_convex() {
        return Err(RoofFallback::Concave);
    }
    let ccw = ring.is_ccw();
    let base = roof.base.as_m();
    let mut top = Vec::new();
    let mut bottom = Vec::new();
    for (i, slope) in slopes.iter().enumerate() {
        let (Some(pitch), Some(edge)) = (slope, ring.edge(i)) else { continue };
        let rad = pitch.as_rad();
        if !(rad > 1e-6 && rad < std::f64::consts::FRAC_PI_2 - 1e-6) {
            continue;
        }
        let Some(d) = edge.direction() else { continue };
        let inward = if ccw { d.perp() } else { d.perp() * -1.0 };
        let k = rad.tan();
        let a = inward * k;
        let b = base - a.dot(edge.start.to_vec());
        top.push(HeightPlane { a, b });
        bottom.push(HeightPlane { a, b: b - thickness / rad.cos() });
    }
    if top.is_empty() {
        return Err(RoofFallback::NoSlopes);
    }
    let outline: Vec<Point2> = ring.to_ccw().vertices;
    let lift = |pts: &[Point2], pl: &HeightPlane| -> Vec<Point3> {
        pts.iter().map(|&p| Point3::from_plan(p, pl.at(p))).collect()
    };
    let mut b = PolyBuilder::default();
    let top_regions = envelope_regions(&outline, &top);
    let bottom_regions = envelope_regions(&outline, &bottom);
    for (region, plane) in top_regions.iter().zip(&top) {
        if region.len() >= 3 {
            b.face(&lift(region, plane));
        }
    }
    for (region, plane) in bottom_regions.iter().zip(&bottom) {
        if region.len() >= 3 {
            let mut pts = lift(region, plane);
            pts.reverse();
            b.face(&pts);
        }
    }
    // Side faces: along each outline edge, the bottom chain forwards and the top chain backwards.
    let n = outline.len();
    for i in 0..n {
        let (Some(&p), Some(&q)) = (outline.get(i), outline.get((i + 1) % n)) else { continue };
        let on_edge = |regions: &[Vec<Point2>]| -> Vec<f64> {
            let seg = kanavos_geom::Segment2::new(p, q);
            let mut ts: Vec<f64> = regions
                .iter()
                .flatten()
                .filter(|v| seg.distance_to(**v).as_m() < 1e-7)
                .map(|v| seg.project_param(*v).clamp(0.0, 1.0))
                .collect();
            ts.push(0.0);
            ts.push(1.0);
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            ts
        };
        let bottom_ts = on_edge(&bottom_regions);
        let top_ts = on_edge(&top_regions);
        let mut face: Vec<Point3> = bottom_ts
            .iter()
            .map(|&t| {
                let x = p.lerp(q, t);
                Point3::from_plan(x, lower(&bottom, x))
            })
            .collect();
        face.extend(top_ts.iter().rev().map(|&t| {
            let x = p.lerp(q, t);
            Point3::from_plan(x, lower(&top, x))
        }));
        b.face(&face);
    }
    Ok(b.finish())
}

/// The polygon shrunk by `distance` (inwards), by offsetting every edge and intersecting neighbours. `None` if the
/// result is degenerate or flips.
pub fn inset(poly: &Polygon2, distance: f64) -> Option<Polygon2> {
    let ring = poly.to_ccw().simplified(&kanavos_geom::Tolerance::default());
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let lines: Vec<Line2> = (0..n)
        .filter_map(|i| {
            let e = ring.edge(i)?;
            let d = e.direction()?;
            Some(Line2 { origin: e.start + d.perp() * distance, direction: d })
        })
        .collect();
    if lines.len() != n {
        return None;
    }
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let prev = lines.get((i + n - 1) % n)?;
        let cur = lines.get(i)?;
        pts.push(prev.intersection(cur)?);
    }
    let out = Polygon2::new(pts);
    if out.is_ccw() && out.area().as_m2() > 1e-9 && out.area() < ring.area() { Some(out) } else { None }
}

/// A rectangle polygon centred on a segment, `width` wide, in plan.
pub fn band(a: Point2, b: Point2, width: f64) -> Option<Polygon2> {
    let d = (b - a).normalized()?;
    let n = d.perp() * (width / 2.0);
    Some(Polygon2::new(vec![a - n, b - n, b + n, a + n]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::{Angle, Length};
    use kanavos_model::{Element, ElementKind, ReferenceLine, Storey, Structure, Wall};

    type Segment = ((f64, f64), (f64, f64));

    fn doc_with_walls(walls: &[Segment], thickness: f64) -> (Document, Vec<Id>) {
        let doc = Document::new();
        let (doc, s) = doc.with_new_id();
        let mut doc = doc.with_storey(Storey::new(s, "0", Length::ZERO, Length::m(3.0)));
        let mut ids = Vec::new();
        for (a, b) in walls {
            let (d, id) = doc.with_new_id();
            doc = d.with_element(Element::on_storey(
                id,
                s,
                ElementKind::Wall(Wall {
                    start: Point2::new(a.0, a.1),
                    end: Point2::new(b.0, b.1),
                    bulge: 0.0,
                    reference: ReferenceLine::Centre,
                    structure: Structure::Basic { material: Id(0), thickness: Length::m(thickness) },
                    height: Length::m(3.0),
                    base_offset: Length::ZERO,
                }),
            ));
            ids.push(id);
        }
        (doc, ids)
    }

    #[test]
    fn rectangle_of_walls_gets_mitred_corners() {
        let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
        let walls: Vec<_> = (0..4).map(|i| (corners[i], corners[(i + 1) % 4])).collect();
        let (doc, ids) = doc_with_walls(&walls, 0.2);
        let shapes = wall_shapes(&doc);
        let total: f64 = ids.iter().map(|id| shapes[id].outline.area().as_m2()).sum();
        // Outer 10.2 × 8.2 minus inner 9.8 × 7.8, with no overlap and no gap at the corners.
        let expected = 10.2 * 8.2 - 9.8 * 7.8;
        assert!((total - expected).abs() < 1e-9, "{total} vs {expected}");
        let south = &shapes[&ids[0]].outline;
        let bb = south.bbox().unwrap();
        assert!((bb.min.x.as_m() + 0.1).abs() < 1e-9 && (bb.max.x.as_m() - 10.1).abs() < 1e-9);
    }

    #[test]
    fn walls_meeting_head_on_or_tail_to_tail_also_join() {
        // Both walls start at the corner.
        let (doc, ids) = doc_with_walls(&[((0.0, 0.0), (5.0, 0.0)), ((0.0, 0.0), (0.0, 4.0))], 0.3);
        let shapes = wall_shapes(&doc);
        let total: f64 = ids.iter().map(|id| shapes[id].outline.area().as_m2()).sum();
        // The union of the two walls, an L: no overlap and no notch at the corner.
        let expected = 5.15 * 0.3 + 4.15 * 0.3 - 0.3 * 0.3;
        assert!((total - expected).abs() < 1e-9, "{total} vs {expected}");
        assert!(shapes.values().all(|s| s.outline.signed_area() > 0.0));
    }

    #[test]
    fn t_junction_and_collinear_walls_keep_square_ends() {
        let (doc, ids) = doc_with_walls(
            &[((0.0, 0.0), (5.0, 0.0)), ((5.0, 0.0), (10.0, 0.0)), ((5.0, 0.0), (5.0, 3.0))],
            0.2,
        );
        let shapes = wall_shapes(&doc);
        for id in &ids {
            let s = &shapes[id];
            let len = if *id == ids[2] { 3.0 } else { 5.0 };
            assert!((s.outline.area().as_m2() - len * 0.2).abs() < 1e-9);
        }
    }

    #[test]
    fn arc_walls_are_bands() {
        let arc = Arc2::from_bulge(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0), 1.0).unwrap();
        let band = arc_band(&arc, 0.1, -0.1).unwrap();
        // Half annulus between radii 1.9 and 2.1.
        let expected = std::f64::consts::PI / 2.0 * (2.1 * 2.1 - 1.9 * 1.9);
        assert!((band.area().as_m2() - expected).abs() < 0.01, "{}", band.area().as_m2());
    }

    fn stair(shape: StairShape) -> Stair {
        Stair {
            origin: Point2::ORIGIN,
            direction: Angle::ZERO,
            shape,
            width: Length::m(1.0),
            rise: Length::m(3.0),
            risers: 18,
            going: Length::m(0.28),
            first_flight_risers: None,
            gap: Length::m(0.1),
            thickness: Length::m(0.15),
            material: Id(0),
            usage: Default::default(),
            base_offset: Length::ZERO,
        }
    }

    #[test]
    fn straight_flight_profile() {
        let (flights, landings) = stair_parts(&stair(StairShape::Straight)).unwrap();
        assert_eq!(flights.len(), 1);
        assert!(landings.is_empty());
        let bb = flights[0].profile.bbox().unwrap();
        assert!((bb.max.x.as_m() - 17.0 * 0.28).abs() < 1e-9);
        assert!((bb.max.y.as_m() - 3.0).abs() < 1e-9);
        assert!(bb.min.y.as_m().abs() < 1e-9);
        assert!(flights[0].profile.is_ccw());
    }

    #[test]
    fn half_turn_has_two_flights_and_a_wide_landing() {
        let (flights, landings) = stair_parts(&stair(StairShape::HalfTurnLeft)).unwrap();
        assert_eq!(flights.len(), 2);
        assert_eq!(flights[0].risers + flights[1].risers, 18);
        let l = &landings[0];
        let bb = l.outline.bbox().unwrap();
        assert!((bb.height().as_m() - 2.1).abs() < 1e-9);
        assert!((l.top - 1.5).abs() < 1e-9);
        assert!(flights[1].direction.x < 0.0);
        let (_, right) = stair_parts(&stair(StairShape::HalfTurnRight)).unwrap();
        // Mirrored: the landing reaches from the first flight's left edge to the far edge of the second flight.
        assert!((right[0].outline.bbox().unwrap().min.y.as_m() + 1.6).abs() < 1e-9);
    }

    #[test]
    fn gable_roof_volume() {
        let roof = Roof {
            outline: Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]),
            base: Length::m(3.0),
            structure: Structure::Basic { material: Id(0), thickness: Length::m(0.2) },
            holes: Vec::new(),
            shape: RoofShape::Pitched {
                slopes: vec![Some(Angle::deg(30.0)), None, Some(Angle::deg(30.0)), None],
            },
        };
        let p = pitched_roof(&roof, 0.2).unwrap();
        // Two slabs, each 10 m long, 3 m deep in plan, 0.2 m thick perpendicular to a 30° slope, plus the
        // gable triangles where the bottom surface meets at the ridge.
        let v = p.volume();
        let slab = 10.0 * (3.0 / 30f64.to_radians().cos()) * 0.2;
        assert!(v > 2.0 * slab * 0.95 && v < 2.0 * slab * 1.1, "volume {v}, slabs {}", 2.0 * slab);
        assert!(p.faces.len() >= 6);
        let top = p.vertices.iter().map(|v| v.z).fold(f64::MIN, f64::max);
        assert!((top - (3.0 + 3.0 * 30f64.to_radians().tan())).abs() < 1e-9);
    }

    #[test]
    fn hip_roof_is_closed_and_positive() {
        let roof = Roof {
            outline: Polygon2::from_m(&[(0.0, 0.0), (8.0, 0.0), (8.0, 6.0), (0.0, 6.0)]),
            base: Length::ZERO,
            structure: Structure::Basic { material: Id(0), thickness: Length::m(0.25) },
            holes: Vec::new(),
            shape: RoofShape::Pitched { slopes: vec![Some(Angle::deg(25.0)); 4] },
        };
        let p = pitched_roof(&roof, 0.25).unwrap();
        assert!(p.volume() > 0.0);
        // Every edge is shared by exactly two faces (closed, manifold).
        let mut edges: HashMap<(usize, usize), i32> = HashMap::new();
        for f in &p.faces {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                *edges.entry((a.min(b), a.max(b))).or_default() += if a < b { 1 } else { -1 };
            }
        }
        assert!(edges.values().all(|&c| c == 0), "open or inconsistently oriented edges");
    }

    #[test]
    fn concave_roofs_fall_back() {
        let roof = Roof {
            outline: Polygon2::from_m(&[
                (0.0, 0.0),
                (4.0, 0.0),
                (4.0, 2.0),
                (2.0, 2.0),
                (2.0, 4.0),
                (0.0, 4.0),
            ]),
            base: Length::ZERO,
            structure: Structure::Basic { material: Id(0), thickness: Length::m(0.2) },
            holes: Vec::new(),
            shape: RoofShape::Pitched { slopes: vec![Some(Angle::deg(30.0)); 6] },
        };
        assert_eq!(pitched_roof(&roof, 0.2), Err(RoofFallback::Concave));
    }

    #[test]
    fn inset_shrinks_rectangles() {
        let r = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)]);
        let i = inset(&r, 0.25).unwrap();
        assert!((i.area().as_m2() - 3.5 * 1.5).abs() < 1e-9);
        assert!(inset(&r, 1.5).is_none());
    }
}

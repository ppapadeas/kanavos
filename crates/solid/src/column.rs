//! Vertical prisms with planar tops and horizontal through-holes, built as one closed mesh.
//!
//! A *column solid* is described by plan pieces that tile a region. Each piece carries the vertical intervals
//! where it is solid; interval bounds are planes (`z = z0 + gx·x + gy·y`), so tops can follow a roof slope. A wall
//! layer with a window is, for instance, three pieces along the wall: full height on each side, and below the
//! sill plus above the head in the middle. The builder emits:
//!
//! - the bottom and top faces of every interval,
//! - side faces on the outer boundary of the tiling,
//! - on edges shared by two pieces, only the parts where one side is solid and the other is not (the jambs of
//!   a window, the step where a top changes height).
//!
//! The result is a single closed, consistently oriented mesh with true holes, without Boolean operations.
//! T-junctions between pieces (a vertex of one piece lying on an edge of another) are resolved first so that the
//! mesh is watertight.

use std::collections::HashMap;

use kanavos_geom::{HalfPlane, Plane, Point2, Point3, Polygon2, PolygonWithHoles, Vec2, clip_convex};

use crate::{Mesh, SolidError, triangulate};

/// A non-vertical plane as a height function `z(x, y) = z0 + gx·x + gy·y` (metres).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ZPlane {
    /// Height at the origin.
    pub z0: f64,
    /// Slope along X (dz/dx).
    pub gx: f64,
    /// Slope along Y (dz/dy).
    pub gy: f64,
}

impl ZPlane {
    /// A horizontal plane at height `z`.
    pub const fn constant(z: f64) -> ZPlane {
        ZPlane { z0: z, gx: 0.0, gy: 0.0 }
    }

    /// The plane through `p` (plan point) at height `z` rising with gradient `grad` (dz per metre).
    pub fn through(p: Point2, z: f64, grad: Vec2) -> ZPlane {
        let [x, y] = p.xy();
        ZPlane { z0: z - grad.x * x - grad.y * y, gx: grad.x, gy: grad.y }
    }

    /// The height function of a 3D plane; `None` for (nearly) vertical planes.
    pub fn from_plane(plane: &Plane) -> Option<ZPlane> {
        let n = plane.normal;
        if n.z.abs() < 1e-9 {
            return None;
        }
        // n · (p - o) = 0  ⇒  z = o.z - (n.x (x - o.x) + n.y (y - o.y)) / n.z
        let gx = -n.x / n.z;
        let gy = -n.y / n.z;
        Some(ZPlane { z0: plane.origin.z - gx * plane.origin.x - gy * plane.origin.y, gx, gy })
    }

    /// Height at a plan point.
    pub fn at(&self, p: Point2) -> f64 {
        let [x, y] = p.xy();
        self.z0 + self.gx * x + self.gy * y
    }

    /// The plane moved up by `dz`.
    pub fn shifted(&self, dz: f64) -> ZPlane {
        ZPlane { z0: self.z0 + dz, ..*self }
    }

    /// Gradient (dz per metre in plan).
    pub fn gradient(&self) -> Vec2 {
        Vec2::new(self.gx, self.gy)
    }
}

/// One plan piece of a column solid: a simple polygon and its solid vertical intervals `(bottom, top)`, ordered
/// and disjoint.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnPiece {
    /// Plan region (simple polygon, any orientation).
    pub region: Polygon2,
    /// Solid intervals from bottom to top.
    pub intervals: Vec<(ZPlane, ZPlane)>,
}

/// A rectangular hole through a prism, perpendicular to its `along` direction: positions `u` along it and
/// absolute heights `z`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThroughHole {
    /// Start along the direction, metres from the origin.
    pub u0: f64,
    /// End along the direction.
    pub u1: f64,
    /// Bottom of the hole (absolute height).
    pub z0: f64,
    /// Top of the hole (absolute height).
    pub z1: f64,
}

type Key = (i64, i64);

struct PointIndex {
    eps: f64,
    grid: HashMap<Key, Vec<usize>>,
    points: Vec<Point2>,
}

impl PointIndex {
    fn new(eps: f64) -> Self {
        PointIndex { eps, grid: HashMap::new(), points: Vec::new() }
    }
    fn key(&self, p: Point2) -> Key {
        let [x, y] = p.xy();
        ((x / self.eps).floor() as i64, (y / self.eps).floor() as i64)
    }
    fn id(&mut self, p: Point2) -> usize {
        let k = self.key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.grid.get(&(k.0 + dx, k.1 + dy)) {
                    for &i in list {
                        if self.points.get(i).is_some_and(|q| (*q - p).length() <= self.eps) {
                            return i;
                        }
                    }
                }
            }
        }
        self.points.push(p);
        let i = self.points.len() - 1;
        self.grid.entry(k).or_default().push(i);
        i
    }
}

fn subtract(
    own: &[(ZPlane, ZPlane)],
    other: &[(ZPlane, ZPlane)],
    m: Point2,
    eps: f64,
) -> Vec<(ZPlane, ZPlane)> {
    let mut result: Vec<(ZPlane, ZPlane)> = own.to_vec();
    for &(olo, ohi) in other {
        let (ol, oh) = (olo.at(m), ohi.at(m));
        let mut next = Vec::with_capacity(result.len() + 1);
        for (lo, hi) in result {
            let (l, h) = (lo.at(m), hi.at(m));
            if oh <= l + eps || ol >= h - eps {
                next.push((lo, hi));
                continue;
            }
            if ol > l + eps {
                next.push((lo, olo));
            }
            if oh < h - eps {
                next.push((ohi, hi));
            }
        }
        result = next;
    }
    result
}

/// Builds the closed mesh of a column solid (see the module documentation).
///
/// `eps` (metres) is the distance below which plan points are the same and heights equal; 10⁻⁶ suits building
/// elements. Pieces must not overlap. Failure modes: a piece that cannot be triangulated gives
/// [`SolidError::Triangulation`]; pieces with fewer than three distinct points or no intervals are skipped.
pub fn column_solid(pieces: &[ColumnPiece], eps: f64) -> Result<Mesh, SolidError> {
    let eps = if eps.is_finite() && eps > 0.0 { eps } else { 1e-6 };
    // Shared point ids.
    let mut index = PointIndex::new(eps);
    let mut rings: Vec<Vec<usize>> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let poly = piece.region.to_ccw();
        let mut ring: Vec<usize> = Vec::with_capacity(poly.len());
        for &p in &poly.vertices {
            let id = index.id(p);
            if ring.last() != Some(&id) {
                ring.push(id);
            }
        }
        while ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        rings.push(ring);
    }
    // T-junctions: insert every point lying on an edge of another ring.
    let all: Vec<(usize, Point2)> = index.points.iter().copied().enumerate().collect();
    for ring in rings.iter_mut() {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        let mut fixed: Vec<usize> = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (Some(&ia), Some(&ib)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
            fixed.push(ia);
            let (Some(&a), Some(&b)) = (index.points.get(ia), index.points.get(ib)) else { continue };
            let d = b - a;
            let len2 = d.length_squared();
            if len2 <= eps * eps {
                continue;
            }
            let len = len2.sqrt();
            let mut on: Vec<(f64, usize)> = all
                .iter()
                .filter(|(id, _)| *id != ia && *id != ib)
                .filter_map(|&(id, p)| {
                    let t = (p - a).dot(d) / len2;
                    if t * len <= eps || (1.0 - t) * len <= eps {
                        return None;
                    }
                    let dist = (p - a).cross(d).abs() / len;
                    if dist <= eps { Some((t, id)) } else { None }
                })
                .collect();
            on.sort_by(|x, y| x.0.total_cmp(&y.0));
            fixed.extend(on.into_iter().map(|(_, id)| id));
        }
        *ring = fixed;
    }
    // Directed edge → piece.
    let mut owner: HashMap<(usize, usize), usize> = HashMap::new();
    for (k, ring) in rings.iter().enumerate() {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            if let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) {
                owner.insert((a, b), k);
            }
        }
    }
    let mut mesh = Mesh::new();
    let pt = |id: usize| index.points.get(id).copied().unwrap_or(Point2::ORIGIN);
    // Every height at which some face has a vertex above each plan point.
    let mut heights: HashMap<usize, Vec<f64>> = HashMap::new();
    for (ring, piece) in rings.iter().zip(pieces) {
        for &id in ring {
            let p = pt(id);
            let list = heights.entry(id).or_default();
            for (lo, hi) in &piece.intervals {
                list.push(lo.at(p));
                list.push(hi.at(p));
            }
        }
    }
    for list in heights.values_mut() {
        list.sort_by(f64::total_cmp);
        list.dedup_by(|x, y| (*x - *y).abs() <= eps);
    }
    for (k, (ring, piece)) in rings.iter().zip(pieces).enumerate() {
        let n = ring.len();
        if n < 3 || piece.intervals.is_empty() {
            continue;
        }
        let poly = Polygon2::new(ring.iter().map(|&i| pt(i)).collect());
        if poly.area().as_m2() <= eps * eps {
            continue;
        }
        let (points, tris) = triangulate(&PolygonWithHoles::from(poly))?;
        for &(lo, hi) in &piece.intervals {
            let base = mesh.positions.len() as u32;
            for &p in &points {
                mesh.push_vertex(Point3::from_plan(p, lo.at(p)));
            }
            for t in &tris {
                mesh.push_triangle(base + t[0], base + t[2], base + t[1]);
            }
            let base = mesh.positions.len() as u32;
            for &p in &points {
                mesh.push_vertex(Point3::from_plan(p, hi.at(p)));
            }
            for t in &tris {
                mesh.push_triangle(base + t[0], base + t[1], base + t[2]);
            }
        }
        for i in 0..n {
            let (Some(&ia), Some(&ib)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
            let (a, b) = (pt(ia), pt(ib));
            let exposed = match owner.get(&(ib, ia)) {
                Some(&other) if other != k => {
                    let theirs = pieces.get(other).map(|p| p.intervals.as_slice()).unwrap_or(&[]);
                    subtract(&piece.intervals, theirs, a.midpoint(b), eps)
                }
                _ => piece.intervals.clone(),
            };
            let none: &[f64] = &[];
            let ha_list = heights.get(&ia).map(Vec::as_slice).unwrap_or(none);
            let hb_list = heights.get(&ib).map(Vec::as_slice).unwrap_or(none);
            for (lo, hi) in exposed {
                side_face(&mut mesh, a, b, (lo, hi), ha_list, hb_list, eps)?;
            }
        }
    }
    Ok(mesh.welded(eps * 0.5))
}

/// Emits the vertical face along plan edge `a → b` between `lo` and `hi`, facing to the right of the edge, with
/// extra vertices at every height where neighbouring faces have one (so the mesh has no T-junctions).
fn side_face(
    mesh: &mut Mesh,
    a: Point2,
    b: Point2,
    (lo, hi): (ZPlane, ZPlane),
    heights_a: &[f64],
    heights_b: &[f64],
    eps: f64,
) -> Result<(), SolidError> {
    let (la, lb, ha, hb) = (lo.at(a), lo.at(b), hi.at(a), hi.at(b));
    if ha - la <= eps && hb - lb <= eps {
        return Ok(());
    }
    let len = (b - a).length();
    // Outline in the face's own coordinates (s along the edge, z up), counter-clockwise, with 3D twins.
    let mut flat: Vec<Point2> = Vec::new();
    let mut solid: Vec<Point3> = Vec::new();
    let mut push = |s: f64, z: f64, at: Point2| {
        let q = Point2::new(s, z);
        if flat.last().is_none_or(|p| (*p - q).length() > eps) {
            flat.push(q);
            solid.push(Point3::from_plan(at, z));
        }
    };
    push(0.0, la, a);
    push(len, lb, b);
    for &h in heights_b.iter().filter(|h| **h > lb + eps && **h < hb - eps) {
        push(len, h, b);
    }
    push(len, hb, b);
    push(0.0, ha, a);
    for &h in heights_a.iter().rev().filter(|h| **h > la + eps && **h < ha - eps) {
        push(0.0, h, a);
    }
    if flat.len() > 1 && flat.first().zip(flat.last()).is_some_and(|(p, q)| (*p - *q).length() <= eps) {
        flat.pop();
        solid.pop();
    }
    let outline = Polygon2::new(flat);
    if outline.len() < 3 || outline.signed_area() <= eps * eps {
        return Ok(());
    }
    let (points, tris) = triangulate(&PolygonWithHoles::from(outline))?;
    if points.len() != solid.len() {
        return Err(SolidError::Triangulation("side face changed its outline".into()));
    }
    let base = mesh.positions.len() as u32;
    for p in &solid {
        mesh.push_vertex(*p);
    }
    for t in &tris {
        mesh.push_triangle(base + t[0], base + t[1], base + t[2]);
    }
    Ok(())
}

/// A vertical prism over a convex `profile` between `bottom` and `top`, with rectangular holes running through it
/// perpendicular to `along` (for straight walls: holes for doors and windows). `u` positions are measured along
/// `along` from `origin`.
///
/// Failure modes: a non-convex profile or a zero `along` gives [`SolidError::Degenerate`]; see [`column_solid`].
pub fn prism_with_through_holes(
    profile: &Polygon2,
    origin: Point2,
    along: Vec2,
    bottom: ZPlane,
    top: ZPlane,
    holes: &[ThroughHole],
    eps: f64,
) -> Result<Mesh, SolidError> {
    if !profile.is_convex() {
        return Err(SolidError::Degenerate("profile must be convex"));
    }
    let dir = along.normalized().ok_or(SolidError::Degenerate("zero direction"))?;
    // Cut positions.
    let mut cuts: Vec<f64> = holes.iter().flat_map(|h| [h.u0.min(h.u1), h.u0.max(h.u1)]).collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() <= eps);
    let mut pieces = Vec::with_capacity(cuts.len() + 1);
    let mut bounds: Vec<Option<f64>> = vec![None];
    bounds.extend(cuts.iter().map(|&c| Some(c)));
    bounds.push(None);
    for w in bounds.windows(2) {
        let [lo, hi] = w else { continue };
        let mut poly = Some(profile.to_ccw());
        if let Some(u) = lo
            && let Some(hp) = HalfPlane::new(origin + dir * *u, dir)
        {
            poly = poly.and_then(|p| clip_convex(&p, &hp, eps));
        }
        if let Some(u) = hi
            && let Some(hp) = HalfPlane::new(origin + dir * *u, -dir)
        {
            poly = poly.and_then(|p| clip_convex(&p, &hp, eps));
        }
        let Some(region) = poly else { continue };
        let Some(c) = region.centroid() else { continue };
        let uc = (c - origin).dot(dir);
        let mut intervals = vec![(bottom, top)];
        for h in holes {
            if uc > h.u0.min(h.u1) && uc < h.u0.max(h.u1) {
                intervals = subtract(&intervals, &[(ZPlane::constant(h.z0), ZPlane::constant(h.z1))], c, eps);
            }
        }
        pieces.push(ColumnPiece { region, intervals });
    }
    column_solid(&pieces, eps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip() -> Polygon2 {
        // A 5 m long, 0.3 m thick wall along +X.
        Polygon2::from_m(&[(0.0, 0.0), (5.0, 0.0), (5.0, 0.3), (0.0, 0.3)])
    }

    #[test]
    fn wall_with_a_window_is_closed_and_has_net_volume() {
        let holes = [ThroughHole { u0: 1.0, u1: 2.2, z0: 0.9, z1: 2.1 }];
        let m = prism_with_through_holes(
            &strip(),
            Point2::ORIGIN,
            Vec2::X,
            ZPlane::constant(0.0),
            ZPlane::constant(3.0),
            &holes,
            1e-6,
        )
        .unwrap();
        assert!(m.is_closed(1e-7), "open edges: {}", m.open_edge_count());
        let expected = 5.0 * 0.3 * 3.0 - 1.2 * 1.2 * 0.3;
        assert!((m.volume() - expected).abs() < 1e-9, "volume {}", m.volume());
    }

    #[test]
    fn door_reaching_the_floor_and_two_windows() {
        let holes = [
            ThroughHole { u0: 0.5, u1: 1.4, z0: 0.0, z1: 2.1 },
            ThroughHole { u0: 2.0, u1: 3.0, z0: 1.0, z1: 2.0 },
            ThroughHole { u0: 3.5, u1: 4.5, z0: 1.0, z1: 2.0 },
        ];
        let m = prism_with_through_holes(
            &strip(),
            Point2::ORIGIN,
            Vec2::X,
            ZPlane::constant(0.0),
            ZPlane::constant(3.0),
            &holes,
            1e-6,
        )
        .unwrap();
        assert!(m.is_closed(1e-7));
        let expected = 5.0 * 0.3 * 3.0 - 0.3 * (0.9 * 2.1 + 1.0 + 1.0);
        assert!((m.volume() - expected).abs() < 1e-9);
    }

    #[test]
    fn sloped_top_follows_the_plane() {
        // Top rising along X from 3 m to 4 m: a gable-like wall.
        let top = ZPlane::through(Point2::ORIGIN, 3.0, Vec2::new(0.2, 0.0));
        let m = prism_with_through_holes(
            &strip(),
            Point2::ORIGIN,
            Vec2::X,
            ZPlane::constant(0.0),
            top,
            &[],
            1e-6,
        )
        .unwrap();
        assert!(m.is_closed(1e-7));
        assert!((m.volume() - 0.3 * 5.0 * 3.5).abs() < 1e-9);
    }

    #[test]
    fn pieces_with_different_tops_step() {
        // Two pieces side by side, 3 m and 2 m tall: the step face closes the mesh.
        let a = ColumnPiece {
            region: Polygon2::from_m(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
            intervals: vec![(ZPlane::constant(0.0), ZPlane::constant(3.0))],
        };
        let b = ColumnPiece {
            region: Polygon2::from_m(&[(1.0, 0.0), (2.0, 0.0), (2.0, 0.5), (2.0, 1.0), (1.0, 1.0)]),
            intervals: vec![(ZPlane::constant(0.0), ZPlane::constant(2.0))],
        };
        // A third piece creates a T-junction on b's right edge.
        let c = ColumnPiece {
            region: Polygon2::from_m(&[(2.0, 0.0), (3.0, 0.0), (3.0, 0.5), (2.0, 0.5)]),
            intervals: vec![(ZPlane::constant(0.0), ZPlane::constant(1.0))],
        };
        let d = ColumnPiece {
            region: Polygon2::from_m(&[(2.0, 0.5), (3.0, 0.5), (3.0, 1.0), (2.0, 1.0)]),
            intervals: vec![(ZPlane::constant(0.0), ZPlane::constant(1.0))],
        };
        let m = column_solid(&[a, b, c, d], 1e-6).unwrap();
        assert!(m.is_closed(1e-7), "open edges {}", m.open_edge_count());
        assert!((m.volume() - (3.0 + 2.0 + 0.5 + 0.5)).abs() < 1e-9);
    }

    #[test]
    fn section_along_a_wall_with_a_window_shows_the_hole() {
        let holes = [ThroughHole { u0: 1.0, u1: 2.2, z0: 0.9, z1: 2.1 }];
        let m = prism_with_through_holes(
            &strip(),
            Point2::ORIGIN,
            Vec2::X,
            ZPlane::constant(0.0),
            ZPlane::constant(3.0),
            &holes,
            1e-6,
        )
        .unwrap();
        // Longitudinal section through the middle of the wall: an elevation with the window as a hole.
        let f = crate::PlaneFrame::vertical(Point2::new(0.0, 0.15), Point2::new(5.0, 0.15), 0.0).unwrap();
        let s = crate::section(&m, &f, 1e-9);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].holes.len(), 1);
        assert!((s[0].area().as_m2() - (15.0 - 1.44)).abs() < 1e-9);
        // Horizontal section at 1.5 m: two pieces either side of the window.
        let h = crate::section(&m, &crate::PlaneFrame::new(&Plane::horizontal(1.5)), 1e-9);
        assert_eq!(h.len(), 2);
        assert!((kanavos_geom::total_area(&h) - 0.3 * (5.0 - 1.2)).abs() < 1e-9);
    }

    #[test]
    fn zplane_from_plane() {
        let p = Plane::new(Point3::new(1.0, 2.0, 3.0), kanavos_geom::Vec3::new(0.0, -1.0, 1.0)).unwrap();
        let z = ZPlane::from_plane(&p).unwrap();
        assert!((z.at(Point2::new(1.0, 2.0)) - 3.0).abs() < 1e-12);
        assert!((z.at(Point2::new(1.0, 3.0)) - 4.0).abs() < 1e-12);
    }
}

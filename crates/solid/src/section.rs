//! Sections and clipping of closed meshes by a plane.
//!
//! Vertices within `eps` of the plane are treated as lying slightly on its front side (symbolic perturbation),
//! so no triangle is ever coplanar with the plane and every crossing edge has endpoints strictly on opposite
//! sides. Crossing points are identified by the mesh edge they lie on, which makes the loops of a welded,
//! closed mesh close exactly. A section exactly through a face therefore shows the solid just behind it.

use std::collections::HashMap;

use kanavos_geom::{Plane, Point2, Point3, Polygon2, PolygonWithHoles, Tolerance, Vec3};

use crate::{Mesh, SolidError, triangulate};

/// A plane with an orthonormal 2D frame `(u, v)` on it; `u × v = normal`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneFrame {
    /// Origin of the 2D coordinates.
    pub origin: Point3,
    /// First axis (2D x).
    pub u: Vec3,
    /// Second axis (2D y).
    pub v: Vec3,
    /// Unit normal (`u × v`), pointing to the plane's front side.
    pub normal: Vec3,
}

impl PlaneFrame {
    /// A frame on `plane`. For non-horizontal planes `v` points up (along the projection of +Z) and `u` is
    /// horizontal, so a vertical section reads as an elevation; for horizontal planes `u` is +X.
    pub fn new(plane: &Plane) -> PlaneFrame {
        let n = plane.normal;
        let up = Vec3::Z - n * Vec3::Z.dot(n);
        let (u, v) = match up.normalized() {
            Some(v) if n.z.abs() < 1.0 - 1e-9 => (v.cross(n), v),
            _ => {
                let u = (Vec3::X - n * Vec3::X.dot(n)).normalized().unwrap_or(Vec3::Y);
                (u, n.cross(u))
            }
        };
        PlaneFrame { origin: plane.origin, u, v, normal: n }
    }

    /// A vertical section plane through `a` and `b` (plan points), looking to the left of `a → b`: the front side
    /// (normal) points to the right, so the material kept by [`clip`] is on the left, `u` runs from `a` to `b`
    /// and `v` is up.
    pub fn vertical(a: Point2, b: Point2, z: f64) -> Option<PlaneFrame> {
        let d = (b - a).normalized()?;
        let u = Vec3::new(d.x, d.y, 0.0);
        let v = Vec3::Z;
        Some(PlaneFrame { origin: Point3::from_plan(a, z), u, v, normal: u.cross(v) })
    }

    /// The plane of the frame.
    pub fn plane(&self) -> Plane {
        Plane { origin: self.origin, normal: self.normal }
    }

    /// 2D coordinates of a point (projected onto the plane).
    pub fn to_2d(&self, p: Point3) -> Point2 {
        let d = p - self.origin;
        Point2::new(d.dot(self.u), d.dot(self.v))
    }

    /// The 3D point of 2D coordinates on the plane.
    pub fn to_3d(&self, p: Point2) -> Point3 {
        let [x, y] = p.xy();
        self.origin + self.u * x + self.v * y
    }
}

type EdgeKey = (u32, u32);

fn edge_key(a: u32, b: u32) -> EdgeKey {
    if a < b { (a, b) } else { (b, a) }
}

struct Classified {
    mesh: Mesh,
    dist: Vec<f64>,
    front: Vec<bool>,
}

fn classify(mesh: &Mesh, frame: &PlaneFrame, eps: f64) -> Classified {
    let mesh = mesh.welded(eps.max(1e-12) * 0.5);
    let plane = frame.plane();
    let dist: Vec<f64> = mesh.positions.iter().map(|p| plane.signed_distance(*p)).collect();
    // On-plane vertices count as front.
    let front: Vec<bool> = dist.iter().map(|d| *d > -eps).collect();
    Classified { mesh, dist, front }
}

impl Classified {
    fn crossing(&self, a: u32, b: u32, eps: f64) -> Point3 {
        let pa = self.mesh.positions.get(a as usize).copied().unwrap_or(Point3::ORIGIN);
        let pb = self.mesh.positions.get(b as usize).copied().unwrap_or(Point3::ORIGIN);
        let da = self.dist.get(a as usize).copied().unwrap_or(0.0);
        let db = self.dist.get(b as usize).copied().unwrap_or(0.0);
        if da.abs() <= eps {
            return pa;
        }
        if db.abs() <= eps {
            return pb;
        }
        let t = (da / (da - db)).clamp(0.0, 1.0);
        pa.lerp(pb, t)
    }
    fn is_front(&self, i: u32) -> bool {
        self.front.get(i as usize).copied().unwrap_or(true)
    }
}

/// Directed section segments of every crossing triangle, keyed by the mesh edges they start and end on. The
/// solid is on the left of each segment seen from the front of the plane.
fn segments(c: &Classified, eps: f64) -> Vec<(EdgeKey, EdgeKey, Point3, Point3)> {
    let mut out = Vec::new();
    for t in &c.mesh.triangles {
        let f = [c.is_front(t[0]), c.is_front(t[1]), c.is_front(t[2])];
        if f[0] == f[1] && f[1] == f[2] {
            continue;
        }
        // Walking the triangle in its (outward) order, the section enters the solid's cut face on the edge that
        // goes from front to behind and leaves it on the edge that goes from behind to front.
        let mut enter: Option<(EdgeKey, Point3)> = None;
        let mut leave: Option<(EdgeKey, Point3)> = None;
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (Some(&fi), Some(&fj), Some(&a), Some(&b)) = (f.get(i), f.get(j), t.get(i), t.get(j)) else {
                continue;
            };
            if fi && !fj {
                enter = Some((edge_key(a, b), c.crossing(a, b, eps)));
            } else if !fi && fj {
                leave = Some((edge_key(a, b), c.crossing(a, b, eps)));
            }
        }
        if let (Some((k0, p0)), Some((k1, p1))) = (enter, leave) {
            out.push((k0, k1, p0, p1));
        }
    }
    out
}

/// Closed loops (in 3D) from directed segments.
fn chain(segs: &[(EdgeKey, EdgeKey, Point3, Point3)]) -> Vec<Vec<(EdgeKey, Point3)>> {
    let mut next: HashMap<EdgeKey, usize> = HashMap::with_capacity(segs.len());
    for (i, s) in segs.iter().enumerate() {
        next.insert(s.0, i);
    }
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for start in 0..segs.len() {
        if used.get(start).copied().unwrap_or(true) {
            continue;
        }
        let mut ring = Vec::new();
        let mut i = start;
        loop {
            if used.get(i).copied().unwrap_or(true) {
                break;
            }
            if let Some(u) = used.get_mut(i) {
                *u = true;
            }
            let Some(s) = segs.get(i) else { break };
            ring.push((s.0, s.2));
            match next.get(&s.1) {
                Some(&j) => i = j,
                None => break,
            }
        }
        if ring.len() >= 3 {
            loops.push(ring);
        }
    }
    loops
}

/// Groups oriented loops into polygons with holes: counter-clockwise loops are outer boundaries, clockwise loops
/// holes of the smallest outer boundary that contains them.
fn assemble(loops: Vec<Polygon2>) -> Vec<PolygonWithHoles> {
    let tol = Tolerance::default();
    let mut outers: Vec<PolygonWithHoles> = Vec::new();
    let mut holes: Vec<Polygon2> = Vec::new();
    for l in loops {
        let a = l.signed_area();
        if a > 1e-14 {
            outers.push(l.into());
        } else if a < -1e-14 {
            holes.push(l);
        }
    }
    for h in holes {
        let Some(&probe) = h.vertices.first() else { continue };
        let target = outers
            .iter_mut()
            .filter(|o| o.outer.contains(probe, &tol))
            .min_by(|a, b| a.outer.area().as_m2().total_cmp(&b.outer.area().as_m2()));
        if let Some(o) = target {
            o.holes.push(h);
        }
    }
    outers
}

/// The section of a closed mesh by a plane: polygons with holes in the frame's 2D coordinates (outer rings
/// counter-clockwise seen from the front of the plane).
///
/// Tolerance: vertices within `eps` metres of the plane count as on it (and are treated as in front). Failure
/// modes: an open or inconsistently oriented mesh can leave chains that do not close; those are dropped. Meshes
/// made of several overlapping closed shells give overlapping polygons (merge them with
/// `kanavos_geom::union_all` if needed).
pub fn section(mesh: &Mesh, frame: &PlaneFrame, eps: f64) -> Vec<PolygonWithHoles> {
    let c = classify(mesh, frame, eps);
    let segs = segments(&c, eps);
    let loops = chain(&segs)
        .into_iter()
        .map(|ring| Polygon2::new(ring.into_iter().map(|(_, p)| frame.to_2d(p)).collect()))
        .collect();
    assemble(loops)
}

/// The part of a closed mesh behind the frame's plane (opposite to its normal), closed with a cap on the plane.
///
/// Tolerance and failure modes as for [`section`]; a cap that cannot be triangulated gives
/// [`SolidError::Triangulation`].
pub fn clip(mesh: &Mesh, frame: &PlaneFrame, eps: f64) -> Result<Mesh, SolidError> {
    let c = classify(mesh, frame, eps);
    let mut out = Mesh::new();
    let mut index: HashMap<u32, u32> = HashMap::new();
    let mut cross_index: HashMap<EdgeKey, u32> = HashMap::new();
    let mut vertex = |out: &mut Mesh, i: u32| -> u32 {
        *index.entry(i).or_insert_with(|| {
            out.push_vertex(c.mesh.positions.get(i as usize).copied().unwrap_or(Point3::ORIGIN))
        })
    };
    for t in &c.mesh.triangles {
        let f = [c.is_front(t[0]), c.is_front(t[1]), c.is_front(t[2])];
        let behind = f.iter().filter(|x| !**x).count();
        if behind == 0 {
            continue;
        }
        if behind == 3 {
            let a = vertex(&mut out, t[0]);
            let b = vertex(&mut out, t[1]);
            let cc = vertex(&mut out, t[2]);
            out.push_triangle(a, b, cc);
            continue;
        }
        // Rotate so that the lone vertex (behind or in front) is first.
        let lone_behind = behind == 1;
        let k = (0..3).find(|&i| !f.get(i).copied().unwrap_or(true) == lone_behind).unwrap_or(0);
        let (Some(&i0), Some(&i1), Some(&i2)) = (t.get(k), t.get((k + 1) % 3), t.get((k + 2) % 3)) else {
            continue;
        };
        let mut cross = |out: &mut Mesh, a: u32, b: u32| -> u32 {
            let key = edge_key(a, b);
            *cross_index.entry(key).or_insert_with(|| out.push_vertex(c.crossing(a, b, eps)))
        };
        let x01 = cross(&mut out, i0, i1);
        let x20 = cross(&mut out, i2, i0);
        if lone_behind {
            // Keep the corner triangle at i0.
            let a = vertex(&mut out, i0);
            out.push_triangle(a, x01, x20);
        } else {
            // i0 is in front: keep the quad i1, i2, x20, x01.
            let b = vertex(&mut out, i1);
            let cc = vertex(&mut out, i2);
            out.push_triangle(x01, b, cc);
            out.push_triangle(x01, cc, x20);
        }
    }
    // Cap: the section loops, facing the front side.
    let segs = segments(&c, eps);
    let loops: Vec<Polygon2> = chain(&segs)
        .into_iter()
        .map(|ring| Polygon2::new(ring.into_iter().map(|(_, p)| frame.to_2d(p)).collect()))
        .collect();
    for shape in assemble(loops) {
        let (pts, tris) = triangulate(&shape)?;
        let base = out.positions.len() as u32;
        for p in &pts {
            out.push_vertex(frame.to_3d(*p));
        }
        for tri in &tris {
            // Counter-clockwise in (u, v) means facing +normal: the cap faces the removed side.
            out.push_triangle(base + tri[0], base + tri[1], base + tri[2]);
        }
    }
    Ok(out.welded(eps.max(1e-12) * 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cuboid, extrude};

    #[test]
    fn horizontal_section_of_a_box() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 3.0, 2.0));
        let f = PlaneFrame::new(&Plane::horizontal(1.0));
        let s = section(&m, &f, 1e-9);
        assert_eq!(s.len(), 1);
        assert!((s[0].area().as_m2() - 12.0).abs() < 1e-9);
        assert!(s[0].outer.is_ccw());
    }

    #[test]
    fn vertical_section_reads_as_elevation() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 3.0, 2.0));
        let f = PlaneFrame::vertical(Point2::new(-1.0, 1.5), Point2::new(5.0, 1.5), 0.0).unwrap();
        let s = section(&m, &f, 1e-9);
        assert_eq!(s.len(), 1);
        let b = s[0].bbox().unwrap();
        assert!((b.min.x.as_m() - 1.0).abs() < 1e-9 && (b.max.x.as_m() - 5.0).abs() < 1e-9);
        assert!(b.min.y.as_m().abs() < 1e-9 && (b.max.y.as_m() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn section_through_a_hole_gives_a_ring() {
        let shape = PolygonWithHoles {
            outer: Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
            holes: vec![Polygon2::from_m(&[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)])],
        };
        let m = extrude(&shape, 0.0, 3.0).unwrap();
        let s = section(&m, &PlaneFrame::new(&Plane::horizontal(1.5)), 1e-9);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].holes.len(), 1);
        assert!((s[0].area().as_m2() - 84.0).abs() < 1e-9);
    }

    #[test]
    fn section_exactly_at_a_face_is_stable() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0));
        // At the top face the box is just behind (below) the plane: section of its top.
        let s = section(&m, &PlaneFrame::new(&Plane::horizontal(1.0)), 1e-9);
        assert_eq!(s.len(), 1);
        // At the bottom face nothing is behind the plane.
        let below = section(&m, &PlaneFrame::new(&Plane::horizontal(0.0)), 1e-9);
        assert!(below.is_empty() || below.iter().all(|p| p.area().as_m2() > 0.99));
    }

    #[test]
    fn clipping_keeps_a_closed_half() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 3.0, 2.0));
        let f = PlaneFrame::new(&Plane::horizontal(0.5));
        let c = clip(&m, &f, 1e-9).unwrap();
        assert!((c.volume() - 6.0).abs() < 1e-9, "volume {}", c.volume());
        assert!(c.is_closed(1e-9));
        // A slanted plane.
        let p = Plane::new(Point3::new(2.0, 1.5, 1.0), Vec3::new(1.0, 0.0, 1.0)).unwrap();
        let c = clip(&m, &PlaneFrame::new(&p), 1e-9).unwrap();
        assert!(c.is_closed(1e-9));
        assert!((c.volume() - 12.0).abs() < 1e-9, "volume {}", c.volume());
    }

    #[test]
    fn clipping_a_ring_shape_caps_with_a_hole() {
        let shape = PolygonWithHoles {
            outer: Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
            holes: vec![Polygon2::from_m(&[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)])],
        };
        let m = extrude(&shape, 0.0, 3.0).unwrap();
        let c = clip(&m, &PlaneFrame::new(&Plane::horizontal(1.0)), 1e-9).unwrap();
        assert!(c.is_closed(1e-9));
        assert!((c.volume() - 84.0).abs() < 1e-9);
    }
}

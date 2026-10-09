//! Mesh topology, clipping and plane sections used by the projected views.
//!
//! - [`Topology`] welds the solids of one element into a single vertex set, removes internal faces (two
//!   coincident faces of touching solids, e.g. the boundary between two layers of a wall) and records, for
//!   every edge, its incident faces and how sharp the crease is. Views use it to pick the edges that can be
//!   seen: creases, silhouettes and open boundaries.
//! - [`clip_polygon`] is Sutherland–Hodgman clipping of a convex or concave planar polygon against a half-space
//!   (I. E. Sutherland, G. W. Hodgman, "Reentrant polygon clipping", CACM 17(1), 1974).
//! - [`slice_loops`] is the mesh–plane section: the closed loops where a closed mesh crosses a plane. It is a
//!   local stand-in for the shared mesh–plane section planned in `kanavos-solid`.
//!
//! Tolerances: vertices closer than 0.1 µm are welded; coincident faces are recognised to 0.1 mm in plane offset
//! and 1e-3 in normal direction; section loops are joined across gaps up to 10 µm.

use std::collections::HashMap;

use crate::poly::Contour;

/// A 3D point or vector as raw metres.
pub(crate) type V3 = [f64; 3];

pub(crate) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub(crate) fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(crate) fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub(crate) fn normalize(a: V3) -> Option<V3> {
    let l = length(a);
    if l > 1e-15 && l.is_finite() { Some(scale(a, 1.0 / l)) } else { None }
}
pub(crate) fn lerp(a: V3, b: V3, t: f64) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Marker for "no face" on a boundary edge.
pub(crate) const NONE: u32 = u32::MAX;

/// An edge of a welded mesh with its incident faces.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TopoEdge {
    /// First vertex.
    pub a: u32,
    /// Second vertex.
    pub b: u32,
    /// First incident face.
    pub f0: u32,
    /// Second incident face, or [`NONE`] for a boundary edge.
    pub f1: u32,
    /// Smallest dot product between the normals of any two incident faces (1 for a flat edge).
    pub min_dot: f64,
    /// Whether more than two faces meet at the edge.
    pub non_manifold: bool,
    /// Further incident faces of a non-manifold edge.
    pub others: Vec<u32>,
}

/// Welded topology of one element's solids.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Topology {
    /// Welded positions.
    pub positions: Vec<V3>,
    /// Triangles (degenerate ones removed).
    pub triangles: Vec<[u32; 3]>,
    /// Unit normal per triangle.
    pub normals: Vec<V3>,
    /// Whether a triangle is an internal face (coincident with an opposite face of a touching solid).
    pub internal: Vec<bool>,
    /// Edges of non-internal faces.
    pub edges: Vec<TopoEdge>,
}

const WELD: f64 = 1e7;

fn key(p: V3) -> [i64; 3] {
    [(p[0] * WELD).round() as i64, (p[1] * WELD).round() as i64, (p[2] * WELD).round() as i64]
}

impl Topology {
    /// Builds the topology of several meshes (positions and triangles) as one welded mesh.
    pub(crate) fn build<'a>(meshes: impl IntoIterator<Item = (&'a [V3], &'a [[u32; 3]])>) -> Topology {
        let mut index: HashMap<[i64; 3], u32> = HashMap::new();
        let mut seen: std::collections::HashSet<[u32; 3]> = std::collections::HashSet::new();
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for (pos, tris) in meshes {
            let mut map = Vec::with_capacity(pos.len());
            for p in pos {
                let id = *index.entry(key(*p)).or_insert_with(|| {
                    positions.push(*p);
                    (positions.len() - 1) as u32
                });
                map.push(id);
            }
            for t in tris {
                let (Some(&a), Some(&b), Some(&c)) =
                    (map.get(t[0] as usize), map.get(t[1] as usize), map.get(t[2] as usize))
                else {
                    continue;
                };
                if a != b && b != c && a != c {
                    // Drop exact duplicates with the same orientation (the same solid appended twice); opposite
                    // copies stay and are recognised as internal faces below.
                    let k = if a < b && a < c {
                        [a, b, c]
                    } else if b < c {
                        [b, c, a]
                    } else {
                        [c, a, b]
                    };
                    if seen.insert(k) {
                        triangles.push([a, b, c]);
                    }
                }
            }
        }
        let p = |i: u32| positions.get(i as usize).copied().unwrap_or([0.0; 3]);
        let mut normals = Vec::with_capacity(triangles.len());
        let mut kept = Vec::with_capacity(triangles.len());
        for t in &triangles {
            let n = cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0])));
            if let Some(n) = normalize(n) {
                normals.push(n);
                kept.push(*t);
            }
        }
        let triangles = kept;
        let planes = Planes::new(&positions, &triangles, &normals);
        let internal = planes.internal_faces(&positions, &triangles);

        let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (i, t) in triangles.iter().enumerate() {
            if internal.get(i).copied().unwrap_or(false) {
                continue;
            }
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let k = if a < b { (a, b) } else { (b, a) };
                edge_faces.entry(k).or_default().push(i as u32);
            }
        }
        let normal = |f: u32| normals.get(f as usize).copied().unwrap_or([0.0; 3]);
        let mut edges: Vec<TopoEdge> = edge_faces
            .into_iter()
            .filter(|((a, b), faces)| {
                // An edge with one face is an open boundary, unless the surface continues in the same plane
                // across it (a T-junction or a dropped sliver triangle): then it is a seam and not drawn.
                match faces.as_slice() {
                    [f] => !planes.is_seam(&positions, &triangles, &internal, *f, *a, *b),
                    _ => true,
                }
            })
            .map(|((a, b), faces)| {
                let mut min_dot: f64 = 1.0;
                for (i, &fi) in faces.iter().enumerate() {
                    for &fj in faces.iter().skip(i + 1) {
                        min_dot = min_dot.min(dot(normal(fi), normal(fj)));
                    }
                }
                TopoEdge {
                    a,
                    b,
                    f0: faces.first().copied().unwrap_or(NONE),
                    f1: if faces.len() >= 2 { faces.get(1).copied().unwrap_or(NONE) } else { NONE },
                    min_dot,
                    non_manifold: faces.len() > 2,
                    others: faces.get(2..).map(<[u32]>::to_vec).unwrap_or_default(),
                }
            })
            .collect();
        edges.sort_by_key(|e| (e.a, e.b));
        Topology { positions, triangles, normals, internal, edges }
    }

    /// Position of a vertex.
    pub(crate) fn pos(&self, i: u32) -> V3 {
        self.positions.get(i as usize).copied().unwrap_or([0.0; 3])
    }

    /// Normal of a face.
    pub(crate) fn normal(&self, f: u32) -> V3 {
        self.normals.get(f as usize).copied().unwrap_or([0.0; 3])
    }
}

/// How the viewer looks at the scene, for front/back classification of faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Eye {
    /// Parallel projection looking along a direction.
    Parallel(V3),
    /// Perspective projection from an eye point.
    Perspective(V3),
}

impl Eye {
    /// Whether a face with normal `n` through point `p` faces the viewer. Edge-on faces count as back faces.
    pub(crate) fn front(&self, n: V3, p: V3) -> bool {
        let dir = match self {
            Eye::Parallel(d) => *d,
            Eye::Perspective(e) => sub(p, *e),
        };
        dot(n, dir) < -1e-9 * length(dir).max(1e-300)
    }
}

/// Edges of a topology that can be visible: creases sharper than `crease_cos` between faces that are not both
/// back-facing, silhouettes (a front and a back face) and open boundaries.
pub(crate) fn candidate_edges(t: &Topology, eye: &Eye, crease_cos: f64, out: &mut Vec<(V3, V3)>) {
    for e in &t.edges {
        let (a, b) = (t.pos(e.a), t.pos(e.b));
        let mid = lerp(a, b, 0.5);
        if e.f1 == NONE {
            out.push((a, b));
            continue;
        }
        let fr0 = eye.front(t.normal(e.f0), mid);
        let fr1 = eye.front(t.normal(e.f1), mid);
        if e.non_manifold {
            // Several faces meet: drawn when they crease and one of them faces the viewer, or when front and
            // back faces meet (a silhouette).
            let fronts: Vec<bool> =
                [e.f0, e.f1].iter().chain(&e.others).map(|&f| eye.front(t.normal(f), mid)).collect();
            let any_front = fronts.iter().any(|f| *f);
            let any_back = fronts.iter().any(|f| !*f);
            if (e.min_dot < crease_cos && any_front) || (any_front && any_back) {
                out.push((a, b));
            }
            continue;
        }
        if !fr0 && !fr1 {
            continue;
        }
        if fr0 != fr1 || e.min_dot < crease_cos {
            out.push((a, b));
        }
    }
}

type PlaneKey = (i32, i32, i32, i64);

/// Faces grouped by the plane they lie in (normal to 1e-3, offset to 0.1 mm).
struct Planes {
    buckets: HashMap<PlaneKey, Vec<u32>>,
    keys: Vec<(V3, f64)>,
}

fn plane_key(n: V3, d: f64) -> PlaneKey {
    (
        (n[0] * 1e3).round() as i32,
        (n[1] * 1e3).round() as i32,
        (n[2] * 1e3).round() as i32,
        (d * 1e4).round() as i64,
    )
}

impl Planes {
    fn new(pos: &[V3], tris: &[[u32; 3]], normals: &[V3]) -> Planes {
        let mut buckets: HashMap<PlaneKey, Vec<u32>> = HashMap::new();
        let mut keys = Vec::with_capacity(tris.len());
        for (i, t) in tris.iter().enumerate() {
            let n = normals.get(i).copied().unwrap_or([0.0; 3]);
            let d = dot(n, pos.get(t[0] as usize).copied().unwrap_or([0.0; 3]));
            buckets.entry(plane_key(n, d)).or_default().push(i as u32);
            keys.push((n, d));
        }
        Planes { buckets, keys }
    }

    /// Whether `q` lies on a face (other than `skip`) in the plane `(n, d)`, ignoring faces flagged in `exclude`.
    #[allow(clippy::too_many_arguments)]
    fn on_plane(
        &self,
        pos: &[V3],
        tris: &[[u32; 3]],
        n: V3,
        d: f64,
        q: V3,
        skip: u32,
        exclude: &[bool],
    ) -> bool {
        let p = |i: u32| pos.get(i as usize).copied().unwrap_or([0.0; 3]);
        let Some(candidates) = self.buckets.get(&plane_key(n, d)) else { return false };
        candidates.iter().any(|&j| {
            j != skip
                && !exclude.get(j as usize).copied().unwrap_or(false)
                && tris
                    .get(j as usize)
                    .is_some_and(|tj| point_in_triangle_3d(q, [p(tj[0]), p(tj[1]), p(tj[2])], n))
        })
    }

    /// Faces wholly covered by opposite faces in the same plane (internal boundaries between touching solids,
    /// such as two layers of a wall). A face counts as covered when its centroid and its corners (pulled 1 %
    /// towards the centroid) all lie on opposite faces; partly covered faces stay.
    fn internal_faces(&self, pos: &[V3], tris: &[[u32; 3]]) -> Vec<bool> {
        let p = |i: u32| pos.get(i as usize).copied().unwrap_or([0.0; 3]);
        let none = vec![false; tris.len()];
        let mut internal = vec![false; tris.len()];
        for (i, t) in tris.iter().enumerate() {
            let Some(&(n, d)) = self.keys.get(i) else { continue };
            let neg = scale(n, -1.0);
            if !self.buckets.contains_key(&plane_key(neg, -d)) {
                continue;
            }
            let c = scale(add(add(p(t[0]), p(t[1])), p(t[2])), 1.0 / 3.0);
            let covered = std::iter::once(c)
                .chain(t.iter().map(|&k| lerp(p(k), c, 0.01)))
                .all(|q| self.on_plane(pos, tris, neg, -d, q, u32::MAX, &none));
            if covered && let Some(slot) = internal.get_mut(i) {
                *slot = true;
            }
        }
        internal
    }

    /// Whether the single face `f` on edge `a–b` continues, in its own plane, on the other side of the edge.
    fn is_seam(&self, pos: &[V3], tris: &[[u32; 3]], internal: &[bool], f: u32, a: u32, b: u32) -> bool {
        let p = |i: u32| pos.get(i as usize).copied().unwrap_or([0.0; 3]);
        let (Some(t), Some(&(n, d))) = (tris.get(f as usize), self.keys.get(f as usize)) else {
            return false;
        };
        let Some(&c) = t.iter().find(|&&k| k != a && k != b) else { return false };
        let (pa, pb) = (p(a), p(b));
        let Some(mut w) = normalize(cross(n, sub(pb, pa))) else { return false };
        if dot(w, sub(p(c), pa)) > 0.0 {
            w = scale(w, -1.0);
        }
        let mid = lerp(pa, pb, 0.5);
        let q = add(mid, scale(w, 1e-5));
        self.on_plane(pos, tris, n, d, q, f, internal)
    }
}

/// Whether a point (assumed in the triangle's plane) lies inside the triangle, boundary included.
fn point_in_triangle_3d(q: V3, t: [V3; 3], n: V3) -> bool {
    // Project onto the plane of the two smallest normal components.
    let ax = if n[0].abs() >= n[1].abs() && n[0].abs() >= n[2].abs() {
        0
    } else if n[1].abs() >= n[2].abs() {
        1
    } else {
        2
    };
    let (u, v) = match ax {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    let g = |p: V3, i: usize| p.get(i).copied().unwrap_or(0.0);
    let (x0, y0) = (g(t[0], u), g(t[0], v));
    let (x1, y1) = (g(t[1], u) - x0, g(t[1], v) - y0);
    let (x2, y2) = (g(t[2], u) - x0, g(t[2], v) - y0);
    let (px, py) = (g(q, u) - x0, g(q, v) - y0);
    let det = x1 * y2 - x2 * y1;
    if det.abs() < 1e-18 {
        return false;
    }
    let a = (px * y2 - x2 * py) / det;
    let b = (x1 * py - px * y1) / det;
    let eps = 1e-9;
    a >= -eps && b >= -eps && a + b <= 1.0 + eps
}

/// Clips a planar polygon to the half-space `f(p) ≥ 0` where `f` is affine (Sutherland–Hodgman).
pub(crate) fn clip_polygon(poly: &[V3], f: impl Fn(&V3) -> f64) -> Vec<V3> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % n)) else { continue };
        let (fa, fb) = (f(&a), f(&b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push(lerp(a, b, t));
        }
    }
    out
}

/// Clips a segment to `f(p) ≥ 0` (affine `f`), narrowing a parameter range on `a → b`.
pub(crate) fn clip_segment(a: V3, b: V3, f: impl Fn(&V3) -> f64, range: (f64, f64)) -> Option<(f64, f64)> {
    let (fa, fb) = (f(&a), f(&b));
    let (mut t0, mut t1) = range;
    if fa < 0.0 && fb < 0.0 {
        return None;
    }
    if fa < 0.0 {
        t0 = t0.max(fa / (fa - fb));
    } else if fb < 0.0 {
        t1 = t1.min(fa / (fa - fb));
    }
    if t1 > t0 { Some((t0, t1)) } else { None }
}

fn canonical(a: V3, b: V3) -> (V3, V3) {
    let ka = (a[0].to_bits(), a[1].to_bits(), a[2].to_bits());
    let kb = (b[0].to_bits(), b[1].to_bits(), b[2].to_bits());
    if ka <= kb { (a, b) } else { (b, a) }
}

/// The loops where a mesh (in view coordinates) crosses the plane `p[2] = 0`, as `(x, y)` rings.
///
/// Vertices exactly on the plane count as lying behind it (symbolic perturbation), so faces lying in the plane
/// produce no loop and every crossing is classified consistently. Returns closed loops and any chains that could
/// not be closed (open meshes).
pub(crate) fn slice_loops(pos: &[V3], tris: &[[u32; 3]]) -> (Vec<Contour>, Vec<Vec<[f64; 2]>>) {
    let p = |i: u32| pos.get(i as usize).copied().unwrap_or([0.0; 3]);
    let mut segs: Vec<([f64; 2], [f64; 2])> = Vec::new();
    for t in tris {
        let v = [p(t[0]), p(t[1]), p(t[2])];
        let side = [v[0][2] >= 0.0, v[1][2] >= 0.0, v[2][2] >= 0.0];
        if side[0] == side[1] && side[1] == side[2] {
            continue;
        }
        let mut hits = Vec::with_capacity(2);
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (Some(&si), Some(&sj)) = (side.get(i), side.get(j)) else { continue };
            if si != sj {
                let (Some(&a), Some(&b)) = (v.get(i), v.get(j)) else { continue };
                let (a, b) = canonical(a, b);
                let t = a[2] / (a[2] - b[2]);
                let q = lerp(a, b, t);
                hits.push([q[0], q[1]]);
            }
        }
        if let [h0, h1] = hits[..] {
            segs.push((h0, h1));
        }
    }
    chain(segs)
}

fn qkey(p: [f64; 2]) -> (i64, i64) {
    ((p[0] * WELD).round() as i64, (p[1] * WELD).round() as i64)
}

/// Joins unordered segments into closed loops and open chains.
pub(crate) fn chain(segs: Vec<([f64; 2], [f64; 2])>) -> (Vec<Contour>, Vec<Vec<[f64; 2]>>) {
    let mut at: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        if qkey(*a) == qkey(*b) {
            continue;
        }
        at.entry(qkey(*a)).or_default().push(i);
        at.entry(qkey(*b)).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    let mut open: Vec<Vec<[f64; 2]>> = Vec::new();
    for start in 0..segs.len() {
        if used.get(start).copied().unwrap_or(true) {
            continue;
        }
        let Some(&(a, b)) = segs.get(start) else { continue };
        if qkey(a) == qkey(b) {
            continue;
        }
        if let Some(u) = used.get_mut(start) {
            *u = true;
        }
        let mut pts = vec![a, b];
        let start_key = qkey(a);
        let mut cur = b;
        let mut closed = false;
        loop {
            let k = qkey(cur);
            if k == start_key {
                closed = true;
                pts.pop();
                break;
            }
            let next =
                at.get(&k).and_then(|v| v.iter().copied().find(|&i| !used.get(i).copied().unwrap_or(true)));
            let Some(i) = next else { break };
            if let Some(u) = used.get_mut(i) {
                *u = true;
            }
            let Some(&(sa, sb)) = segs.get(i) else { break };
            cur = if qkey(sa) == k { sb } else { sa };
            pts.push(cur);
        }
        if closed && pts.len() >= 3 {
            loops.push(pts);
        } else if pts.len() >= 2 {
            open.push(pts);
        }
    }
    // Join open chains whose ends nearly meet (T-junctions in the source mesh).
    let tol = 1e-5;
    let near = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) <= tol;
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < open.len() {
            let (Some(first), Some(last)) =
                (open.get(i).and_then(|c| c.first().copied()), open.get(i).and_then(|c| c.last().copied()))
            else {
                i += 1;
                continue;
            };
            if near(first, last) && open.get(i).is_some_and(|c| c.len() >= 4) {
                let mut c = open.remove(i);
                c.pop();
                loops.push(c);
                changed = true;
                continue;
            }
            let mut joined = false;
            for j in 0..open.len() {
                if j == i {
                    continue;
                }
                let (Some(f2), Some(l2)) = (
                    open.get(j).and_then(|c| c.first().copied()),
                    open.get(j).and_then(|c| c.last().copied()),
                ) else {
                    continue;
                };
                let other = if near(last, f2) {
                    Some(false)
                } else if near(last, l2) {
                    Some(true)
                } else {
                    None
                };
                if let Some(reverse) = other {
                    let mut c = open.remove(j);
                    if reverse {
                        c.reverse();
                    }
                    let idx = if j < i { i - 1 } else { i };
                    if let Some(target) = open.get_mut(idx) {
                        target.extend(c.into_iter().skip(1));
                    }
                    joined = true;
                    break;
                }
            }
            if joined {
                changed = true;
            } else {
                i += 1;
            }
        }
    }
    (loops, open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poly;
    use kanavos_geom::Point3;
    use kanavos_solid::cuboid;

    fn mesh_arrays(m: &kanavos_solid::Mesh) -> (Vec<V3>, Vec<[u32; 3]>) {
        (m.positions.iter().map(|p| p.to_array()).collect(), m.triangles.clone())
    }

    #[test]
    fn stacked_layers_share_no_visible_edge() {
        use kanavos_geom::{Point2, Polygon2};
        let p = Polygon2::rectangle(Point2::new(-0.155, -0.155), Point2::new(12.155, 9.155));
        let a = kanavos_solid::extrude_polygon(&p, -0.06, 0.0).unwrap();
        let b = kanavos_solid::extrude_polygon(&p, -0.11, -0.06).unwrap();
        let (pa, ta) = mesh_arrays(&a);
        let (pb, tb) = mesh_arrays(&b);
        let topo = Topology::build([(pa.as_slice(), ta.as_slice()), (pb.as_slice(), tb.as_slice())]);
        // The two coincident caps at z = −0.06 are internal; no crease remains at that height.
        assert_eq!(topo.internal.iter().filter(|i| **i).count(), 4);
        assert!(
            topo.edges
                .iter()
                .filter(|e| e.min_dot < 0.9)
                .all(|e| (topo.pos(e.a)[2] + 0.06).abs() > 1e-9 || (topo.pos(e.b)[2] + 0.06).abs() > 1e-9)
        );
    }

    #[test]
    fn box_has_twelve_crease_edges() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 1.0, 1.0));
        let (p, t) = mesh_arrays(&m);
        let topo = Topology::build([(p.as_slice(), t.as_slice())]);
        assert_eq!(topo.positions.len(), 8);
        let creases = topo.edges.iter().filter(|e| e.min_dot < 0.5).count();
        assert_eq!(creases, 12);
        // Looking along +y only the front face faces the viewer; its 4 edges are silhouettes against the
        // edge-on side faces, and every other edge is culled.
        let mut out = Vec::new();
        candidate_edges(&topo, &Eye::Parallel([0.0, 1.0, 0.0]), 0.96, &mut out);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn touching_boxes_lose_their_shared_faces() {
        let a = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0));
        let b = cuboid(Point3::new(1.0, 0.0, 0.0), Point3::new(2.0, 1.0, 1.0));
        let (pa, ta) = mesh_arrays(&a);
        let (pb, tb) = mesh_arrays(&b);
        let topo = Topology::build([(pa.as_slice(), ta.as_slice()), (pb.as_slice(), tb.as_slice())]);
        assert_eq!(topo.internal.iter().filter(|i| **i).count(), 4);
        // The joint between the boxes is flat on every outer face, so only the edges of the 2×1×1 box remain
        // (its four long edges in two pieces each) and none lies in the joint plane x = 1.
        let creases: Vec<&TopoEdge> = topo.edges.iter().filter(|e| e.min_dot < 0.5).collect();
        assert_eq!(creases.len(), 16);
        assert!(
            creases
                .iter()
                .all(|e| (topo.pos(e.a)[0] - 1.0).abs() > 1e-9 || (topo.pos(e.b)[0] - 1.0).abs() > 1e-9)
        );
    }

    #[test]
    fn slicing_a_box_gives_a_rectangle() {
        let m = cuboid(Point3::new(0.0, 0.0, -1.0), Point3::new(2.0, 3.0, 1.0));
        let (p, t) = mesh_arrays(&m);
        let (loops, open) = slice_loops(&p, &t);
        assert!(open.is_empty());
        assert_eq!(loops.len(), 1);
        let shapes = poly::even_odd(&loops);
        assert!((poly::shape_area(&shapes[0]) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn polygon_clipping_halves_a_square() {
        let sq = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 2.0, 0.0], [0.0, 2.0, 0.0]];
        let c = clip_polygon(&sq, |p| p[0] - 1.0);
        assert_eq!(c.len(), 4);
        assert!(c.iter().all(|p| p[0] >= 1.0 - 1e-12));
    }
}

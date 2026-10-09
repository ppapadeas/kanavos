//! The 3D scene prepared for display, once per document revision: triangles subdivided for depth sorting, flat
//! shading per face from a fixed sky/ground and sun light, feature edges, and bounds per item for culling and
//! picking.

use std::collections::HashMap;

use egui::Color32;
use kanavos_model::{Category, Id};

use super::camera::{Aabb, V3, v3};

/// Most display triangles: beyond it, subdivision gets coarser.
pub const TRIANGLE_BUDGET: usize = 60_000;
/// Two faces meeting at more than this angle (degrees) make a feature edge.
pub const FEATURE_ANGLE_DEG: f64 = 25.0;

/// A display triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tri {
    /// Vertex indices into [`Prepared3d::positions`].
    pub v: [u32; 3],
    /// Item index.
    pub item: u32,
    /// Shaded colour (alpha < 255 for glass).
    pub color: Color32,
}

/// A feature edge segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    /// End points (indices into [`Prepared3d::positions`]).
    pub v: [u32; 2],
    /// Item index.
    pub item: u32,
}

/// One coloured mesh of an element.
#[derive(Debug, Clone, PartialEq)]
pub struct Item3d {
    /// The element.
    pub element: Id,
    /// Its storey.
    pub storey: Option<Id>,
    /// Its category.
    pub category: Category,
    /// Bounds.
    pub bounds: Aabb,
    /// Display triangles.
    pub tris: std::ops::Range<usize>,
    /// Feature edges.
    pub edges: std::ops::Range<usize>,
    /// Transparent (glass): drawn from both sides.
    pub transparent: bool,
    /// Base colour.
    pub color: kanavos_drawing::Rgba,
}

/// A scene prepared for display.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prepared3d {
    /// Vertex positions, metres.
    pub positions: Vec<V3>,
    /// Display triangles, grouped by item.
    pub tris: Vec<Tri>,
    /// Feature edges, grouped by item.
    pub edges: Vec<Edge>,
    /// Items.
    pub items: Vec<Item3d>,
    /// Bounds of everything.
    pub bounds: Option<Aabb>,
    /// Triangles before subdivision.
    pub source_triangles: usize,
}

/// An edge keyed by its quantised end points.
type EdgeKey = ((i64, i64, i64), (i64, i64, i64));
/// First face normal on an edge, its end vertices and how many faces share it.
type EdgeInfo = (V3, u32, u32, usize);

/// Unit vector of the sun (towards the light): from the south-west, high.
const SUN: V3 = v3(-0.35, -0.55, 0.76);

/// Flat shade of a base colour for a face normal: a soft sky/ground hemisphere plus a sun term.
pub fn shade(base: kanavos_drawing::Rgba, n: V3) -> Color32 {
    let up = n.z.clamp(-1.0, 1.0);
    // Sky light from above, darker bounce from the ground.
    let hemi = 0.62 + 0.18 * up;
    let sun = n.dot(SUN.normalized().unwrap_or(SUN)).max(0.0) * 0.38;
    let k = (hemi + sun).clamp(0.0, 1.15);
    let ch = |c: u8| (f64::from(c) * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(ch(base.r), ch(base.g), ch(base.b), base.a)
}

fn key(p: V3) -> (i64, i64, i64) {
    let q = |v: f64| (v * 10_000.0).round() as i64;
    (q(p.x), q(p.y), q(p.z))
}

/// Subdivides a triangle until no edge is longer than `max_len`, sharing midpoints through `mids`.
fn subdivide(
    positions: &mut Vec<V3>,
    mids: &mut HashMap<(u32, u32), u32>,
    t: [u32; 3],
    max_len: f64,
    depth: u32,
    out: &mut Vec<[u32; 3]>,
) {
    let p = |i: u32| positions.get(i as usize).copied().unwrap_or_default();
    let [a, b, c] = t;
    let (la, lb, lc) = ((p(b) - p(a)).length(), (p(c) - p(b)).length(), (p(a) - p(c)).length());
    let longest = la.max(lb).max(lc);
    if longest <= max_len || depth > 12 {
        out.push(t);
        return;
    }
    // Rotate so that the longest edge is (x, y).
    let (x, y, z) = if la >= lb && la >= lc {
        (a, b, c)
    } else if lb >= lc {
        (b, c, a)
    } else {
        (c, a, b)
    };
    let k = if x < y { (x, y) } else { (y, x) };
    let m = match mids.get(&k) {
        Some(&m) => m,
        None => {
            let mp = (p(x) + p(y)) * 0.5;
            positions.push(mp);
            let m = (positions.len() - 1) as u32;
            mids.insert(k, m);
            m
        }
    };
    subdivide(positions, mids, [x, m, z], max_len, depth + 1, out);
    subdivide(positions, mids, [m, y, z], max_len, depth + 1, out);
}

impl Prepared3d {
    /// Prepares a scene. Large faces are subdivided so that no triangle edge is longer than about a sixtieth of the
    /// scene (at least 0.5 m), which keeps back-to-front sorting of triangles reliable.
    pub fn new(scene: &kanavos_views::Scene) -> Prepared3d {
        let source_triangles: usize = scene.items.iter().map(|i| i.mesh.triangles.len()).sum();
        let bounds = Aabb::from_points(
            scene.items.iter().flat_map(|i| i.mesh.positions.iter().map(|p| v3(p.x, p.y, p.z))),
        );
        let diag = bounds.map(|b| b.radius() * 2.0).unwrap_or(10.0);
        let mut max_len = (diag / 60.0).max(0.5);
        let mut out = Prepared3d::default();
        // Coarsen until the budget fits (two attempts are enough in practice).
        for _ in 0..4 {
            out = Self::build(scene, max_len);
            if out.tris.len() <= TRIANGLE_BUDGET || out.tris.len() <= source_triangles {
                break;
            }
            max_len *= 2.0;
        }
        out.bounds = bounds;
        out.source_triangles = source_triangles;
        out
    }

    fn build(scene: &kanavos_views::Scene, max_len: f64) -> Prepared3d {
        let mut out = Prepared3d::default();
        for it in &scene.items {
            let base = out.positions.len() as u32;
            let pts: Vec<V3> = it.mesh.positions.iter().map(|p| v3(p.x, p.y, p.z)).collect();
            let Some(item_bounds) = Aabb::from_points(pts.iter().copied()) else { continue };
            out.positions.extend(pts.iter().copied());
            let n_pts = pts.len() as u32;
            let item_index = out.items.len() as u32;
            let tri_start = out.tris.len();
            let edge_start = out.edges.len();
            let transparent = it.color.a < 250;
            let mut mids: HashMap<(u32, u32), u32> = HashMap::new();
            // Edge adjacency on positions (meshes may repeat vertices per face).
            let mut adjacency: HashMap<EdgeKey, EdgeInfo> = HashMap::new();
            let mut feature: Vec<(u32, u32)> = Vec::new();
            for t in &it.mesh.triangles {
                if t.iter().any(|&i| i >= n_pts) {
                    continue;
                }
                let (Some(&a), Some(&b), Some(&c)) =
                    (pts.get(t[0] as usize), pts.get(t[1] as usize), pts.get(t[2] as usize))
                else {
                    continue;
                };
                let Some(n) = (b - a).cross(c - a).normalized() else { continue };
                let color = shade(it.color, n);
                let mut sub = Vec::new();
                subdivide(
                    &mut out.positions,
                    &mut mids,
                    [base + t[0], base + t[1], base + t[2]],
                    max_len,
                    0,
                    &mut sub,
                );
                out.tris.extend(sub.into_iter().map(|v| Tri { v, item: item_index, color }));
                for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    let (Some(&pi), Some(&pj)) = (pts.get(i as usize), pts.get(j as usize)) else { continue };
                    let (ki, kj) = (key(pi), key(pj));
                    let k = if ki < kj { (ki, kj) } else { (kj, ki) };
                    match adjacency.get_mut(&k) {
                        Some(e) => {
                            // Second (or later) face on this edge: a feature if the faces bend enough.
                            if e.3 == 1 && e.0.dot(n) < FEATURE_ANGLE_DEG.to_radians().cos() {
                                feature.push((e.1, e.2));
                            }
                            e.3 += 1;
                        }
                        None => {
                            adjacency.insert(k, (n, base + i, base + j, 1));
                        }
                    }
                }
            }
            // Boundary edges of open meshes are features too.
            for (_, (_, i, j, count)) in adjacency {
                if count == 1 {
                    feature.push((i, j));
                }
            }
            for (i, j) in feature {
                let (Some(&pi), Some(&pj)) = (out.positions.get(i as usize), out.positions.get(j as usize))
                else {
                    continue;
                };
                // Split long edges like the faces, so they sort with them.
                let pieces = ((pj - pi).length() / max_len).ceil().clamp(1.0, 256.0) as u32;
                let mut prev = i;
                for k in 1..=pieces {
                    let next = if k == pieces {
                        j
                    } else {
                        out.positions.push(pi + (pj - pi) * (f64::from(k) / f64::from(pieces)));
                        (out.positions.len() - 1) as u32
                    };
                    out.edges.push(Edge { v: [prev, next], item: item_index });
                    prev = next;
                }
            }
            out.items.push(Item3d {
                element: it.element,
                storey: it.storey,
                category: it.category,
                bounds: item_bounds,
                tris: tri_start..out.tris.len(),
                edges: edge_start..out.edges.len(),
                transparent,
                color: it.color,
            });
        }
        out
    }
}

/// The face of a section box that cuts an item: triangles on the box plane facing out of the box, and the outline.
#[derive(Debug, Clone, PartialEq)]
pub struct Cap {
    /// Item index.
    pub item: usize,
    /// Triangles in world coordinates, counter-clockwise seen from outside the box.
    pub tris: Vec<[V3; 3]>,
    /// Outline segments.
    pub outline: Vec<[V3; 2]>,
    /// Colour of the cut face.
    pub color: Color32,
}

/// Colour of cut faces: the material darkened towards a neutral grey, so cuts read as solid.
pub fn cut_color(base: kanavos_drawing::Rgba) -> Color32 {
    let m = |c: u8| (f64::from(c) * 0.55 + 70.0 * 0.45).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(base.r), m(base.g), m(base.b))
}

fn axis_of(p: V3, axis: usize) -> f64 {
    match axis {
        0 => p.x,
        1 => p.y,
        _ => p.z,
    }
}

/// The plane coordinate axes `(u, v)` for a box face with normal `±axis`, such that `u × v` is the outward normal.
fn plane_axes(axis: usize, outward: f64) -> (usize, usize) {
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    if outward > 0.0 { (u, v) } else { (v, u) }
}

fn compose(axis: usize, value: f64, (u, v): (usize, usize), a: f64, b: f64) -> V3 {
    let mut c = [0.0; 3];
    for (i, x) in [(axis, value), (u, a), (v, b)] {
        if let Some(slot) = c.get_mut(i) {
            *slot = x;
        }
    }
    v3(c[0], c[1], c[2])
}

/// Section caps of the items a box cuts (closed, opaque items only). Each cut is the cross-section of the item's
/// surface with the box face, chained into loops, clipped to the box and triangulated.
pub fn section_caps(scene: &Prepared3d, b: &Aabb) -> Vec<Cap> {
    let mut out = Vec::new();
    for (ii, it) in scene.items.iter().enumerate() {
        if it.transparent || !b.intersects(&it.bounds) || b.contains_box(&it.bounds) {
            continue;
        }
        let mut cap = Cap { item: ii, tris: Vec::new(), outline: Vec::new(), color: cut_color(it.color) };
        for axis in 0..3 {
            for (value, outward) in [(axis_of(b.min, axis), -1.0), (axis_of(b.max, axis), 1.0)] {
                let (lo, hi) = (axis_of(it.bounds.min, axis), axis_of(it.bounds.max, axis));
                if !(value > lo && value < hi) {
                    continue;
                }
                let n = match axis {
                    0 => v3(outward, 0.0, 0.0),
                    1 => v3(0.0, outward, 0.0),
                    _ => v3(0.0, 0.0, outward),
                };
                let segs = cross_section(scene, it, axis, value, n);
                let (u, v) = plane_axes(axis, outward);
                let (umin, umax) = (axis_of(b.min, u), axis_of(b.max, u));
                let (vmin, vmax) = (axis_of(b.min, v), axis_of(b.max, v));
                for lp in chain(&segs) {
                    let mut poly: Vec<(f64, f64)> =
                        lp.iter().map(|p| (axis_of(*p, u), axis_of(*p, v))).collect();
                    poly = clip2(&poly, |q| q.0 - umin);
                    poly = clip2(&poly, |q| umax - q.0);
                    poly = clip2(&poly, |q| q.1 - vmin);
                    poly = clip2(&poly, |q| vmax - q.1);
                    if poly.len() < 3 {
                        continue;
                    }
                    let shape = kanavos_geom::PolygonWithHoles::from(kanavos_geom::Polygon2::new(
                        poly.iter().map(|q| kanavos_geom::Point2::new(q.0, q.1)).collect(),
                    ));
                    let Ok((pts, tris)) = kanavos_solid::triangulate(&shape) else { continue };
                    let at = |i: u32| {
                        pts.get(i as usize).map(|p| {
                            let [x, y] = p.xy();
                            compose(axis, value, (u, v), x, y)
                        })
                    };
                    for t in tris {
                        if let (Some(a), Some(b2), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
                            cap.tris.push([a, b2, c]);
                        }
                    }
                    let k = poly.len();
                    for i in 0..k {
                        if let (Some(p), Some(q)) = (poly.get(i), poly.get((i + 1) % k)) {
                            cap.outline.push([
                                compose(axis, value, (u, v), p.0, p.1),
                                compose(axis, value, (u, v), q.0, q.1),
                            ]);
                        }
                    }
                }
            }
        }
        if !cap.tris.is_empty() {
            out.push(cap);
        }
    }
    out
}

/// Intersection segments of an item's triangles with the plane `axis = value`, oriented so that loops run
/// counter-clockwise seen from the outward normal `n` (the solid on the left).
fn cross_section(scene: &Prepared3d, it: &Item3d, axis: usize, value: f64, n: V3) -> Vec<[V3; 2]> {
    let mut segs = Vec::new();
    for t in scene.tris.get(it.tris.clone()).unwrap_or(&[]) {
        let [a, b, c] = t.v.map(|i| scene.positions.get(i as usize).copied().unwrap_or_default());
        let Some(normal) = (b - a).cross(c - a).normalized() else { continue };
        let d = [a, b, c].map(|p| axis_of(p, axis) - value);
        let mut hits: Vec<V3> = Vec::with_capacity(2);
        for (p, q, dp, dq) in [(a, b, d[0], d[1]), (b, c, d[1], d[2]), (c, a, d[2], d[0])] {
            if (dp >= 0.0) != (dq >= 0.0) {
                let s = dp / (dp - dq);
                hits.push(p + (q - p) * s);
            }
        }
        if let [p, q] = hits.as_slice() {
            let dir = n.cross(normal);
            if (*q - *p).dot(dir) >= 0.0 { segs.push([*p, *q]) } else { segs.push([*q, *p]) }
        }
    }
    segs
}

/// Chains oriented segments into closed loops (by matching end points).
fn chain(segs: &[[V3; 2]]) -> Vec<Vec<V3>> {
    let mut by_start: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        by_start.entry(key(s[0])).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for start in 0..segs.len() {
        if used.get(start).copied().unwrap_or(true) {
            continue;
        }
        let mut lp = Vec::new();
        let mut cur = start;
        loop {
            if let Some(u) = used.get_mut(cur) {
                *u = true;
            }
            let Some(s) = segs.get(cur) else { break };
            lp.push(s[0]);
            let next = by_start
                .get(&key(s[1]))
                .and_then(|c| c.iter().copied().find(|&j| !used.get(j).copied().unwrap_or(true)));
            match next {
                Some(j) => cur = j,
                None => break,
            }
            if lp.len() > segs.len() {
                break;
            }
        }
        if lp.len() >= 3 {
            loops.push(lp);
        }
    }
    loops
}

/// Sutherland–Hodgman clipping of a 2D polygon against `f(q) >= 0`.
fn clip2(poly: &[(f64, f64)], f: impl Fn(&(f64, f64)) -> f64) -> Vec<(f64, f64)> {
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
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Point3;

    fn cube_scene(size: f64) -> kanavos_views::Scene {
        kanavos_views::Scene {
            items: vec![kanavos_views::SceneItem {
                element: Id(1),
                category: Category::Wall,
                storey: None,
                material: None,
                mesh: kanavos_solid::cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(size, size, size)),
                color: kanavos_drawing::Rgba::rgb(200, 200, 200),
            }],
        }
    }

    #[test]
    fn a_cube_has_twelve_feature_edges_and_no_diagonals() {
        let p = Prepared3d::new(&cube_scene(0.3));
        assert_eq!(p.tris.len(), 12);
        assert_eq!(p.edges.len(), 12);
        let item = p.items.first().unwrap();
        assert_eq!(item.tris, 0..12);
    }

    #[test]
    fn large_faces_are_subdivided_without_losing_area() {
        let s = cube_scene(30.0);
        let p = Prepared3d::new(&s);
        assert!(p.tris.len() > 12);
        let area: f64 = p
            .tris
            .iter()
            .map(|t| {
                let [a, b, c] = t.v.map(|i| p.positions[i as usize]);
                (b - a).cross(c - a).length() / 2.0
            })
            .sum();
        assert!((area - 6.0 * 900.0).abs() < 1e-6, "{area}");
        // Every display triangle edge is short.
        let max_len = (p.bounds.unwrap().radius() * 2.0 / 60.0).max(0.5);
        for t in &p.tris {
            let [a, b, c] = t.v.map(|i| p.positions[i as usize]);
            for l in [(b - a).length(), (c - b).length(), (a - c).length()] {
                assert!(l <= max_len * 2.0 + 1e-9);
            }
        }
    }

    #[test]
    fn a_box_cut_through_a_cube_gets_one_square_cap_facing_out() {
        let p = Prepared3d::new(&cube_scene(2.0));
        let b = Aabb { min: v3(-1.0, -1.0, -1.0), max: v3(3.0, 3.0, 1.0) };
        let caps = section_caps(&p, &b);
        assert_eq!(caps.len(), 1);
        let cap = caps.first().unwrap();
        let area: f64 = cap.tris.iter().map(|[a, b, c]| (*b - *a).cross(*c - *a).length() / 2.0).sum();
        assert!((area - 4.0).abs() < 1e-6, "{area}");
        // Facing up (out of the box through its top face).
        for [a, b, c] in &cap.tris {
            assert!((*b - *a).cross(*c - *a).z > 0.0);
            assert!((a.z - 1.0).abs() < 1e-9);
        }
        assert!(cap.outline.len() >= 4);
    }

    #[test]
    fn faces_towards_the_sun_are_lighter() {
        let base = kanavos_drawing::Rgba::rgb(200, 200, 200);
        let top = shade(base, v3(0.0, 0.0, 1.0));
        let north = shade(base, v3(0.0, 1.0, 0.0));
        let bottom = shade(base, v3(0.0, 0.0, -1.0));
        assert!(top.r() > north.r() && north.r() > bottom.r());
    }
}

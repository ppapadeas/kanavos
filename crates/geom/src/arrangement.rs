//! Faces of a planar arrangement of segments: the closed regions that a set of lines divides the plane into.
//!
//! Segments are split at every crossing, touching point and overlap end (within tolerance), joined into a planar
//! graph, and the faces are traced with a half-edge walk: at each vertex the walk turns to the edge immediately
//! clockwise from the one it arrived along, which keeps the face on its left (de Berg et al., "Computational
//! Geometry", 3rd ed., §2.2, doubly connected edge lists). Counter-clockwise cycles are bounded faces; clockwise
//! cycles bound connected components and become holes of the smallest face around them. Dangling segments do not
//! close anything and only add zero-area spikes, which are removed.

use std::collections::HashMap;

use kanavos_units::Tolerance;

use crate::d2::{Point2, Segment2};
use crate::intersect::{SegmentIntersection, segment_intersection};
use crate::polygon::{Polygon2, PolygonWithHoles};

struct Points {
    eps: f64,
    pts: Vec<Point2>,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl Points {
    fn id(&mut self, p: Point2) -> usize {
        let cell = |v: f64| (v / self.eps).floor() as i64;
        let (cx, cy) = (cell(p.x.as_m()), cell(p.y.as_m()));
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.grid.get(&(cx + dx, cy + dy)) {
                    for &i in list {
                        if self.pts.get(i).is_some_and(|q| q.distance(p).as_m() <= self.eps) {
                            return i;
                        }
                    }
                }
            }
        }
        self.pts.push(p);
        let i = self.pts.len() - 1;
        self.grid.entry((cx, cy)).or_default().push(i);
        i
    }
}

/// The bounded faces of the arrangement of `segments`, as polygons with holes (outer rings counter-clockwise).
///
/// Tolerance: points closer than `tol.length` are merged, and crossings are found within it. Cost O(n²) in the
/// number of segments. Failure modes: none; degenerate segments are ignored and an arrangement without cycles
/// has no faces.
pub fn arrangement_faces(segments: &[Segment2], tol: &Tolerance) -> Vec<PolygonWithHoles> {
    let eps = tol.length.as_m().max(1e-12);
    let segs: Vec<&Segment2> = segments
        .iter()
        .filter(|s| s.length().as_m() > eps && s.start.is_finite() && s.end.is_finite())
        .collect();
    // Split parameters per segment.
    let mut cuts: Vec<Vec<f64>> = segs.iter().map(|_| vec![0.0, 1.0]).collect();
    for i in 0..segs.len() {
        for j in i + 1..segs.len() {
            let (Some(a), Some(b)) = (segs.get(i), segs.get(j)) else { continue };
            match segment_intersection(a, b, tol) {
                SegmentIntersection::Point { t, u, .. } => {
                    if let Some(c) = cuts.get_mut(i) {
                        c.push(t);
                    }
                    if let Some(c) = cuts.get_mut(j) {
                        c.push(u);
                    }
                }
                SegmentIntersection::Overlap { t, u, .. } => {
                    if let Some(c) = cuts.get_mut(i) {
                        c.extend([t.0, t.1]);
                    }
                    if let Some(c) = cuts.get_mut(j) {
                        c.extend([u.0, u.1]);
                    }
                }
                SegmentIntersection::None => {}
            }
        }
    }
    let mut points = Points { eps, pts: Vec::new(), grid: HashMap::new() };
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (s, c) in segs.iter().zip(cuts.iter_mut()) {
        c.sort_by(f64::total_cmp);
        let ids: Vec<usize> = c.iter().map(|&t| points.id(s.at(t.clamp(0.0, 1.0)))).collect();
        for w in ids.windows(2) {
            if let [a, b] = w
                && a != b
            {
                edges.push(((*a).min(*b), (*a).max(*b)));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    // Half-edges: 2k is a→b, 2k+1 is b→a.
    let pt = |i: usize| points.pts.get(i).copied().unwrap_or(Point2::ORIGIN);
    let mut outgoing: Vec<Vec<(f64, usize)>> = vec![Vec::new(); points.pts.len()];
    let half = |h: usize| -> (usize, usize) {
        let (a, b) = edges.get(h / 2).copied().unwrap_or((0, 0));
        if h.is_multiple_of(2) { (a, b) } else { (b, a) }
    };
    for h in 0..edges.len() * 2 {
        let (from, to) = half(h);
        let angle = (pt(to) - pt(from)).angle().as_rad();
        if let Some(list) = outgoing.get_mut(from) {
            list.push((angle, h));
        }
    }
    for list in &mut outgoing {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    let next = |h: usize| -> Option<usize> {
        let (_, v) = half(h);
        let twin = h ^ 1;
        let list = outgoing.get(v)?;
        let k = list.iter().position(|&(_, e)| e == twin)?;
        // The edge just before the twin in counter-clockwise order, i.e. the next clockwise.
        let prev = if k == 0 { list.len() - 1 } else { k - 1 };
        list.get(prev).map(|&(_, e)| e)
    };
    let mut used = vec![false; edges.len() * 2];
    let mut faces: Vec<Polygon2> = Vec::new();
    let mut boundaries: Vec<Polygon2> = Vec::new();
    for start in 0..edges.len() * 2 {
        if used.get(start).copied().unwrap_or(true) {
            continue;
        }
        let mut ring = Vec::new();
        let mut h = start;
        loop {
            if let Some(u) = used.get_mut(h) {
                *u = true;
            }
            ring.push(pt(half(h).0));
            match next(h) {
                Some(n) if n != start && !used.get(n).copied().unwrap_or(true) => h = n,
                _ => break,
            }
            if ring.len() > edges.len() * 2 + 1 {
                break;
            }
        }
        let poly = remove_spikes(Polygon2::new(ring), eps);
        let a = poly.signed_area();
        if a > eps * eps {
            faces.push(poly);
        } else if a < -eps * eps {
            boundaries.push(poly);
        }
    }
    let mut out: Vec<PolygonWithHoles> = faces.into_iter().map(Into::into).collect();
    for b in boundaries {
        let Some(&probe) = b.vertices.first() else { continue };
        // Probe slightly inside the hole's outside: use a vertex; the smallest face strictly containing the
        // component (not one of its own faces) is the one whose area exceeds the component's.
        let comp_area = b.area().as_m2();
        let target = out
            .iter_mut()
            .filter(|f| f.outer.area().as_m2() > comp_area + eps * eps && f.outer.contains(probe, tol))
            .min_by(|x, y| x.outer.area().as_m2().total_cmp(&y.outer.area().as_m2()));
        if let Some(f) = target {
            f.holes.push(b);
        }
    }
    out
}

/// Removes back-and-forth spikes (a vertex whose neighbours coincide) left by dangling edges.
fn remove_spikes(p: Polygon2, eps: f64) -> Polygon2 {
    let mut v = p.vertices;
    let mut changed = true;
    while changed && v.len() >= 3 {
        changed = false;
        let n = v.len();
        for i in 0..n {
            let (Some(&a), Some(&c)) = (v.get((i + n - 1) % n), v.get((i + 1) % n)) else { continue };
            if a.distance(c).as_m() <= eps {
                // Remove the tip and one copy of the base.
                let tip = i;
                let base = (i + 1) % n;
                let (first, second) = if tip > base { (tip, base) } else { (base, tip) };
                v.remove(first);
                v.remove(second);
                changed = true;
                break;
            }
        }
    }

    Polygon2::new(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: (f64, f64), b: (f64, f64)) -> Segment2 {
        Segment2::new(Point2::new(a.0, a.1), Point2::new(b.0, b.1))
    }

    #[test]
    fn grid_of_lines_gives_cells() {
        // Two horizontal and two vertical lines overshooting each other: one closed cell.
        let s = [
            seg((0.0, 1.0), (5.0, 1.0)),
            seg((0.0, 3.0), (5.0, 3.0)),
            seg((1.0, 0.0), (1.0, 4.0)),
            seg((4.0, 0.0), (4.0, 4.0)),
        ];
        let f = arrangement_faces(&s, &Tolerance::default());
        assert_eq!(f.len(), 1);
        assert!((f[0].area().as_m2() - 6.0).abs() < 1e-9);
        // A rectangle split by a middle line and a dangling stub.
        let s = [
            seg((0.0, 0.0), (4.0, 0.0)),
            seg((4.0, 0.0), (4.0, 2.0)),
            seg((4.0, 2.0), (0.0, 2.0)),
            seg((0.0, 2.0), (0.0, 0.0)),
            seg((2.0, 0.0), (2.0, 2.0)),
            seg((3.0, 1.0), (3.5, 1.0)),
        ];
        let f = arrangement_faces(&s, &Tolerance::default());
        assert_eq!(f.len(), 2);
        let total: f64 = f.iter().map(|p| p.area().as_m2()).sum();
        assert!((total - 8.0).abs() < 1e-9);
    }

    #[test]
    fn island_becomes_a_hole() {
        let mut s = vec![];
        let outer = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let inner = [(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)];
        for r in [outer, inner] {
            for i in 0..4 {
                s.push(seg(r[i], r[(i + 1) % 4]));
            }
        }
        let f = arrangement_faces(&s, &Tolerance::default());
        assert_eq!(f.len(), 2);
        let big = f.iter().find(|p| p.outer.area().as_m2() > 50.0).unwrap();
        assert_eq!(big.holes.len(), 1);
        assert!((big.area().as_m2() - 96.0).abs() < 1e-9);
    }
}

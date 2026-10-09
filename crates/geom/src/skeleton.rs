//! Weighted straight skeleton of a simple polygon, by wavefront propagation.
//!
//! Every edge of the polygon moves inwards, parallel to itself, at its own speed (its weight); a speed of zero
//! keeps the edge in place. The vertices of the shrinking polygon (the wavefront) move along the bisectors of
//! their two edges, and the wavefront changes topology at two kinds of events (O. Aichholzer, F. Aurenhammer,
//! D. Alberts, B. Gärtner, "A Novel Type of Skeleton for Polygons", J. UCS 1 (1995); P. Felkel, Š. Obdržálek,
//! "Straight Skeleton Implementation", SCCG 1998; weights as in D. Eppstein, J. Erickson, "Raising Roofs,
//! Crashing Cycles, and Playing Pool", DCG 22 (1999)):
//!
//! - an **edge event**, when an edge shrinks to zero length and its neighbours become adjacent;
//! - a **split event**, when a reflex vertex runs into an edge on the other side and splits the wavefront.
//!
//! The region an edge sweeps before it disappears is its **face**. With speeds `cot(pitch)` the time at which
//! the wavefront reaches a point is the height of a roof with those pitches, and the faces are its roof planes;
//! speed zero gives a gable.
//!
//! This implementation finds events by brute force (O(n²) per event, O(n³) overall), which is fast for building
//! outlines (tens of vertices). Degenerate events (several at the same time and place, which are the rule in
//! orthogonal buildings) are handled by processing one event at a time and then cleaning up the wavefront at
//! that instant: zero-length edges are merged, zero-width spikes ("needles") between opposite edges are cut off,
//! and rings with no area are removed.
//!
//! Limits: the input must be a simple polygon without holes. Consecutive collinear edges must have the same
//! speed. At least one edge must have a positive speed. The result is checked: the faces must tile the
//! polygon, otherwise [`GeomError::Invalid`] is returned.

use crate::GeomError;
use crate::boolean::union_all;
use crate::d2::{Point2, Vec2};
use crate::polygon::{Polygon2, PolygonWithHoles};
use crate::predicates::orient2d;

/// The face swept by one edge of the polygon (or by several collinear edges with the same speed).
#[derive(Debug, Clone, PartialEq)]
pub struct SkeletonFace {
    /// Input edges this face rises from (edge `i` runs from vertex `i` to vertex `i + 1`). Usually one.
    pub edges: Vec<usize>,
    /// Inward unit normal of the edge line.
    pub normal: Vec2,
    /// Offset of the edge line at time zero: points `p` on it satisfy `normal · p = offset`.
    pub offset: f64,
    /// Speed of the edge (distance per unit time).
    pub speed: f64,
    /// Region swept by the edge. Empty for zero-speed edges.
    pub region: Vec<PolygonWithHoles>,
}

impl SkeletonFace {
    /// Time at which this face's wavefront reaches `p` (the roof height for unit-time = unit-height speeds).
    /// `None` for zero-speed faces.
    pub fn time_at(&self, p: Point2) -> Option<f64> {
        if self.speed > 0.0 { Some((self.normal.dot(p.to_vec()) - self.offset) / self.speed) } else { None }
    }
}

/// A trajectory of a wavefront vertex: an arc of the skeleton between two faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonArc {
    /// Start point.
    pub from: Point2,
    /// End point.
    pub to: Point2,
    /// Time at the start.
    pub t_from: f64,
    /// Time at the end.
    pub t_to: f64,
    /// Face on the left of the arc (index into [`Skeleton::faces`]).
    pub left: usize,
    /// Face on the right of the arc.
    pub right: usize,
}

/// The straight skeleton of a polygon.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skeleton {
    /// Faces, one per (merged) input edge.
    pub faces: Vec<SkeletonFace>,
    /// Vertex trajectories with non-zero length.
    pub arcs: Vec<SkeletonArc>,
    /// Time at which the last part of the wavefront vanished.
    pub max_time: f64,
}

impl Skeleton {
    /// The face that rises from input edge `edge`.
    pub fn face_of_edge(&self, edge: usize) -> Option<&SkeletonFace> {
        self.faces.iter().find(|f| f.edges.contains(&edge))
    }
}

#[derive(Debug, Clone, Copy)]
struct Line {
    n: Vec2,
    d: Vec2,
    c: f64,
    s: f64,
    face: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Motion {
    Moving(Vec2),
    /// Between two opposite edges whose lines coincide: the tip of a zero-width spike.
    Needle,
    /// Between two parallel edges with different speeds: needs a connecting zero-speed edge.
    Step,
}

#[derive(Debug, Clone, Copy)]
struct Vert {
    p: Vec2,
    t0: f64,
    motion: Motion,
    prev: usize,
    next: usize,
    ein: usize,
    eout: usize,
    alive: bool,
}

impl Vert {
    fn at(&self, t: f64) -> Vec2 {
        match self.motion {
            Motion::Moving(v) => self.p + v * (t - self.t0),
            Motion::Needle | Motion::Step => self.p,
        }
    }
    fn vel(&self) -> Vec2 {
        match self.motion {
            Motion::Moving(v) => v,
            Motion::Needle | Motion::Step => Vec2::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Event {
    Edge { u: usize, time: f64 },
    Split { v: usize, j: usize, time: f64 },
}

impl Event {
    fn time(&self) -> f64 {
        match self {
            Event::Edge { time, .. } | Event::Split { time, .. } => *time,
        }
    }
}

struct State {
    lines: Vec<Line>,
    verts: Vec<Vert>,
    quads: Vec<Vec<Vec<Point2>>>,
    arcs: Vec<SkeletonArc>,
    eps: f64,
}

fn pt(v: Vec2) -> Point2 {
    Point2::new(v.x, v.y)
}

impl State {
    fn line(&self, i: usize) -> Result<Line, GeomError> {
        self.lines.get(i).copied().ok_or_else(|| GeomError::Invalid("skeleton: bad edge index".into()))
    }
    fn vert(&self, i: usize) -> Result<Vert, GeomError> {
        self.verts.get(i).copied().ok_or_else(|| GeomError::Invalid("skeleton: bad vertex index".into()))
    }
    fn vert_mut(&mut self, i: usize) -> Result<&mut Vert, GeomError> {
        self.verts.get_mut(i).ok_or_else(|| GeomError::Invalid("skeleton: bad vertex index".into()))
    }

    fn motion(&self, a: usize, b: usize) -> Result<Motion, GeomError> {
        let la = self.line(a)?;
        let lb = self.line(b)?;
        let det = la.n.cross(lb.n);
        if det.abs() > 1e-9 {
            let v = Vec2::new((la.s * lb.n.y - lb.s * la.n.y) / det, (la.n.x * lb.s - lb.n.x * la.s) / det);
            return Ok(Motion::Moving(v));
        }
        if la.n.dot(lb.n) > 0.0 {
            if (la.s - lb.s).abs() <= 1e-9 * (1.0 + la.s.abs()) {
                return Ok(Motion::Moving(la.n * la.s));
            }
            return Ok(Motion::Step);
        }
        Ok(Motion::Needle)
    }

    /// Creates the vertex between lines `a` and `b` at time `t` (near `hint`) and links it between `prev` and
    /// `next` (the far ends of `a` and `b`).
    ///
    /// When `a` and `b` are parallel with different speeds (they became collinear at this instant), the faster
    /// edge overruns the slower one: the slower edge is removed and the faster edge is joined to the slower
    /// edge's other neighbour.
    fn insert_vert(
        &mut self,
        hint: Vec2,
        t: f64,
        a: usize,
        b: usize,
        prev: usize,
        next: usize,
    ) -> Result<(), GeomError> {
        if self.motion(a, b)? == Motion::Step {
            let la = self.line(a)?;
            let lb = self.line(b)?;
            if la.s > lb.s {
                // Remove `b`: join `a` to the edge after `next`.
                let vn = self.vert(next)?;
                if vn.next == prev || next == prev {
                    return self.collapse_pair(prev, next, t);
                }
                let after = vn.next;
                self.emit_edge(next, after, t)?;
                let at = vn.at(t);
                self.kill(next, t)?;
                return self.insert_vert(at, t, a, vn.eout, prev, after);
            }
            // Remove `a`: join the edge before `prev` to `b`.
            let vp = self.vert(prev)?;
            if vp.prev == next || next == prev {
                return self.collapse_pair(prev, next, t);
            }
            let before = vp.prev;
            self.emit_edge(before, prev, t)?;
            let at = vp.at(t);
            self.kill(prev, t)?;
            return self.insert_vert(at, t, vp.ein, b, before, next);
        }
        let m = self.push_vert(hint, t, a, b)?;
        self.link(prev, m)?;
        self.link(m, next)?;
        Ok(())
    }

    /// Ends a ring that has shrunk to the two vertices `u` and `w`.
    fn collapse_pair(&mut self, u: usize, w: usize, t: f64) -> Result<(), GeomError> {
        if self.vert(u)?.alive {
            self.kill(u, t)?;
        }
        if w != u && self.vert(w)?.alive {
            self.kill(w, t)?;
        }
        Ok(())
    }

    /// Adds the zero-speed line through `at` that connects parallel lines `a` and `b` of different speeds.
    fn step_line(&mut self, a: usize, b: usize, at: Vec2) -> Result<usize, GeomError> {
        let la = self.line(a)?;
        let lb = self.line(b)?;
        // The faster edge runs ahead; the step goes back out (or forward in) to the slower one.
        let d = if la.s > lb.s { -la.n } else { la.n };
        let n = d.perp();
        self.lines.push(Line { n, d, c: n.dot(at), s: 0.0, face: usize::MAX });
        Ok(self.lines.len() - 1)
    }

    /// Creates an unlinked vertex between lines `a` and `b`.
    fn push_vert(&mut self, hint: Vec2, t: f64, a: usize, b: usize) -> Result<usize, GeomError> {
        let motion = self.motion(a, b)?;
        if motion == Motion::Step {
            return Err(GeomError::Invalid("skeleton: parallel edges need a step".into()));
        }
        let la = self.line(a)?;
        let lb = self.line(b)?;
        // Snap onto both lines when they are well conditioned.
        let det = la.n.cross(lb.n);
        let p = if det.abs() > 0.05 {
            let ra = la.c + la.s * t;
            let rb = lb.c + lb.s * t;
            Vec2::new((ra * lb.n.y - rb * la.n.y) / det, (la.n.x * rb - lb.n.x * ra) / det)
        } else {
            hint
        };
        self.verts.push(Vert {
            p,
            t0: t,
            motion,
            prev: usize::MAX,
            next: usize::MAX,
            ein: a,
            eout: b,
            alive: true,
        });
        Ok(self.verts.len() - 1)
    }

    fn link(&mut self, a: usize, b: usize) -> Result<(), GeomError> {
        self.vert_mut(a)?.next = b;
        self.vert_mut(b)?.prev = a;
        Ok(())
    }

    /// Records the quad swept by ring edge `u → w` up to time `t`.
    fn emit_edge(&mut self, u: usize, w: usize, t: f64) -> Result<(), GeomError> {
        let vu = self.vert(u)?;
        let vw = self.vert(w)?;
        let face = self.line(vu.eout)?.face;
        let tc = vu.t0.max(vw.t0);
        let quad = vec![pt(vu.at(tc)), pt(vw.at(tc)), pt(vw.at(t)), pt(vu.at(t))];
        let poly = Polygon2::new(quad);
        if poly.signed_area().abs() > self.eps * self.eps
            && let Some(q) = self.quads.get_mut(face)
        {
            q.push(poly.to_ccw().vertices);
        }
        Ok(())
    }

    fn kill(&mut self, u: usize, t: f64) -> Result<(), GeomError> {
        let v = self.vert(u)?;
        let a = v.at(v.t0);
        let b = v.at(t);
        let left = self.line(v.eout)?.face;
        let right = self.line(v.ein)?.face;
        if (b - a).length() > self.eps && left != usize::MAX && right != usize::MAX {
            self.arcs.push(SkeletonArc { from: pt(a), to: pt(b), t_from: v.t0, t_to: t, left, right });
        }
        self.vert_mut(u)?.alive = false;
        Ok(())
    }

    fn rings(&self) -> Result<Vec<Vec<usize>>, GeomError> {
        let mut seen = vec![false; self.verts.len()];
        let mut out = Vec::new();
        for (i, v) in self.verts.iter().enumerate() {
            if !v.alive || seen.get(i).copied().unwrap_or(true) {
                continue;
            }
            let mut ring = Vec::new();
            let mut k = i;
            loop {
                if let Some(s) = seen.get_mut(k) {
                    *s = true;
                }
                ring.push(k);
                k = self.vert(k)?.next;
                if k == i {
                    break;
                }
                if ring.len() > self.verts.len() {
                    return Err(GeomError::Invalid("skeleton: broken wavefront ring".into()));
                }
            }
            out.push(ring);
        }
        Ok(out)
    }

    fn ring_area_and_perimeter(&self, ring: &[usize], t: f64) -> Result<(f64, f64), GeomError> {
        let n = ring.len();
        let mut area = 0.0;
        let mut per = 0.0;
        for i in 0..n {
            let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
            let pa = self.vert(a)?.at(t);
            let pb = self.vert(b)?.at(t);
            area += pa.cross(pb);
            per += (pb - pa).length();
        }
        Ok((area / 2.0, per))
    }

    /// Cleans the wavefront at time `t`; returns whether anything changed.
    fn cleanup_once(&mut self, t: f64) -> Result<bool, GeomError> {
        let eps = self.eps;
        // Rings without area.
        for ring in self.rings()? {
            let (area, per) = self.ring_area_and_perimeter(&ring, t)?;
            if ring.len() < 3 || area <= eps * per.max(eps) {
                let n = ring.len();
                for i in 0..n {
                    let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
                    self.emit_edge(a, b, t)?;
                }
                for &a in &ring {
                    self.kill(a, t)?;
                }
                return Ok(true);
            }
        }
        // Zero-length edges.
        for u in 0..self.verts.len() {
            let vu = self.vert(u)?;
            if !vu.alive {
                continue;
            }
            let w = vu.next;
            let vw = self.vert(w)?;
            let pu = vu.at(t);
            let pw = vw.at(t);
            // A zero-length edge that is growing was just born (a step between parallel edges): keep it.
            let growing = (vw.vel() - vu.vel()).dot(self.line(vu.eout)?.d) > 1e-9;
            if (pw - pu).length() <= eps && !growing {
                let p = vu.prev;
                let q = vw.next;
                self.emit_edge(p, u, t)?;
                self.emit_edge(u, w, t)?;
                self.emit_edge(w, q, t)?;
                self.insert_vert((pu + pw) * 0.5, t, vu.ein, vw.eout, p, q)?;
                self.kill(u, t)?;
                self.kill(w, t)?;
                return Ok(true);
            }
        }
        // Needles: spikes of zero width between opposite edges.
        for v in 0..self.verts.len() {
            let vv = self.vert(v)?;
            if !vv.alive || vv.motion != Motion::Needle {
                continue;
            }
            let u = vv.prev;
            let w = vv.next;
            let vu = self.vert(u)?;
            let vw = self.vert(w)?;
            let pv = vv.at(t);
            let pu = vu.at(t);
            let pw = vw.at(t);
            let du = (pu - pv).length();
            let dw = (pw - pv).length();
            if (du - dw).abs() <= eps {
                if vu.prev == w {
                    // The ring is just the spike.
                    self.emit_edge(u, v, t)?;
                    self.emit_edge(v, w, t)?;
                    self.emit_edge(w, u, t)?;
                    self.kill(u, t)?;
                    self.kill(v, t)?;
                    self.kill(w, t)?;
                    return Ok(true);
                }
                let p = vu.prev;
                let q = vw.next;
                self.emit_edge(p, u, t)?;
                self.emit_edge(u, v, t)?;
                self.emit_edge(v, w, t)?;
                self.emit_edge(w, q, t)?;
                self.insert_vert((pu + pw) * 0.5, t, vu.ein, vw.eout, p, q)?;
                self.kill(u, t)?;
                self.kill(v, t)?;
                self.kill(w, t)?;
            } else if du < dw {
                let p = vu.prev;
                self.emit_edge(p, u, t)?;
                self.emit_edge(u, v, t)?;
                self.emit_edge(v, w, t)?;
                self.insert_vert(pu, t, vu.ein, vv.eout, p, w)?;
                self.kill(u, t)?;
                self.kill(v, t)?;
            } else {
                let q = vw.next;
                self.emit_edge(u, v, t)?;
                self.emit_edge(v, w, t)?;
                self.emit_edge(w, q, t)?;
                self.insert_vert(pw, t, vv.ein, vw.eout, u, q)?;
                self.kill(v, t)?;
                self.kill(w, t)?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn next_event(&self, t: f64) -> Result<Option<Event>, GeomError> {
        let eps = self.eps;
        let mut best: Option<Event> = None;
        let consider = |e: Event, best: &mut Option<Event>| {
            let better = match best {
                None => true,
                Some(b) => {
                    let (te, tb) = (e.time(), b.time());
                    // Prefer edge events on ties: they simplify the wavefront without splitting it.
                    te < tb - eps * 1e-3
                        || (te <= tb + eps * 1e-3
                            && matches!(e, Event::Edge { .. })
                            && matches!(b, Event::Split { .. }))
                }
            };
            if better {
                *best = Some(e);
            }
        };
        let rings = self.rings()?;
        for ring in &rings {
            for &u in ring {
                let vu = self.vert(u)?;
                let vw = self.vert(vu.next)?;
                let line = self.line(vu.eout)?;
                // Edge event.
                let len = (vw.at(t) - vu.at(t)).dot(line.d);
                let rate = (vw.vel() - vu.vel()).dot(line.d);
                if rate < -1e-12 {
                    let dt = len.max(0.0) / -rate;
                    consider(Event::Edge { u, time: t + dt }, &mut best);
                }
            }
            for &v in ring {
                let vv = self.vert(v)?;
                let la = self.line(vv.ein)?;
                let lb = self.line(vv.eout)?;
                let reflex = la.d.cross(lb.d) < -1e-9;
                if !reflex {
                    continue;
                }
                let pv = vv.at(t);
                let vel = vv.vel();
                for &j in ring {
                    let vj = self.vert(j)?;
                    let k = vj.next;
                    if j == v || k == v {
                        continue;
                    }
                    let vk = self.vert(k)?;
                    let e = self.line(vj.eout)?;
                    let dist = e.n.dot(pv) - (e.c + e.s * t);
                    if dist < -eps {
                        continue;
                    }
                    let rate = e.n.dot(vel) - e.s;
                    if rate >= -1e-12 {
                        continue;
                    }
                    let th = t + dist.max(0.0) / -rate;
                    let x = vv.at(th);
                    let pj = vj.at(th);
                    let pk = vk.at(th);
                    let le = (pk - pj).dot(e.d);
                    if le <= eps {
                        continue;
                    }
                    let lam = (x - pj).dot(e.d) / le;
                    let slack = eps / le;
                    if lam < -slack || lam > 1.0 + slack {
                        continue;
                    }
                    consider(Event::Split { v, j, time: th }, &mut best);
                }
            }
        }
        Ok(best)
    }

    fn apply(&mut self, ev: Event, t: f64) -> Result<(), GeomError> {
        match ev {
            Event::Edge { u, .. } => {
                let vu = self.vert(u)?;
                let w = vu.next;
                let vw = self.vert(w)?;
                let p = vu.prev;
                let q = vw.next;
                if p == w {
                    // Two-vertex ring: it vanishes.
                    self.emit_edge(u, w, t)?;
                    self.emit_edge(w, u, t)?;
                    self.kill(u, t)?;
                    self.kill(w, t)?;
                    return Ok(());
                }
                self.emit_edge(p, u, t)?;
                self.emit_edge(u, w, t)?;
                self.emit_edge(w, q, t)?;
                let x = (vu.at(t) + vw.at(t)) * 0.5;
                self.insert_vert(x, t, vu.ein, vw.eout, p, q)?;
                self.kill(u, t)?;
                self.kill(w, t)?;
            }
            Event::Split { v, j, .. } => {
                let vv = self.vert(v)?;
                let vj = self.vert(j)?;
                let k = vj.next;
                let x = vv.at(t);
                let a = vv.ein;
                let b = vv.eout;
                let e = vj.eout;
                let vp = vv.prev;
                let vn = vv.next;
                self.emit_edge(vp, v, t)?;
                self.emit_edge(v, vn, t)?;
                self.emit_edge(j, k, t)?;
                // Ring 1: … vp → x1 → k …   Ring 2: … j → x2 → vn …
                self.insert_vert(x, t, a, e, vp, k)?;
                self.insert_vert(x, t, e, b, j, vn)?;
                self.kill(v, t)?;
            }
        }
        Ok(())
    }
}

/// Removes repeated points and merges collinear edges; returns the cleaned ring and, for each cleaned edge, the
/// input edges it covers.
/// Cleaned ring, its edge speeds, and the input edges each cleaned edge covers.
type Prepared = (Vec<Point2>, Vec<f64>, Vec<Vec<usize>>);

fn prepare(points: &[Point2], speeds: &[f64]) -> Result<Prepared, GeomError> {
    let n = points.len();
    // Edge list (start vertex, input edge index), skipping zero-length edges.
    let mut verts: Vec<Point2> = Vec::with_capacity(n);
    let mut edges: Vec<Vec<usize>> = Vec::with_capacity(n);
    let mut spd: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let (Some(&a), Some(&b), Some(&s)) = (points.get(i), points.get((i + 1) % n), speeds.get(i)) else {
            continue;
        };
        if (b - a).length() <= 1e-12 {
            continue;
        }
        verts.push(a);
        edges.push(vec![i]);
        spd.push(s);
    }
    // Merge collinear consecutive edges.
    let mut changed = true;
    while changed && verts.len() > 3 {
        changed = false;
        let m = verts.len();
        for i in 0..m {
            let j = (i + 1) % m;
            let (Some(&a), Some(&b), Some(&c)) = (verts.get(i), verts.get(j), verts.get((i + 2) % m)) else {
                continue;
            };
            let d0 = (b - a).normalized().unwrap_or(Vec2::X);
            let d1 = (c - b).normalized().unwrap_or(Vec2::X);
            if d0.cross(d1).abs() < 1e-12 {
                if d0.dot(d1) < 0.0 {
                    return Err(GeomError::Invalid("the outline doubles back on itself".into()));
                }
                let (si, sj) = (spd.get(i).copied().unwrap_or(0.0), spd.get(j).copied().unwrap_or(0.0));
                if (si - sj).abs() > 1e-9 * (1.0 + si.abs()) {
                    // Different speeds: keep both edges; the wavefront gets a step between them.
                    continue;
                }
                // Merge edge j into edge i: drop vertex j.
                let moved = edges.get(j).cloned().unwrap_or_default();
                if let Some(e) = edges.get_mut(i) {
                    e.extend(moved);
                }
                edges.remove(j);
                spd.remove(j);
                verts.remove(j);
                changed = true;
                break;
            }
        }
    }
    if verts.len() < 3 {
        return Err(GeomError::Degenerate("polygon needs at least three corners"));
    }
    Ok((verts, spd, edges))
}

fn check_simple(points: &[Point2]) -> Result<(), GeomError> {
    let n = points.len();
    let seg = |i: usize| -> Option<([f64; 2], [f64; 2])> {
        Some((points.get(i)?.xy(), points.get((i + 1) % n)?.xy()))
    };
    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (Some((a, b)), Some((c, d))) = (seg(i), seg(j)) else { continue };
            let o1 = orient2d(a, b, c);
            let o2 = orient2d(a, b, d);
            let o3 = orient2d(c, d, a);
            let o4 = orient2d(c, d, b);
            if o1 != o2 && o3 != o4 && o1 != std::cmp::Ordering::Equal && o2 != std::cmp::Ordering::Equal {
                return Err(GeomError::Invalid("the outline crosses itself".into()));
            }
        }
    }
    Ok(())
}

/// Computes the weighted straight skeleton of a simple polygon.
///
/// `speeds[i]` is the inward speed of edge `i` (from vertex `i` to vertex `i + 1`); it must be finite and not
/// negative, and at least one must be positive. The polygon may be given in either orientation; edge indices
/// always refer to the input order.
///
/// Tolerance: events closer than about 10⁻⁹ of the polygon's size are treated as simultaneous. Failure modes:
/// fewer than three distinct vertices ([`GeomError::Degenerate`]), a self-crossing outline, collinear neighbours
/// with different speeds, or a numerical breakdown detected by the final area check ([`GeomError::Invalid`]).
pub fn weighted_straight_skeleton(polygon: &Polygon2, speeds: &[f64]) -> Result<Skeleton, GeomError> {
    let n = polygon.len();
    if speeds.len() != n {
        return Err(GeomError::Invalid("one speed per edge is required".into()));
    }
    if speeds.iter().any(|s| !s.is_finite() || *s < 0.0) {
        return Err(GeomError::Invalid("speeds must be finite and not negative".into()));
    }
    if !speeds.iter().any(|s| *s > 0.0) {
        return Err(GeomError::Invalid("at least one edge must move".into()));
    }
    if polygon.vertices.iter().any(|p| !p.is_finite()) {
        return Err(GeomError::Invalid("non-finite coordinates".into()));
    }
    // Work counter-clockwise; map edge indices back for clockwise input.
    let ccw = polygon.signed_area() > 0.0;
    let (pts, spd, edge_map): (Vec<Point2>, Vec<f64>, Vec<usize>) = if ccw {
        (polygon.vertices.clone(), speeds.to_vec(), (0..n).collect())
    } else {
        let pts: Vec<Point2> = polygon.vertices.iter().rev().copied().collect();
        // Reversed edge k joins input vertices n-1-k and n-2-k: input edge (n-2-k) mod n.
        let map: Vec<usize> = (0..n).map(|k| (2 * n - 2 - k) % n).collect();
        let spd: Vec<f64> = map.iter().map(|&i| speeds.get(i).copied().unwrap_or(0.0)).collect();
        (pts, spd, map)
    };
    let (verts, spd, groups) = prepare(&pts, &spd)?;
    check_simple(&verts)?;
    let m = verts.len();
    let bbox = Polygon2::new(verts.clone()).bbox().ok_or(GeomError::Degenerate("empty polygon"))?;
    let scale = bbox.width().as_m().max(bbox.height().as_m()).max(1e-6);
    let eps = scale * 1e-9;

    let mut lines = Vec::with_capacity(m);
    for i in 0..m {
        let (Some(&a), Some(&b)) = (verts.get(i), verts.get((i + 1) % m)) else { continue };
        let d = (b - a).normalized().ok_or(GeomError::Degenerate("zero-length edge"))?;
        let nrm = d.perp();
        lines.push(Line {
            n: nrm,
            d,
            c: nrm.dot(a.to_vec()),
            s: spd.get(i).copied().unwrap_or(0.0),
            face: i,
        });
    }
    let mut st =
        State { lines, verts: Vec::with_capacity(4 * m), quads: vec![Vec::new(); m], arcs: Vec::new(), eps };
    // Initial wavefront: one vertex per corner (two where parallel edges with different speeds meet).
    let mut chains: Vec<(usize, usize)> = Vec::with_capacity(m);
    for i in 0..m {
        let p = verts.get(i).map(|q| q.to_vec()).unwrap_or(Vec2::ZERO);
        let ein = (i + m - 1) % m;
        if st.motion(ein, i)? == Motion::Step {
            let c = st.step_line(ein, i, p)?;
            let first = st.push_vert(p, 0.0, ein, c)?;
            let second = st.push_vert(p, 0.0, c, i)?;
            st.link(first, second)?;
            chains.push((first, second));
        } else {
            let v = st.push_vert(p, 0.0, ein, i)?;
            chains.push((v, v));
        }
    }
    for i in 0..m {
        let (Some(&(_, last)), Some(&(first, _))) = (chains.get(i), chains.get((i + 1) % m)) else {
            continue;
        };
        st.link(last, first)?;
    }

    let mut t = 0.0f64;
    let guard = 64 * m * m + 1000;
    let mut steps = 0usize;
    loop {
        steps += 1;
        if steps > guard {
            return Err(GeomError::Invalid("skeleton: too many events".into()));
        }
        if st.cleanup_once(t)? {
            continue;
        }
        if !st.verts.iter().any(|v| v.alive) {
            break;
        }
        let Some(ev) = st.next_event(t)? else {
            return Err(GeomError::Invalid("skeleton: the wavefront does not collapse".into()));
        };
        t = ev.time().max(t);
        st.apply(ev, t)?;
    }

    // Faces from the swept quads.
    let mut faces = Vec::with_capacity(m);
    let mut total = 0.0;
    for (i, line) in st.lines.iter().take(m).enumerate() {
        let quads: Vec<PolygonWithHoles> = st
            .quads
            .get(i)
            .map(|q| q.iter().map(|r| PolygonWithHoles::from(Polygon2::new(r.clone()))).collect())
            .unwrap_or_default();
        let region = if quads.is_empty() { Vec::new() } else { union_all(&quads)? };
        total += region.iter().map(|r| r.area().as_m2()).sum::<f64>();
        let edges: Vec<usize> = groups
            .get(i)
            .map(|g| g.iter().filter_map(|&k| edge_map.get(k).copied()).collect())
            .unwrap_or_default();
        faces.push(SkeletonFace { edges, normal: line.n, offset: line.c, speed: line.s, region });
    }
    let area = Polygon2::new(verts).area().as_m2();
    if (total - area).abs() > 1e-6 * area.max(1e-12) + 1e-9 {
        return Err(GeomError::Invalid(format!("skeleton faces cover {total} instead of {area}")));
    }
    Ok(Skeleton { faces, arcs: st.arcs, max_time: t })
}

/// The unweighted straight skeleton (every edge moves at unit speed).
pub fn straight_skeleton(polygon: &Polygon2) -> Result<Skeleton, GeomError> {
    weighted_straight_skeleton(polygon, &vec![1.0; polygon.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_area(s: &Skeleton, edge: usize) -> f64 {
        s.face_of_edge(edge).map(|f| f.region.iter().map(|r| r.area().as_m2()).sum()).unwrap_or(0.0)
    }

    #[test]
    fn square_gives_four_triangles() {
        let sq = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        let s = straight_skeleton(&sq).unwrap();
        assert_eq!(s.faces.len(), 4);
        for e in 0..4 {
            assert!((face_area(&s, e) - 4.0).abs() < 1e-9);
        }
        assert!((s.max_time - 2.0).abs() < 1e-9);
    }

    #[test]
    fn rectangle_has_a_ridge() {
        let r = Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]);
        let s = straight_skeleton(&r).unwrap();
        // Long sides: trapezoids (10 + 4) / 2 * 3 = 21; short sides: triangles 6 * 3 / 2 = 9.
        assert!((face_area(&s, 0) - 21.0).abs() < 1e-9);
        assert!((face_area(&s, 1) - 9.0).abs() < 1e-9);
        assert!((face_area(&s, 2) - 21.0).abs() < 1e-9);
        assert!((face_area(&s, 3) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn gable_edges_do_not_move() {
        let r = Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]);
        let s = weighted_straight_skeleton(&r, &[1.0, 0.0, 1.0, 0.0]).unwrap();
        assert!((face_area(&s, 0) - 30.0).abs() < 1e-9);
        assert!((face_area(&s, 2) - 30.0).abs() < 1e-9);
        assert!(face_area(&s, 1).abs() < 1e-12);
        // Mono-pitch: only the south edge moves; it sweeps everything.
        let mono = weighted_straight_skeleton(&r, &[1.0, 0.0, 0.0, 0.0]).unwrap();
        assert!((face_area(&mono, 0) - 60.0).abs() < 1e-9);
        assert!((mono.max_time - 6.0).abs() < 1e-9);
    }

    #[test]
    fn l_shape_equal_speeds() {
        let l =
            Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 5.0), (5.0, 10.0), (0.0, 10.0)]);
        let s = straight_skeleton(&l).unwrap();
        let total: f64 = (0..6).map(|e| face_area(&s, e)).sum();
        assert!((total - 75.0).abs() < 1e-9);
        // The bottom face is the trapezoid (0,0)-(10,0)-(7.5,2.5)-(2.5,2.5).
        assert!((face_area(&s, 0) - (10.0 + 5.0) / 2.0 * 2.5).abs() < 1e-9);
        // Short end edges: triangles of base 5 and height 2.5.
        assert!((face_area(&s, 1) - 6.25).abs() < 1e-9);
        assert!((s.max_time - 2.5).abs() < 1e-9);
    }

    #[test]
    fn mono_pitch_over_an_l() {
        let l =
            Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 5.0), (5.0, 10.0), (0.0, 10.0)]);
        let s = weighted_straight_skeleton(&l, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]).unwrap();
        assert!((face_area(&s, 0) - 75.0).abs() < 1e-9);
    }

    #[test]
    fn clockwise_input_keeps_edge_numbers() {
        let cw = Polygon2::from_m(&[(0.0, 0.0), (0.0, 6.0), (10.0, 6.0), (10.0, 0.0)]);
        // Edge 1 runs (0,6)→(10,6) (north, long); edge 0 is the west short edge.
        let s = weighted_straight_skeleton(&cw, &[0.0, 1.0, 0.0, 1.0]).unwrap();
        assert!((face_area(&s, 1) - 30.0).abs() < 1e-9);
        assert!((face_area(&s, 3) - 30.0).abs() < 1e-9);
        assert!(face_area(&s, 0).abs() < 1e-12);
    }

    #[test]
    fn different_speeds_shift_the_ridge() {
        // South edge twice as fast: the ridge sits at y = 4 in a 6 m deep rectangle with gable ends.
        let r = Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]);
        let s = weighted_straight_skeleton(&r, &[2.0, 0.0, 1.0, 0.0]).unwrap();
        assert!((face_area(&s, 0) - 40.0).abs() < 1e-9);
        assert!((face_area(&s, 2) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn plus_shape_and_u_shape() {
        let plus = Polygon2::from_m(&[
            (4.0, 0.0),
            (8.0, 0.0),
            (8.0, 4.0),
            (12.0, 4.0),
            (12.0, 8.0),
            (8.0, 8.0),
            (8.0, 12.0),
            (4.0, 12.0),
            (4.0, 8.0),
            (0.0, 8.0),
            (0.0, 4.0),
            (4.0, 4.0),
        ]);
        let s = straight_skeleton(&plus).unwrap();
        let total: f64 = (0..12).map(|e| face_area(&s, e)).sum();
        assert!((total - 80.0).abs() < 1e-9);
        let u = Polygon2::from_m(&[
            (0.0, 0.0),
            (12.0, 0.0),
            (12.0, 10.0),
            (8.0, 10.0),
            (8.0, 4.0),
            (4.0, 4.0),
            (4.0, 10.0),
            (0.0, 10.0),
        ]);
        let s = straight_skeleton(&u).unwrap();
        let total: f64 = (0..8).map(|e| face_area(&s, e)).sum();
        assert!((total - 96.0).abs() < 1e-9);
    }

    #[test]
    fn dumbbell_splits_into_two_rings() {
        let d = Polygon2::from_m(&[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (20.0, 4.0),
            (20.0, 0.0),
            (30.0, 0.0),
            (30.0, 10.0),
            (20.0, 10.0),
            (20.0, 6.0),
            (10.0, 6.0),
            (10.0, 10.0),
            (0.0, 10.0),
        ]);
        let s = straight_skeleton(&d).unwrap();
        let total: f64 = (0..12).map(|e| face_area(&s, e)).sum();
        assert!((total - 220.0).abs() < 1e-9);
        assert!((s.max_time - 5.0).abs() < 1e-9);
    }

    #[test]
    fn convex_polygon_matches_lower_envelope() {
        // For convex polygons the roof is the lower envelope of the edge planes: check heights at sample points.
        let hexagon: Vec<(f64, f64)> = (0..6)
            .map(|i| {
                (
                    5.0 * (f64::from(i) * std::f64::consts::FRAC_PI_3).cos(),
                    3.0 * (f64::from(i) * std::f64::consts::FRAC_PI_3).sin(),
                )
            })
            .collect();
        let poly = Polygon2::from_m(&hexagon);
        let speeds = [1.0, 0.5, 2.0, 1.0, 0.7, 1.3];
        let s = weighted_straight_skeleton(&poly, &speeds).unwrap();
        let tol = kanavos_units::Tolerance::default();
        for f in &s.faces {
            for r in &f.region {
                let Some(c) = r.outer.centroid() else { continue };
                let own = f.time_at(c).unwrap();
                let min = s.faces.iter().filter_map(|g| g.time_at(c)).fold(f64::INFINITY, f64::min);
                assert!((own - min).abs() < 1e-9, "face {:?}: {own} vs {min}", f.edges);
                assert!(poly.contains(c, &tol));
            }
        }
    }

    /// Deterministic pseudo-random numbers (64-bit LCG) for stress tests.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn random_orthogonal_outline(rng: &mut Lcg) -> Polygon2 {
        let mut rects = Vec::new();
        for _ in 0..(2 + (rng.next() * 5.0) as usize) {
            let x = (rng.next() * 8.0).floor();
            let y = (rng.next() * 8.0).floor();
            let w = 2.0 + (rng.next() * 6.0).floor();
            let h = 2.0 + (rng.next() * 6.0).floor();
            rects.push(PolygonWithHoles::from(Polygon2::rectangle(
                Point2::new(x, y),
                Point2::new(x + w, y + h),
            )));
        }
        let u = union_all(&rects).unwrap();
        u.into_iter().max_by(|a, b| a.area().as_m2().total_cmp(&b.area().as_m2())).unwrap().outer
    }

    #[test]
    fn random_orthogonal_outlines_tile() {
        let mut rng = Lcg(42);
        for case in 0..1000 {
            let poly = random_orthogonal_outline(&mut rng);
            let n = poly.len();
            let s = straight_skeleton(&poly).unwrap_or_else(|e| panic!("case {case}: {e} for {poly:?}"));
            let total: f64 = s.faces.iter().flat_map(|f| f.region.iter()).map(|r| r.area().as_m2()).sum();
            assert!((total - poly.area().as_m2()).abs() < 1e-6, "case {case}");
            // Weighted variant with random pitches.
            let speeds: Vec<f64> = (0..n).map(|_| 0.5 + rng.next() * 1.5).collect();
            weighted_straight_skeleton(&poly, &speeds)
                .unwrap_or_else(|e| panic!("weighted case {case}: {e}"));
            // Some gable (zero-speed) edges.
            let mut gabled: Vec<f64> =
                (0..n).map(|_| if rng.next() < 0.3 { 0.0 } else { 0.5 + rng.next() }).collect();
            if gabled.iter().all(|s| *s == 0.0) {
                gabled[0] = 1.0;
            }
            // Random gables can enclose a region that no moving edge faces; that is reported, not hidden.
            match weighted_straight_skeleton(&poly, &gabled) {
                Ok(_) => {}
                Err(GeomError::Invalid(m)) if m.contains("does not collapse") => {}
                Err(e) => panic!("gabled case {case}: {e} {poly:?} {gabled:?}"),
            }
        }
    }

    #[test]
    fn random_star_polygons_tile() {
        let mut rng = Lcg(99);
        for case in 0..500 {
            let n = 5 + (rng.next() * 15.0) as usize;
            let pts: Vec<(f64, f64)> = (0..n)
                .map(|i| {
                    let a = std::f64::consts::TAU * (i as f64 + 0.8 * rng.next()) / n as f64;
                    let r = 3.0 + 7.0 * rng.next();
                    (r * a.cos(), r * a.sin())
                })
                .collect();
            let poly = Polygon2::from_m(&pts);
            let s = straight_skeleton(&poly).unwrap_or_else(|e| panic!("star {case}: {e} {pts:?}"));
            let total: f64 = s.faces.iter().flat_map(|f| f.region.iter()).map(|r| r.area().as_m2()).sum();
            assert!((total - poly.area().as_m2()).abs() < 1e-6, "star {case}");
            let speeds: Vec<f64> = (0..n).map(|_| 0.4 + rng.next() * 2.0).collect();
            weighted_straight_skeleton(&poly, &speeds)
                .unwrap_or_else(|e| panic!("weighted star {case}: {e}"));
        }
    }

    #[test]
    fn single_moving_edge_sweeps_past_a_reflex_corner() {
        let l =
            Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 5.0), (5.0, 10.0), (0.0, 10.0)]);
        let s = weighted_straight_skeleton(&l, &[0.0, 1.0, 0.0, 0.0, 0.0, 0.0]).unwrap();
        assert!((face_area(&s, 1) - 75.0).abs() < 1e-9);
        assert!((s.max_time - 10.0).abs() < 1e-9);
    }

    #[test]
    fn collinear_edges_with_different_speeds_meet_at_a_step() {
        // South eave split in two halves with different pitches.
        let r = Polygon2::from_m(&[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]);
        let s = weighted_straight_skeleton(&r, &[1.0, 2.0, 0.0, 1.0, 0.0]).unwrap();
        let total: f64 = (0..5).map(|e| face_area(&s, e)).sum();
        assert!((total - 60.0).abs() < 1e-9);
        // The faster half reaches further north than the slower one.
        assert!(face_area(&s, 1) > face_area(&s, 0));
    }

    #[test]
    fn random_convex_polygons_match_lower_envelope() {
        let mut rng = Lcg(7);
        for case in 0..200 {
            let pts: Vec<Point2> =
                (0..12).map(|_| Point2::new(rng.next() * 20.0, rng.next() * 12.0)).collect();
            let hull = crate::hull::convex_hull(&pts);
            if hull.len() < 3 {
                continue;
            }
            let speeds: Vec<f64> = (0..hull.len()).map(|_| 0.3 + rng.next() * 2.0).collect();
            let s = weighted_straight_skeleton(&hull, &speeds).unwrap_or_else(|e| panic!("case {case}: {e}"));
            for f in &s.faces {
                for r in &f.region {
                    let Some(c) = r.outer.centroid() else { continue };
                    let own = f.time_at(c).unwrap();
                    let min = s.faces.iter().filter_map(|g| g.time_at(c)).fold(f64::INFINITY, f64::min);
                    assert!((own - min).abs() < 1e-7, "case {case}: {own} vs {min}");
                }
            }
        }
    }

    #[test]
    fn rejects_bad_input() {
        let sq = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        assert!(weighted_straight_skeleton(&sq, &[0.0; 4]).is_err());
        assert!(weighted_straight_skeleton(&sq, &[1.0; 3]).is_err());
        let bow = Polygon2::from_m(&[(0.0, 0.0), (4.0, 4.0), (4.0, 0.0), (0.0, 4.0)]);
        assert!(straight_skeleton(&bow).is_err());
    }
}

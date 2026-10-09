//! The projection pipeline shared by sections, elevations, 3D line views and roof plans: per element, pick the
//! edges that can be seen, map them and the occluding faces to view space, clip, and hand them to the
//! hidden-line remover.

use kanavos_model::{Category, Id, Phase};
use kanavos_solid::Mesh;

use crate::hlr::{Edge, Occluders, Visible};
use crate::meshops::{Eye, Topology, V3, candidate_edges, clip_polygon, clip_segment, lerp};

/// An element to project: its meshes and what the drawing needs to know about it.
#[derive(Debug, Clone)]
pub(crate) struct ProjItem<'a> {
    /// Element.
    pub element: Id,
    /// Category, for layer names.
    pub category: Category,
    /// Phase, for styling.
    pub phase: Phase,
    /// The element's meshes in world coordinates.
    pub meshes: Vec<&'a Mesh>,
}

/// A projection from world space to view space `(x, y, depth)`.
///
/// Points first go through an affine map to a *pre* space where clipping happens (view space for parallel
/// projections, camera space for perspective ones), then, for perspective, through
/// `(X, Y, Z) → (D·X/Z, D·Y/Z, −D/Z)` so that `(x, y)` are metres on the plane at distance `D`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Projection {
    /// How faces are classified as front or back, in world space.
    pub eye: Eye,
    /// Affine rows mapping world to pre space.
    pub rows: [[f64; 4]; 3],
    /// Half-spaces `n · q + c ≥ 0` in pre space that bound what is shown.
    pub clips: Vec<[f64; 4]>,
    /// Distance of the projection plane for perspective views.
    pub perspective: Option<f64>,
}

impl Projection {
    /// Maps a world point to pre space.
    pub(crate) fn pre(&self, p: V3) -> V3 {
        let r = |row: &[f64; 4]| row[0] * p[0] + row[1] * p[1] + row[2] * p[2] + row[3];
        [r(&self.rows[0]), r(&self.rows[1]), r(&self.rows[2])]
    }
    /// Maps a pre-space point to view space.
    pub(crate) fn post(&self, q: V3) -> V3 {
        match self.perspective {
            Some(dist) if q[2] > 1e-12 => [dist * q[0] / q[2], dist * q[1] / q[2], -dist / q[2]],
            _ => q,
        }
    }
    fn clip_value(c: &[f64; 4], q: &V3) -> f64 {
        c[0] * q[0] + c[1] * q[1] + c[2] * q[2] + c[3]
    }
}

/// Edges and occluders of projected items, in view space.
#[derive(Debug, Clone, Default)]
pub(crate) struct Projected {
    /// Candidate edges; the tag is the index of the item.
    pub edges: Vec<Edge>,
    /// Front-facing triangles.
    pub tris: Vec<[V3; 3]>,
}

fn project_item(index: usize, item: &ProjItem<'_>, proj: &Projection, crease_cos: f64, out: &mut Projected) {
    let arrays: Vec<(Vec<V3>, &[[u32; 3]])> = item
        .meshes
        .iter()
        .map(|m| (m.positions.iter().map(|p| p.to_array()).collect(), m.triangles.as_slice()))
        .collect();
    // Quick rejection: every vertex outside one clip half-space.
    let outside = proj.clips.iter().any(|c| {
        arrays.iter().all(|(pos, _)| pos.iter().all(|p| Projection::clip_value(c, &proj.pre(*p)) < 0.0))
    });
    if outside || arrays.iter().all(|(p, _)| p.is_empty()) {
        return;
    }
    let topo = Topology::build(arrays.iter().map(|(p, t)| (p.as_slice(), *t)));
    let mut cands = Vec::new();
    candidate_edges(&topo, &proj.eye, crease_cos, &mut cands);
    for (a, b) in cands {
        let (pa, pb) = (proj.pre(a), proj.pre(b));
        let mut range = Some((0.0, 1.0));
        for c in &proj.clips {
            range = range.and_then(|r| clip_segment(pa, pb, |q| Projection::clip_value(c, q), r));
        }
        let Some((t0, t1)) = range else { continue };
        let qa = proj.post(lerp(pa, pb, t0));
        let qb = proj.post(lerp(pa, pb, t1));
        out.edges.push(Edge { a: qa, b: qb, tag: index as u32 });
    }
    for (f, t) in topo.triangles.iter().enumerate() {
        if topo.internal.get(f).copied().unwrap_or(false) {
            continue;
        }
        let n = topo.normal(f as u32);
        let p0 = topo.pos(t[0]);
        if !proj.eye.front(n, p0) {
            continue;
        }
        let mut poly: Vec<V3> = vec![proj.pre(p0), proj.pre(topo.pos(t[1])), proj.pre(topo.pos(t[2]))];
        for c in &proj.clips {
            poly = clip_polygon(&poly, |q| Projection::clip_value(c, q));
            if poly.len() < 3 {
                break;
            }
        }
        if poly.len() < 3 {
            continue;
        }
        let poly: Vec<V3> = poly.into_iter().map(|q| proj.post(q)).collect();
        if let Some(&first) = poly.first() {
            for w in poly.windows(2).skip(1) {
                if let [b, c] = w {
                    out.tris.push([first, *b, *c]);
                }
            }
        }
    }
}

/// Projects items in parallel.
pub(crate) fn project(items: &[ProjItem<'_>], proj: &Projection, crease_deg: f64) -> Projected {
    let crease_cos = crease_deg.to_radians().cos();
    let n = items.len();
    let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1).clamp(1, 16);
    if n < 16 || threads == 1 {
        let mut out = Projected::default();
        for (i, it) in items.iter().enumerate() {
            project_item(i, it, proj, crease_cos, &mut out);
        }
        return out;
    }
    let chunk = n.div_ceil(threads);
    let parts: Vec<Projected> = std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .enumerate()
            .map(|(ci, ch)| {
                s.spawn(move || {
                    let mut out = Projected::default();
                    for (j, it) in ch.iter().enumerate() {
                        project_item(ci * chunk + j, it, proj, crease_cos, &mut out);
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    let mut out = Projected::default();
    for p in parts {
        out.edges.extend(p.edges);
        out.tris.extend(p.tris);
    }
    out
}

/// Hidden-line removal of projected items plus extra occluders (section caps) and their outlines, returning
/// visible pieces and the occluder index (for profile tests).
pub(crate) fn visible(
    p: &Projected,
    extra: &[[V3; 3]],
    extra_outline: &[Edge],
    depth_eps: f64,
) -> (Vec<Visible>, Occluders) {
    let mut tris = p.tris.clone();
    tris.extend_from_slice(extra);
    let occ = Occluders::new(&tris, &p.edges, extra_outline);
    let v = occ.visible(depth_eps);
    (v, occ)
}

/// Merges collinear overlapping visible pieces with the same tag, so lines shared by touching solids are drawn
/// once.
pub(crate) fn merge_collinear(mut v: Vec<Visible>) -> Vec<Visible> {
    // Key: tag, direction angle (mod π) and offset, quantised.
    let key = |s: &Visible| -> (u32, i64, i64) {
        let (dx, dy) = (s.b[0] - s.a[0], s.b[1] - s.a[1]);
        let mut ang = dy.atan2(dx);
        if ang < 0.0 {
            ang += std::f64::consts::PI;
        }
        if ang >= std::f64::consts::PI - 1e-9 {
            ang = 0.0;
        }
        let (ux, uy) = (ang.cos(), ang.sin());
        let off = s.a[0] * -uy + s.a[1] * ux;
        (s.tag, (ang * 1e6).round() as i64, (off * 1e6).round() as i64)
    };
    v.sort_by_key(|s| key(s));
    let mut out: Vec<Visible> = Vec::with_capacity(v.len());
    let mut i = 0;
    while i < v.len() {
        let Some(first) = v.get(i) else { break };
        let k = key(first);
        let mut j = i;
        while v.get(j).is_some_and(|s| key(s) == k) {
            j += 1;
        }
        // Project the group on its direction and merge intervals.
        let (dx, dy) = (first.b[0] - first.a[0], first.b[1] - first.a[1]);
        let len = dx.hypot(dy).max(1e-300);
        let (ux, uy) = (dx / len, dy / len);
        let origin = first.a;
        let t = |p: [f64; 2]| (p[0] - origin[0]) * ux + (p[1] - origin[1]) * uy;
        let mut ivs: Vec<(f64, f64, f64)> = v
            .get(i..j)
            .unwrap_or(&[])
            .iter()
            .map(|s| {
                let (a, b) = (t(s.a), t(s.b));
                (a.min(b), a.max(b), s.depth)
            })
            .collect();
        ivs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cur: Option<(f64, f64, f64)> = None;
        let emit = |c: (f64, f64, f64), out: &mut Vec<Visible>| {
            out.push(Visible {
                a: [origin[0] + ux * c.0, origin[1] + uy * c.0],
                b: [origin[0] + ux * c.1, origin[1] + uy * c.1],
                tag: k.0,
                depth: c.2,
            });
        };
        for iv in ivs {
            match cur {
                Some(c) if iv.0 <= c.1 + 1e-7 => cur = Some((c.0, c.1.max(iv.1), c.2.min(iv.2))),
                Some(c) => {
                    emit(c, &mut out);
                    cur = Some(iv);
                }
                None => cur = Some(iv),
            }
        }
        if let Some(c) = cur {
            emit(c, &mut out);
        }
        i = j;
    }
    out
}

/// Whether a visible piece separates covered drawing from empty background (a profile line).
pub(crate) fn is_profile(occ: &Occluders, s: &Visible, offset: f64) -> bool {
    let (dx, dy) = (s.b[0] - s.a[0], s.b[1] - s.a[1]);
    let len = dx.hypot(dy);
    if len < 1e-12 {
        return false;
    }
    let (nx, ny) = (-dy / len * offset, dx / len * offset);
    let samples = [0.25, 0.5, 0.75];
    samples.iter().any(|&t| {
        let m = [s.a[0] + dx * t, s.a[1] + dy * t];
        !occ.covered(m[0] + nx, m[1] + ny) || !occ.covered(m[0] - nx, m[1] - ny)
    })
}

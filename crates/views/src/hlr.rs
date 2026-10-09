//! Exact hidden-line removal for polyhedral scenes.
//!
//! Input is in *view space*: `(x, y)` is the drawing position and `d` the depth along a parallel projection
//! (smaller is nearer). Perspective views are mapped to this space first by the projective transform
//! `(X, Y, Z) → (D·X/Z, D·Y/Z, −D/Z)`, which keeps lines straight, planes planar and depth order, so one engine
//! serves sections, elevations and 3D views.
//!
//! Method (edge splitting with occlusion tests, after A. Appel, "The notion of quantitative invisibility and the
//! machine rendering of solids", Proc. ACM National Conference, 1967, and the survey in Foley, van Dam, Feiner
//! and Hughes, *Computer Graphics: Principles and Practice*, 2nd ed., §15.3):
//!
//! 1. Visibility along a candidate edge can only change where its projection crosses the projection of another
//!    edge (the contour of an occluding face) or where the edge pierces a face. Each candidate edge is split at
//!    all such points: projected crossings with every other candidate edge (collinear overlaps contribute the
//!    other edge's end points) and pierce points with every front-facing triangle.
//! 2. Each piece is then entirely visible or entirely hidden, so its midpoint decides: it is hidden when a
//!    front-facing triangle covers the midpoint's projection nearer than the midpoint by more than `depth_eps`.
//! 3. Consecutive visible pieces are merged.
//!
//! A uniform grid over the drawing accelerates both steps; its cell size is chosen so that large faces do not
//! flood it. Triangles in each cell are sorted by their nearest depth, so a test stops at the first triangle
//! that lies wholly behind the point. Edges are processed in parallel.
//!
//! Tolerances: points on the boundary of an occluding triangle count as covered (so lines that coincide with a
//! nearer contour are dropped in favour of that contour); depth differences below `depth_eps` never hide.

use crate::meshops::V3;

/// A candidate edge in view space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Edge {
    /// Start.
    pub a: V3,
    /// End.
    pub b: V3,
    /// Caller's tag (e.g. the index of the element the edge belongs to).
    pub tag: u32,
}

/// A visible piece of an edge, projected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Visible {
    /// Start in drawing coordinates.
    pub a: [f64; 2],
    /// End in drawing coordinates.
    pub b: [f64; 2],
    /// Tag of the source edge.
    pub tag: u32,
    /// Depth at the middle of the piece.
    pub depth: f64,
}

#[derive(Debug, Clone, Copy)]
struct Tri {
    x0: f64,
    y0: f64,
    // Inverse of [p1 - p0, p2 - p0] for barycentric coordinates.
    i00: f64,
    i01: f64,
    i10: f64,
    i11: f64,
    d0: f64,
    dd1: f64,
    dd2: f64,
    min_d: f64,
    bbox: [f64; 4],
}

impl Tri {
    fn new(t: &[V3; 3]) -> Option<Tri> {
        let [p0, p1, p2] = *t;
        let (e1x, e1y) = (p1[0] - p0[0], p1[1] - p0[1]);
        let (e2x, e2y) = (p2[0] - p0[0], p2[1] - p0[1]);
        let det = e1x * e2y - e2x * e1y;
        let scale = (e1x.abs() + e1y.abs()) * (e2x.abs() + e2y.abs());
        if det.abs() <= 1e-14 * scale.max(1e-300) || !det.is_finite() {
            return None;
        }
        Some(Tri {
            x0: p0[0],
            y0: p0[1],
            i00: e2y / det,
            i01: -e2x / det,
            i10: -e1y / det,
            i11: e1x / det,
            d0: p0[2],
            dd1: p1[2] - p0[2],
            dd2: p2[2] - p0[2],
            min_d: p0[2].min(p1[2]).min(p2[2]),
            bbox: [
                p0[0].min(p1[0]).min(p2[0]),
                p0[1].min(p1[1]).min(p2[1]),
                p0[0].max(p1[0]).max(p2[0]),
                p0[1].max(p1[1]).max(p2[1]),
            ],
        })
    }

    #[inline]
    fn bary(&self, x: f64, y: f64) -> (f64, f64) {
        let (px, py) = (x - self.x0, y - self.y0);
        (self.i00 * px + self.i01 * py, self.i10 * px + self.i11 * py)
    }

    #[inline]
    fn depth_at(&self, u: f64, v: f64) -> f64 {
        self.d0 + u * self.dd1 + v * self.dd2
    }
}

const BARY_EPS: f64 = 1e-9;
const FLUSH_EPS: f64 = 1e-7;

/// A uniform grid with compressed per-cell lists.
#[derive(Debug, Clone, Default)]
struct Grid {
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    start: Vec<u32>,
    items: Vec<u32>,
}

impl Grid {
    fn ix(&self, x: f64) -> usize {
        let i = ((x - self.x0) / self.cell).floor();
        if i.is_nan() || i < 0.0 { 0 } else { (i as usize).min(self.nx.saturating_sub(1)) }
    }
    fn iy(&self, y: f64) -> usize {
        let i = ((y - self.y0) / self.cell).floor();
        if i.is_nan() || i < 0.0 { 0 } else { (i as usize).min(self.ny.saturating_sub(1)) }
    }
    fn cell_of(&self, x: f64, y: f64) -> usize {
        self.iy(y) * self.nx + self.ix(x)
    }
    fn list(&self, c: usize) -> &[u32] {
        let (Some(&s), Some(&e)) = (self.start.get(c), self.start.get(c + 1)) else { return &[] };
        self.items.get(s as usize..e as usize).unwrap_or(&[])
    }
    fn bbox_cells(&self, b: &[f64; 4]) -> (usize, usize, usize, usize) {
        (self.ix(b[0]), self.iy(b[1]), self.ix(b[2]), self.iy(b[3]))
    }

    /// Cells crossed by a segment (Amanatides and Woo, "A fast voxel traversal algorithm for ray tracing",
    /// Eurographics 1987), appended to `out`.
    fn segment_cells(&self, a: [f64; 2], b: [f64; 2], out: &mut Vec<u32>) {
        out.clear();
        let fx = |x: f64| (x - self.x0) / self.cell;
        let fy = |y: f64| (y - self.y0) / self.cell;
        let (ax, ay, bx, by) = (fx(a[0]), fy(a[1]), fx(b[0]), fy(b[1]));
        let mut ix = self.ix(a[0]) as i64;
        let mut iy = self.iy(a[1]) as i64;
        let ex = self.ix(b[0]) as i64;
        let ey = self.iy(b[1]) as i64;
        let (dx, dy) = (bx - ax, by - ay);
        let step_x = if dx > 0.0 { 1 } else { -1 };
        let step_y = if dy > 0.0 { 1 } else { -1 };
        let t_dx = if dx.abs() > 1e-300 { 1.0 / dx.abs() } else { f64::INFINITY };
        let t_dy = if dy.abs() > 1e-300 { 1.0 / dy.abs() } else { f64::INFINITY };
        let mut t_mx = if dx > 0.0 {
            ((ix as f64 + 1.0) - ax) * t_dx
        } else if dx < 0.0 {
            (ax - ix as f64) * t_dx
        } else {
            f64::INFINITY
        };
        let mut t_my = if dy > 0.0 {
            ((iy as f64 + 1.0) - ay) * t_dy
        } else if dy < 0.0 {
            (ay - iy as f64) * t_dy
        } else {
            f64::INFINITY
        };
        let limit = self.nx + self.ny + 4;
        for _ in 0..limit {
            if ix >= 0 && iy >= 0 && (ix as usize) < self.nx && (iy as usize) < self.ny {
                out.push((iy as usize * self.nx + ix as usize) as u32);
            }
            if ix == ex && iy == ey {
                break;
            }
            if t_mx < t_my {
                t_mx += t_dx;
                ix += step_x;
            } else {
                t_my += t_dy;
                iy += step_y;
            }
            if t_mx > 1.0 + 1e-9 && t_my > 1.0 + 1e-9 && (ix != ex || iy != ey) {
                // Rounding: finish at the end cell.
                if ex >= 0 && ey >= 0 && (ex as usize) < self.nx && (ey as usize) < self.ny {
                    out.push((ey as usize * self.nx + ex as usize) as u32);
                }
                break;
            }
        }
    }
}

/// Builds compressed lists from (cell, item) registrations.
fn compress(cells: usize, regs: &[(u32, u32)]) -> (Vec<u32>, Vec<u32>) {
    let mut count = vec![0u32; cells + 1];
    for &(c, _) in regs {
        if let Some(n) = count.get_mut(c as usize + 1) {
            *n += 1;
        }
    }
    for i in 1..count.len() {
        let prev = count.get(i - 1).copied().unwrap_or(0);
        if let Some(n) = count.get_mut(i) {
            *n += prev;
        }
    }
    let start = count.clone();
    let mut fill = count;
    let mut items = vec![0u32; regs.len()];
    for &(c, it) in regs {
        if let Some(pos) = fill.get_mut(c as usize) {
            if let Some(slot) = items.get_mut(*pos as usize) {
                *slot = it;
            }
            *pos += 1;
        }
    }
    (start, items)
}

/// Front-facing triangles indexed for occlusion and coverage queries.
#[derive(Debug, Clone, Default)]
pub(crate) struct Occluders {
    tris: Vec<Tri>,
    grid: Grid,
    edge_grid: Grid,
    edges: Vec<Edge>,
    n_draw: usize,
}

fn bounds_of(edges: &[Edge], tris: &[[V3; 3]]) -> Option<[f64; 4]> {
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut any = false;
    let mut add = |p: &V3| {
        if p[0].is_finite() && p[1].is_finite() {
            b[0] = b[0].min(p[0]);
            b[1] = b[1].min(p[1]);
            b[2] = b[2].max(p[0]);
            b[3] = b[3].max(p[1]);
            any = true;
        }
    };
    for e in edges {
        add(&e.a);
        add(&e.b);
    }
    for t in tris {
        for p in t {
            add(p);
        }
    }
    if any { Some(b) } else { None }
}

impl Occluders {
    /// Indexes occluding triangles together with the candidate edges that will be tested against them.
    /// `splitters` are contours of occluders that are not candidate edges themselves (e.g. the outline of a
    /// section's cut faces): edges are split where they cross them, but they are not drawn.
    pub(crate) fn new(tris: &[[V3; 3]], edges: &[Edge], splitters: &[Edge]) -> Occluders {
        let n_draw = edges.len();
        let all: Vec<Edge> = edges.iter().chain(splitters).copied().collect();
        let edges = all.as_slice();
        let tris_data: Vec<Tri> = tris.iter().filter_map(Tri::new).collect();
        let Some(b) = bounds_of(edges, tris) else { return Occluders::default() };
        let w = (b[2] - b[0]).max(1e-6);
        let h = (b[3] - b[1]).max(1e-6);
        let extent = w.max(h);
        let n = tris_data.len() + edges.len();
        // Pick the finest grid whose registrations and cell count stay proportional to the input.
        let mut cell = extent / 8.0;
        for k in 3..=11 {
            let c = extent / f64::from(1u32 << k);
            let nx = (w / c).ceil() as usize + 1;
            let ny = (h / c).ceil() as usize + 1;
            let cells = nx * ny;
            let mut regs = 0usize;
            for t in &tris_data {
                let cx = ((t.bbox[2] - t.bbox[0]) / c).floor() as usize + 2;
                let cy = ((t.bbox[3] - t.bbox[1]) / c).floor() as usize + 2;
                regs += cx * cy;
            }
            for e in edges {
                let len = (e.b[0] - e.a[0]).abs() + (e.b[1] - e.a[1]).abs();
                regs += (len / c).ceil() as usize + 2;
            }
            if regs > 6 * n + 1024 || cells > 4 * n + 1024 {
                break;
            }
            cell = c;
        }
        let nx = ((w / cell).ceil() as usize).max(1) + 1;
        let ny = ((h / cell).ceil() as usize).max(1) + 1;
        let mut grid = Grid { x0: b[0], y0: b[1], cell, nx, ny, start: Vec::new(), items: Vec::new() };
        let mut regs: Vec<(u32, u32)> = Vec::new();
        for (i, t) in tris_data.iter().enumerate() {
            let (x0, y0, x1, y1) = grid.bbox_cells(&t.bbox);
            for iy in y0..=y1 {
                for ix in x0..=x1 {
                    regs.push(((iy * nx + ix) as u32, i as u32));
                }
            }
        }
        let (start, mut items) = compress(nx * ny, &regs);
        // Sort every cell by nearest depth.
        for c in 0..nx * ny {
            let (Some(&s), Some(&e)) = (start.get(c), start.get(c + 1)) else { continue };
            if let Some(slice) = items.get_mut(s as usize..e as usize) {
                slice.sort_by(|&a, &b| {
                    let da = tris_data.get(a as usize).map(|t| t.min_d).unwrap_or(0.0);
                    let db = tris_data.get(b as usize).map(|t| t.min_d).unwrap_or(0.0);
                    da.total_cmp(&db)
                });
            }
        }
        grid.start = start;
        grid.items = items;

        let mut edge_grid = Grid { start: Vec::new(), items: Vec::new(), ..grid.clone() };
        let mut eregs: Vec<(u32, u32)> = Vec::new();
        let mut scratch = Vec::new();
        for (i, e) in edges.iter().enumerate() {
            edge_grid.segment_cells([e.a[0], e.a[1]], [e.b[0], e.b[1]], &mut scratch);
            for &c in &scratch {
                eregs.push((c, i as u32));
            }
        }
        let (es, ei) = compress(nx * ny, &eregs);
        edge_grid.start = es;
        edge_grid.items = ei;
        Occluders { tris: tris_data, grid, edge_grid, edges: all, n_draw }
    }

    /// Whether a view-space point is hidden by a triangle nearer than it by more than `eps`.
    pub(crate) fn hidden(&self, p: V3, eps: f64) -> bool {
        if self.grid.nx == 0 {
            return false;
        }
        let c = self.grid.cell_of(p[0], p[1]);
        for &i in self.grid.list(c) {
            let Some(t) = self.tris.get(i as usize) else { continue };
            if t.min_d > p[2] + eps {
                break;
            }
            if p[0] < t.bbox[0] || p[0] > t.bbox[2] || p[1] < t.bbox[1] || p[1] > t.bbox[3] {
                continue;
            }
            let (u, v) = t.bary(p[0], p[1]);
            if u >= -BARY_EPS && v >= -BARY_EPS && u + v <= 1.0 + BARY_EPS {
                let depth = t.depth_at(u, v);
                if depth < p[2] - eps {
                    return true;
                }
                // Flush: a point strictly inside a face lying in the same plane is covered by that face (an edge
                // of one solid buried in the face of another, e.g. where overlapping walls meet).
                if (depth - p[2]).abs() <= eps && u > FLUSH_EPS && v > FLUSH_EPS && u + v < 1.0 - FLUSH_EPS {
                    return true;
                }
            }
        }
        false
    }

    /// Whether any occluding triangle covers a drawing point (used to find profile lines).
    pub(crate) fn covered(&self, x: f64, y: f64) -> bool {
        if self.grid.nx == 0 {
            return false;
        }
        let c = self.grid.cell_of(x, y);
        self.grid.list(c).iter().any(|&i| {
            self.tris.get(i as usize).is_some_and(|t| {
                if x < t.bbox[0] || x > t.bbox[2] || y < t.bbox[1] || y > t.bbox[3] {
                    return false;
                }
                let (u, v) = t.bary(x, y);
                u > 1e-9 && v > 1e-9 && u + v < 1.0 - 1e-9
            })
        })
    }

    /// Visible pieces of every indexed edge.
    pub(crate) fn visible(&self, depth_eps: f64) -> Vec<Visible> {
        let n = self.n_draw.min(self.edges.len());
        if n == 0 {
            return Vec::new();
        }
        let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1).clamp(1, 16);
        let chunk = n.div_ceil(threads).max(256);
        let ranges: Vec<(usize, usize)> = (0..n).step_by(chunk).map(|s| (s, (s + chunk).min(n))).collect();
        if ranges.len() <= 1 {
            return self.visible_range(0, n, depth_eps);
        }
        let results: Vec<Vec<Visible>> = std::thread::scope(|s| {
            let handles: Vec<_> =
                ranges.iter().map(|&(a, b)| s.spawn(move || self.visible_range(a, b, depth_eps))).collect();
            handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
        });
        results.into_iter().flatten().collect()
    }

    fn visible_range(&self, from: usize, to: usize, eps: f64) -> Vec<Visible> {
        let mut out = Vec::new();
        let mut edge_stamp = vec![u32::MAX; self.edges.len()];
        let mut tri_stamp = vec![u32::MAX; self.tris.len()];
        let mut cells = Vec::new();
        let mut params: Vec<f64> = Vec::new();
        for ei in from..to {
            let Some(e) = self.edges.get(ei) else { continue };
            let (a2, b2) = ([e.a[0], e.a[1]], [e.b[0], e.b[1]]);
            let (dx, dy) = (b2[0] - a2[0], b2[1] - a2[1]);
            let len2 = dx * dx + dy * dy;
            if len2 < 1e-18 {
                continue; // Seen end-on.
            }
            let stamp = ei as u32;
            self.grid.segment_cells(a2, b2, &mut cells);
            params.clear();
            params.push(0.0);
            params.push(1.0);
            let lin_eps = 1e-9 * len2.sqrt().max(1.0);
            for &c in &cells {
                // Crossings with other edges.
                for &oj in self.edge_grid.list(c as usize) {
                    let Some(slot) = edge_stamp.get_mut(oj as usize) else { continue };
                    if *slot == stamp || oj as usize == ei {
                        continue;
                    }
                    *slot = stamp;
                    let Some(o) = self.edges.get(oj as usize) else { continue };
                    split_by_edge(
                        a2,
                        [dx, dy],
                        len2,
                        [o.a[0], o.a[1]],
                        [o.b[0], o.b[1]],
                        lin_eps,
                        &mut params,
                    );
                }
                // Pierce points through triangles.
                for &ti in self.grid.list(c as usize) {
                    let Some(slot) = tri_stamp.get_mut(ti as usize) else { continue };
                    if *slot == stamp {
                        continue;
                    }
                    *slot = stamp;
                    let Some(t) = self.tris.get(ti as usize) else { continue };
                    if e.a[2].max(e.b[2]) < t.min_d - eps {
                        continue; // The edge is wholly in front of the triangle.
                    }
                    let (ua, va) = t.bary(e.a[0], e.a[1]);
                    let (ub, vb) = t.bary(e.b[0], e.b[1]);
                    let sa = e.a[2] - t.depth_at(ua, va);
                    let sb = e.b[2] - t.depth_at(ub, vb);
                    if (sa > eps && sb < -eps) || (sa < -eps && sb > eps) {
                        let tt = sa / (sa - sb);
                        let (u, v) = (ua + (ub - ua) * tt, va + (vb - va) * tt);
                        if u >= -BARY_EPS && v >= -BARY_EPS && u + v <= 1.0 + BARY_EPS && tt > 0.0 && tt < 1.0
                        {
                            params.push(tt);
                        }
                    }
                }
            }
            params.sort_by(f64::total_cmp);
            params.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
            // Classify pieces and merge visible runs.
            let mut run: Option<f64> = None;
            let mut last_end = 0.0;
            for w in params.windows(2) {
                let (t0, t1) = match w {
                    [t0, t1] => (*t0, *t1),
                    _ => continue,
                };
                let tm = 0.5 * (t0 + t1);
                let p = [e.a[0] + dx * tm, e.a[1] + dy * tm, e.a[2] + (e.b[2] - e.a[2]) * tm];
                let visible = !self.hidden(p, eps);
                if visible {
                    if run.is_none() {
                        run = Some(t0);
                    }
                    last_end = t1;
                } else if let Some(s) = run.take() {
                    out.push(piece(e, s, last_end));
                }
            }
            if let Some(s) = run {
                out.push(piece(e, s, last_end));
            }
        }
        out
    }
}

fn piece(e: &Edge, t0: f64, t1: f64) -> Visible {
    let at = |t: f64| [e.a[0] + (e.b[0] - e.a[0]) * t, e.a[1] + (e.b[1] - e.a[1]) * t];
    let tm = 0.5 * (t0 + t1);
    Visible { a: at(t0), b: at(t1), tag: e.tag, depth: e.a[2] + (e.b[2] - e.a[2]) * tm }
}

/// Adds the parameters on `a + t·d` where another projected edge `r → s` crosses or overlaps it.
fn split_by_edge(
    a: [f64; 2],
    d: [f64; 2],
    len2: f64,
    r: [f64; 2],
    s: [f64; 2],
    lin_eps: f64,
    out: &mut Vec<f64>,
) {
    let e = [s[0] - r[0], s[1] - r[1]];
    let elen2 = e[0] * e[0] + e[1] * e[1];
    if elen2 < 1e-18 {
        return;
    }
    let w = [r[0] - a[0], r[1] - a[1]];
    let den = d[0] * e[1] - d[1] * e[0];
    let scale = (len2 * elen2).sqrt();
    if den.abs() > 1e-12 * scale {
        let t = (w[0] * e[1] - w[1] * e[0]) / den;
        let u = (w[0] * d[1] - w[1] * d[0]) / den;
        let ue = 1e-9;
        if t > 0.0 && t < 1.0 && u >= -ue && u <= 1.0 + ue {
            out.push(t);
        }
        return;
    }
    // Parallel: if collinear, the other edge's end points split this one.
    let len = len2.sqrt();
    let dist = (w[0] * d[1] - w[1] * d[0]).abs() / len;
    if dist > lin_eps {
        return;
    }
    for q in [r, s] {
        let t = ((q[0] - a[0]) * d[0] + (q[1] - a[1]) * d[1]) / len2;
        if t > 0.0 && t < 1.0 {
            out.push(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(x0: f64, y0: f64, x1: f64, y1: f64, d: f64) -> Vec<[V3; 3]> {
        vec![[[x0, y0, d], [x1, y0, d], [x1, y1, d]], [[x0, y0, d], [x1, y1, d], [x0, y1, d]]]
    }

    #[test]
    fn a_line_behind_a_square_is_split_into_two_visible_pieces() {
        let tris = quad(1.0, -1.0, 2.0, 1.0, 1.0);
        let edges = vec![Edge { a: [0.0, 0.0, 5.0], b: [3.0, 0.0, 5.0], tag: 7 }];
        // The square's outline splits the line but is not drawn.
        let outline: Vec<Edge> = [[1.0, -1.0], [2.0, -1.0], [2.0, 1.0], [1.0, 1.0]]
            .iter()
            .zip([[2.0, -1.0], [2.0, 1.0], [1.0, 1.0], [1.0, -1.0]].iter())
            .map(|(a, b)| Edge { a: [a[0], a[1], 1.0], b: [b[0], b[1], 1.0], tag: 0 })
            .collect();
        let occ = Occluders::new(&tris, &edges, &outline);
        let mut v = occ.visible(1e-6);
        v.sort_by(|a, b| a.a[0].total_cmp(&b.a[0]));
        assert_eq!(v.len(), 2);
        assert!((v[0].a[0] - 0.0).abs() < 1e-9 && (v[0].b[0] - 1.0).abs() < 1e-9);
        assert!((v[1].a[0] - 2.0).abs() < 1e-9 && (v[1].b[0] - 3.0).abs() < 1e-9);
        assert_eq!(v[0].tag, 7);
    }

    #[test]
    fn a_line_in_front_stays_whole() {
        let tris = quad(1.0, -1.0, 2.0, 1.0, 5.0);
        let edges = vec![Edge { a: [0.0, 0.0, 1.0], b: [3.0, 0.0, 1.0], tag: 0 }];
        let v = Occluders::new(&tris, &edges, &[]).visible(1e-6);
        assert_eq!(v.len(), 1);
        assert!((v[0].b[0] - v[0].a[0] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_line_piercing_a_face_is_hidden_after_the_pierce_point() {
        // The face is at depth 2; the line runs from depth 1 (in front) to depth 3 (behind) across it.
        let tris = quad(-1.0, -1.0, 4.0, 1.0, 2.0);
        let edges = vec![Edge { a: [0.0, 0.0, 1.0], b: [2.0, 0.0, 3.0], tag: 0 }];
        let v = Occluders::new(&tris, &edges, &[]).visible(1e-6);
        assert_eq!(v.len(), 1);
        assert!((v[0].b[0] - 1.0).abs() < 1e-9, "{v:?}");
    }

    #[test]
    fn edges_on_the_occluder_itself_stay_visible() {
        let tris = quad(0.0, 0.0, 2.0, 1.0, 3.0);
        let edges = vec![
            Edge { a: [0.0, 0.0, 3.0], b: [2.0, 0.0, 3.0], tag: 0 },
            Edge { a: [0.0, 0.0, 3.0], b: [2.0, 1.0, 3.0], tag: 1 },
        ];
        let v = Occluders::new(&tris, &edges, &[]).visible(1e-6);
        assert_eq!(v.len(), 2);
    }
}

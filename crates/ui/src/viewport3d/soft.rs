//! Software rendering of a prepared scene into an egui mesh.
//!
//! Faces: back-face culling, near-plane and section-box clipping, then the painter's algorithm — every triangle
//! (opaque and transparent alike) sorted far to near and drawn in that order, which also blends glass correctly
//! over what lies behind it.
//!
//! Feature edges: drawn on top of the faces where they are visible. Visibility comes from a small depth buffer
//! rasterised from the opaque faces (hidden-line removal by sampling), so edges never show through walls.
//!
//! This is the fallback renderer and the reference for the GPU path: [`SceneRenderer`] is the seam where a
//! `kanavos-render` backend replaces the triangle submission.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Rect, pos2};

use super::camera::{Aabb, Projection, V3};
use super::scene::{Cap, Prepared3d};

/// What to draw and how.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Items drawn (by item index); missing entries count as hidden.
    pub visible: Vec<bool>,
    /// Items tinted as selected.
    pub selected: Vec<bool>,
    /// Item tinted as hovered.
    pub hover: Option<usize>,
    /// Draw feature edges.
    pub edges: bool,
    /// Resolution of the depth buffer (edge visibility and occlusion) relative to screen points.
    pub edge_resolution: f32,
    /// Drop triangles that the depth buffer shows to be completely hidden.
    pub occlusion: bool,
    /// Section box: only what lies inside is drawn; cut items show their inside faces darkened.
    pub section_box: Option<Aabb>,
    /// Caps closing the solids where the section box cuts them (see `scene::section_caps`).
    pub caps: std::sync::Arc<Vec<Cap>>,
}

/// Counts from the last render.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RenderStats {
    /// Triangles submitted.
    pub triangles: usize,
    /// Visible edge pieces submitted.
    pub edges: usize,
    /// Triangles culled as back faces.
    pub culled: usize,
    /// Triangles dropped as hidden behind others.
    pub occluded: usize,
}

/// Something that draws a prepared scene into a rectangle of the UI.
pub trait SceneRenderer {
    /// Draws the scene into `rect` with a projection.
    fn paint(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &Prepared3d,
        proj: &Projection,
        opts: &RenderOptions,
    ) -> RenderStats;
}

/// A triangle ready to draw: screen points, view depths and colour.
#[derive(Debug, Clone, Copy)]
struct Prim {
    pts: [[f32; 2]; 3],
    z: [f32; 3],
    color: Color32,
    /// Writes to the depth buffer (opaque).
    solid: bool,
}

/// The painter's-algorithm renderer.
#[derive(Debug, Default)]
pub struct SoftwareRenderer {
    view: Vec<V3>,
    screen: Vec<[f32; 2]>,
    prims: Vec<Prim>,
    order: Vec<(f32, u32)>,
    depth: DepthBuffer,
}

/// Selection blue.
const SELECT: Color32 = Color32::from_rgb(24, 104, 222);
/// Edge colour.
const EDGE: Color32 = Color32::from_rgba_premultiplied(26, 28, 32, 210);

/// Mixes two colours (unmultiplied), keeping the alpha of `a`.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, _] = b.to_srgba_unmultiplied();
    let l = |x: u8, y: u8| (f32::from(x) * (1.0 - t) + f32::from(y) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(ar, br), l(ag, bg), l(ab, bb), aa)
}

impl SoftwareRenderer {
    /// Renders into a mesh (screen coordinates) for a viewport `rect`.
    pub fn render(
        &mut self,
        rect: Rect,
        scene: &Prepared3d,
        proj: &Projection,
        opts: &RenderOptions,
        out: &mut Mesh,
    ) -> RenderStats {
        let mut stats = RenderStats::default();
        self.view.clear();
        self.screen.clear();
        self.prims.clear();
        self.order.clear();
        // Transform every vertex once.
        self.view.extend(scene.positions.iter().map(|p| proj.view(*p)));
        self.screen.extend(self.view.iter().map(|q| match proj.screen_of_view(*q) {
            Some([x, y]) => [x as f32, y as f32],
            None => [f32::NAN, f32::NAN],
        }));
        let near = proj.near;
        let boxed = opts.section_box;
        for (ii, item) in scene.items.iter().enumerate() {
            if !opts.visible.get(ii).copied().unwrap_or(false) {
                continue;
            }
            // Section box: skip items outside; clip items that cross it.
            let cut = match &boxed {
                Some(b) if !b.intersects(&item.bounds) => continue,
                Some(b) => !b.contains_box(&item.bounds),
                None => false,
            };
            let selected = opts.selected.get(ii).copied().unwrap_or(false);
            let hovered = opts.hover == Some(ii);
            for ti in item.tris.clone() {
                let Some(t) = scene.tris.get(ti) else { continue };
                let [ia, ib, ic] = t.v.map(|i| i as usize);
                let (Some(&a), Some(&b), Some(&c)) =
                    (self.view.get(ia), self.view.get(ib), self.view.get(ic))
                else {
                    continue;
                };
                let mut color = t.color;
                if selected {
                    color = mix(color, SELECT, 0.45);
                } else if hovered {
                    color = mix(color, SELECT, 0.22);
                }
                let outside_box = cut
                    && boxed.is_some_and(|bx| {
                        t.v.iter()
                            .any(|&i| scene.positions.get(i as usize).is_some_and(|p| !bx.contains(*p, 1e-9)))
                    });
                let behind = !proj.parallel && (a.z < near || b.z < near || c.z < near);
                if outside_box || behind {
                    let world: Vec<V3> =
                        t.v.iter().filter_map(|&i| scene.positions.get(i as usize).copied()).collect();
                    self.clip_and_push(&world, proj, boxed.as_ref(), color, item.transparent, &mut stats);
                    continue;
                }
                let (Some(sa), Some(sb), Some(sc)) =
                    (self.screen.get(ia), self.screen.get(ib), self.screen.get(ic))
                else {
                    continue;
                };
                let area = (sb[0] - sa[0]) * (sc[1] - sa[1]) - (sc[0] - sa[0]) * (sb[1] - sa[1]);
                // Counter-clockwise seen from outside becomes clockwise on a Y-down screen: front faces have area < 0.
                let front = area < 0.0;
                if !front && !item.transparent {
                    stats.culled += 1;
                    continue;
                }
                self.push(Prim {
                    pts: [*sa, *sb, *sc],
                    z: [a.z as f32, b.z as f32, c.z as f32],
                    color,
                    solid: !item.transparent,
                });
            }
        }
        // Section caps close the cut solids.
        if boxed.is_some() {
            for cap in opts.caps.iter() {
                if !opts.visible.get(cap.item).copied().unwrap_or(false) {
                    continue;
                }
                let color = if opts.selected.get(cap.item).copied().unwrap_or(false) {
                    mix(cap.color, SELECT, 0.45)
                } else {
                    cap.color
                };
                for t in &cap.tris {
                    self.clip_and_push(t, proj, None, color, false, &mut stats);
                }
            }
        }
        // Depth buffer of the opaque faces: drops triangles hidden behind others (which sorting alone would get
        // wrong for nearly coplanar surfaces) and decides which edges are visible.
        let use_depth = opts.edges || opts.occlusion;
        if use_depth {
            let res = if opts.edge_resolution > 0.0 { opts.edge_resolution.min(2.0) } else { 1.0 };
            self.depth.reset(rect, res, proj.parallel);
            for p in &self.prims {
                if p.solid {
                    self.depth.raster(p.pts, p.z);
                }
            }
        }
        if opts.occlusion {
            self.refine(&mut stats);
        }
        // Far to near.
        self.order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
        out.reserve_triangles(self.order.len());
        out.reserve_vertices(self.order.len() * 3);
        for &(_, k) in &self.order {
            if let Some(p) = self.prims.get(k as usize) {
                push_tri(out, p.pts, p.color);
                stats.triangles += 1;
            }
        }
        if opts.edges {
            stats.edges = self.edges(scene, proj, opts, out);
            if boxed.is_some() {
                for cap in opts.caps.iter() {
                    if !opts.visible.get(cap.item).copied().unwrap_or(false) {
                        continue;
                    }
                    for [a, b] in &cap.outline {
                        let (va, vb) = (proj.view(*a), proj.view(*b));
                        if !proj.parallel && (va.z < proj.near || vb.z < proj.near) {
                            continue;
                        }
                        let (Some(sa), Some(sb)) = (proj.screen_of_view(va), proj.screen_of_view(vb)) else {
                            continue;
                        };
                        let f = |q: [f64; 2]| [q[0] as f32, q[1] as f32];
                        stats.edges +=
                            self.visible_runs(f(sa), f(sb), va.z as f32, vb.z as f32, proj.parallel, out);
                    }
                }
            }
        }
        stats
    }

    /// Drops triangles the depth buffer shows to be hidden, and splits partly hidden ones (in screen space, with
    /// perspective-correct depth) until the pieces are a couple of pixels small, keeping only visible pieces. What
    /// remains overlaps only along true visibility boundaries, so sorting by depth draws it correctly even where
    /// solids interpenetrate or are cut open.
    fn refine(&mut self, stats: &mut RenderStats) {
        let order = std::mem::take(&mut self.order);
        let mut stack: Vec<Prim> = Vec::new();
        for &(depth, k) in &order {
            let Some(p) = self.prims.get(k as usize).copied() else { continue };
            match self.depth.classify(p.pts, p.z) {
                Coverage::Hidden => stats.occluded += 1,
                Coverage::Visible => self.order.push((depth, k)),
                Coverage::Partial => {
                    stack.push(p);
                    while let Some(q) = stack.pop() {
                        match self.depth.classify(q.pts, q.z) {
                            Coverage::Hidden => {}
                            Coverage::Visible => self.push(q),
                            Coverage::Partial if self.depth.small(q.pts) => self.push(q),
                            Coverage::Partial => stack.extend(split(&q, self.depth.parallel)),
                        }
                    }
                }
            }
        }
    }

    fn push(&mut self, p: Prim) {
        let depth = (p.z[0] + p.z[1] + p.z[2]) / 3.0;
        self.prims.push(p);
        self.order.push((depth, (self.prims.len() - 1) as u32));
    }

    /// Draws the visible parts of the feature edges; returns the number of pieces drawn.
    fn edges(
        &mut self,
        scene: &Prepared3d,
        proj: &Projection,
        opts: &RenderOptions,
        out: &mut Mesh,
    ) -> usize {
        let mut pieces = 0;
        for (ii, item) in scene.items.iter().enumerate() {
            if !opts.visible.get(ii).copied().unwrap_or(false) {
                continue;
            }
            if let Some(b) = &opts.section_box
                && !b.intersects(&item.bounds)
            {
                continue;
            }
            let cut = opts.section_box.is_some_and(|b| !b.contains_box(&item.bounds));
            for ei in item.edges.clone() {
                let Some(e) = scene.edges.get(ei) else { continue };
                let [i, j] = e.v.map(|v| v as usize);
                if cut
                    && let Some(bx) = &opts.section_box
                    && [i, j].iter().any(|&k| scene.positions.get(k).is_some_and(|p| !bx.contains(*p, 1e-9)))
                {
                    continue;
                }
                let (Some(a), Some(b)) = (self.view.get(i), self.view.get(j)) else { continue };
                if !proj.parallel && (a.z < proj.near || b.z < proj.near) {
                    continue;
                }
                let (Some(sa), Some(sb)) = (self.screen.get(i), self.screen.get(j)) else { continue };
                pieces += self.visible_runs(*sa, *sb, a.z as f32, b.z as f32, proj.parallel, out);
            }
        }
        pieces
    }

    /// Samples an edge against the depth buffer and draws its visible runs.
    fn visible_runs(
        &self,
        sa: [f32; 2],
        sb: [f32; 2],
        za: f32,
        zb: f32,
        parallel: bool,
        out: &mut Mesh,
    ) -> usize {
        let len = ((sb[0] - sa[0]).powi(2) + (sb[1] - sa[1]).powi(2)).sqrt();
        if !len.is_finite() {
            return 0;
        }
        let n = ((len / 1.5).ceil() as usize).clamp(1, 4000);
        let mut pieces = 0;
        let mut run_start: Option<f32> = None;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let x = sa[0] + (sb[0] - sa[0]) * t;
            let y = sa[1] + (sb[1] - sa[1]) * t;
            // Perspective-correct depth along the edge.
            let z = if parallel {
                za + (zb - za) * t
            } else {
                1.0 / ((1.0 - t) / za.max(1e-6) + t / zb.max(1e-6))
            };
            let visible = self.depth.visible(x, y, z);
            match (visible, run_start) {
                (true, None) => run_start = Some(t),
                (false, Some(s)) => {
                    let e = (k as f32 - 0.5) / n as f32;
                    pieces += self.emit(sa, sb, s, e.max(s), out);
                    run_start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = run_start {
            pieces += self.emit(sa, sb, s, 1.0, out);
        }
        pieces
    }

    fn emit(&self, sa: [f32; 2], sb: [f32; 2], t0: f32, t1: f32, out: &mut Mesh) -> usize {
        let p = |t: f32| pos2(sa[0] + (sb[0] - sa[0]) * t, sa[1] + (sb[1] - sa[1]) * t);
        push_line(out, p(t0), p(t1), 0.9, EDGE);
        1
    }

    /// Clips a world-space triangle against the section box and the near plane, then queues the pieces.
    fn clip_and_push(
        &mut self,
        world: &[V3],
        proj: &Projection,
        boxed: Option<&Aabb>,
        color: Color32,
        transparent: bool,
        stats: &mut RenderStats,
    ) {
        let mut poly: Vec<V3> = world.to_vec();
        if let Some(b) = boxed {
            for (axis, lo, hi) in [(0, b.min.x, b.max.x), (1, b.min.y, b.max.y), (2, b.min.z, b.max.z)] {
                poly = clip_plane(&poly, |p| comp(p, axis) - lo);
                poly = clip_plane(&poly, |p| hi - comp(p, axis));
                if poly.len() < 3 {
                    return;
                }
            }
        }
        let mut view: Vec<V3> = poly.iter().map(|p| proj.view(*p)).collect();
        if !proj.parallel {
            view = clip_plane(&view, |q| q.z - proj.near);
        }
        if view.len() < 3 {
            return;
        }
        let screen: Vec<[f32; 2]> =
            view.iter().filter_map(|q| proj.screen_of_view(*q)).map(|[x, y]| [x as f32, y as f32]).collect();
        if screen.len() != view.len() {
            return;
        }
        let (Some(s0), Some(s1), Some(s2)) = (screen.first(), screen.get(1), screen.get(2)) else { return };
        let area = (s1[0] - s0[0]) * (s2[1] - s0[1]) - (s2[0] - s0[0]) * (s1[1] - s0[1]);
        let front = area < 0.0;
        if !front && !transparent {
            stats.culled += 1;
            return;
        }
        for k in 1..screen.len().saturating_sub(1) {
            let (Some(a), Some(b), Some(c)) = (screen.first(), screen.get(k), screen.get(k + 1)) else {
                continue;
            };
            let (Some(za), Some(zb), Some(zc)) = (view.first(), view.get(k), view.get(k + 1)) else {
                continue;
            };
            self.push(Prim {
                pts: [*a, *b, *c],
                z: [za.z as f32, zb.z as f32, zc.z as f32],
                color,
                solid: !transparent,
            });
        }
    }
}

impl SceneRenderer for SoftwareRenderer {
    fn paint(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &Prepared3d,
        proj: &Projection,
        opts: &RenderOptions,
    ) -> RenderStats {
        let mut mesh = Mesh::default();
        let stats = self.render(rect, scene, proj, opts, &mut mesh);
        if !mesh.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
        stats
    }
}

/// How much of a triangle is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coverage {
    Hidden,
    Partial,
    Visible,
}

/// Splits a screen triangle into four at its edge midpoints, with perspective-correct depths.
fn split(p: &Prim, parallel: bool) -> [Prim; 4] {
    let [a, b, c] = p.pts;
    let [za, zb, zc] = p.z;
    let mid = |p: [f32; 2], q: [f32; 2]| [(p[0] + q[0]) * 0.5, (p[1] + q[1]) * 0.5];
    let zmid =
        |u: f32, v: f32| if parallel { (u + v) * 0.5 } else { 2.0 / (1.0 / u.max(1e-6) + 1.0 / v.max(1e-6)) };
    let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
    let (zab, zbc, zca) = (zmid(za, zb), zmid(zb, zc), zmid(zc, za));
    let child = |pts: [[f32; 2]; 3], z: [f32; 3]| Prim { pts, z, ..*p };
    [
        child([a, ab, ca], [za, zab, zca]),
        child([ab, b, bc], [zab, zb, zbc]),
        child([ca, bc, c], [zca, zbc, zc]),
        child([ab, bc, ca], [zab, zbc, zca]),
    ]
}

/// A depth buffer over the viewport, for edge visibility. Stores view depth (nearest wins).
#[derive(Debug, Default)]
struct DepthBuffer {
    w: usize,
    h: usize,
    origin: [f32; 2],
    res: f32,
    parallel: bool,
    data: Vec<f32>,
}

impl DepthBuffer {
    fn reset(&mut self, rect: Rect, res: f32, parallel: bool) {
        self.w = ((rect.width() * res).ceil().max(1.0) as usize).min(8192);
        self.h = ((rect.height() * res).ceil().max(1.0) as usize).min(8192);
        self.origin = [rect.min.x, rect.min.y];
        self.res = res;
        self.parallel = parallel;
        self.data.clear();
        self.data.resize(self.w * self.h, f32::INFINITY);
    }

    /// Rasterises a triangle (screen points, view depths) with pixel-centre sampling.
    fn raster(&mut self, pts: [[f32; 2]; 3], z: [f32; 3]) {
        let s = self.res;
        let p = pts.map(|q| [(q[0] - self.origin[0]) * s, (q[1] - self.origin[1]) * s]);
        if p.iter().any(|q| !(q[0].is_finite() && q[1].is_finite())) {
            return;
        }
        let [a, b, c] = p;
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-6 {
            return;
        }
        // Interpolate 1/z (perspective) or z (parallel) linearly in screen space.
        let attr = |v: f32| if self.parallel { v } else { 1.0 / v.max(1e-6) };
        let [za, zb, zc] = z.map(attr);
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(self.w);
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let inv = 1.0 / area;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let row = y * self.w;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let w0 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) * inv;
                let w1 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let v = w0 * za + w1 * zb + w2 * zc;
                let depth = if self.parallel { v } else { 1.0 / v.max(1e-12) };
                if let Some(d) = self.data.get_mut(row + x)
                    && depth < *d
                {
                    *d = depth;
                }
            }
        }
    }

    /// How much of a triangle the stored depths leave visible (pixel-centre sampling). Triangles covering no
    /// pixel centre count as visible.
    fn classify(&self, pts: [[f32; 2]; 3], z: [f32; 3]) -> Coverage {
        let s = self.res;
        let p = pts.map(|q| [(q[0] - self.origin[0]) * s, (q[1] - self.origin[1]) * s]);
        if p.iter().any(|q| !(q[0].is_finite() && q[1].is_finite())) {
            return Coverage::Visible;
        }
        let [a, b, c] = p;
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-6 {
            return Coverage::Visible;
        }
        let attr = |v: f32| if self.parallel { v } else { 1.0 / v.max(1e-6) };
        let [za, zb, zc] = z.map(attr);
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(self.w);
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return Coverage::Hidden;
        }
        let inv = 1.0 / area;
        let (mut seen, mut hidden) = (false, false);
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let row = y * self.w;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let w0 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) * inv;
                let w1 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let v = w0 * za + w1 * zb + w2 * zc;
                let depth = if self.parallel { v } else { 1.0 / v.max(1e-12) };
                let stored = self.data.get(row + x).copied().unwrap_or(f32::INFINITY);
                if depth <= stored + 0.002 + depth.abs() * 0.0005 {
                    seen = true;
                } else {
                    hidden = true;
                }
                if seen && hidden {
                    return Coverage::Partial;
                }
            }
        }
        if hidden { Coverage::Hidden } else { Coverage::Visible }
    }

    /// Whether a triangle is at most two buffer pixels across.
    fn small(&self, pts: [[f32; 2]; 3]) -> bool {
        let xs = pts.map(|q| q[0]);
        let ys = pts.map(|q| q[1]);
        let w = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - xs.iter().copied().fold(f32::INFINITY, f32::min);
        let h = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - ys.iter().copied().fold(f32::INFINITY, f32::min);
        w.max(h) * self.res <= 2.0
    }

    /// Whether a point at depth `z` (screen position `x, y`) is in front of the stored faces, with a tolerance
    /// that covers sampling and the slope of faces meeting at an edge. The farthest of the 2 × 2 neighbourhood is
    /// used, so edges on the outline of faces count as visible.
    fn visible(&self, x: f32, y: f32, z: f32) -> bool {
        let fx = (x - self.origin[0]) * self.res - 0.5;
        let fy = (y - self.origin[1]) * self.res - 0.5;
        if !(fx.is_finite() && fy.is_finite()) {
            return false;
        }
        let (x0, y0) = (fx.floor(), fy.floor());
        let mut far = f32::NEG_INFINITY;
        for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (xx, yy) = (x0 + dx, y0 + dy);
            if xx < 0.0 || yy < 0.0 || xx >= self.w as f32 || yy >= self.h as f32 {
                return true;
            }
            let d = self.data.get(yy as usize * self.w + xx as usize).copied().unwrap_or(f32::INFINITY);
            far = far.max(d);
        }
        let tol = 0.003 + z.abs() * 0.0006;
        z <= far + tol
    }
}

fn comp(p: &V3, axis: usize) -> f64 {
    match axis {
        0 => p.x,
        1 => p.y,
        _ => p.z,
    }
}

/// Sutherland–Hodgman clipping of a convex polygon against the half-space `f(p) >= 0`.
pub fn clip_plane(poly: &[V3], f: impl Fn(&V3) -> f64) -> Vec<V3> {
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
            out.push(a + (b - a) * t);
        }
    }
    out
}

fn push_tri(out: &mut Mesh, p: [[f32; 2]; 3], color: Color32) {
    if p.iter().any(|q| !(q[0].is_finite() && q[1].is_finite())) {
        return;
    }
    let base = out.vertices.len() as u32;
    for q in p {
        out.vertices.push(Vertex { pos: pos2(q[0], q[1]), uv: WHITE_UV, color });
    }
    out.add_triangle(base, base + 1, base + 2);
}

/// A thin anti-aliased line as a quad with feathered sides.
fn push_line(out: &mut Mesh, a: Pos2, b: Pos2, width: f32, color: Color32) {
    if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
        return;
    }
    let d = b - a;
    let len = d.length();
    if len < 0.01 {
        return;
    }
    let n = egui::vec2(-d.y, d.x) / len;
    let core = width * 0.5;
    let feather = core + 0.8;
    let clear = Color32::TRANSPARENT;
    let base = out.vertices.len() as u32;
    for (p, off, c) in [
        (a, -feather, clear),
        (a, -core, color),
        (a, core, color),
        (a, feather, clear),
        (b, -feather, clear),
        (b, -core, color),
        (b, core, color),
        (b, feather, clear),
    ] {
        out.vertices.push(Vertex { pos: p + n * off, uv: WHITE_UV, color: c });
    }
    for k in 0..3 {
        out.add_triangle(base + k, base + k + 1, base + 4 + k);
        out.add_triangle(base + k + 1, base + 5 + k, base + 4 + k);
    }
}

#[cfg(test)]
mod tests {
    use super::super::camera::{OrbitCamera, v3};
    use super::super::scene::Prepared3d;
    use super::*;
    use kanavos_geom::Point3;
    use kanavos_model::{Category, Id};

    fn cubes(colors: [kanavos_drawing::Rgba; 2]) -> Prepared3d {
        let item = |id: u64, x: f64, color: kanavos_drawing::Rgba| kanavos_views::SceneItem {
            element: Id(id),
            category: Category::Wall,
            storey: None,
            material: None,
            mesh: kanavos_solid::cuboid(Point3::new(x, 0.0, 0.0), Point3::new(x + 1.0, 1.0, 1.0)),
            color,
        };
        Prepared3d::new(&kanavos_views::Scene {
            items: vec![item(1, 0.0, colors[0]), item(2, 3.0, colors[1])],
        })
    }

    fn scene() -> Prepared3d {
        let c = kanavos_drawing::Rgba::rgb(200, 190, 180);
        cubes([c, c])
    }

    const RECT: Rect = Rect { min: Pos2 { x: 0.0, y: 0.0 }, max: Pos2 { x: 800.0, y: 600.0 } };

    fn render(s: &Prepared3d, opts: &RenderOptions) -> (Mesh, RenderStats) {
        let mut c = OrbitCamera::default();
        if let Some(b) = &s.bounds {
            c.fit(b, 4.0 / 3.0);
        }
        let p = c.projection([800.0, 600.0], [400.0, 300.0]);
        let mut m = Mesh::default();
        let st = SoftwareRenderer::default().render(RECT, s, &p, opts, &mut m);
        (m, st)
    }

    #[test]
    fn back_faces_are_culled() {
        let s = scene();
        let opts = RenderOptions { visible: vec![true, true], ..RenderOptions::default() };
        let (m, st) = render(&s, &opts);
        // A cube seen from a general direction shows three of its six faces: half of its triangles.
        assert_eq!(st.triangles, s.tris.len() / 2);
        assert_eq!(st.culled, s.tris.len() / 2);
        assert!(m.is_valid());
    }

    #[test]
    fn hidden_items_are_not_drawn_and_hidden_edges_are_removed() {
        let s = scene();
        let opts = RenderOptions { visible: vec![true, false], edges: true, ..RenderOptions::default() };
        let (_, st) = render(&s, &opts);
        let first = s.items.first().unwrap();
        assert_eq!(st.triangles, first.tris.len() / 2);
        // Of a cube's 12 edges, the 9 on its three visible faces show; the 3 meeting at the hidden corner do not.
        let pieces_per_edge = first.edges.len() / 12;
        assert!(
            st.edges >= 9 * pieces_per_edge && st.edges < 12 * pieces_per_edge,
            "{st:?} of {}",
            first.edges.len()
        );
    }

    #[test]
    fn the_section_box_cuts_and_shows_the_inside() {
        let s = scene();
        let bx = Aabb { min: v3(-1.0, -1.0, -1.0), max: v3(0.5, 2.0, 2.0) };
        let opts =
            RenderOptions { visible: vec![true, true], section_box: Some(bx), ..RenderOptions::default() };
        let caps = std::sync::Arc::new(super::super::scene::section_caps(&s, &bx));
        assert_eq!(caps.len(), 1);
        let opts = RenderOptions { caps, ..opts };
        let (m, st) = render(&s, &opts);
        assert!(st.triangles > 0);
        assert!(m.vertices.iter().any(|v| v.color.r() < 150), "the cap is drawn in the cut colour");
        let empty = Aabb { min: v3(10.0, 10.0, 10.0), max: v3(11.0, 11.0, 11.0) };
        let opts =
            RenderOptions { visible: vec![true, true], section_box: Some(empty), ..RenderOptions::default() };
        assert_eq!(render(&s, &opts).1.triangles, 0);
    }

    #[test]
    fn nearer_triangles_are_drawn_later() {
        let s = cubes([kanavos_drawing::Rgba::rgb(10, 10, 250), kanavos_drawing::Rgba::rgb(250, 10, 10)]);
        let opts = RenderOptions { visible: vec![true, true], ..RenderOptions::default() };
        // Looking from +X: the red cube at x = 3..4 is nearer and must come last.
        let c = OrbitCamera {
            target: v3(2.0, 0.5, 0.5),
            yaw: 0.0,
            pitch: 0.2,
            distance: 12.0,
            ..OrbitCamera::default()
        };
        let p = c.projection([800.0, 600.0], [400.0, 300.0]);
        let mut m = Mesh::default();
        SoftwareRenderer::default().render(RECT, &s, &p, &opts, &mut m);
        let first = m.vertices.first().unwrap().color;
        let last = m.vertices.last().unwrap().color;
        assert!(first.b() > first.r(), "far blue cube first");
        assert!(last.r() > last.b(), "near red cube last");
    }

    #[test]
    fn clipping_keeps_the_inside_part() {
        let tri = [v3(0.0, 0.0, 0.0), v3(2.0, 0.0, 0.0), v3(0.0, 2.0, 0.0)];
        let out = clip_plane(&tri, |p| 1.0 - p.x);
        assert!(out.iter().all(|p| p.x <= 1.0 + 1e-12));
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn depth_buffer_keeps_the_nearest() {
        let mut d = DepthBuffer::default();
        d.reset(RECT, 1.0, true);
        d.raster([[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]], [5.0, 5.0, 5.0]);
        d.raster([[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]], [9.0, 9.0, 9.0]);
        assert!(d.visible(20.0, 20.0, 5.0));
        assert!(!d.visible(20.0, 20.0, 8.0));
        assert!(d.visible(300.0, 300.0, 50.0));
    }
}

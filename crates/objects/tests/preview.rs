//! Visual previews of the whole object catalogue.
//!
//! Renders every catalogue entry's plan symbol (at 1:50 and 1:100, line weights to scale at 150 dpi) and a
//! shaded isometric view of its 3D parts, plus a door and a window in a wall in all four flip states, into PNG
//! sheets under `target/tmp/objects-preview/`. The sheets are for people to look at when changing objects; the
//! test itself checks only that every cell received ink.
//!
//! Each wall runs from left to right, so its left face is at the top of the plan: unflipped (`side=0`), windows
//! show their outer face (sill, shutters, roller box) at the top and doors open downwards
//! (`specs/objects/orientation.md`). The 3D views look from the bottom of the plan, at the right face.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::type_complexity
)]

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use kanavos_drawing::{Fill, Item, Rect, Rgba, Shape, pt};
use kanavos_geom::Point2;
use kanavos_model::{Id, Object, Opening, OpeningKind};
use kanavos_objects::{
    ObjectCategory, ObjectDef, OpeningContext, Part, catalogue, find, object_footprint, object_parts,
    object_symbol_at, opening_parts, opening_symbol_at,
};
use kanavos_units::{Angle, Length};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke as SkStroke, StrokeDash, Transform,
};

/// Pixels per paper millimetre (150 dpi).
const PX_MM: f64 = 150.0 / 25.4;
/// Wall thickness used for openings.
const WALL: f64 = 0.3;

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("objects-preview");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------------------------------------
// A tiny 5×7 bitmap font for labels.

fn glyph(c: char) -> [&'static str; 7] {
    match c.to_ascii_lowercase() {
        'a' => [".....", ".....", ".###.", "....#", ".####", "#...#", ".####"],
        'b' => ["#....", "#....", "#.##.", "##..#", "#...#", "#...#", "####."],
        'c' => [".....", ".....", ".###.", "#....", "#....", "#...#", ".###."],
        'd' => ["....#", "....#", ".##.#", "#..##", "#...#", "#...#", ".####"],
        'e' => [".....", ".....", ".###.", "#...#", "#####", "#....", ".###."],
        'f' => ["..##.", ".#..#", ".#...", "###..", ".#...", ".#...", ".#..."],
        'g' => [".....", ".####", "#...#", "#...#", ".####", "....#", ".###."],
        'h' => ["#....", "#....", "#.##.", "##..#", "#...#", "#...#", "#...#"],
        'i' => ["..#..", ".....", ".##..", "..#..", "..#..", "..#..", ".###."],
        'j' => ["...#.", ".....", "..##.", "...#.", "...#.", "#..#.", ".##.."],
        'k' => ["#....", "#....", "#..#.", "#.#..", "##...", "#.#..", "#..#."],
        'l' => [".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."],
        'm' => [".....", ".....", "##.#.", "#.#.#", "#.#.#", "#...#", "#...#"],
        'n' => [".....", ".....", "#.##.", "##..#", "#...#", "#...#", "#...#"],
        'o' => [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###."],
        'p' => [".....", ".....", "####.", "#...#", "####.", "#....", "#...."],
        'q' => [".....", ".....", ".##.#", "#..##", ".####", "....#", "....#"],
        'r' => [".....", ".....", "#.##.", "##..#", "#....", "#....", "#...."],
        's' => [".....", ".....", ".###.", "#....", ".###.", "....#", "####."],
        't' => [".#...", ".#...", "###..", ".#...", ".#...", ".#..#", "..##."],
        'u' => [".....", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#"],
        'v' => [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
        'w' => [".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#."],
        'x' => [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#"],
        'y' => [".....", ".....", "#...#", "#...#", ".####", "....#", ".###."],
        'z' => [".....", ".....", "#####", "...#.", "..#..", ".#...", "#####"],
        '0' => [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
        '2' => [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
        '3' => ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."],
        '4' => ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
        '5' => ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
        '6' => ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
        '7' => ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
        '8' => [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
        '9' => [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
        '.' => [".....", ".....", ".....", ".....", ".....", ".##..", ".##.."],
        '_' => [".....", ".....", ".....", ".....", ".....", ".....", "#####"],
        '-' => [".....", ".....", ".....", "#####", ".....", ".....", "....."],
        ':' => [".....", ".##..", ".##..", ".....", ".##..", ".##..", "....."],
        '(' => ["...#.", "..#..", ".#...", ".#...", ".#...", "..#..", "...#."],
        ')' => [".#...", "..#..", "...#.", "...#.", "...#.", "..#..", ".#..."],
        '/' => [".....", "....#", "...#.", "..#..", ".#...", "#....", "....."],
        '=' => [".....", ".....", "#####", ".....", "#####", ".....", "....."],
        '+' => [".....", "..#..", "..#..", "#####", "..#..", "..#..", "....."],
        _ => [".....", ".....", ".....", ".....", ".....", ".....", "....."],
    }
}

fn label(pm: &mut Pixmap, x: f32, y: f32, text: &str, scale: f32) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(40, 40, 120, 255);
    let mut cx = x;
    for c in text.chars() {
        for (r, row) in glyph(c).iter().enumerate() {
            for (k, ch) in row.chars().enumerate() {
                if ch == '#' {
                    let rect =
                        tiny_skia::Rect::from_xywh(cx + k as f32 * scale, y + r as f32 * scale, scale, scale);
                    if let Some(rect) = rect {
                        pm.fill_rect(rect, &paint, Transform::identity(), None);
                    }
                }
            }
        }
        cx += 6.0 * scale;
    }
}

// ---------------------------------------------------------------------------------------------------------
// Plan drawing

/// Maps metres to pixels: `px = ox + x·k`, `py = oy − y·k`.
#[derive(Clone, Copy)]
struct View {
    ox: f64,
    oy: f64,
    k: f64,
}

impl View {
    fn p(&self, x: f64, y: f64) -> (f32, f32) {
        ((self.ox + x * self.k) as f32, (self.oy - y * self.k) as f32)
    }
}

fn rgba(c: Rgba) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(c.r, c.g, c.b, c.a);
    paint.anti_alias = true;
    paint
}

fn path_of(shape: &Shape, v: &View) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let poly = |pb: &mut PathBuilder, pts: &[(f64, f64)], closed: bool| {
        for (i, (x, y)) in pts.iter().enumerate() {
            let (px, py) = v.p(*x, *y);
            if i == 0 {
                pb.move_to(px, py);
            } else {
                pb.line_to(px, py);
            }
        }
        if closed {
            pb.close();
        }
    };
    match shape {
        Shape::Line { a, b } => poly(&mut pb, &[(a.x, a.y), (b.x, b.y)], false),
        Shape::Polyline { points, closed } => {
            poly(&mut pb, &points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(), *closed)
        }
        Shape::Polygon { rings } => {
            for r in rings {
                poly(&mut pb, &r.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(), true);
            }
        }
        Shape::Arc { centre, radius, start, sweep } => {
            let n = ((sweep.abs() / std::f64::consts::TAU) * 96.0).ceil().max(4.0) as usize;
            let pts: Vec<(f64, f64)> = (0..=n)
                .map(|i| {
                    let a = start + sweep * i as f64 / n as f64;
                    (centre.x + radius * a.cos(), centre.y + radius * a.sin())
                })
                .collect();
            poly(&mut pb, &pts, false);
        }
        Shape::Circle { centre, radius } => {
            let (cx, cy) = v.p(centre.x, centre.y);
            pb.push_circle(cx, cy, (radius * v.k) as f32);
        }
        Shape::Text(_) => return None,
    }
    pb.finish()
}

fn draw_items(pm: &mut Pixmap, items: &[Item], v: &View) {
    for it in items {
        let Some(path) = path_of(&it.shape, v) else { continue };
        if let Some(fill) = &it.fill {
            let c = match fill {
                Fill::Solid(c) => *c,
                Fill::Hatch { background, .. } => background.unwrap_or(Rgba::LIGHT_GREY),
            };
            pm.fill_path(&path, &rgba(c), FillRule::EvenOdd, Transform::identity(), None);
        }
        if let Some(st) = &it.stroke {
            let mut s = SkStroke { width: (st.weight_mm * PX_MM).max(0.6) as f32, ..SkStroke::default() };
            s.line_cap = LineCap::Butt;
            s.line_join = LineJoin::Miter;
            let dashes: Vec<f32> = st.pattern.dashes_mm().iter().map(|d| (d * PX_MM) as f32).collect();
            if dashes.len() >= 2 && dashes.len().is_multiple_of(2) {
                s.dash = StrokeDash::new(dashes, 0.0);
            }
            pm.stroke_path(&path, &rgba(st.color), &s, Transform::identity(), None);
        }
    }
}

fn rect_px(pm: &mut Pixmap, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgba) {
    if let Some(r) = tiny_skia::Rect::from_ltrb(x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)) {
        pm.fill_rect(r, &rgba(c), Transform::identity(), None);
    }
}

/// The host wall around an opening, as a plan would draw it: cut wall with a light poché, and the faces seen
/// below a raised sill.
fn wall_items(w: f64, raised: bool) -> Vec<Item> {
    use kanavos_drawing::{Stroke, weights};
    let (yl, yr) = (WALL / 2.0, -WALL / 2.0);
    let ext = 0.55;
    let mut v = Vec::new();
    // Each piece: its far end is open, its end at the opening is cut.
    for (far, near) in [(-w / 2.0 - ext, -w / 2.0), (w / 2.0 + ext, w / 2.0)] {
        v.push(Item::polygon(
            vec![vec![pt(far, yr), pt(near, yr), pt(near, yl), pt(far, yl)]],
            Some(Fill::Solid(Rgba::rgb(222, 222, 222))),
            None,
            "wall",
        ));
        v.push(Item::polyline(
            vec![pt(far, yl), pt(near, yl), pt(near, yr), pt(far, yr)],
            false,
            Stroke::solid(weights::THICK),
            "wall",
        ));
    }
    if raised {
        for y in [yl, yr] {
            v.push(Item::line(pt(-w / 2.0, y), pt(w / 2.0, y), Stroke::solid(weights::FINE), "wall.below"));
        }
    }
    v
}

fn bounds(items: &[Item]) -> Option<Rect> {
    kanavos_drawing::Drawing { items: items.to_vec(), ..kanavos_drawing::Drawing::model(50.0) }.bounds()
}

// ---------------------------------------------------------------------------------------------------------
// 3D drawing

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// An orthographic camera looking from `eye` (a direction) towards the origin.
struct Camera {
    eye: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
}

impl Camera {
    fn new(eye: [f64; 3]) -> Camera {
        let eye = norm(eye);
        let right = norm([-eye[1], eye[0], 0.0]);
        let up = cross(eye, right);
        Camera { eye, right, up }
    }
    fn project(&self, p: [f64; 3]) -> (f64, f64) {
        (dot(p, self.right), dot(p, self.up))
    }
}

fn shade(c: Rgba, n: [f64; 3]) -> Rgba {
    let light = norm([-0.35, -0.75, 0.9]);
    let k = 0.5 + 0.5 * dot(n, light).max(0.0);
    let f = |v: u8| ((v as f64) * k).round().clamp(0.0, 255.0) as u8;
    Rgba::new(f(c.r), f(c.g), f(c.b), c.a)
}

/// Draws the parts flat-shaded with feature edges, fitted into the pixel box, with a z-buffer at 3× resolution
/// (opaque surfaces first, then glass blended back to front).
fn draw_parts(pm: &mut Pixmap, parts: &[Part], cam: &Camera, bx: (f32, f32, f32, f32), max_k: f64) {
    const SS: usize = 3;
    struct Tri {
        pts: [[f64; 3]; 3],
        color: Rgba,
        edges: Vec<usize>,
    }
    let q = |p: [f64; 3]| -> (i64, i64, i64) {
        ((p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64, (p[2] * 1e4).round() as i64)
    };
    let mut tris: Vec<Tri> = Vec::new();
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for part in parts {
        let all: Vec<[[f64; 3]; 3]> = part.mesh.triangle_points().map(|t| t.map(|p| p.to_array())).collect();
        let normals: Vec<[f64; 3]> =
            all.iter().map(|t| norm(cross(sub(t[1], t[0]), sub(t[2], t[0])))).collect();
        let mut edge_map: HashMap<((i64, i64, i64), (i64, i64, i64)), Vec<usize>> = HashMap::new();
        for (i, t) in all.iter().enumerate() {
            for e in 0..3 {
                let (a, b) = (q(t[e]), q(t[(e + 1) % 3]));
                let key = if a < b { (a, b) } else { (b, a) };
                edge_map.entry(key).or_default().push(i);
            }
        }
        for (i, t) in all.iter().enumerate() {
            for p in t {
                let (x, y) = cam.project(*p);
                minx = minx.min(x);
                maxx = maxx.max(x);
                miny = miny.min(y);
                maxy = maxy.max(y);
            }
            let n = normals[i];
            if dot(n, cam.eye) <= 1e-9 {
                continue;
            }
            let mut edges = Vec::new();
            for e in 0..3 {
                let (a, b) = (q(t[e]), q(t[(e + 1) % 3]));
                let key = if a < b { (a, b) } else { (b, a) };
                let others = &edge_map[&key];
                let feature = others.len() < 2
                    || others
                        .iter()
                        .any(|&j| j != i && (dot(normals[j], n) < 0.9 || dot(normals[j], cam.eye) <= 1e-9));
                if feature {
                    edges.push(e);
                }
            }
            tris.push(Tri { pts: *t, color: shade(part.color, n), edges });
        }
    }
    if tris.is_empty() {
        return;
    }
    let (bw, bh) = ((bx.2 - bx.0).max(1.0) as f64, (bx.3 - bx.1).max(1.0) as f64);
    let k = (bw / (maxx - minx).max(1e-6)).min(bh / (maxy - miny).max(1e-6)).min(max_k) * 0.95;
    let (w, h) = (bw as usize * SS, bh as usize * SS);
    let ks = k * SS as f64;
    let ox = w as f64 / 2.0 - (minx + maxx) / 2.0 * ks;
    let oy = h as f64 / 2.0 + (miny + maxy) / 2.0 * ks;
    // Screen x, y and depth (larger is nearer).
    let to_px = |p: [f64; 3]| {
        let (x, y) = cam.project(p);
        (ox + x * ks, oy - y * ks, dot(p, cam.eye))
    };
    let mut zbuf = vec![f64::MIN; w * h];
    let mut cbuf = vec![[0.0_f64; 4]; w * h];
    let raster = |t: &Tri, zbuf: &mut Vec<f64>, cbuf: &mut Vec<[f64; 4]>, write_depth: bool| {
        let p: Vec<(f64, f64, f64)> = t.pts.iter().map(|p| to_px(*p)).collect();
        let (x0, x1) =
            (p.iter().map(|v| v.0).fold(f64::MAX, f64::min), p.iter().map(|v| v.0).fold(f64::MIN, f64::max));
        let (y0, y1) =
            (p.iter().map(|v| v.1).fold(f64::MAX, f64::min), p.iter().map(|v| v.1).fold(f64::MIN, f64::max));
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area.abs() < 1e-12 {
            return;
        }
        let c = [t.color.r as f64, t.color.g as f64, t.color.b as f64, t.color.a as f64 / 255.0];
        for py in (y0.floor().max(0.0) as usize)..((y1.ceil().max(0.0) as usize).min(h)) {
            for px in (x0.floor().max(0.0) as usize)..((x1.ceil().max(0.0) as usize).min(w)) {
                let (sx, sy) = (px as f64 + 0.5, py as f64 + 0.5);
                let w0 = ((p[1].0 - sx) * (p[2].1 - sy) - (p[2].0 - sx) * (p[1].1 - sy)) / area;
                let w1 = ((p[2].0 - sx) * (p[0].1 - sy) - (p[0].0 - sx) * (p[2].1 - sy)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let z = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                let i = py * w + px;
                if z >= zbuf[i] - 1e-7 {
                    if write_depth {
                        zbuf[i] = z;
                        cbuf[i] = [c[0], c[1], c[2], 1.0];
                    } else {
                        let a = c[3];
                        let d = &mut cbuf[i];
                        let base = if d[3] > 0.0 { [d[0], d[1], d[2]] } else { [255.0, 255.0, 255.0] };
                        *d = [
                            base[0] * (1.0 - a) + c[0] * a,
                            base[1] * (1.0 - a) + c[1] * a,
                            base[2] * (1.0 - a) + c[2] * a,
                            d[3].max(a),
                        ];
                    }
                }
            }
        }
    };
    let (opaque, mut glass): (Vec<&Tri>, Vec<&Tri>) = tris.iter().partition(|t| t.color.a == 255);
    for t in &opaque {
        raster(t, &mut zbuf, &mut cbuf, true);
    }
    // Feature edges, depth-tested against the opaque surfaces.
    let mut edge_buf = vec![false; w * h];
    for t in &opaque {
        for &e in &t.edges {
            let (a, b) = (to_px(t.pts[e]), to_px(t.pts[(e + 1) % 3]));
            let len = (b.0 - a.0).hypot(b.1 - a.1);
            let n = (len * 2.0).ceil().max(1.0) as usize;
            for s in 0..=n {
                let f = s as f64 / n as f64;
                let (x, y, z) = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (px, py) = (x as i64 + dx - 1, y as i64 + dy - 1);
                    if px < 0 || py < 0 || px as usize >= w || py as usize >= h {
                        continue;
                    }
                    let i = py as usize * w + px as usize;
                    if z >= zbuf[i] - 0.01 {
                        edge_buf[i] = true;
                    }
                }
            }
        }
    }
    glass.sort_by(|a, b| {
        let d = |t: &Tri| t.pts.iter().map(|p| dot(*p, cam.eye)).sum::<f64>();
        d(a).total_cmp(&d(b))
    });
    for t in glass {
        raster(t, &mut zbuf, &mut cbuf, false);
    }
    // Downsample into the pixmap.
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let data = pm.data_mut();
    for oyp in 0..(h / SS) {
        for oxp in 0..(w / SS) {
            let (tx, ty) = (bx.0 as usize + oxp, bx.1 as usize + oyp);
            if tx >= pw || ty >= ph {
                continue;
            }
            let mut acc = [0.0_f64; 3];
            let mut cov = 0.0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = (oyp * SS + sy) * w + oxp * SS + sx;
                    let mut c = cbuf[i];
                    if edge_buf[i] {
                        c = [
                            c[0] * 0.25 + 45.0 * 0.75,
                            c[1] * 0.25 + 45.0 * 0.75,
                            c[2] * 0.25 + 50.0 * 0.75,
                            1.0,
                        ];
                    }
                    if c[3] > 0.0 {
                        acc[0] += c[0];
                        acc[1] += c[1];
                        acc[2] += c[2];
                        cov += 1.0;
                    }
                }
            }
            if cov > 0.0 {
                let n = (SS * SS) as f64;
                let o = (ty * pw + tx) * 4;
                for ch in 0..3 {
                    let bg = data[o + ch] as f64;
                    data[o + ch] = ((acc[ch] + bg * (n - cov)) / n).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Sheets

fn opening(def: &ObjectDef, params: &[(&str, f64)], flip_side: bool, flip_hand: bool) -> Opening {
    Opening {
        host: Id(1),
        kind: match def.category {
            ObjectCategory::Door => OpeningKind::Door,
            ObjectCategory::Window => OpeningKind::Window,
            _ => OpeningKind::Void,
        },
        style: def.id.into(),
        offset: Length::m(2.0),
        width: Length::m(def.width),
        height: Length::m(def.height),
        sill: Length::m(def.sill),
        flip_side,
        flip_hand,
        params: params.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
    }
}

fn object(def: &ObjectDef, params: &[(&str, f64)]) -> Object {
    Object {
        definition: def.id.into(),
        position: Point2::new(0.0, 0.0),
        rotation: Angle::ZERO,
        mirrored: false,
        elevation: Length::ZERO,
        params: params.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect::<BTreeMap<_, _>>(),
    }
}

/// The 3D context of an opening: the wall around the hole, so frames read in place.
fn wall_parts(o: &Opening) -> Vec<Part> {
    use kanavos_geom::Point3;
    use kanavos_solid::cuboid;
    let (w, h, s) = (o.width.as_m(), o.height.as_m(), o.sill.as_m());
    let (y0, y1) = (-WALL / 2.0, WALL / 2.0);
    let ext = 0.45;
    let top = h + 0.35;
    let c = Rgba::rgb(236, 228, 214);
    let mut mesh = kanavos_solid::Mesh::new();
    mesh.append(&cuboid(Point3::new(-w / 2.0 - ext, y0, -s), Point3::new(-w / 2.0, y1, top)));
    mesh.append(&cuboid(Point3::new(w / 2.0, y0, -s), Point3::new(w / 2.0 + ext, y1, top)));
    mesh.append(&cuboid(Point3::new(-w / 2.0, y0, h), Point3::new(w / 2.0, y1, top)));
    if s > 0.01 {
        mesh.append(&cuboid(Point3::new(-w / 2.0, y0, -s), Point3::new(w / 2.0, y1, 0.0)));
    }
    vec![Part { mesh, color: c }]
}

/// One cell of a sheet: label, plan items (already including context), 3D parts.
struct Cell {
    label: String,
    items: Vec<Item>,
    parts: Vec<Part>,
    extent: Rect,
}

fn opening_cell(
    def: &ObjectDef,
    params: &[(&str, f64)],
    fs: bool,
    fh: bool,
    scale: f64,
    label: &str,
) -> Cell {
    let o = opening(def, params, fs, fh);
    let ctx = OpeningContext { opening: &o, left_face: WALL / 2.0, right_face: -WALL / 2.0 };
    let mut items = wall_items(def.width, def.sill > 0.01);
    items.extend(opening_symbol_at(&ctx, scale));
    let extent = bounds(&items).unwrap();
    let mut parts = wall_parts(&o);
    parts.extend(opening_parts(&ctx));
    Cell { label: label.to_owned(), items, parts, extent }
}

fn object_cell(def: &ObjectDef, params: &[(&str, f64)], scale: f64, label: &str) -> Cell {
    let obj = object(def, params);
    let items = object_symbol_at(&obj, scale);
    let fp = object_footprint(&obj);
    let extent = bounds(&items).map(|b| b.union(&fp)).unwrap_or(fp);
    Cell { label: label.to_owned(), items, parts: object_parts(&obj), extent }
}

/// Lays cells out in rows: plan at true scale on the left of each cell, 3D view on the right.
fn render_sheet(name: &str, cells: &[Cell], scale: f64, width_px: u32, with_3d: bool) {
    let k = 1000.0 / scale * PX_MM;
    let pad = 14.0;
    let label_h = 22.0;
    let iso = if with_3d { 220.0 } else { 0.0 };
    // Shelf packing.
    let mut placed = Vec::new();
    let (mut x, mut y, mut row_h) = (pad, pad, 0.0_f64);
    for c in cells {
        let w = (c.extent.width() * k).max(40.0) + iso + pad;
        let h = ((c.extent.height() * k).max(iso * 0.8) + label_h).max(60.0);
        if x + w > width_px as f64 && x > pad {
            x = pad;
            y += row_h + pad;
            row_h = 0.0;
        }
        placed.push((x, y, w, h));
        x += w + pad;
        row_h = row_h.max(h);
    }
    let height = (y + row_h + pad) as u32;
    let mut pm = Pixmap::new(width_px, height.max(10)).unwrap();
    pm.fill(tiny_skia::Color::WHITE);
    for (c, (x, y, w, h)) in cells.iter().zip(&placed) {
        rect_px(
            &mut pm,
            *x as f32 - 4.0,
            *y as f32 - 4.0,
            (*x + *w) as f32 - 6.0,
            (*y + *h) as f32 + 4.0,
            Rgba::rgb(246, 247, 250),
        );
        label(&mut pm, *x as f32, *y as f32 + 2.0, &c.label, 2.0);
        let plan_w = c.extent.width() * k;
        let v = View { ox: *x - c.extent.min.x * k, oy: *y + label_h + c.extent.max.y * k, k };
        let before = pm.data().iter().filter(|b| **b < 200).count();
        draw_items(&mut pm, &c.items, &v);
        let after = pm.data().iter().filter(|b| **b < 200).count();
        assert!(after > before, "{name}: {} drew nothing", c.label);
        if with_3d && !c.parts.is_empty() {
            let bx = (
                (*x + plan_w.max(40.0) + 6.0) as f32,
                (*y + label_h) as f32,
                (*x + *w - pad) as f32,
                (*y + *h) as f32,
            );
            let cam = Camera::new([-0.55, -1.0, 0.75]);
            draw_parts(&mut pm, &c.parts, &cam, bx, 160.0);
        }
    }
    let path = out_dir().join(format!("{name}.png"));
    pm.save_png(&path).unwrap();
    println!("wrote {}", path.display());
}

fn category_cells(cat: ObjectCategory, scale: f64) -> Vec<Cell> {
    catalogue()
        .iter()
        .filter(|d| d.category == cat)
        .map(|d| {
            if cat.is_opening() {
                opening_cell(d, &[], false, false, scale, d.id)
            } else {
                object_cell(d, &[], scale, d.id)
            }
        })
        .collect()
}

#[test]
fn render_catalogue_sheets() {
    // Openings, with variants of their options.
    let mut doors = category_cells(ObjectCategory::Door, 50.0);
    let d = |id: &str| find(id).unwrap();
    doors.push(opening_cell(d("door.pocket"), &[("leaves", 2.0)], false, false, 50.0, "pocket leaves=2"));
    doors.push(opening_cell(
        d("door.balcony"),
        &[("shutters", 1.0)],
        false,
        false,
        50.0,
        "balcony +shutters",
    ));
    doors.push(opening_cell(
        d("door.balcony_sliding"),
        &[("roller", 1.0), ("panes", 4.0)],
        false,
        false,
        50.0,
        "balc.sliding 4 +roller",
    ));
    doors.push(opening_cell(
        d("door.single_swing"),
        &[("swing", 45.0), ("threshold", 1.0), ("glazing", 60.0)],
        false,
        false,
        50.0,
        "swing=45 +threshold",
    ));
    render_sheet("doors", &doors, 50.0, 1800, true);

    let mut windows = category_cells(ObjectCategory::Window, 50.0);
    let w = |id: &str| find(id).unwrap();
    windows.push(opening_cell(
        w("window.casement_2"),
        &[("shutters", 1.0)],
        false,
        false,
        50.0,
        "casement_2 +shutters",
    ));
    windows.push(opening_cell(
        w("window.casement_2"),
        &[("roller", 1.0), ("transom", 0.4)],
        false,
        false,
        50.0,
        "casement_2 +roller +transom",
    ));
    windows.push(opening_cell(w("window.sliding"), &[("panes", 3.0)], false, false, 50.0, "sliding panes=3"));
    windows.push(opening_cell(
        w("window.sliding"),
        &[("panes", 4.0), ("shutters", 1.0)],
        false,
        false,
        50.0,
        "sliding panes=4 +shutters",
    ));
    windows.push(opening_cell(
        w("window.fixed"),
        &[("mullions", 2.0)],
        false,
        false,
        50.0,
        "fixed mullions=2",
    ));
    windows.extend(category_cells(ObjectCategory::Void, 50.0));
    render_sheet("windows", &windows, 50.0, 1800, true);

    for (name, cats) in [
        ("furniture", vec![ObjectCategory::Furniture]),
        ("kitchen", vec![ObjectCategory::Kitchen]),
        ("bathroom", vec![ObjectCategory::Sanitary, ObjectCategory::Equipment]),
        ("outdoor", vec![ObjectCategory::Vegetation, ObjectCategory::Vehicle]),
    ] {
        let cells: Vec<Cell> = cats.into_iter().flat_map(|c| category_cells(c, 50.0)).collect();
        render_sheet(name, &cells, 50.0, 1800, true);
    }

    // Everything at 1:100, plan only.
    let all: Vec<Cell> = ObjectCategory::ALL.iter().flat_map(|c| category_cells(*c, 100.0)).collect();
    render_sheet("all_1_100", &all, 100.0, 1800, false);
}

#[test]
fn render_details() {
    let d = |id: &str| find(id).unwrap();
    let cells = vec![
        opening_cell(
            d("door.single_swing"),
            &[("threshold", 1.0)],
            false,
            false,
            20.0,
            "door.single_swing 1:20",
        ),
        opening_cell(d("door.entrance"), &[], false, false, 20.0, "door.entrance 1:20"),
        opening_cell(
            d("window.casement_2"),
            &[("shutters", 1.0), ("roller", 1.0)],
            false,
            false,
            20.0,
            "casement_2 +shutters +roller 1:20",
        ),
        opening_cell(d("window.sliding"), &[], false, false, 20.0, "window.sliding 1:20"),
        object_cell(d("sanitary.wc"), &[], 20.0, "wc 1:20"),
        object_cell(d("kitchen.sink_double"), &[], 20.0, "sink_double 1:20"),
        object_cell(d("vehicle.car"), &[], 20.0, "car 1:20"),
        object_cell(d("vegetation.tree"), &[], 20.0, "tree"),
    ];
    render_sheet("details", &cells, 20.0, 1800, true);
}

#[test]
fn render_flip_states() {
    let mut cells = Vec::new();
    for (id, params) in [
        ("door.single_swing", vec![("threshold", 1.0)]),
        ("window.casement_2", vec![("shutters", 1.0)]),
        ("window.sliding", vec![("roller", 1.0)]),
    ] {
        let def = find(id).unwrap();
        for (fs, fh) in [(false, false), (false, true), (true, false), (true, true)] {
            let lbl = format!("{} side={} hand={}", id.split('.').nth(1).unwrap(), fs as u8, fh as u8);
            cells.push(opening_cell(def, &params, fs, fh, 50.0, &lbl));
        }
    }
    render_sheet("flips", &cells, 50.0, 1800, true);
}

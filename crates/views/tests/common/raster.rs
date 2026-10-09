//! A small rasteriser for drawings, used by the visual tests to write PNG files that people can look at.
//!
//! It renders strokes with their paper weights and dash patterns, fills and the hatch patterns, and text with
//! real glyph outlines from a system font when one is available (boxes otherwise). It is test tooling, not the
//! application's renderer.

use std::path::Path;

use kanavos_drawing::{
    Anchor, Drawing, DrawingUnits, Fill, HatchPattern, Item, LinePattern, Rgba, Shape, Stroke, Text,
};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Stroke as SkStroke, StrokeDash,
    Transform,
};

struct Font {
    data: Vec<u8>,
}

impl Font {
    fn load(path: &str) -> Option<Font> {
        std::fs::read(path).ok().map(|data| Font { data })
    }
    fn face(&self) -> Option<ttf_parser::Face<'_>> {
        ttf_parser::Face::parse(&self.data, 0).ok()
    }
}

/// Paper-space view of a drawing: millimetres, y up.
struct View {
    k: f64,
    min_x: f64,
    max_y: f64,
    px: f64,
    margin: f64,
}

impl View {
    fn to_px(&self, x: f64, y: f64) -> (f32, f32) {
        let mx = (x - self.min_x) * self.k + self.margin;
        let my = (self.max_y - y) * self.k + self.margin;
        ((mx * self.px) as f32, (my * self.px) as f32)
    }
}

fn color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color(c));
    p.anti_alias = true;
    p
}

pub struct Raster {
    regular: Option<Font>,
    bold: Option<Font>,
}

impl Raster {
    pub fn new() -> Raster {
        Raster {
            regular: Font::load("C:/Windows/Fonts/arial.ttf")
                .or_else(|| Font::load("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")),
            bold: Font::load("C:/Windows/Fonts/arialbd.ttf")
                .or_else(|| Font::load("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")),
        }
    }

    /// Renders a drawing at `px_per_mm` pixels per paper millimetre and saves it as PNG. Returns the size.
    pub fn render(&self, d: &Drawing, px_per_mm: f64, path: &Path) -> (u32, u32) {
        let k = match d.units {
            DrawingUnits::Metres => 1000.0 / d.scale.max(1.0),
            DrawingUnits::Millimetres => 1.0,
        };
        let b = d.bounds().expect("empty drawing");
        let margin = 15.0;
        let w_mm = b.width() * k + 2.0 * margin;
        let h_mm = b.height() * k + 2.0 * margin;
        let w = (w_mm * px_per_mm).ceil() as u32;
        let h = (h_mm * px_per_mm).ceil() as u32;
        let mut pm = Pixmap::new(w, h).expect("pixmap");
        pm.fill(Color::WHITE);
        let view = View { k, min_x: b.min.x, max_y: b.max.y, px: px_per_mm, margin };
        for item in &d.items {
            self.item(&mut pm, &view, item);
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("dir");
        }
        pm.save_png(path).expect("png");
        (w, h)
    }

    fn item(&self, pm: &mut Pixmap, v: &View, item: &Item) {
        match &item.shape {
            Shape::Text(t) => self.text(pm, v, t),
            Shape::Polygon { rings } => {
                let mut pb = PathBuilder::new();
                for r in rings {
                    for (i, p) in r.iter().enumerate() {
                        let (x, y) = v.to_px(p.x, p.y);
                        if i == 0 {
                            pb.move_to(x, y);
                        } else {
                            pb.line_to(x, y);
                        }
                    }
                    pb.close();
                }
                let Some(path) = pb.finish() else { return };
                if let Some(f) = &item.fill {
                    fill(pm, v, &path, f);
                }
                if let Some(s) = &item.stroke {
                    stroke(pm, v, &path, s);
                }
            }
            Shape::Line { a, b } => {
                let mut pb = PathBuilder::new();
                let (x0, y0) = v.to_px(a.x, a.y);
                let (x1, y1) = v.to_px(b.x, b.y);
                pb.move_to(x0, y0);
                pb.line_to(x1, y1);
                if let (Some(path), Some(s)) = (pb.finish(), &item.stroke) {
                    stroke(pm, v, &path, s);
                }
            }
            Shape::Polyline { points, closed } => {
                let mut pb = PathBuilder::new();
                for (i, p) in points.iter().enumerate() {
                    let (x, y) = v.to_px(p.x, p.y);
                    if i == 0 {
                        pb.move_to(x, y);
                    } else {
                        pb.line_to(x, y);
                    }
                }
                if *closed {
                    pb.close();
                }
                if let (Some(path), Some(s)) = (pb.finish(), &item.stroke) {
                    stroke(pm, v, &path, s);
                }
            }
            Shape::Arc { centre, radius, start, sweep } => {
                let n = ((sweep.abs() / std::f64::consts::TAU) * 96.0).ceil().max(4.0) as usize;
                let mut pb = PathBuilder::new();
                for i in 0..=n {
                    let a = start + sweep * i as f64 / n as f64;
                    let (x, y) = v.to_px(centre.x + radius * a.cos(), centre.y + radius * a.sin());
                    if i == 0 {
                        pb.move_to(x, y);
                    } else {
                        pb.line_to(x, y);
                    }
                }
                if let (Some(path), Some(s)) = (pb.finish(), &item.stroke) {
                    stroke(pm, v, &path, s);
                }
            }
            Shape::Circle { centre, radius } => {
                let (x, y) = v.to_px(centre.x, centre.y);
                let r = (*radius * v.k * v.px) as f32;
                let Some(path) = PathBuilder::from_circle(x, y, r.max(0.1)) else { return };
                if let Some(f) = &item.fill {
                    fill(pm, v, &path, f);
                }
                if let Some(s) = &item.stroke {
                    stroke(pm, v, &path, s);
                }
            }
        }
    }

    fn text(&self, pm: &mut Pixmap, v: &View, t: &Text) {
        let font = if t.bold { self.bold.as_ref().or(self.regular.as_ref()) } else { self.regular.as_ref() };
        let (x, y) = v.to_px(t.position.x, t.position.y);
        let h_px = t.height_mm * v.px;
        let rot = t.rotation;
        let place = |lx: f64, ly: f64| -> (f32, f32) {
            // Local: x right, y up (px); rotate CCW, then flip to pixel y-down.
            let (s, c) = rot.sin_cos();
            let rx = lx * c - ly * s;
            let ry = lx * s + ly * c;
            ((x as f64 + rx) as f32, (y as f64 - ry) as f32)
        };
        let Some(face) = font.and_then(|f| f.face()) else {
            // Fallback: a box of the approximate size.
            let w = t.approx_width_mm() * v.px;
            let dx = -w * t.anchor.h_fraction();
            let dy = -h_px * t.anchor.v_fraction();
            let mut pb = PathBuilder::new();
            let (a, b) = place(dx, dy);
            pb.move_to(a, b);
            for (lx, ly) in [(dx + w, dy), (dx + w, dy + h_px), (dx, dy + h_px)] {
                let (a, b) = place(lx, ly);
                pb.line_to(a, b);
            }
            pb.close();
            if let Some(p) = pb.finish() {
                pm.fill_path(&p, &paint(Rgba::GREY), FillRule::Winding, Transform::identity(), None);
            }
            return;
        };
        let cap = f64::from(face.capital_height().unwrap_or((face.units_per_em() as f64 * 0.716) as i16));
        let s = h_px / cap;
        let glyphs: Vec<(ttf_parser::GlyphId, f64)> = t
            .content
            .chars()
            .map(|ch| {
                let g = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
                let adv = f64::from(face.glyph_hor_advance(g).unwrap_or(0)) * s;
                (g, adv)
            })
            .collect();
        let width: f64 = glyphs.iter().map(|(_, a)| a).sum();
        let dx = -width * t.anchor.h_fraction();
        let dy = -h_px * t.anchor.v_fraction();
        let mut pb = PathBuilder::new();
        let mut pen = dx;
        for (g, adv) in glyphs {
            let mut b = GlyphBuilder { pb: &mut pb, place: &place, s, ox: pen, oy: dy };
            face.outline_glyph(g, &mut b);
            pen += adv;
        }
        if let Some(p) = pb.finish() {
            pm.fill_path(&p, &paint(t.color), FillRule::Winding, Transform::identity(), None);
        }
        let _ = Anchor::BottomLeft;
    }
}

struct GlyphBuilder<'a, F: Fn(f64, f64) -> (f32, f32)> {
    pb: &'a mut PathBuilder,
    place: &'a F,
    s: f64,
    ox: f64,
    oy: f64,
}

impl<F: Fn(f64, f64) -> (f32, f32)> GlyphBuilder<'_, F> {
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        (self.place)(self.ox + f64::from(x) * self.s, self.oy + f64::from(y) * self.s)
    }
}

impl<F: Fn(f64, f64) -> (f32, f32)> ttf_parser::OutlineBuilder for GlyphBuilder<'_, F> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.p(x, y);
        self.pb.move_to(a, b);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.p(x, y);
        self.pb.line_to(a, b);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (a, b) = self.p(x1, y1);
        let (c, d) = self.p(x, y);
        self.pb.quad_to(a, b, c, d);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b) = self.p(x1, y1);
        let (c, d) = self.p(x2, y2);
        let (e, f) = self.p(x, y);
        self.pb.cubic_to(a, b, c, d, e, f);
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

fn stroke(pm: &mut Pixmap, v: &View, path: &tiny_skia::Path, s: &Stroke) {
    let w = (s.weight_mm * v.px).max(0.6) as f32;
    let dashes = s.pattern.dashes_mm();
    let dash = if dashes.is_empty() {
        None
    } else {
        StrokeDash::new(dashes.iter().map(|d| (d * v.px) as f32).collect(), 0.0)
    };
    let solid = s.pattern == LinePattern::Solid;
    let st = SkStroke {
        width: w,
        miter_limit: 4.0,
        line_cap: if solid { LineCap::Square } else { LineCap::Butt },
        line_join: LineJoin::Miter,
        dash,
    };
    pm.stroke_path(path, &paint(s.color), &st, Transform::identity(), None);
}

fn fill(pm: &mut Pixmap, v: &View, path: &tiny_skia::Path, f: &Fill) {
    match f {
        Fill::Solid(c) => pm.fill_path(path, &paint(*c), FillRule::EvenOdd, Transform::identity(), None),
        Fill::Hatch { pattern, color: c, background, scale, angle } => {
            if let Some(bg) = background {
                pm.fill_path(path, &paint(*bg), FillRule::EvenOdd, Transform::identity(), None);
            }
            if *pattern == HatchPattern::None {
                return;
            }
            let Some(mut mask) = Mask::new(pm.width(), pm.height()) else { return };
            mask.fill_path(path, FillRule::EvenOdd, true, Transform::identity());
            let b = path.bounds();
            hatch(pm, &mask, (b.left(), b.top(), b.right(), b.bottom()), *pattern, *c, v.px * scale, *angle);
        }
    }
}

fn hatch(
    pm: &mut Pixmap,
    mask: &Mask,
    b: (f32, f32, f32, f32),
    pattern: HatchPattern,
    c: Rgba,
    px: f64,
    angle: f64,
) {
    let line_w = (0.13 * px).max(0.7) as f32;
    let st = SkStroke { width: line_w, ..SkStroke::default() };
    let pt = paint(c);
    let (x0, y0, x1, y1) = (b.0 as f64, b.1 as f64, b.2 as f64, b.3 as f64);
    let lines = |pm: &mut Pixmap, spacing_mm: f64, deg: f64| {
        let a = (deg.to_radians()) - angle;
        let (dx, dy) = (a.cos(), -a.sin());
        let (nx, ny) = (-dy, dx);
        let sp = spacing_mm * px;
        let cx = (x0 + x1) / 2.0;
        let cy = (y0 + y1) / 2.0;
        let r = ((x1 - x0).hypot(y1 - y0)) / 2.0 + sp;
        let n = (r / sp).ceil() as i64;
        let mut pb = PathBuilder::new();
        for i in -n..=n {
            // Lines anchored to a global grid so neighbouring areas line up.
            let off = ((cx * nx + cy * ny) / sp).floor() * sp + i as f64 * sp;
            let (px0, py0) = (nx * off, ny * off);
            let t0 = (cx - px0) * dx + (cy - py0) * dy;
            let (ax, ay) = (px0 + dx * (t0 - r), py0 + dy * (t0 - r));
            let (bx, by) = (px0 + dx * (t0 + r), py0 + dy * (t0 + r));
            pb.move_to(ax as f32, ay as f32);
            pb.line_to(bx as f32, by as f32);
        }
        if let Some(p) = pb.finish() {
            pm.stroke_path(&p, &pt, &st, Transform::identity(), Some(mask));
        }
    };
    let dots = |pm: &mut Pixmap, spacing_mm: f64, radius_mm: f64, jitter: bool, ring: bool| {
        let sp = spacing_mm * px;
        let mut pb = PathBuilder::new();
        let mut j = (x0 / sp).floor() as i64;
        while (j as f64) * sp < x1 + sp {
            let mut i = (y0 / sp).floor() as i64;
            while (i as f64) * sp < y1 + sp {
                let h = ((j.wrapping_mul(73856093)) ^ (i.wrapping_mul(19349663))) as u64;
                let (jx, jy) = if jitter {
                    (
                        ((h % 97) as f64 / 97.0 - 0.5) * sp * 0.7,
                        (((h / 97) % 89) as f64 / 89.0 - 0.5) * sp * 0.7,
                    )
                } else {
                    (0.0, if j % 2 == 0 { 0.0 } else { sp / 2.0 })
                };
                let cx = j as f64 * sp + jx;
                let cy = i as f64 * sp + jy;
                if let Some(c) =
                    PathBuilder::from_circle(cx as f32, cy as f32, (radius_mm * px).max(0.5) as f32)
                {
                    pb.push_path(&c);
                }
                i += 1;
            }
            j += 1;
        }
        if let Some(p) = pb.finish() {
            if ring {
                pm.stroke_path(&p, &pt, &st, Transform::identity(), Some(mask));
            } else {
                pm.fill_path(&p, &pt, FillRule::Winding, Transform::identity(), Some(mask));
            }
        }
    };
    match pattern {
        HatchPattern::None => {}
        HatchPattern::Diagonal => lines(pm, 2.0, 45.0),
        HatchPattern::Masonry => lines(pm, 1.0, 45.0),
        HatchPattern::CrossDiagonal => {
            lines(pm, 1.5, 45.0);
            lines(pm, 1.5, -45.0);
        }
        HatchPattern::Concrete => dots(pm, 1.2, 0.12, true, false),
        HatchPattern::Insulation => {
            // Loose waves.
            let sp = 1.6 * px;
            let mut pb = PathBuilder::new();
            let mut y = (y0 / sp).floor() * sp;
            while y < y1 + sp {
                let mut x = (x0 / sp).floor() * sp;
                pb.move_to(x as f32, y as f32);
                while x < x1 + sp {
                    pb.quad_to((x + sp * 0.5) as f32, (y - sp * 0.6) as f32, (x + sp) as f32, y as f32);
                    pb.quad_to((x + sp * 1.5) as f32, (y + sp * 0.6) as f32, (x + sp * 2.0) as f32, y as f32);
                    x += sp * 2.0;
                }
                y += sp;
            }
            if let Some(p) = pb.finish() {
                pm.stroke_path(&p, &pt, &st, Transform::identity(), Some(mask));
            }
        }
        HatchPattern::Wood => lines(pm, 1.0, 0.0),
        HatchPattern::Grid => {
            lines(pm, 2.0, 0.0);
            lines(pm, 2.0, 90.0);
        }
        HatchPattern::Dots => dots(pm, 0.8, 0.08, false, false),
        HatchPattern::Earth => {
            lines(pm, 1.6, 60.0);
        }
        HatchPattern::Gravel => dots(pm, 1.4, 0.35, true, true),
        HatchPattern::Horizontal => lines(pm, 0.8, 0.0),
    }
}

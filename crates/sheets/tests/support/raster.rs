//! A small rasteriser for paper-space drawings, used only to look at sheets in tests.
//!
//! Strokes, dash patterns, solid fills, hatches (as parallel lines clipped to the area), arcs, circles and text
//! (glyph outlines from a system font when one is found, otherwise boxes) are drawn with tiny-skia and saved as
//! PNG under `target/visual/`.

#![allow(dead_code)]

use std::path::PathBuf;

use kanavos_drawing::{Anchor, Drawing, DrawingUnits, Fill, HatchPattern, Item, Rgba, Shape, Text};
use tiny_skia::{FillRule, Mask, Paint, Path, PathBuilder, Pixmap, Stroke, StrokeDash, Transform};

/// A loaded font: regular and optional bold faces.
pub struct Fonts {
    regular: Vec<u8>,
    bold: Option<Vec<u8>>,
}

impl Fonts {
    /// Loads a system font with Greek glyphs, if any is installed.
    pub fn system() -> Option<Fonts> {
        let candidates = [
            ("C:/Windows/Fonts/arial.ttf", "C:/Windows/Fonts/arialbd.ttf"),
            (
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            ),
            (
                "/System/Library/Fonts/Supplemental/Arial.ttf",
                "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            ),
        ];
        candidates.iter().find_map(|(r, b)| {
            let regular = std::fs::read(r).ok()?;
            Some(Fonts { regular, bold: std::fs::read(b).ok() })
        })
    }
}

struct Outline<'a> {
    pb: &'a mut PathBuilder,
    map: &'a dyn Fn(f32, f32) -> (f32, f32),
}

impl ttf_parser::OutlineBuilder for Outline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = (self.map)(x, y);
        self.pb.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = (self.map)(x, y);
        self.pb.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = (self.map)(x1, y1);
        let (x, y) = (self.map)(x, y);
        self.pb.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = (self.map)(x1, y1);
        let (x2, y2) = (self.map)(x2, y2);
        let (x, y) = (self.map)(x, y);
        self.pb.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

/// Renders paper-space drawings onto a pixmap.
pub struct Canvas {
    pub pixmap: Pixmap,
    ppm: f64,
    height_mm: f64,
    fonts: Option<Fonts>,
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, c.a);
    p.anti_alias = true;
    p
}

impl Canvas {
    /// A white canvas of `w × h` mm at `dpi`.
    pub fn new(w_mm: f64, h_mm: f64, dpi: f64) -> Canvas {
        let ppm = dpi / 25.4;
        let mut pixmap =
            Pixmap::new((w_mm * ppm).ceil() as u32, (h_mm * ppm).ceil() as u32).expect("pixmap size");
        pixmap.fill(tiny_skia::Color::WHITE);
        Canvas { pixmap, ppm, height_mm: h_mm, fonts: Fonts::system() }
    }

    fn px(&self, x: f64, y: f64) -> (f32, f32) {
        ((x * self.ppm) as f32, ((self.height_mm - y) * self.ppm) as f32)
    }

    fn path(&self, pts: &[kanavos_drawing::Pt], closed: bool) -> Option<Path> {
        let mut pb = PathBuilder::new();
        for (i, p) in pts.iter().enumerate() {
            let (x, y) = self.px(p.x, p.y);
            if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
        }
        if closed {
            pb.close();
        }
        pb.finish()
    }

    fn stroke(&mut self, path: &Path, s: &kanavos_drawing::Stroke, mask: Option<&Mask>) {
        let mut st = Stroke { width: ((s.weight_mm * self.ppm) as f32).max(0.8), ..Stroke::default() };
        let dashes: Vec<f32> = s.pattern.dashes_mm().iter().map(|d| (d * self.ppm) as f32).collect();
        if dashes.len() >= 2 && dashes.len().is_multiple_of(2) {
            st.dash = StrokeDash::new(dashes, 0.0);
        }
        self.pixmap.stroke_path(path, &paint(s.color), &st, Transform::identity(), mask);
    }

    fn hatch(&mut self, path: &Path, pattern: HatchPattern, color: Rgba, scale: f64, angle: f64) {
        use HatchPattern::*;
        let (angles, spacing, dots): (&[f64], f64, bool) = match pattern {
            None => return,
            Diagonal => (&[45.0], 2.0, false),
            Masonry => (&[45.0], 1.0, false),
            CrossDiagonal => (&[45.0, -45.0], 1.5, false),
            Concrete => (&[45.0], 2.5, true),
            Insulation => (&[45.0, -45.0], 3.0, false),
            Wood => (&[0.0], 1.5, false),
            Grid => (&[0.0, 90.0], 3.0, false),
            Dots => (&[0.0], 1.2, true),
            Earth => (&[45.0], 2.0, true),
            Gravel => (&[0.0, 90.0], 2.0, true),
            Horizontal => (&[0.0], 1.0, false),
        };
        let Some(mut mask) = Mask::new(self.pixmap.width(), self.pixmap.height()) else { return };
        mask.fill_path(path, FillRule::EvenOdd, true, Transform::identity());
        let b = path.bounds();
        let (cx, cy) = ((b.left() + b.right()) / 2.0, (b.top() + b.bottom()) / 2.0);
        let r = ((b.width().powi(2) + b.height().powi(2)).sqrt() / 2.0 + 2.0) as f64;
        let step = (spacing * scale.max(0.1) * self.ppm).max(2.0);
        for a in angles {
            // Screen Y points down, so negate the angle.
            let th = -(a.to_radians() + angle);
            let (ux, uy) = (th.cos(), th.sin());
            let (nx, ny) = (-uy, ux);
            let n = (r / step).ceil() as i64;
            let mut pb = PathBuilder::new();
            for i in -n..=n {
                let o = i as f64 * step;
                let (ox, oy) = (f64::from(cx) + nx * o, f64::from(cy) + ny * o);
                pb.move_to((ox - ux * r) as f32, (oy - uy * r) as f32);
                pb.line_to((ox + ux * r) as f32, (oy + uy * r) as f32);
            }
            if let Some(lines) = pb.finish() {
                let mut st = Stroke { width: ((0.13 * self.ppm) as f32).max(0.7), ..Stroke::default() };
                if dots {
                    st.dash = StrokeDash::new(vec![(0.3 * self.ppm) as f32, (1.2 * self.ppm) as f32], 0.0);
                }
                self.pixmap.stroke_path(&lines, &paint(color), &st, Transform::identity(), Some(&mask));
            }
        }
    }

    fn text(&mut self, t: &Text) {
        let face_data = self
            .fonts
            .as_ref()
            .map(|f| if t.bold { f.bold.as_ref().unwrap_or(&f.regular) } else { &f.regular });
        let face = face_data.and_then(|d| ttf_parser::Face::parse(d, 0).ok());
        let Some(face) = face else {
            // No font: a box of the approximate text size.
            let w = t.approx_width_mm();
            let (fx, fy) = (t.anchor.h_fraction() * w, t.anchor.v_fraction() * t.height_mm);
            let (s, c) = t.rotation.sin_cos();
            let corner = |x: f64, y: f64| {
                let (x, y) = (x - fx, y - fy);
                kanavos_drawing::pt(t.position.x + x * c - y * s, t.position.y + x * s + y * c)
            };
            let pts = [corner(0.0, 0.0), corner(w, 0.0), corner(w, t.height_mm), corner(0.0, t.height_mm)];
            if let Some(path) = self.path(&pts, true) {
                self.stroke(&path, &kanavos_drawing::Stroke::solid(0.1), None);
            }
            return;
        };
        let upem = f64::from(face.units_per_em());
        let cap = face.capital_height().map(f64::from).unwrap_or(upem * 0.7);
        let k = t.height_mm / cap; // mm per font unit
        let glyphs: Vec<(ttf_parser::GlyphId, f64)> = t
            .content
            .chars()
            .map(|ch| {
                let g = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
                (g, f64::from(face.glyph_hor_advance(g).unwrap_or((upem * 0.5) as u16)))
            })
            .collect();
        let width: f64 = glyphs.iter().map(|(_, a)| a).sum::<f64>() * k;
        let (fx, fy) = (t.anchor.h_fraction() * width, t.anchor.v_fraction() * t.height_mm);
        let (s, c) = t.rotation.sin_cos();
        let (px0, py0, ppm, hmm) = (t.position.x, t.position.y, self.ppm, self.height_mm);
        let mut pb = PathBuilder::new();
        let mut pen = 0.0;
        for (g, adv) in glyphs {
            let map = move |x: f32, y: f32| {
                let lx = pen + f64::from(x) * k - fx;
                let ly = f64::from(y) * k - fy;
                let (mx, my) = (px0 + lx * c - ly * s, py0 + lx * s + ly * c);
                ((mx * ppm) as f32, ((hmm - my) * ppm) as f32)
            };
            let mut o = Outline { pb: &mut pb, map: &map };
            let _ = face.outline_glyph(g, &mut o);
            pen += adv * k;
        }
        if let Some(path) = pb.finish() {
            self.pixmap.fill_path(&path, &paint(t.color), FillRule::Winding, Transform::identity(), None);
        }
    }

    fn arc_points(cx: f64, cy: f64, r: f64, start: f64, sweep: f64) -> Vec<kanavos_drawing::Pt> {
        let n = ((sweep.abs() / std::f64::consts::TAU) * 96.0).ceil().max(4.0) as usize;
        (0..=n)
            .map(|i| {
                let a = start + sweep * i as f64 / n as f64;
                kanavos_drawing::pt(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    /// Draws one item.
    pub fn item(&mut self, item: &Item) {
        match &item.shape {
            Shape::Line { a, b } => {
                if let (Some(path), Some(s)) = (self.path(&[*a, *b], false), &item.stroke) {
                    self.stroke(&path, s, None);
                }
            }
            Shape::Polyline { points, closed } => {
                if let (Some(path), Some(s)) = (self.path(points, *closed), &item.stroke) {
                    self.stroke(&path, s, None);
                }
            }
            Shape::Polygon { rings } => {
                let mut pb = PathBuilder::new();
                for ring in rings {
                    for (i, p) in ring.iter().enumerate() {
                        let (x, y) = self.px(p.x, p.y);
                        if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
                    }
                    pb.close();
                }
                let Some(path) = pb.finish() else { return };
                match &item.fill {
                    Some(Fill::Solid(c)) => self.pixmap.fill_path(
                        &path,
                        &paint(*c),
                        FillRule::EvenOdd,
                        Transform::identity(),
                        None,
                    ),
                    Some(Fill::Hatch { pattern, color, background, scale, angle }) => {
                        if let Some(bg) = background {
                            self.pixmap.fill_path(
                                &path,
                                &paint(*bg),
                                FillRule::EvenOdd,
                                Transform::identity(),
                                None,
                            );
                        }
                        self.hatch(&path, *pattern, *color, *scale, *angle);
                    }
                    None => {}
                }
                if let Some(s) = &item.stroke {
                    self.stroke(&path, s, None);
                }
            }
            Shape::Arc { centre, radius, start, sweep } => {
                let pts = Self::arc_points(centre.x, centre.y, *radius, *start, *sweep);
                if let (Some(path), Some(s)) = (self.path(&pts, false), &item.stroke) {
                    self.stroke(&path, s, None);
                }
            }
            Shape::Circle { centre, radius } => {
                let pts = Self::arc_points(centre.x, centre.y, *radius, 0.0, std::f64::consts::TAU);
                let Some(path) = self.path(&pts, true) else { return };
                if let Some(Fill::Solid(c)) = &item.fill {
                    self.pixmap.fill_path(&path, &paint(*c), FillRule::EvenOdd, Transform::identity(), None);
                }
                if let Some(s) = &item.stroke {
                    self.stroke(&path, s, None);
                }
            }
            Shape::Text(t) => self.text(t),
        }
    }

    /// Draws a whole paper-space drawing.
    pub fn drawing(&mut self, d: &Drawing) {
        assert_eq!(d.units, DrawingUnits::Millimetres, "rasterise paper drawings only");
        for item in &d.items {
            self.item(item);
        }
    }

    /// Saves the canvas as `target/visual/<name>.png` and returns the path.
    pub fn save(&self, name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/visual");
        std::fs::create_dir_all(&dir).expect("create target/visual");
        let path = dir.join(format!("{name}.png"));
        self.pixmap.save_png(&path).expect("write png");
        path
    }
}

/// A model-space drawing placed on paper at its scale with a margin, as paper-space drawing and paper size.
pub fn model_on_paper(d: &Drawing, margin_mm: f64) -> (Drawing, f64, f64) {
    let k = 1000.0 / d.scale.max(1.0);
    let b = d.bounds().expect("drawing has content");
    let m = kanavos_drawing::Affine::scale_translate(k, margin_mm - b.min.x * k, margin_mm - b.min.y * k);
    let mut paper = d.transformed(&m);
    paper.units = DrawingUnits::Millimetres;
    paper.scale = 1.0;
    (paper, b.width() * k + 2.0 * margin_mm, b.height() * k + 2.0 * margin_mm)
}

/// Text anchor fractions, re-exported for tests.
pub fn anchor_fractions(a: Anchor) -> (f64, f64) {
    (a.h_fraction(), a.v_fraction())
}

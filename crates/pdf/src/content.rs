//! Page content streams: drawing items as PDF path operators.
//!
//! Coordinates are paper millimetres; the stream starts with a `cm` that scales millimetres to points, so every
//! length below (line widths, dash arrays) is in millimetres too. PDF is Y-up like drawings, so no flip is needed.

use std::collections::BTreeMap;
use std::f64::consts::FRAC_PI_2;
use std::fmt::Write as _;

use kanavos_drawing::fonts::{Fonts, PathEl};
use kanavos_drawing::geometry::{FLATTEN_TOLERANCE_MM, fill_rings};
use kanavos_drawing::hatch::{HatchOptions, expand_fill};
use kanavos_drawing::text::layout_text;
use kanavos_drawing::{Drawing, Fill, Item, LinePattern, Pt, Rgba, Shape, Stroke};

/// Points per millimetre.
pub const PT_PER_MM: f64 = 72.0 / 25.4;

/// Formats a number for a content stream: at most 4 decimals, no trailing zeros, never `-0` or exponents.
pub fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".to_owned() } else { s.to_owned() }
}

/// Builds one page's content stream.
pub struct Content<'a> {
    out: String,
    fonts: &'a Fonts,
    /// Whether transparency is allowed (it is not in PDF/A-1).
    alpha: bool,
    /// Graphics states used for transparency, by resource name: (applies to strokes, alpha).
    pub ext_g_states: BTreeMap<String, (bool, f64)>,
}

impl<'a> Content<'a> {
    /// A new stream; `alpha` allows transparency through graphics states.
    pub fn new(fonts: &'a Fonts, alpha: bool) -> Content<'a> {
        let mut out = String::new();
        let k = num(PT_PER_MM);
        let _ = writeln!(out, "{k} 0 0 {k} 0 0 cm");
        Content { out, fonts, alpha, ext_g_states: BTreeMap::new() }
    }

    /// The finished stream.
    pub fn finish(self) -> (String, BTreeMap<String, (bool, f64)>) {
        (self.out, self.ext_g_states)
    }

    /// Colour components in 0–1; without transparency, alpha is blended over white.
    fn rgb(&self, c: Rgba) -> [f64; 3] {
        let a = f64::from(c.a) / 255.0;
        let ch = |v: u8| {
            let v = f64::from(v) / 255.0;
            if self.alpha { v } else { v * a + (1.0 - a) }
        };
        [ch(c.r), ch(c.g), ch(c.b)]
    }

    fn set_alpha(&mut self, c: Rgba, stroke: bool) {
        if !self.alpha || c.a == 255 {
            return;
        }
        let a = f64::from(c.a) / 255.0;
        let name = format!("GS{}{}", if stroke { 'S' } else { 'F' }, c.a);
        self.ext_g_states.insert(name.clone(), (stroke, a));
        let _ = writeln!(self.out, "/{name} gs");
    }

    fn stroke_color(&mut self, c: Rgba) {
        let [r, g, b] = self.rgb(c);
        let _ = writeln!(self.out, "{} {} {} RG", num(r), num(g), num(b));
        self.set_alpha(c, true);
    }

    fn fill_color(&mut self, c: Rgba) {
        let [r, g, b] = self.rgb(c);
        let _ = writeln!(self.out, "{} {} {} rg", num(r), num(g), num(b));
        self.set_alpha(c, false);
    }

    fn rings(&mut self, rings: &[Vec<Pt>]) {
        for r in rings {
            for (i, p) in r.iter().enumerate() {
                let op = if i == 0 { "m" } else { "l" };
                let _ = writeln!(self.out, "{} {} {op}", num(p.x), num(p.y));
            }
            self.out.push_str("h\n");
        }
    }

    fn polyline(&mut self, pts: &[Pt], closed: bool) {
        for (i, p) in pts.iter().enumerate() {
            let op = if i == 0 { "m" } else { "l" };
            let _ = writeln!(self.out, "{} {} {op}", num(p.x), num(p.y));
        }
        if closed {
            self.out.push_str("h\n");
        }
    }

    /// An arc as cubic Béziers of at most a quarter turn each, starting with a move to its first point.
    fn arc(&mut self, c: Pt, r: f64, start: f64, sweep: f64) {
        let pieces = (sweep.abs() / FRAC_PI_2).ceil().clamp(1.0, 16.0) as usize;
        let step = sweep / pieces as f64;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let at = |a: f64| (c.x + r * a.cos(), c.y + r * a.sin());
        let (x0, y0) = at(start);
        let _ = writeln!(self.out, "{} {} m", num(x0), num(y0));
        for i in 0..pieces {
            let a0 = start + step * i as f64;
            let a1 = a0 + step;
            let (x0, y0) = at(a0);
            let (x3, y3) = at(a1);
            let (c1x, c1y) = (x0 - k * r * a0.sin(), y0 + k * r * a0.cos());
            let (c2x, c2y) = (x3 + k * r * a1.sin(), y3 - k * r * a1.cos());
            let _ = writeln!(
                self.out,
                "{} {} {} {} {} {} c",
                num(c1x),
                num(c1y),
                num(c2x),
                num(c2y),
                num(x3),
                num(y3)
            );
        }
    }

    fn stroke_style(&mut self, s: &Stroke) {
        let _ = writeln!(self.out, "{} w", num(s.weight_mm.max(0.0)));
        let dashes = s.pattern.dashes_mm();
        let valid = !dashes.is_empty()
            && dashes.iter().all(|d| d.is_finite() && *d >= 0.0)
            && dashes.iter().sum::<f64>() > 0.0;
        if s.pattern != LinePattern::Solid && valid {
            let list: Vec<String> = dashes.iter().map(|d| num(*d)).collect();
            let _ = writeln!(self.out, "[{}] 0 d 0 J 1 j", list.join(" "));
        } else {
            self.out.push_str("1 J 1 j\n");
        }
        self.stroke_color(s.color);
    }

    /// Writes one drawing item (paper drawing).
    pub fn item(&mut self, item: &Item, paper: &Drawing, hatch: &HatchOptions) {
        if let Shape::Text(t) = &item.shape {
            self.text(t, paper);
            return;
        }
        if let Some(fill) = &item.fill {
            let rings = fill_rings(&item.shape, FLATTEN_TOLERANCE_MM);
            if !rings.is_empty() {
                match fill {
                    Fill::Solid(c) => {
                        self.out.push_str("q\n");
                        self.fill_color(*c);
                        self.rings(&rings);
                        self.out.push_str("f*\nQ\n");
                    }
                    Fill::Hatch { background, .. } => {
                        if let Some(bg) = background {
                            self.out.push_str("q\n");
                            self.fill_color(*bg);
                            self.rings(&rings);
                            self.out.push_str("f*\nQ\n");
                        }
                        // A hatch too dense to draw keeps its background only.
                        if let Ok(Some(lines)) = expand_fill(fill, &rings, paper, hatch)
                            && !lines.segments.is_empty()
                        {
                            self.out.push_str("q\n");
                            let _ = writeln!(self.out, "{} w 0 J", num(lines.weight_mm));
                            self.stroke_color(lines.color);
                            for [a, b] in &lines.segments {
                                let _ = writeln!(
                                    self.out,
                                    "{} {} m {} {} l",
                                    num(a.x),
                                    num(a.y),
                                    num(b.x),
                                    num(b.y)
                                );
                            }
                            self.out.push_str("S\nQ\n");
                        }
                    }
                }
            }
        }
        let Some(s) = &item.stroke else { return };
        let start = self.out.len();
        self.out.push_str("q\n");
        self.stroke_style(s);
        let before = self.out.len();
        match &item.shape {
            Shape::Line { a, b } => self.polyline(&[*a, *b], false),
            Shape::Polyline { points, closed } if points.len() >= 2 => self.polyline(points, *closed),
            Shape::Polygon { rings } => {
                let rings: Vec<Vec<Pt>> = rings.iter().filter(|r| r.len() >= 2).cloned().collect();
                self.rings(&rings);
            }
            Shape::Circle { centre, radius } if radius.is_finite() && *radius > 0.0 => {
                self.arc(*centre, *radius, 0.0, std::f64::consts::TAU);
                self.out.push_str("h\n");
            }
            Shape::Arc { centre, radius, start, sweep }
                if radius.is_finite() && *radius > 0.0 && start.is_finite() && sweep.is_finite() =>
            {
                let sweep = sweep.clamp(-std::f64::consts::TAU, std::f64::consts::TAU);
                if sweep.abs() > 1e-12 {
                    self.arc(*centre, *radius, *start, sweep);
                }
            }
            _ => {}
        }
        if self.out.len() == before {
            // Nothing to stroke: drop the state setup.
            self.out.truncate(start);
        } else {
            self.out.push_str("S\nQ\n");
        }
    }

    fn text(&mut self, t: &kanavos_drawing::Text, paper: &Drawing) {
        let placed = layout_text(t, paper, self.fonts);
        if !placed.fills.is_empty() {
            self.out.push_str("q\n");
            self.fill_color(t.color);
            let mut last = [0.0, 0.0];
            for el in &placed.fills {
                match *el {
                    PathEl::MoveTo(p) => {
                        let _ = writeln!(self.out, "{} {} m", num(p[0]), num(p[1]));
                        last = p;
                    }
                    PathEl::LineTo(p) => {
                        let _ = writeln!(self.out, "{} {} l", num(p[0]), num(p[1]));
                        last = p;
                    }
                    PathEl::QuadTo(c, p) => {
                        // Exact quadratic → cubic conversion.
                        let c1 =
                            [last[0] + 2.0 / 3.0 * (c[0] - last[0]), last[1] + 2.0 / 3.0 * (c[1] - last[1])];
                        let c2 = [p[0] + 2.0 / 3.0 * (c[0] - p[0]), p[1] + 2.0 / 3.0 * (c[1] - p[1])];
                        let _ = writeln!(
                            self.out,
                            "{} {} {} {} {} {} c",
                            num(c1[0]),
                            num(c1[1]),
                            num(c2[0]),
                            num(c2[1]),
                            num(p[0]),
                            num(p[1])
                        );
                        last = p;
                    }
                    PathEl::CubicTo(c1, c2, p) => {
                        let _ = writeln!(
                            self.out,
                            "{} {} {} {} {} {} c",
                            num(c1[0]),
                            num(c1[1]),
                            num(c2[0]),
                            num(c2[1]),
                            num(p[0]),
                            num(p[1])
                        );
                        last = p;
                    }
                    PathEl::Close => self.out.push_str("h\n"),
                }
            }
            if placed.fill_embolden > 0.0 {
                let _ = writeln!(self.out, "{} w 1 j", num(placed.fill_embolden));
                self.stroke_color(t.color);
                self.out.push_str("B\nQ\n");
            } else {
                self.out.push_str("f\nQ\n");
            }
        }
        if !placed.strokes.is_empty() {
            self.out.push_str("q\n");
            let _ = writeln!(self.out, "{} w 1 J 1 j", num(placed.stroke_width));
            self.stroke_color(t.color);
            for s in &placed.strokes {
                self.polyline(s, false);
            }
            self.out.push_str("S\nQ\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::pt;

    #[test]
    fn numbers_are_compact() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.00001), "0");
        assert_eq!(num(2.83464567), "2.8346");
        assert_eq!(num(f64::INFINITY), "0");
        assert_eq!(num(1e7), "10000000");
    }

    #[test]
    fn quarter_arcs_are_close_to_circles() {
        let fonts = Fonts::builtin();
        let mut c = Content::new(&fonts, true);
        c.arc(pt(0.0, 0.0), 10.0, 0.0, FRAC_PI_2);
        let (s, _) = c.finish();
        // Standard control-point distance for a quarter circle: 4/3·tan(π/8)·r ≈ 5.5228.
        assert!(s.contains("10 5.5228 5.5228 10 0 10 c"), "{s}");
    }

    #[test]
    fn transparency_is_blended_without_alpha_support() {
        let fonts = Fonts::builtin();
        let mut c = Content::new(&fonts, false);
        c.fill_color(Rgba::new(0, 0, 0, 128));
        let (s, gs) = c.finish();
        assert!(gs.is_empty());
        assert!(s.contains("0.498 0.498 0.498 rg"), "{s}");
        let mut c = Content::new(&fonts, true);
        c.fill_color(Rgba::new(0, 0, 0, 128));
        let (s, gs) = c.finish();
        assert!(s.contains("/GSF128 gs"));
        assert_eq!(gs.len(), 1);
    }
}

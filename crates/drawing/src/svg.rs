//! Standalone SVG export.
//!
//! The SVG is sized in millimetres (`width="…mm"`), and one SVG user unit is one paper millimetre, so the file
//! prints at the drawing's scale. Drawings are Y-up; SVG is Y-down, so every Y is flipped against the page height.
//!
//! - Strokes carry their true width, colour and dash array (paper millimetres).
//! - Solid fills and hatch backgrounds are filled paths with the even-odd rule; hatch lines are expanded by
//!   [`crate::hatch`] into one path per area, so the SVG matches the PDF and raster output exactly.
//! - Text is written as `<text>` with a Greek-capable font-family list ([`SvgText::Text`]), or as glyph outlines
//!   ([`SvgText::Outlines`]) when the file must look the same without fonts.
//! - Consecutive items on the same layer are grouped in `<g data-layer="…">`.
//! - All text content and attribute values are XML-escaped.

use std::fmt::Write as _;

use kanavos_fonts::{Fonts, PathEl};

use crate::geometry::{FLATTEN_TOLERANCE_MM, fill_rings};
use crate::hatch::{HatchOptions, expand_fill};
use crate::page::PageFrame;
use crate::text::layout_text;
use crate::{Drawing, Fill, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt};

/// Default font-family list: common Greek-capable sans-serif fonts on Windows, Linux and macOS.
pub const DEFAULT_FONT_FAMILY: &str =
    "'Segoe UI', 'Noto Sans', 'DejaVu Sans', 'Liberation Sans', Arial, Helvetica, sans-serif";

/// Cap height as a fraction of the font size assumed for `<text>` output (typical of sans-serif fonts).
pub const SVG_CAP_HEIGHT_RATIO: f64 = 0.70;

/// How text is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvgText {
    /// `<text>` elements, editable and searchable; the viewer picks the font.
    #[default]
    Text,
    /// Glyph outlines and strokes from the fonts passed to [`to_svg_with_fonts`].
    Outlines,
}

/// Options for [`to_svg`].
#[derive(Debug, Clone, PartialEq)]
pub struct SvgOptions {
    /// Page and placement; `None` fits the page to the content plus [`margin_mm`](SvgOptions::margin_mm).
    pub frame: Option<PageFrame>,
    /// Margin around the content when fitting, paper millimetres.
    pub margin_mm: f64,
    /// Page background, if any (transparent otherwise).
    pub background: Option<Rgba>,
    /// CSS font-family list for `<text>`.
    pub font_family: String,
    /// Decimals written for coordinates (millimetres).
    pub decimals: usize,
    /// Document title (`<title>`).
    pub title: Option<String>,
    /// Text output mode.
    pub text: SvgText,
    /// Limits for hatch expansion. Areas whose hatch is too dense get their background only.
    pub hatch: HatchOptions,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            frame: None,
            margin_mm: 5.0,
            background: None,
            font_family: DEFAULT_FONT_FAMILY.to_owned(),
            decimals: 3,
            title: None,
            text: SvgText::Text,
            hatch: HatchOptions::default(),
        }
    }
}

/// Escapes text for XML content and attribute values, dropping characters XML 1.0 does not allow.
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

/// Exports a drawing as a standalone SVG document, measuring text with the system fonts.
pub fn to_svg(drawing: &Drawing, opts: &SvgOptions) -> String {
    to_svg_with_fonts(drawing, opts, Fonts::system())
}

/// Exports a drawing as a standalone SVG document with explicit fonts (used to measure text when fitting the
/// page, and for [`SvgText::Outlines`]). Pass [`Fonts::builtin`] for output identical on every machine.
pub fn to_svg_with_fonts(drawing: &Drawing, opts: &SvgOptions, fonts: &Fonts) -> String {
    let frame = match opts.frame {
        Some(f) if f.is_valid() => f,
        _ => PageFrame::fit(drawing, fonts, opts.margin_mm),
    };
    let paper = frame.to_paper_drawing(drawing);
    let mut w = Writer { out: String::new(), h: frame.height_mm, dec: opts.decimals.min(9) };
    let (pw, ph) = (w.num(frame.width_mm), w.num(frame.height_mm));
    w.out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    let _ = writeln!(
        w.out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{pw}mm\" height=\"{ph}mm\" \
         viewBox=\"0 0 {pw} {ph}\">"
    );
    if let Some(t) = &opts.title {
        let _ = writeln!(w.out, "<title>{}</title>", escape_xml(t));
    }
    if let Some(bg) = opts.background {
        let _ = writeln!(w.out, "<rect x=\"0\" y=\"0\" width=\"{pw}\" height=\"{ph}\"{}/>", fill_attrs(bg));
    }
    let font_family = escape_xml(&opts.font_family);
    let mut layer: Option<&str> = None;
    for item in &paper.items {
        if layer != Some(item.layer.as_str()) {
            if layer.is_some() {
                w.out.push_str("</g>\n");
            }
            let _ = writeln!(w.out, "<g data-layer=\"{}\">", escape_xml(&item.layer));
            layer = Some(item.layer.as_str());
        }
        match &item.shape {
            Shape::Text(t) => match opts.text {
                SvgText::Text => w.text(t, &font_family),
                SvgText::Outlines => w.text_outlines(t, &paper, fonts),
            },
            _ => w.item(item, &paper, &opts.hatch),
        }
    }
    if layer.is_some() {
        w.out.push_str("</g>\n");
    }
    w.out.push_str("</svg>\n");
    w.out
}

fn fill_attrs(c: Rgba) -> String {
    let mut s = format!(" fill=\"{}\"", c.to_hex());
    if c.a < 255 {
        let _ = write!(s, " fill-opacity=\"{:.3}\"", f64::from(c.a) / 255.0);
    }
    s
}

struct Writer {
    out: String,
    h: f64,
    dec: usize,
}

impl Writer {
    fn num(&self, v: f64) -> String {
        if !v.is_finite() {
            return "0".to_owned();
        }
        let s = format!("{v:.*}", self.dec);
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s.as_str() };
        if s == "-0" { "0".to_owned() } else { s.to_owned() }
    }

    fn xy(&self, p: Pt) -> String {
        format!("{} {}", self.num(p.x), self.num(self.h - p.y))
    }

    fn stroke_attrs(&self, s: &Stroke) -> String {
        let dashes = s.pattern.dashes_mm();
        let valid_dashes = !dashes.is_empty() && dashes.iter().all(|d| d.is_finite() && *d >= 0.0);
        let cap = if s.pattern == LinePattern::Solid || !valid_dashes { "round" } else { "butt" };
        let mut a = format!(
            " fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"{cap}\" stroke-linejoin=\"round\"",
            s.color.to_hex(),
            self.num(s.weight_mm.max(0.0))
        );
        if s.color.a < 255 {
            let _ = write!(a, " stroke-opacity=\"{:.3}\"", f64::from(s.color.a) / 255.0);
        }
        if valid_dashes && dashes.iter().sum::<f64>() > 0.0 {
            let list: Vec<String> = dashes.iter().map(|d| self.num(*d)).collect();
            let _ = write!(a, " stroke-dasharray=\"{}\"", list.join(" "));
        }
        a
    }

    fn rings_path(&self, rings: &[Vec<Pt>]) -> String {
        let mut d = String::new();
        for r in rings {
            for (i, p) in r.iter().enumerate() {
                d.push_str(if i == 0 { "M" } else { "L" });
                d.push_str(&self.xy(*p));
            }
            d.push('Z');
        }
        d
    }

    fn item(&mut self, item: &Item, paper: &Drawing, hatch: &HatchOptions) {
        if let Some(fill) = &item.fill {
            let rings = fill_rings(&item.shape, FLATTEN_TOLERANCE_MM);
            if !rings.is_empty() {
                let d = self.rings_path(&rings);
                match fill {
                    Fill::Solid(c) => {
                        let _ =
                            writeln!(self.out, "<path d=\"{d}\" fill-rule=\"evenodd\"{}/>", fill_attrs(*c));
                    }
                    Fill::Hatch { background, .. } => {
                        if let Some(bg) = background {
                            let _ = writeln!(
                                self.out,
                                "<path d=\"{d}\" fill-rule=\"evenodd\"{}/>",
                                fill_attrs(*bg)
                            );
                        }
                        // A hatch too dense to draw keeps its background only.
                        if let Ok(Some(lines)) = expand_fill(fill, &rings, paper, hatch)
                            && !lines.segments.is_empty()
                        {
                            let mut hd = String::new();
                            for [a, b] in &lines.segments {
                                let _ = write!(hd, "M{}L{}", self.xy(*a), self.xy(*b));
                            }
                            let mut attrs = format!(
                                " fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"butt\"",
                                lines.color.to_hex(),
                                self.num(lines.weight_mm)
                            );
                            if lines.color.a < 255 {
                                let _ = write!(
                                    attrs,
                                    " stroke-opacity=\"{:.3}\"",
                                    f64::from(lines.color.a) / 255.0
                                );
                            }
                            let _ = writeln!(self.out, "<path d=\"{hd}\"{attrs}/>");
                        }
                    }
                }
            }
        }
        let Some(stroke) = &item.stroke else { return };
        let attrs = self.stroke_attrs(stroke);
        let d = match &item.shape {
            Shape::Line { a, b } => format!("M{}L{}", self.xy(*a), self.xy(*b)),
            Shape::Polyline { points, closed } => {
                if points.len() < 2 {
                    return;
                }
                let mut d = self.polyline(points);
                if *closed {
                    d.push('Z');
                }
                d
            }
            Shape::Polygon { rings } => {
                let rings: Vec<Vec<Pt>> = rings.iter().filter(|r| r.len() >= 2).cloned().collect();
                self.rings_path(&rings)
            }
            Shape::Circle { centre, radius } => {
                if !(radius.is_finite() && *radius > 0.0) {
                    return;
                }
                let _ = writeln!(
                    self.out,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"{attrs}/>",
                    self.num(centre.x),
                    self.num(self.h - centre.y),
                    self.num(*radius)
                );
                return;
            }
            Shape::Arc { centre, radius, start, sweep } => {
                match self.arc_path(*centre, *radius, *start, *sweep) {
                    Some(d) => d,
                    None => return,
                }
            }
            Shape::Text(_) => return,
        };
        if !d.is_empty() {
            let _ = writeln!(self.out, "<path d=\"{d}\"{attrs}/>");
        }
    }

    fn polyline(&self, points: &[Pt]) -> String {
        let mut d = String::new();
        for (i, p) in points.iter().enumerate() {
            d.push_str(if i == 0 { "M" } else { "L" });
            d.push_str(&self.xy(*p));
        }
        d
    }

    /// An arc as SVG arc commands, split into pieces of at most half a turn. The Y flip turns a
    /// counter-clockwise arc into SVG's negative sweep direction.
    fn arc_path(&self, c: Pt, r: f64, start: f64, sweep: f64) -> Option<String> {
        if !(r.is_finite() && r > 0.0 && start.is_finite() && sweep.is_finite()) || sweep.abs() < 1e-12 {
            return None;
        }
        let sweep = sweep.clamp(-std::f64::consts::TAU, std::f64::consts::TAU);
        let pieces = (sweep.abs() / std::f64::consts::PI).ceil().max(1.0) as usize;
        let at = |a: f64| pt(c.x + r * a.cos(), c.y + r * a.sin());
        let mut d = format!("M{}", self.xy(at(start)));
        let flag = if sweep > 0.0 { 0 } else { 1 };
        for i in 1..=pieces {
            let a = start + sweep * i as f64 / pieces as f64;
            let rr = self.num(r);
            let _ = write!(d, "A{rr} {rr} 0 0 {flag} {}", self.xy(at(a)));
        }
        Some(d)
    }

    fn text(&mut self, t: &Text, font_family: &str) {
        if t.content.is_empty() || !(t.height_mm.is_finite() && t.height_mm > 0.0) {
            return;
        }
        let rot = if t.rotation.is_finite() { t.rotation } else { 0.0 };
        let (s, c) = rot.sin_cos();
        // Baseline point under the anchor: shift down by the anchor's fraction of the cap height.
        let dy = -t.anchor.v_fraction() * t.height_mm;
        let base = pt(t.position.x - s * dy, t.position.y + c * dy);
        let (x, y) = (self.num(base.x), self.num(self.h - base.y));
        let anchor = match t.anchor.h_fraction() {
            f if f < 0.25 => "start",
            f if f < 0.75 => "middle",
            _ => "end",
        };
        let size = self.num(t.height_mm / SVG_CAP_HEIGHT_RATIO);
        let _ = write!(
            self.out,
            "<text x=\"{x}\" y=\"{y}\" font-family=\"{font_family}\" font-size=\"{size}\" text-anchor=\"{anchor}\""
        );
        if t.bold {
            self.out.push_str(" font-weight=\"bold\"");
        }
        self.out.push_str(&fill_attrs(t.color));
        if rot.abs() > 1e-12 {
            let _ = write!(self.out, " transform=\"rotate({} {x} {y})\"", self.num(-rot.to_degrees()));
        }
        let _ = writeln!(self.out, " xml:space=\"preserve\">{}</text>", escape_xml(&t.content));
    }

    fn text_outlines(&mut self, t: &Text, paper: &Drawing, fonts: &Fonts) {
        let placed = layout_text(t, paper, fonts);
        if !placed.fills.is_empty() {
            let mut d = String::new();
            for el in &placed.fills {
                match el {
                    PathEl::MoveTo(p) => d.push_str(&format!("M{}", self.xy(pt(p[0], p[1])))),
                    PathEl::LineTo(p) => d.push_str(&format!("L{}", self.xy(pt(p[0], p[1])))),
                    PathEl::QuadTo(c, p) => {
                        d.push_str(&format!("Q{} {}", self.xy(pt(c[0], c[1])), self.xy(pt(p[0], p[1]))))
                    }
                    PathEl::CubicTo(c1, c2, p) => d.push_str(&format!(
                        "C{} {} {}",
                        self.xy(pt(c1[0], c1[1])),
                        self.xy(pt(c2[0], c2[1])),
                        self.xy(pt(p[0], p[1]))
                    )),
                    PathEl::Close => d.push('Z'),
                }
            }
            let mut attrs = fill_attrs(t.color);
            if placed.fill_embolden > 0.0 {
                let _ = write!(
                    attrs,
                    " stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"",
                    t.color.to_hex(),
                    self.num(placed.fill_embolden)
                );
            }
            let _ = writeln!(self.out, "<path d=\"{d}\" fill-rule=\"nonzero\"{attrs}/>");
        }
        if !placed.strokes.is_empty() {
            let d: String = placed.strokes.iter().map(|s| self.polyline(s)).collect();
            let stroke = Stroke::solid(placed.stroke_width).with_color(t.color);
            let _ = writeln!(self.out, "<path d=\"{d}\"{}/>", self.stroke_attrs(&stroke));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, HatchPattern, weights};

    fn sample() -> Drawing {
        let mut d = Drawing::model(50.0);
        d.push(Item::polygon(
            vec![vec![pt(0.0, 0.0), pt(4.0, 0.0), pt(4.0, 0.25), pt(0.0, 0.25)]],
            Some(Fill::hatch(HatchPattern::Masonry, Some(Rgba::LIGHT_GREY))),
            Some(Stroke::solid(weights::EXTRA_THICK)),
            "wall.cut",
        ));
        d.push(Item::line(
            pt(0.0, 1.0),
            pt(4.0, 1.0),
            Stroke::patterned(weights::THIN, LinePattern::Dashed),
            "wall.above",
        ));
        d.push(Item {
            shape: Shape::Arc {
                centre: pt(1.0, 0.25),
                radius: 0.9,
                start: 0.0,
                sweep: std::f64::consts::FRAC_PI_2,
            },
            stroke: Some(Stroke::solid(weights::FINE)),
            fill: None,
            layer: "door".into(),
            source: None,
        });
        d.push(Item::text(
            Text::new(pt(2.0, 1.5), "Κάτοψη <ισογείου> & \"1:50\"", 3.5).anchored(Anchor::BottomCentre),
            "text",
        ));
        d
    }

    #[test]
    fn output_is_well_formed_xml_with_mm_size() {
        let svg = to_svg_with_fonts(&sample(), &SvgOptions::default(), &Fonts::builtin());
        let doc = roxmltree::Document::parse(&svg).expect("well-formed SVG");
        let root = doc.root_element();
        assert_eq!(root.tag_name().name(), "svg");
        // 4 m at 1:50 = 80 mm plus 2 × 5 mm margin.
        assert_eq!(root.attribute("width"), Some("90mm"));
        let text = doc.descendants().find(|n| n.has_tag_name("text")).unwrap();
        assert_eq!(text.text(), Some("Κάτοψη <ισογείου> & \"1:50\""));
        assert_eq!(text.attribute("text-anchor"), Some("middle"));
        assert_eq!(text.attribute("font-size"), Some("5"));
        let layers: Vec<&str> = doc
            .descendants()
            .filter(|n| n.has_tag_name("g"))
            .filter_map(|n| n.attribute("data-layer"))
            .collect();
        assert_eq!(layers, ["wall.cut", "wall.above", "door", "text"]);
    }

    #[test]
    fn strokes_hatches_and_dashes_are_written() {
        let svg = to_svg_with_fonts(&sample(), &SvgOptions::default(), &Fonts::builtin());
        let doc = roxmltree::Document::parse(&svg).unwrap();
        let paths: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("path")).collect();
        // Background, hatch lines, outline, dashed line, arc.
        assert_eq!(paths.len(), 5);
        assert_eq!(paths[0].attribute("fill"), Some("#d2d2d2"));
        assert_eq!(paths[1].attribute("stroke-width"), Some("0.13"));
        assert!(paths[1].attribute("d").unwrap().matches('M').count() > 50);
        assert_eq!(paths[2].attribute("stroke-width"), Some("0.7"));
        assert_eq!(paths[3].attribute("stroke-dasharray"), Some("3 1.5"));
        assert!(paths[4].attribute("d").unwrap().contains('A'));
    }

    #[test]
    fn y_axis_is_flipped() {
        let mut d = Drawing::paper();
        d.push(Item::line(pt(10.0, 10.0), pt(20.0, 30.0), Stroke::solid(0.25), "x"));
        let opts = SvgOptions { frame: Some(PageFrame::paper(100.0, 50.0)), ..SvgOptions::default() };
        let svg = to_svg_with_fonts(&d, &opts, &Fonts::builtin());
        assert!(svg.contains("d=\"M10 40L20 20\""), "{svg}");
        // A counter-clockwise quarter arc from (10, 0) round to (0, 10) about the origin.
        let w = Writer { out: String::new(), h: 50.0, dec: 3 };
        let a = w.arc_path(pt(0.0, 0.0), 10.0, 0.0, std::f64::consts::FRAC_PI_2).unwrap();
        assert_eq!(a, "M10 50A10 10 0 0 0 0 40");
    }

    #[test]
    fn outlines_mode_has_no_text_elements() {
        let opts = SvgOptions { text: SvgText::Outlines, ..SvgOptions::default() };
        let svg = to_svg_with_fonts(&sample(), &opts, &Fonts::builtin());
        let doc = roxmltree::Document::parse(&svg).unwrap();
        assert!(doc.descendants().all(|n| !n.has_tag_name("text")));
        assert_eq!(doc.descendants().filter(|n| n.has_tag_name("path")).count(), 6);
    }

    #[test]
    fn escaping_and_numbers() {
        assert_eq!(escape_xml("a<b>&'\"\u{1}c"), "a&lt;b&gt;&amp;&apos;&quot;c");
        let w = Writer { out: String::new(), h: 0.0, dec: 3 };
        assert_eq!(w.num(-0.0001), "0");
        assert_eq!(w.num(2.5), "2.5");
        assert_eq!(w.num(f64::NAN), "0");
        assert_eq!(w.num(100.0), "100");
    }

    #[test]
    fn titles_backgrounds_and_rotated_text() {
        let mut d = Drawing::paper();
        d.push(Item::text(Text::new(pt(10.0, 10.0), "A", 2.5).rotated(std::f64::consts::FRAC_PI_2), "t"));
        let opts = SvgOptions {
            title: Some("Φύλλο & 1".into()),
            background: Some(Rgba::WHITE),
            ..SvgOptions::default()
        };
        let svg = to_svg_with_fonts(&d, &opts, &Fonts::builtin());
        let doc = roxmltree::Document::parse(&svg).unwrap();
        assert_eq!(doc.descendants().find(|n| n.has_tag_name("title")).unwrap().text(), Some("Φύλλο & 1"));
        assert!(doc.descendants().any(|n| n.has_tag_name("rect")));
        let t = doc.descendants().find(|n| n.has_tag_name("text")).unwrap();
        assert!(t.attribute("transform").unwrap().starts_with("rotate(-90 "));
    }
}

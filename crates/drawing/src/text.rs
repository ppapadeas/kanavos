//! Text layout: measuring text and placing its glyph geometry in drawing coordinates.
//!
//! [`Text`] stores a cap height in paper millimetres, an anchor and a rotation. Layout uses real font metrics from
//! [`kanavos_fonts`]: the system font when one is installed ([`Fonts::system`]), otherwise the built-in stroke font.
//! Pass [`Fonts::builtin`] for output that must be identical on every machine.
//!
//! The text box spans from the baseline (bottom) to the cap height (top); descenders hang below it. Anchors refer
//! to that box: `BottomLeft` puts the start of the baseline at the text position, `Centre` puts the middle of the
//! capitals there, and so on.

use kanavos_fonts::{Fonts, P, PathEl};

use crate::{Affine, Drawing, Pt, Text, pt};

/// A text item laid out in drawing coordinates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlacedText {
    /// Filled glyph outlines (non-zero winding), in drawing coordinates.
    pub fills: Vec<PathEl>,
    /// Width of an extra outline stroke for synthetic bold, drawing units (0 for none).
    pub fill_embolden: f64,
    /// Single-stroke glyph polylines, in drawing coordinates.
    pub strokes: Vec<Vec<Pt>>,
    /// Pen width for `strokes`, drawing units.
    pub stroke_width: f64,
    /// Corners of the text box (baseline to cap height): bottom-left, bottom-right, top-right, top-left.
    pub corners: [Pt; 4],
    /// Advance width, drawing units.
    pub width: f64,
}

/// Map from the local text frame (paper millimetres, pen at the origin on the baseline, Y up) to drawing
/// coordinates, for a text of a given advance width in paper millimetres.
pub fn text_frame(text: &Text, width_mm: f64, drawing: &Drawing) -> Affine {
    let k = drawing.paper_mm(1.0);
    let ox = -text.anchor.h_fraction() * width_mm;
    let oy = -text.anchor.v_fraction() * text.height_mm;
    let rot = if text.rotation.is_finite() { text.rotation } else { 0.0 };
    let place = Affine::rotation(rot);
    let place = Affine { tx: text.position.x, ty: text.position.y, ..place };
    place.then(&Affine::scale_translate(k, ox * k, oy * k))
}

/// Advance width of a text in paper millimetres, measured with the given fonts.
pub fn text_width_mm(text: &Text, fonts: &Fonts) -> f64 {
    fonts.measure(&text.content, text.height_mm, text.bold)
}

/// Corners of a text's box (baseline to cap height) in drawing coordinates: bottom-left, bottom-right, top-right,
/// top-left.
pub fn text_corners(text: &Text, drawing: &Drawing, fonts: &Fonts) -> [Pt; 4] {
    let w = text_width_mm(text, fonts);
    let m = text_frame(text, w, drawing);
    let h = text.height_mm;
    [m.apply(pt(0.0, 0.0)), m.apply(pt(w, 0.0)), m.apply(pt(w, h)), m.apply(pt(0.0, h))]
}

/// Lays out a text item: glyph outlines and strokes in drawing coordinates.
pub fn layout_text(text: &Text, drawing: &Drawing, fonts: &Fonts) -> PlacedText {
    let g = fonts.layout(&text.content, text.height_mm, text.bold);
    let m = text_frame(text, g.advance, drawing);
    let k = drawing.paper_mm(1.0);
    let map = |p: P| {
        let q = m.apply(pt(p[0], p[1]));
        [q.x, q.y]
    };
    let h = text.height_mm;
    PlacedText {
        fills: g.fills.iter().map(|e| e.map(map)).collect(),
        fill_embolden: g.fill_embolden * k,
        strokes: g.strokes.iter().map(|s| s.iter().map(|p| m.apply(pt(p[0], p[1]))).collect()).collect(),
        stroke_width: g.stroke_width * k,
        corners: [
            m.apply(pt(0.0, 0.0)),
            m.apply(pt(g.advance, 0.0)),
            m.apply(pt(g.advance, h)),
            m.apply(pt(0.0, h)),
        ],
        width: g.advance * k,
    }
}

/// Text width estimation for layout code (dimension chains, title blocks, tags): advance width in paper
/// millimetres of `content` at cap height `height_mm`, using the system font when available.
pub fn measure_mm(content: &str, height_mm: f64, bold: bool) -> f64 {
    Fonts::system().measure(content, height_mm, bold)
}

impl Text {
    /// Advance width in paper millimetres, measured with the system font (or the built-in stroke font when no
    /// font is installed). See [`Text::approx_width_mm`] for a font-independent estimate.
    pub fn width_mm(&self) -> f64 {
        text_width_mm(self, Fonts::system())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Anchor;

    #[test]
    fn anchors_place_the_box() {
        let f = Fonts::builtin();
        let d = Drawing::paper();
        let t = Text::new(pt(100.0, 50.0), "ABC", 5.0).anchored(Anchor::Centre);
        let c = text_corners(&t, &d, &f);
        let centre = c[0].midpoint(c[2]);
        assert!(centre.distance(pt(100.0, 50.0)) < 1e-9);
        assert!((c[3].y - c[0].y - 5.0).abs() < 1e-9);
        let t = Text::new(pt(0.0, 0.0), "ABC", 5.0).anchored(Anchor::TopRight);
        let c = text_corners(&t, &d, &f);
        assert!(c[2].distance(pt(0.0, 0.0)) < 1e-9);
    }

    #[test]
    fn model_space_text_scales_with_the_drawing() {
        let f = Fonts::builtin();
        let d = Drawing::model(50.0);
        let t = Text::new(pt(0.0, 0.0), "Κάτοψη", 2.5);
        let p = layout_text(&t, &d, &f);
        // 2.5 mm at 1:50 is 0.125 m.
        assert!((p.corners[3].y - 0.125).abs() < 1e-12);
        assert!((p.stroke_width - 0.0125).abs() < 1e-12);
        assert!((p.width - text_width_mm(&t, &f) * 0.05).abs() < 1e-12);
        assert!(p.strokes.iter().flatten().all(|q| q.x >= -1e-9 && q.x <= p.width + 1e-9));
    }

    #[test]
    fn rotation_turns_the_box() {
        let f = Fonts::builtin();
        let d = Drawing::paper();
        let t = Text::new(pt(0.0, 0.0), "AB", 5.0).rotated(std::f64::consts::FRAC_PI_2);
        let c = text_corners(&t, &d, &f);
        // Rotated a quarter turn: the baseline runs up the Y axis.
        assert!(c[1].x.abs() < 1e-9 && c[1].y > 0.0);
        assert!((c[3].x + 5.0).abs() < 1e-9);
    }

    #[test]
    fn measuring_is_close_to_the_rough_estimate() {
        let t = Text::new(pt(0.0, 0.0), "Κάτοψη ισογείου 1:50", 2.5);
        let w = t.width_mm();
        let rough = t.approx_width_mm();
        assert!(w > rough * 0.5 && w < rough * 1.6, "{w} vs {rough}");
        assert!((measure_mm("", 2.5, false)).abs() < 1e-12);
    }
}

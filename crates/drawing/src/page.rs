//! Placing a drawing on a page: the map from drawing coordinates to paper millimetres.
//!
//! Every page-based output (SVG, PDF, raster) works in paper millimetres with the origin at the lower-left corner
//! of the page and Y up. A [`PageFrame`] gives the page size and the map. Model-space drawings are placed at their
//! own scale (1:`scale`, so one metre is `1000 / scale` millimetres); paper-space drawings are already in
//! millimetres.

use kanavos_fonts::Fonts;

use crate::geometry::{bounds_of, shape_bounds};
use crate::text::text_corners;
use crate::{Affine, Drawing, DrawingUnits, Rect, Shape, pt};

/// Scale denominator used when a model drawing has an unusable scale (zero, negative or not finite).
pub const FALLBACK_SCALE: f64 = 100.0;

/// Size of a page and the map from drawing coordinates onto it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageFrame {
    /// Page width, millimetres.
    pub width_mm: f64,
    /// Page height, millimetres.
    pub height_mm: f64,
    /// Drawing coordinates → paper millimetres (origin at the lower-left corner of the page, Y up).
    pub to_paper: Affine,
}

/// Paper millimetres per drawing unit: `1000 / scale` for model drawings, 1 for paper drawings.
pub fn mm_per_unit(drawing: &Drawing) -> f64 {
    match drawing.units {
        DrawingUnits::Millimetres => 1.0,
        DrawingUnits::Metres => 1000.0 / effective_scale(drawing),
    }
}

/// The drawing's scale denominator, or [`FALLBACK_SCALE`] when it is not a positive finite number.
pub fn effective_scale(drawing: &Drawing) -> f64 {
    if drawing.scale.is_finite() && drawing.scale > 0.0 { drawing.scale } else { FALLBACK_SCALE }
}

/// Bounds of everything visible in a drawing, with arcs measured exactly and text by its full box, in drawing
/// coordinates. `None` for an empty drawing.
pub fn content_bounds(drawing: &Drawing, fonts: &Fonts) -> Option<Rect> {
    let mut acc: Option<Rect> = None;
    for item in &drawing.items {
        let b = match &item.shape {
            Shape::Text(t) => bounds_of(&text_corners(t, drawing, fonts)),
            s => shape_bounds(s),
        };
        if let Some(b) = b {
            acc = Some(match acc {
                Some(a) => a.union(&b),
                None => b,
            });
        }
    }
    acc
}

impl PageFrame {
    /// A page of the given size for a paper-space drawing whose origin is the page's lower-left corner.
    pub fn paper(width_mm: f64, height_mm: f64) -> PageFrame {
        PageFrame { width_mm, height_mm, to_paper: Affine::IDENTITY }
    }

    /// A page just large enough for the drawing's content at its scale, plus a margin on every side.
    /// An empty drawing gives a page of twice the margin (at least 1 mm).
    pub fn fit(drawing: &Drawing, fonts: &Fonts, margin_mm: f64) -> PageFrame {
        let margin = if margin_mm.is_finite() && margin_mm > 0.0 { margin_mm } else { 0.0 };
        let k = mm_per_unit(drawing);
        match content_bounds(drawing, fonts) {
            Some(b) => {
                let w = (b.width() * k + 2.0 * margin).max(1.0);
                let h = (b.height() * k + 2.0 * margin).max(1.0);
                let to_paper = Affine::scale_translate(k, margin - b.min.x * k, margin - b.min.y * k);
                PageFrame { width_mm: w, height_mm: h, to_paper }
            }
            None => {
                let s = (2.0 * margin).max(1.0);
                PageFrame { width_mm: s, height_mm: s, to_paper: Affine::scale_translate(k, 0.0, 0.0) }
            }
        }
    }

    /// The drawing's content centred on a page of the given size, at the drawing's scale. Content larger than the
    /// page is still centred and runs off the edges (it is not shrunk: the scale is part of the drawing).
    pub fn centred(drawing: &Drawing, fonts: &Fonts, width_mm: f64, height_mm: f64) -> PageFrame {
        let k = mm_per_unit(drawing);
        let c = content_bounds(drawing, fonts).map(|b| b.centre()).unwrap_or(pt(0.0, 0.0));
        let to_paper = Affine::scale_translate(k, width_mm / 2.0 - c.x * k, height_mm / 2.0 - c.y * k);
        PageFrame { width_mm, height_mm, to_paper }
    }

    /// The drawing mapped onto the page: a paper-space drawing (millimetres, scale 1) whose coordinates are page
    /// millimetres. Stroke weights, dash lengths, hatch spacing and text heights are unchanged, as they are
    /// already in paper millimetres.
    pub fn to_paper_drawing(&self, drawing: &Drawing) -> Drawing {
        let mut d = drawing.transformed(&self.to_paper);
        d.units = DrawingUnits::Millimetres;
        d.scale = 1.0;
        d
    }

    /// Whether the page has a usable size.
    pub fn is_valid(&self) -> bool {
        self.width_mm.is_finite() && self.height_mm.is_finite() && self.width_mm > 0.0 && self.height_mm > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Item, Stroke, Text, weights};

    #[test]
    fn fit_places_model_content_at_scale_with_margins() {
        let mut d = Drawing::model(100.0);
        d.push(Item::line(pt(10.0, 20.0), pt(20.0, 25.0), Stroke::solid(weights::THIN), "x"));
        let f = PageFrame::fit(&d, &Fonts::builtin(), 5.0);
        // 10 m × 5 m at 1:100 is 100 mm × 50 mm, plus 5 mm margins.
        assert!((f.width_mm - 110.0).abs() < 1e-9);
        assert!((f.height_mm - 60.0).abs() < 1e-9);
        let p = f.to_paper.apply(pt(10.0, 20.0));
        assert!(p.distance(pt(5.0, 5.0)) < 1e-9);
        let paper = f.to_paper_drawing(&d);
        assert_eq!(paper.units, DrawingUnits::Millimetres);
        assert!((paper.paper_mm(1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn text_boxes_count_towards_bounds() {
        let mut d = Drawing::paper();
        d.push(Item::text(Text::new(pt(0.0, 0.0), "ABCDEF", 5.0), "t"));
        let b = content_bounds(&d, &Fonts::builtin()).unwrap();
        assert!(b.width() > 20.0 && (b.height() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn centred_and_empty_pages() {
        let mut d = Drawing::model(50.0);
        d.push(Item::line(pt(0.0, 0.0), pt(2.0, 2.0), Stroke::solid(weights::THIN), "x"));
        let f = PageFrame::centred(&d, &Fonts::builtin(), 297.0, 210.0);
        assert!(f.to_paper.apply(pt(1.0, 1.0)).distance(pt(148.5, 105.0)) < 1e-9);
        let e = PageFrame::fit(&Drawing::model(f64::NAN), &Fonts::builtin(), 0.0);
        assert!(e.is_valid());
        assert!((mm_per_unit(&Drawing::model(0.0)) - 10.0).abs() < 1e-12);
    }
}

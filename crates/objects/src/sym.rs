//! Plan symbol builder.

use kanavos_drawing::{Affine, Drawing, Fill, Item, LinePattern, Rgba, Shape, Stroke, pt, weights};

use crate::shape::P2;

/// Pens used by symbols, following ISO 128 line weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pen {
    /// Door leaves (0.35 mm).
    Leaf,
    /// Frames, sashes and cut profiles (0.25 mm).
    Frame,
    /// Swing arcs, glass, sills and furniture outlines (0.18 mm).
    Fine,
    /// Furniture details and textures (0.13 mm).
    Detail,
    /// Parts above the cut plane (0.18 mm, long dashes).
    Above,
    /// Hidden parts, e.g. a leaf inside a wall pocket (0.13 mm, short dashes).
    Hidden,
}

impl Pen {
    /// The stroke of the pen.
    pub(crate) fn stroke(self) -> Stroke {
        match self {
            Pen::Leaf => Stroke::solid(weights::MEDIUM),
            Pen::Frame => Stroke::solid(weights::THIN),
            Pen::Fine => Stroke::solid(weights::FINE),
            Pen::Detail => Stroke::solid(weights::EXTRA_FINE),
            Pen::Above => Stroke::patterned(weights::FINE, LinePattern::Dashed),
            Pen::Hidden => Stroke::patterned(weights::EXTRA_FINE, LinePattern::Hidden),
        }
    }
}

fn finite(p: P2) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

/// Collects drawing items on a current layer. Coordinates are local metres; sizes that must read the same on
/// paper (arrow heads) use [`Sym::mm`].
#[derive(Debug, Clone)]
pub(crate) struct Sym {
    items: Vec<Item>,
    layer: &'static str,
    scale: f64,
}

impl Sym {
    /// An empty symbol drawn for scale 1:`scale` on `layer`.
    pub(crate) fn new(layer: &'static str, scale: f64) -> Sym {
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 50.0 };
        Sym { items: Vec::new(), layer, scale }
    }

    /// Sets the layer of the following items.
    pub(crate) fn layer(&mut self, layer: &'static str) {
        self.layer = layer;
    }

    /// Whether the scale is large enough (1:50 and larger) for full detail.
    pub(crate) fn fine(&self) -> bool {
        self.scale < 75.0
    }

    /// Metres in the drawing for `v` paper millimetres.
    pub(crate) fn mm(&self, v: f64) -> f64 {
        v * self.scale / 1000.0
    }

    fn push(&mut self, shape: Shape, stroke: Option<Stroke>, fill: Option<Fill>) {
        self.items.push(Item { shape, stroke, fill, layer: self.layer.into(), source: None });
    }

    /// A straight line; skipped when degenerate.
    pub(crate) fn line(&mut self, a: P2, b: P2, pen: Pen) {
        if finite(a) && finite(b) && (a.0 - b.0).hypot(a.1 - b.1) > 1e-6 {
            self.push(Shape::Line { a: pt(a.0, a.1), b: pt(b.0, b.1) }, Some(pen.stroke()), None);
        }
    }

    /// A polyline.
    pub(crate) fn poly(&mut self, pts: &[P2], closed: bool, pen: Pen) {
        if pts.len() < 2 || !pts.iter().all(|p| finite(*p)) {
            return;
        }
        let points = pts.iter().map(|p| pt(p.0, p.1)).collect();
        self.push(Shape::Polyline { points, closed }, Some(pen.stroke()), None);
    }

    /// An axis-aligned rectangle; skipped when it has no area.
    pub(crate) fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, pen: Pen) {
        if (x1 - x0).abs() > 1e-6 && (y1 - y0).abs() > 1e-6 {
            self.poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true, pen);
        }
    }

    /// A circular arc from `start` sweeping `sweep` radians (counter-clockwise positive).
    pub(crate) fn arc(&mut self, c: P2, r: f64, start: f64, sweep: f64, pen: Pen) {
        if finite(c) && r.is_finite() && r > 1e-6 && start.is_finite() && sweep.is_finite() {
            self.push(Shape::Arc { centre: pt(c.0, c.1), radius: r, start, sweep }, Some(pen.stroke()), None);
        }
    }

    /// A solid black area without outline (arrow heads, small marks).
    pub(crate) fn solid(&mut self, pts: &[P2]) {
        if pts.len() >= 3 && pts.iter().all(|p| finite(*p)) {
            let ring = pts.iter().map(|p| pt(p.0, p.1)).collect();
            self.push(Shape::Polygon { rings: vec![ring] }, None, Some(Fill::Solid(Rgba::BLACK)));
        }
    }

    /// An arrow from `a` to `b` with a solid head of 2 paper mm at `b`.
    pub(crate) fn arrow(&mut self, a: P2, b: P2, pen: Pen) {
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len < 1e-6 || !finite(a) || !finite(b) {
            return;
        }
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let head = self.mm(2.0).min(len * 0.6);
        let half = head * 0.3;
        let base = (b.0 - ux * head, b.1 - uy * head);
        self.line(a, base, pen);
        self.solid(&[b, (base.0 - uy * half, base.1 + ux * half), (base.0 + uy * half, base.1 - ux * half)]);
    }

    /// The items.
    pub(crate) fn into_items(self) -> Vec<Item> {
        self.items
    }

    /// The items mapped through `m`.
    pub(crate) fn into_mapped(self, m: &Affine) -> Vec<Item> {
        let d = Drawing { items: self.items, ..Drawing::model(self.scale) };
        d.transformed(m).items
    }
}

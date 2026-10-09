//! Sections and elevations.
//!
//! Section coordinates: `x` runs along the section line from its start, `y` is the absolute elevation. The view
//! looks to the left of `start → end` (to the right when flipped).

use kanavos_drawing::{Drawing, Item, Stroke, pt, weights};
use kanavos_geom::{Point2, Point3, Vec2};
use kanavos_model::{Document, Id, ViewKind};
use kanavos_units::{Length, Locale};

use crate::{SceneOptions, ViewError, scene};

/// The frame of a section or elevation view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionFrame {
    /// Start of the line in plan.
    pub origin: Point2,
    /// Unit vector along the line (drawing +X).
    pub along: Vec2,
    /// Unit vector pointing into the view (depth).
    pub look: Vec2,
    /// How far the view sees, metres.
    pub depth: f64,
    /// Whether the plane cuts (section) or only projects (elevation).
    pub cuts: bool,
    /// Length of the line, metres.
    pub length: f64,
}

impl SectionFrame {
    /// Section-plane coordinates of a world point: (x along, y elevation, depth into the view).
    pub fn to_view(&self, q: Point3) -> (f64, f64, f64) {
        let d = Point2::new(q.x, q.y) - self.origin;
        (d.dot(self.along), q.z, d.dot(self.look))
    }
}

/// The frame of a section or elevation view.
pub fn section_frame(doc: &Document, view: Id) -> Result<SectionFrame, ViewError> {
    let v = doc.view(view).ok_or(ViewError::NotFound(view))?;
    let (start, end, depth, flip, cuts) = match &v.kind {
        ViewKind::Section { start, end, depth, flip, .. } => (*start, *end, *depth, *flip, true),
        ViewKind::Elevation { start, end, depth, flip, .. } => (*start, *end, *depth, *flip, false),
        _ => return Err(ViewError::WrongKind(view)),
    };
    let along = (end - start).normalized().ok_or(ViewError::WrongKind(view))?;
    let look = if flip { -along.perp() } else { along.perp() };
    let depth = if depth > Length::ZERO { depth.as_m() } else { 1e6 };
    Ok(SectionFrame { origin: start, along, look, depth, cuts, length: start.distance(end).as_m() })
}

/// Draws a section or elevation. This first version draws the cut outline of every solid (sections) and the
/// silhouette boxes of what lies beyond; exact hidden-line removal replaces it.
pub fn section(doc: &Document, view: Id, locale: Locale) -> Result<Drawing, ViewError> {
    let v = doc.view(view).ok_or(ViewError::NotFound(view))?;
    let frame = section_frame(doc, view)?;
    let _ = locale;
    let mut d = Drawing::model(f64::from(v.scale));
    let sc = scene(doc, &SceneOptions::default());
    for item in &sc.items {
        let mut cut_segments = Vec::new();
        let (mut xmin, mut xmax, mut ymin, mut ymax, mut wmin) =
            (f64::MAX, f64::MIN, f64::MAX, f64::MIN, f64::MAX);
        for [a, b, c] in item.mesh.triangle_points() {
            let pa = frame.to_view(a);
            let pb = frame.to_view(b);
            let pc = frame.to_view(c);
            for q in [pa, pb, pc] {
                xmin = xmin.min(q.0);
                xmax = xmax.max(q.0);
                ymin = ymin.min(q.1);
                ymax = ymax.max(q.1);
                wmin = wmin.min(q.2);
            }
            if frame.cuts {
                // Intersect the triangle with the plane depth = 0.
                let mut hits = Vec::new();
                for (p0, p1) in [(pa, pb), (pb, pc), (pc, pa)] {
                    if (p0.2 > 0.0) != (p1.2 > 0.0) {
                        let t = p0.2 / (p0.2 - p1.2);
                        hits.push(pt(p0.0 + (p1.0 - p0.0) * t, p0.1 + (p1.1 - p0.1) * t));
                    }
                }
                if let [h0, h1] = hits[..] {
                    cut_segments.push((h0, h1));
                }
            }
        }
        let layer_key = item.category.key();
        if !cut_segments.is_empty() {
            for (a, b) in cut_segments {
                d.push(
                    Item::line(a, b, Stroke::solid(weights::THICK), format!("{layer_key}.cut"))
                        .with_source(item.element.0),
                );
            }
        } else if wmin > 0.0 && wmin < frame.depth && xmax > 0.0 && xmin < frame.length && xmin < xmax {
            d.push(
                Item::polyline(
                    vec![pt(xmin, ymin), pt(xmax, ymin), pt(xmax, ymax), pt(xmin, ymax)],
                    true,
                    Stroke::solid(weights::FINE),
                    format!("{layer_key}.beyond"),
                )
                .with_source(item.element.0),
            );
        }
    }
    Ok(d)
}

//! Drawing preparation: everything about a [`Drawing`] that does not depend on the zoom, computed once per drawing
//! revision — fill triangulation, hatch segments, item bounds, a source index and hit-testing.
//!
//! Sizes that depend on the zoom (stroke widths in screen points, dash visibility, arc segment counts, levels of
//! detail) are decided later, when the prepared drawing is tessellated (see `tess`).

use std::collections::HashMap;

use kanavos_drawing::{Drawing, DrawingUnits, Fill, Item, Pt, Rect, Rgba, Shape, Text, pt};
use kanavos_geom::{Point2, Polygon2, PolygonWithHoles};

use super::hatch::{hatch_area, inside};

/// Segments used for a circle fill (its outline is tessellated per zoom).
const CIRCLE_FILL_SEGMENTS: usize = 72;

/// Triangulated area fill.
#[derive(Debug, Clone, PartialEq)]
pub struct PFill {
    /// Vertices, drawing units.
    pub verts: Vec<Pt>,
    /// Triangles indexing `verts`.
    pub tris: Vec<[u32; 3]>,
    /// Colour (may be transparent when only a hatch is drawn).
    pub color: Rgba,
}

/// Hatch lines of an area.
#[derive(Debug, Clone, PartialEq)]
pub struct PHatch {
    /// Segments, drawing units.
    pub segments: Vec<[Pt; 2]>,
    /// Line colour.
    pub color: Rgba,
    /// Smallest line/mark spacing, drawing units (for the level of detail).
    pub spacing: f64,
    /// Ink coverage when printed (0..1).
    pub coverage: f64,
    /// Background colour under the hatch, if any.
    pub background: Option<Rgba>,
    /// Pen width of the hatch lines, paper millimetres.
    pub weight_mm: f64,
}

/// Geometry of a stroke.
#[derive(Debug, Clone, PartialEq)]
pub enum PPath {
    /// A polyline.
    Poly {
        /// Vertices.
        points: Vec<Pt>,
        /// Whether the last vertex joins the first.
        closed: bool,
    },
    /// A circular arc (a full circle when `sweep` is ±τ).
    Arc {
        /// Centre.
        centre: Pt,
        /// Radius.
        radius: f64,
        /// Start angle, radians.
        start: f64,
        /// Signed sweep, radians.
        sweep: f64,
    },
}

/// A stroked path with its print style.
#[derive(Debug, Clone, PartialEq)]
pub struct PStroke {
    /// Geometry.
    pub path: PPath,
    /// Printed width, paper millimetres.
    pub weight_mm: f64,
    /// Colour.
    pub color: Rgba,
    /// Dash on/off lengths, paper millimetres (empty for solid).
    pub dashes_mm: Vec<f64>,
}

/// One drawing item, prepared.
#[derive(Debug, Clone, PartialEq)]
pub struct PItem {
    /// The model object it shows.
    pub source: Option<u64>,
    /// Hit-test priority (lower wins): openings before walls before slabs before spaces.
    pub rank: u8,
    /// Bounds in drawing units (strokes and fills; text approximately).
    pub bounds: Rect,
    /// Area fill.
    pub fill: Option<PFill>,
    /// Hatch.
    pub hatch: Option<PHatch>,
    /// Strokes.
    pub strokes: Vec<PStroke>,
    /// Text.
    pub text: Option<Text>,
    /// Polygon rings, kept for hit-testing areas.
    pub rings: Vec<Vec<Pt>>,
}

/// A drawing prepared for display.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prepared {
    /// Coordinate units.
    pub units: DrawingUnits,
    /// Scale denominator (1 for paper drawings).
    pub scale: f64,
    /// Items in painting order.
    pub items: Vec<PItem>,
    /// Bounds of all items.
    pub bounds: Option<Rect>,
    /// Item indices by source.
    pub by_source: HashMap<u64, Vec<usize>>,
}

/// Converts a colour.
pub fn color32(c: Rgba) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Hit-test priority of a layer, from its category prefix (`door.symbol` → doors).
pub fn layer_rank(layer: &str) -> u8 {
    let key = layer.split('.').next().unwrap_or(layer);
    match key {
        "door" | "window" | "opening" => 0,
        "space" => 4,
        "slab" | "roof" => 3,
        "underlay" => 5,
        "sheet" => 6,
        _ => 1,
    }
}

impl Prepared {
    /// Prepares a drawing. Never fails: items that cannot be prepared (degenerate polygons, non-finite
    /// coordinates) are drawn as far as possible or skipped.
    pub fn new(d: &Drawing) -> Prepared {
        let scale = if d.scale.is_finite() && d.scale > 0.0 { d.scale } else { 1.0 };
        let paper_unit = match d.units {
            DrawingUnits::Metres => scale / 1000.0,
            DrawingUnits::Millimetres => 1.0,
        };
        let mut items = Vec::with_capacity(d.items.len());
        let mut by_source: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut bounds: Option<Rect> = None;
        for item in &d.items {
            let Some(p) = prepare_item(item, paper_unit) else { continue };
            bounds = Some(match bounds {
                Some(b) => b.union(&p.bounds),
                None => p.bounds,
            });
            if let Some(s) = p.source {
                by_source.entry(s).or_default().push(items.len());
            }
            items.push(p);
        }
        Prepared { units: d.units, scale, items, bounds, by_source }
    }

    /// One paper millimetre in drawing units.
    pub fn paper_unit(&self) -> f64 {
        match self.units {
            DrawingUnits::Metres => self.scale / 1000.0,
            DrawingUnits::Millimetres => 1.0,
        }
    }

    /// Bounds of the items of a set of sources.
    pub fn bounds_of<'a>(&self, sources: impl IntoIterator<Item = &'a u64>) -> Option<Rect> {
        let mut out: Option<Rect> = None;
        for s in sources {
            for &i in self.by_source.get(s).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(it) = self.items.get(i) {
                    out = Some(match out {
                        Some(b) => b.union(&it.bounds),
                        None => it.bounds,
                    });
                }
            }
        }
        out
    }

    /// The source of the item under a drawing point, within `radius` drawing units. Items with a lower rank win;
    /// within a rank the nearest wins. `accept` filters sources (e.g. only existing elements).
    pub fn hit(&self, at: Pt, radius: f64, accept: impl Fn(u64) -> bool) -> Option<u64> {
        let mut best: Option<(u8, f64, u64)> = None;
        for it in &self.items {
            let Some(src) = it.source else { continue };
            let b = it.bounds.expanded(radius);
            if at.x < b.min.x || at.x > b.max.x || at.y < b.min.y || at.y > b.max.y {
                continue;
            }
            let Some(d) = item_distance(it, at, radius) else { continue };
            if d > radius || !accept(src) {
                continue;
            }
            let better = match best {
                None => true,
                Some((r, bd, _)) => it.rank < r || (it.rank == r && d < bd),
            };
            if better {
                best = Some((it.rank, d, src));
            }
        }
        best.map(|(_, _, s)| s)
    }
}

/// Distance from a point to an item's geometry (0 inside filled areas and text boxes), or `None` if the item
/// has nothing to hit.
fn item_distance(it: &PItem, at: Pt, radius: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut take = |d: f64| best = Some(best.map_or(d, |b: f64| b.min(d)));
    if it.fill.is_some() || it.hatch.is_some() {
        let rings: Vec<&Vec<Pt>> = it.rings.iter().collect();
        if !rings.is_empty() && inside(&rings, at) {
            take(0.0);
        }
    }
    for s in &it.strokes {
        match &s.path {
            PPath::Poly { points, closed } => {
                let n = points.len();
                let edges = if *closed { n } else { n.saturating_sub(1) };
                for i in 0..edges {
                    if let (Some(a), Some(b)) = (points.get(i), points.get((i + 1) % n)) {
                        take(segment_distance(at, *a, *b));
                    }
                }
            }
            PPath::Arc { centre, radius: r, start, sweep } => {
                let d = at.distance(*centre);
                let ang = (at.y - centre.y).atan2(at.x - centre.x);
                if on_sweep(ang, *start, *sweep) {
                    take((d - r).abs());
                } else {
                    let p0 = pt(centre.x + r * start.cos(), centre.y + r * start.sin());
                    let e = start + sweep;
                    let p1 = pt(centre.x + r * e.cos(), centre.y + r * e.sin());
                    take(at.distance(p0).min(at.distance(p1)));
                }
            }
        }
    }
    if it.text.is_some() {
        let b = it.bounds;
        if at.x >= b.min.x && at.x <= b.max.x && at.y >= b.min.y && at.y <= b.max.y {
            take(0.0);
        } else {
            let dx = (b.min.x - at.x).max(at.x - b.max.x).max(0.0);
            let dy = (b.min.y - at.y).max(at.y - b.max.y).max(0.0);
            take(dx.hypot(dy));
        }
    }
    let _ = radius;
    best
}

/// Whether angle `a` lies on the arc from `start` sweeping `sweep` (radians).
fn on_sweep(a: f64, start: f64, sweep: f64) -> bool {
    if sweep.abs() >= std::f64::consts::TAU - 1e-12 {
        return true;
    }
    let tau = std::f64::consts::TAU;
    let (s, e) = if sweep >= 0.0 { (start, start + sweep) } else { (start + sweep, start) };
    let rel = (a - s).rem_euclid(tau);
    rel <= e - s
}

/// Distance from a point to a segment.
pub fn segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 <= 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    p.distance(pt(a.x + dx * t, a.y + dy * t))
}

fn finite(p: &Pt) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn rect_of(points: impl IntoIterator<Item = Pt>) -> Option<Rect> {
    let mut it = points.into_iter().filter(finite);
    let first = it.next()?;
    let mut r = Rect { min: first, max: first };
    for p in it {
        r.include(p);
    }
    Some(r)
}

/// Triangulates rings (outline first, holes after). Falls back to a fan for convex outlines when the
/// triangulator rejects the input.
pub fn triangulate_rings(rings: &[Vec<Pt>]) -> Option<(Vec<Pt>, Vec<[u32; 3]>)> {
    let outer = rings.first()?;
    if outer.len() < 3 {
        return None;
    }
    let to_poly = |r: &Vec<Pt>| Polygon2::new(r.iter().map(|p| Point2::new(p.x, p.y)).collect());
    let shape = PolygonWithHoles {
        outer: to_poly(outer),
        holes: rings.iter().skip(1).filter(|r| r.len() >= 3).map(to_poly).collect(),
    };
    match kanavos_solid::triangulate(&shape) {
        Ok((pts, tris)) => {
            let verts = pts
                .iter()
                .map(|p| {
                    let [x, y] = p.xy();
                    pt(x, y)
                })
                .collect();
            Some((verts, tris))
        }
        Err(_) => {
            let n = outer.len() as u32;
            let tris = (1..n.saturating_sub(1)).map(|i| [0, i, i + 1]).collect();
            Some((outer.clone(), tris))
        }
    }
}

fn circle_points(centre: Pt, radius: f64, n: usize) -> Vec<Pt> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            pt(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect()
}

fn stroke_of(item: &Item, path: PPath) -> Option<PStroke> {
    let s = item.stroke.as_ref()?;
    Some(PStroke {
        path,
        weight_mm: if s.weight_mm.is_finite() { s.weight_mm.max(0.0) } else { 0.0 },
        color: s.color,
        dashes_mm: s.pattern.dashes_mm(),
    })
}

/// Fill and hatch of a closed area.
fn area(fill: &Fill, rings: &[Vec<Pt>], paper_unit: f64) -> (Option<PFill>, Option<PHatch>) {
    let (color, hatch) = match fill {
        Fill::Solid(c) => (*c, None),
        Fill::Hatch { pattern, color, background, scale, angle } => {
            let h = hatch_area(*pattern, *scale, *angle, rings, paper_unit);
            let hatch = (h.coverage > 0.0).then_some(PHatch {
                segments: h.segments,
                color: *color,
                spacing: h.spacing,
                coverage: h.coverage,
                background: *background,
                weight_mm: h.weight_mm,
            });
            (background.unwrap_or(Rgba::TRANSPARENT), hatch)
        }
    };
    let fill = triangulate_rings(rings).map(|(verts, tris)| PFill { verts, tris, color });
    (fill, hatch)
}

/// Prepares one item; `None` for items without drawable geometry.
fn prepare_item(item: &Item, paper_unit: f64) -> Option<PItem> {
    let mut fill = None;
    let mut hatch = None;
    let mut strokes = Vec::new();
    let mut text = None;
    let mut rings: Vec<Vec<Pt>> = Vec::new();
    let bounds;
    match &item.shape {
        Shape::Line { a, b } => {
            bounds = rect_of([*a, *b])?;
            strokes.extend(stroke_of(item, PPath::Poly { points: vec![*a, *b], closed: false }));
        }
        Shape::Polyline { points, closed } => {
            let points: Vec<Pt> = points.iter().copied().filter(finite).collect();
            bounds = rect_of(points.iter().copied())?;
            if *closed
                && points.len() >= 3
                && let Some(f) = &item.fill
            {
                rings = vec![points.clone()];
                (fill, hatch) = area(f, &rings, paper_unit);
            }
            strokes.extend(stroke_of(item, PPath::Poly { points, closed: *closed }));
        }
        Shape::Polygon { rings: rs } => {
            rings = rs
                .iter()
                .map(|r| r.iter().copied().filter(finite).collect::<Vec<_>>())
                .filter(|r| r.len() >= 2)
                .collect();
            bounds = rect_of(rings.iter().flatten().copied())?;
            if let Some(f) = &item.fill {
                (fill, hatch) = area(f, &rings, paper_unit);
            }
            for r in &rings {
                strokes.extend(stroke_of(item, PPath::Poly { points: r.clone(), closed: true }));
            }
        }
        Shape::Arc { centre, radius, start, sweep } => {
            if !(finite(centre)
                && radius.is_finite()
                && *radius > 0.0
                && start.is_finite()
                && sweep.is_finite())
            {
                return None;
            }
            let n = ((sweep.abs() / std::f64::consts::TAU) * 64.0).ceil().max(2.0) as usize;
            bounds = rect_of((0..=n).map(|i| {
                let a = start + sweep * i as f64 / n as f64;
                pt(centre.x + radius * a.cos(), centre.y + radius * a.sin())
            }))?;
            strokes.extend(stroke_of(
                item,
                PPath::Arc { centre: *centre, radius: *radius, start: *start, sweep: *sweep },
            ));
        }
        Shape::Circle { centre, radius } => {
            if !(finite(centre) && radius.is_finite() && *radius > 0.0) {
                return None;
            }
            bounds = Rect { min: centre.offset(-radius, -radius), max: centre.offset(*radius, *radius) };
            if let Some(f) = &item.fill {
                rings = vec![circle_points(*centre, *radius, CIRCLE_FILL_SEGMENTS)];
                (fill, hatch) = area(f, &rings, paper_unit);
            }
            strokes.extend(stroke_of(
                item,
                PPath::Arc { centre: *centre, radius: *radius, start: 0.0, sweep: std::f64::consts::TAU },
            ));
        }
        Shape::Text(t) => {
            if !finite(&t.position) || t.content.trim().is_empty() {
                return None;
            }
            bounds = text_bounds(t, paper_unit);
            text = Some(t.clone());
        }
    }
    Some(PItem {
        source: item.source,
        rank: layer_rank(&item.layer),
        bounds,
        fill,
        hatch,
        strokes,
        text,
        rings,
    })
}

/// Approximate box of a text item in drawing units (rotation included).
pub fn text_bounds(t: &Text, paper_unit: f64) -> Rect {
    let w = t.approx_width_mm() * paper_unit;
    let h = t.height_mm * paper_unit;
    let x0 = -t.anchor.h_fraction() * w;
    let y0 = -t.anchor.v_fraction() * h;
    let (s, c) = if t.rotation.is_finite() { t.rotation.sin_cos() } else { (0.0, 1.0) };
    let corners = [(x0, y0), (x0 + w, y0), (x0 + w, y0 + h), (x0, y0 + h)]
        .map(|(x, y)| pt(t.position.x + x * c - y * s, t.position.y + x * s + y * c));
    rect_of(corners).unwrap_or(Rect { min: t.position, max: t.position })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::{HatchPattern, Stroke, weights};

    fn wall_drawing() -> Drawing {
        let mut d = Drawing::model(50.0);
        d.push(
            Item::polygon(
                vec![vec![pt(0.0, 0.0), pt(5.0, 0.0), pt(5.0, 0.3), pt(0.0, 0.3)]],
                Some(Fill::hatch(HatchPattern::Masonry, None)),
                Some(Stroke::solid(weights::THICK)),
                "wall.cut",
            )
            .with_source(7),
        );
        d.push(
            Item::polygon(
                vec![vec![pt(-1.0, -1.0), pt(6.0, -1.0), pt(6.0, 4.0), pt(-1.0, 4.0)]],
                Some(Fill::Solid(Rgba::rgb(250, 244, 225))),
                None,
                "space.fill",
            )
            .with_source(9),
        );
        d.push(
            Item::line(pt(1.0, 0.15), pt(2.0, 0.15), Stroke::solid(weights::FINE), "door.symbol")
                .with_source(8),
        );
        d
    }

    #[test]
    fn polygons_get_triangles_and_hatches() {
        let p = Prepared::new(&wall_drawing());
        let wall = p.items.first().unwrap();
        let f = wall.fill.as_ref().unwrap();
        assert_eq!(f.tris.len(), 2);
        let h = wall.hatch.as_ref().unwrap();
        assert!(!h.segments.is_empty());
        // 1:50 masonry spacing 0.8 mm → 0.04 m.
        assert!((h.spacing - 0.04).abs() < 1e-9);
        assert_eq!(wall.strokes.len(), 1);
    }

    #[test]
    fn concave_polygon_with_hole_is_triangulated() {
        let rings = vec![
            vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 4.0), pt(4.0, 4.0), pt(4.0, 10.0), pt(0.0, 10.0)],
            vec![pt(1.0, 1.0), pt(1.0, 2.0), pt(2.0, 2.0), pt(2.0, 1.0)],
        ];
        let (verts, tris) = triangulate_rings(&rings).unwrap();
        let area: f64 = tris
            .iter()
            .map(|t| {
                let a = verts[t[0] as usize];
                let b = verts[t[1] as usize];
                let c = verts[t[2] as usize];
                ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() / 2.0
            })
            .sum();
        assert!((area - (64.0 - 1.0)).abs() < 1e-6, "area {area}");
    }

    #[test]
    fn hits_prefer_openings_over_walls_over_spaces() {
        let p = Prepared::new(&wall_drawing());
        // On the door line, inside the wall, inside the space.
        assert_eq!(p.hit(pt(1.5, 0.15), 0.05, |_| true), Some(8));
        // Inside the wall only.
        assert_eq!(p.hit(pt(4.0, 0.15), 0.05, |_| true), Some(7));
        // Inside the space only.
        assert_eq!(p.hit(pt(3.0, 2.0), 0.05, |_| true), Some(9));
        // Outside everything.
        assert_eq!(p.hit(pt(30.0, 30.0), 0.05, |_| true), None);
        // Filtered.
        assert_eq!(p.hit(pt(3.0, 2.0), 0.05, |s| s != 9), None);
    }

    #[test]
    fn bounds_cover_items_and_sources_are_indexed() {
        let p = Prepared::new(&wall_drawing());
        let b = p.bounds.unwrap();
        assert!((b.min.x + 1.0).abs() < 1e-12 && (b.max.y - 4.0).abs() < 1e-12);
        let wb = p.bounds_of([7u64].iter()).unwrap();
        assert!((wb.max.x - 5.0).abs() < 1e-12);
        assert_eq!(p.by_source.get(&8).map(Vec::len), Some(1));
    }

    #[test]
    fn rotated_text_bounds_turn_with_the_text() {
        let t = Text::new(pt(0.0, 0.0), "ABCD", 2.5).rotated(std::f64::consts::FRAC_PI_2);
        let b = text_bounds(&t, 1.0);
        // Width 4 × 2.5 × 0.6 = 6 mm, now vertical.
        assert!((b.height() - 6.0).abs() < 1e-9 && (b.width() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn arcs_hit_only_on_their_sweep() {
        let mut d = Drawing::model(50.0);
        d.push(Item {
            shape: Shape::Arc {
                centre: pt(0.0, 0.0),
                radius: 1.0,
                start: 0.0,
                sweep: std::f64::consts::FRAC_PI_2,
            },
            stroke: Some(Stroke::solid(weights::FINE)),
            fill: None,
            layer: "door.symbol".into(),
            source: Some(1),
        });
        let p = Prepared::new(&d);
        let a = std::f64::consts::FRAC_PI_4;
        assert_eq!(p.hit(pt(a.cos(), a.sin()), 0.01, |_| true), Some(1));
        assert_eq!(p.hit(pt(-1.0, 0.0), 0.01, |_| true), None);
    }
}

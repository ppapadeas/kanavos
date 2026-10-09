//! Resolution-independent 2D drawing primitives shared by views, sheets and exports.
//!
//! Layer L2. See `docs/architecture.md`.
//!
//! A [`Drawing`] is a flat list of [`Item`]s. Coordinates are in the drawing's [`DrawingUnits`]: metres for
//! model-space drawings (plans, sections, elevations) and millimetres for paper-space drawings (sheets).
//!
//! Sizes that must look the same on paper whatever the scale — stroke weights, dash lengths, hatch spacing and
//! text heights — are always in **paper millimetres**. For a model-space drawing at scale 1:`scale`, one paper
//! millimetre is `scale / 1000` metres in drawing coordinates (see [`Drawing::paper_mm`]).
//!
//! Output helpers shared by every exporter and renderer:
//!
//! - [`geometry`]: arcs and circles flattened to polylines, fill rings, exact bounds;
//! - [`hatch`]: hatch patterns expanded into clipped line segments;
//! - [`dash`]: line patterns applied to polylines, continuous around corners;
//! - [`text`]: text measured and laid out with real font metrics ([`fonts`]);
//! - [`page`]: placing a drawing on a page in paper millimetres;
//! - [`svg`]: standalone SVG export.

pub mod dash;
pub mod geometry;
pub mod hatch;
pub mod page;
pub mod svg;
pub mod text;

/// Fonts used for text layout (re-exported so downstream crates share one version).
pub use kanavos_fonts as fonts;

use std::f64::consts::TAU;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A point in drawing coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Pt {
    /// X (right).
    pub x: f64,
    /// Y (up).
    pub y: f64,
}

/// Shorthand constructor for [`Pt`].
pub const fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

impl Pt {
    /// Distance to another point.
    pub fn distance(self, o: Pt) -> f64 {
        (o.x - self.x).hypot(o.y - self.y)
    }
    /// Point halfway to another point.
    pub fn midpoint(self, o: Pt) -> Pt {
        pt((self.x + o.x) / 2.0, (self.y + o.y) / 2.0)
    }
    /// Translated copy.
    pub fn offset(self, dx: f64, dy: f64) -> Pt {
        pt(self.x + dx, self.y + dy)
    }
}

/// An RGBA colour, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (255 is opaque).
    pub a: u8,
}

impl Rgba {
    /// Black.
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    /// White.
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);
    /// Mid grey.
    pub const GREY: Rgba = Rgba::rgb(128, 128, 128);
    /// Light grey, used for poché backgrounds.
    pub const LIGHT_GREY: Rgba = Rgba::rgb(210, 210, 210);
    /// Fully transparent.
    pub const TRANSPARENT: Rgba = Rgba { r: 0, g: 0, b: 0, a: 0 };

    /// Opaque colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r, g, b, a: 255 }
    }
    /// Colour with alpha.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Rgba {
        Rgba { r, g, b, a }
    }
    /// `#rrggbb` (alpha ignored).
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Coordinate units of a drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawingUnits {
    /// Model space, metres.
    #[default]
    Metres,
    /// Paper space, millimetres.
    Millimetres,
}

/// Standard pen widths in paper millimetres (ISO 128-20 series).
pub mod weights {
    /// 0.13 mm — hatching, very fine detail.
    pub const EXTRA_FINE: f64 = 0.13;
    /// 0.18 mm — projected edges far away, hatch outlines, furniture.
    pub const FINE: f64 = 0.18;
    /// 0.25 mm — projected edges, dimensions, text.
    pub const THIN: f64 = 0.25;
    /// 0.35 mm — near projected edges, symbols.
    pub const MEDIUM: f64 = 0.35;
    /// 0.50 mm — cut outlines of minor elements.
    pub const THICK: f64 = 0.50;
    /// 0.70 mm — cut outlines of walls and slabs.
    pub const EXTRA_THICK: f64 = 0.70;
    /// 1.00 mm — sheet borders, emphasis.
    pub const HEAVY: f64 = 1.00;
}

/// Dash pattern of a stroke. Lengths are paper millimetres.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinePattern {
    /// Continuous.
    #[default]
    Solid,
    /// Long dashes: elements above the cut plane, hidden edges.
    Dashed,
    /// Short dashes: hidden or overhead detail.
    Hidden,
    /// Dots.
    Dotted,
    /// Dash-dot: axes, section lines, grid lines.
    DashDot,
    /// Dash-dot-dot: boundaries (plot, setback lines).
    DashDotDot,
    /// Custom on/off lengths in paper mm, starting with a dash.
    Custom(Vec<f64>),
}

impl LinePattern {
    /// On/off lengths in paper millimetres; empty for solid lines.
    pub fn dashes_mm(&self) -> Vec<f64> {
        match self {
            LinePattern::Solid => Vec::new(),
            LinePattern::Dashed => vec![3.0, 1.5],
            LinePattern::Hidden => vec![1.5, 1.0],
            LinePattern::Dotted => vec![0.2, 0.8],
            LinePattern::DashDot => vec![6.0, 1.0, 0.5, 1.0],
            LinePattern::DashDotDot => vec![6.0, 1.0, 0.5, 1.0, 0.5, 1.0],
            LinePattern::Custom(v) => v.clone(),
        }
    }
}

/// How an outline is stroked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stroke {
    /// Printed width in paper millimetres.
    pub weight_mm: f64,
    /// Colour.
    pub color: Rgba,
    /// Dash pattern.
    #[serde(default, skip_serializing_if = "is_solid")]
    pub pattern: LinePattern,
}

fn is_solid(p: &LinePattern) -> bool {
    *p == LinePattern::Solid
}

impl Stroke {
    /// A solid black stroke of the given weight.
    pub fn solid(weight_mm: f64) -> Stroke {
        Stroke { weight_mm, color: Rgba::BLACK, pattern: LinePattern::Solid }
    }
    /// A black stroke with a pattern.
    pub fn patterned(weight_mm: f64, pattern: LinePattern) -> Stroke {
        Stroke { weight_mm, color: Rgba::BLACK, pattern }
    }
    /// The same stroke in another colour.
    pub fn with_color(mut self, color: Rgba) -> Stroke {
        self.color = color;
        self
    }
}

/// Hatch patterns for cut materials and surfaces. Spacing is in paper millimetres, so hatches read the same at
/// every scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HatchPattern {
    /// No hatch lines (use the background colour only).
    #[default]
    None,
    /// Parallel lines at 45°.
    Diagonal,
    /// Dense parallel lines at 45°: brick and block masonry.
    Masonry,
    /// Crossed lines at ±45°: reinforced concrete.
    CrossDiagonal,
    /// Scattered dots and small triangles: plain concrete, screed.
    Concrete,
    /// Wavy band: soft insulation.
    Insulation,
    /// Horizontal lines: timber and boards.
    Wood,
    /// Square grid: tiles.
    Grid,
    /// Dots: plaster, render, sand.
    Dots,
    /// Short diagonal strokes with gaps: earth, fill.
    Earth,
    /// Small circles: gravel.
    Gravel,
    /// Parallel horizontal lines: glass and water.
    Horizontal,
}

/// Fill of a closed area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    /// A flat colour.
    Solid(Rgba),
    /// A hatch pattern over an optional background colour.
    Hatch {
        /// Pattern.
        pattern: HatchPattern,
        /// Line colour.
        color: Rgba,
        /// Background colour, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        background: Option<Rgba>,
        /// Spacing multiplier (1.0 = the pattern's standard paper spacing).
        #[serde(default = "one")]
        scale: f64,
        /// Extra rotation in radians.
        #[serde(default)]
        angle: f64,
    },
}

fn one() -> f64 {
    1.0
}

impl Fill {
    /// A black hatch with the standard spacing over a background colour.
    pub fn hatch(pattern: HatchPattern, background: Option<Rgba>) -> Fill {
        Fill::Hatch { pattern, color: Rgba::BLACK, background, scale: 1.0, angle: 0.0 }
    }
}

/// Text anchor: which point of the text box sits at the text position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// Bottom-left.
    #[default]
    BottomLeft,
    /// Bottom-centre.
    BottomCentre,
    /// Bottom-right.
    BottomRight,
    /// Middle-left.
    MiddleLeft,
    /// Centre.
    Centre,
    /// Middle-right.
    MiddleRight,
    /// Top-left.
    TopLeft,
    /// Top-centre.
    TopCentre,
    /// Top-right.
    TopRight,
}

impl Anchor {
    /// Horizontal fraction of the text width to the left of the anchor (0 left, 0.5 centre, 1 right).
    pub fn h_fraction(self) -> f64 {
        match self {
            Anchor::BottomLeft | Anchor::MiddleLeft | Anchor::TopLeft => 0.0,
            Anchor::BottomCentre | Anchor::Centre | Anchor::TopCentre => 0.5,
            Anchor::BottomRight | Anchor::MiddleRight | Anchor::TopRight => 1.0,
        }
    }
    /// Vertical fraction of the text height below the anchor (0 bottom, 0.5 middle, 1 top).
    pub fn v_fraction(self) -> f64 {
        match self {
            Anchor::BottomLeft | Anchor::BottomCentre | Anchor::BottomRight => 0.0,
            Anchor::MiddleLeft | Anchor::Centre | Anchor::MiddleRight => 0.5,
            Anchor::TopLeft | Anchor::TopCentre | Anchor::TopRight => 1.0,
        }
    }
}

/// A single line of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Text {
    /// Anchor position in drawing coordinates.
    pub position: Pt,
    /// The text (one line; split multi-line text into several items).
    pub content: String,
    /// Cap height in paper millimetres (2.5 mm is a typical annotation size).
    pub height_mm: f64,
    /// Rotation in radians, counter-clockwise.
    #[serde(default)]
    pub rotation: f64,
    /// Which point of the text sits at `position`.
    #[serde(default)]
    pub anchor: Anchor,
    /// Bold weight.
    #[serde(default)]
    pub bold: bool,
    /// Colour.
    #[serde(default = "black")]
    pub color: Rgba,
}

fn black() -> Rgba {
    Rgba::BLACK
}

impl Text {
    /// Black text with default anchor and no rotation.
    pub fn new(position: Pt, content: impl Into<String>, height_mm: f64) -> Text {
        Text {
            position,
            content: content.into(),
            height_mm,
            rotation: 0.0,
            anchor: Anchor::BottomLeft,
            bold: false,
            color: Rgba::BLACK,
        }
    }
    /// Sets the anchor.
    pub fn anchored(mut self, anchor: Anchor) -> Text {
        self.anchor = anchor;
        self
    }
    /// Sets the rotation in radians.
    pub fn rotated(mut self, rotation: f64) -> Text {
        self.rotation = rotation;
        self
    }
    /// Approximate advance width in paper millimetres (average glyph width 0.6 × height), for layout.
    pub fn approx_width_mm(&self) -> f64 {
        self.content.chars().count() as f64 * self.height_mm * 0.6
    }
}

/// Geometry of a drawing item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    /// A straight line.
    Line {
        /// Start.
        a: Pt,
        /// End.
        b: Pt,
    },
    /// A polyline, optionally closed.
    Polyline {
        /// Vertices.
        points: Vec<Pt>,
        /// Whether the last vertex joins the first.
        closed: bool,
    },
    /// A filled area; the first ring is the outline, further rings are holes (even-odd rule).
    Polygon {
        /// Closed rings without repeating the first vertex.
        rings: Vec<Vec<Pt>>,
    },
    /// A circular arc from `start` sweeping `sweep` radians (positive counter-clockwise).
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
    /// A full circle.
    Circle {
        /// Centre.
        centre: Pt,
        /// Radius.
        radius: f64,
    },
    /// A line of text.
    Text(Text),
}

impl Shape {
    /// Points that bound the shape (text uses its anchor point only).
    pub fn extent_points(&self) -> Vec<Pt> {
        match self {
            Shape::Line { a, b } => vec![*a, *b],
            Shape::Polyline { points, .. } => points.clone(),
            Shape::Polygon { rings } => rings.iter().flatten().copied().collect(),
            Shape::Arc { centre, radius, start, sweep } => {
                let n = ((sweep.abs() / TAU) * 32.0).ceil().max(2.0) as usize;
                (0..=n)
                    .map(|i| {
                        let a = start + sweep * i as f64 / n as f64;
                        pt(centre.x + radius * a.cos(), centre.y + radius * a.sin())
                    })
                    .collect()
            }
            Shape::Circle { centre, radius } => {
                vec![centre.offset(-radius, -radius), centre.offset(*radius, *radius)]
            }
            Shape::Text(t) => vec![t.position],
        }
    }
}

/// What an item belongs to: used for selection, hit-testing and highlighting in views.
pub type SourceId = u64;

/// One drawn thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Item {
    /// Geometry.
    pub shape: Shape,
    /// Outline, if any. Text ignores strokes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    /// Fill, for closed shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    /// Layer name, used for visibility and as the DXF layer (e.g. `"walls.cut"`, `"dimensions"`).
    pub layer: String,
    /// The model object this item represents, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceId>,
}

impl Item {
    /// A stroked line.
    pub fn line(a: Pt, b: Pt, stroke: Stroke, layer: impl Into<String>) -> Item {
        Item {
            shape: Shape::Line { a, b },
            stroke: Some(stroke),
            fill: None,
            layer: layer.into(),
            source: None,
        }
    }
    /// A stroked polyline.
    pub fn polyline(points: Vec<Pt>, closed: bool, stroke: Stroke, layer: impl Into<String>) -> Item {
        Item {
            shape: Shape::Polyline { points, closed },
            stroke: Some(stroke),
            fill: None,
            layer: layer.into(),
            source: None,
        }
    }
    /// A filled, optionally stroked, polygon with holes.
    pub fn polygon(
        rings: Vec<Vec<Pt>>,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
        layer: impl Into<String>,
    ) -> Item {
        Item { shape: Shape::Polygon { rings }, stroke, fill, layer: layer.into(), source: None }
    }
    /// A text item.
    pub fn text(text: Text, layer: impl Into<String>) -> Item {
        Item { shape: Shape::Text(text), stroke: None, fill: None, layer: layer.into(), source: None }
    }
    /// Sets the source object.
    pub fn with_source(mut self, source: SourceId) -> Item {
        self.source = Some(source);
        self
    }
}

/// An axis-aligned box in drawing coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    /// Minimum corner.
    pub min: Pt,
    /// Maximum corner.
    pub max: Pt,
}

impl Rect {
    /// Width.
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    /// Height.
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
    /// Centre.
    pub fn centre(&self) -> Pt {
        self.min.midpoint(self.max)
    }
    /// Grows the box to include a point.
    pub fn include(&mut self, p: Pt) {
        self.min = pt(self.min.x.min(p.x), self.min.y.min(p.y));
        self.max = pt(self.max.x.max(p.x), self.max.y.max(p.y));
    }
    /// Union of two boxes.
    pub fn union(&self, o: &Rect) -> Rect {
        let mut r = *self;
        r.include(o.min);
        r.include(o.max);
        r
    }
    /// The box grown by a margin on every side.
    pub fn expanded(&self, m: f64) -> Rect {
        Rect { min: self.min.offset(-m, -m), max: self.max.offset(m, m) }
    }
}

/// A 2D affine map for placing drawings (e.g. a model view onto a sheet): `p' = (a·x + b·y + tx, c·x + d·y + ty)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Affine {
    /// Row 0, column 0.
    pub a: f64,
    /// Row 0, column 1.
    pub b: f64,
    /// Row 1, column 0.
    pub c: f64,
    /// Row 1, column 1.
    pub d: f64,
    /// Translation X.
    pub tx: f64,
    /// Translation Y.
    pub ty: f64,
}

impl Affine {
    /// Identity.
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    /// Uniform scale then translation.
    pub fn scale_translate(k: f64, tx: f64, ty: f64) -> Affine {
        Affine { a: k, b: 0.0, c: 0.0, d: k, tx, ty }
    }
    /// Rotation about the origin.
    pub fn rotation(angle: f64) -> Affine {
        let (s, c) = angle.sin_cos();
        Affine { a: c, b: -s, c: s, d: c, tx: 0.0, ty: 0.0 }
    }
    /// Composition: applies `other` first, then `self`.
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            a: self.a * o.a + self.b * o.c,
            b: self.a * o.b + self.b * o.d,
            c: self.c * o.a + self.d * o.c,
            d: self.c * o.b + self.d * o.d,
            tx: self.a * o.tx + self.b * o.ty + self.tx,
            ty: self.c * o.tx + self.d * o.ty + self.ty,
        }
    }
    /// Applies the map to a point.
    pub fn apply(&self, p: Pt) -> Pt {
        pt(self.a * p.x + self.b * p.y + self.tx, self.c * p.x + self.d * p.y + self.ty)
    }
    /// Uniform scale factor (square root of the absolute determinant).
    pub fn scale_factor(&self) -> f64 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }
    /// Rotation angle of the linear part.
    pub fn rotation_angle(&self) -> f64 {
        self.c.atan2(self.a)
    }
    /// Whether the map mirrors.
    pub fn is_mirroring(&self) -> bool {
        self.a * self.d - self.b * self.c < 0.0
    }
}

/// A complete drawing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Drawing {
    /// Coordinate units.
    pub units: DrawingUnits,
    /// Scale denominator for model-space drawings (100 means 1:100). Paper drawings use 1.
    pub scale: f64,
    /// Items in painting order (later items draw over earlier ones).
    pub items: Vec<Item>,
}

impl Drawing {
    /// An empty model-space drawing at scale 1:`scale`.
    pub fn model(scale: f64) -> Drawing {
        Drawing { units: DrawingUnits::Metres, scale, items: Vec::new() }
    }

    /// An empty paper-space drawing (millimetres).
    pub fn paper() -> Drawing {
        Drawing { units: DrawingUnits::Millimetres, scale: 1.0, items: Vec::new() }
    }

    /// Drawing-coordinate length of `mm` paper millimetres.
    pub fn paper_mm(&self, mm: f64) -> f64 {
        match self.units {
            DrawingUnits::Metres => mm * self.scale / 1000.0,
            DrawingUnits::Millimetres => mm,
        }
    }

    /// Adds an item.
    pub fn push(&mut self, item: Item) {
        self.items.push(item);
    }

    /// Appends all items of another drawing (assumed to use the same units and scale).
    pub fn extend(&mut self, other: Drawing) {
        self.items.extend(other.items);
    }

    /// Bounding box of all items (text by anchor point), or `None` when empty.
    pub fn bounds(&self) -> Option<Rect> {
        let mut pts = self.items.iter().flat_map(|i| i.shape.extent_points());
        let first = pts.next()?;
        let mut r = Rect { min: first, max: first };
        for p in pts {
            r.include(p);
        }
        Some(r)
    }

    /// Items whose layer starts with a prefix.
    pub fn layer_items<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a Item> + 'a {
        self.items.iter().filter(move |i| i.layer.starts_with(prefix))
    }

    /// The drawing mapped through an affine transform. Paper sizes (weights, text heights) are unchanged; text
    /// rotation follows the map's rotation.
    pub fn transformed(&self, m: &Affine) -> Drawing {
        let rot = m.rotation_angle();
        let k = m.scale_factor();
        let items = self
            .items
            .iter()
            .map(|it| {
                let shape = match &it.shape {
                    Shape::Line { a, b } => Shape::Line { a: m.apply(*a), b: m.apply(*b) },
                    Shape::Polyline { points, closed } => Shape::Polyline {
                        points: points.iter().map(|p| m.apply(*p)).collect(),
                        closed: *closed,
                    },
                    Shape::Polygon { rings } => Shape::Polygon {
                        rings: rings.iter().map(|r| r.iter().map(|p| m.apply(*p)).collect()).collect(),
                    },
                    Shape::Arc { centre, radius, start, sweep } => {
                        if m.is_mirroring() {
                            // A mirrored arc runs the other way: map both ends and reverse.
                            let end = start + sweep;
                            let p_end = pt(centre.x + radius * end.cos(), centre.y + radius * end.sin());
                            let c = m.apply(*centre);
                            let q = m.apply(p_end);
                            Shape::Arc {
                                centre: c,
                                radius: radius * k,
                                start: (q.y - c.y).atan2(q.x - c.x),
                                sweep: *sweep,
                            }
                        } else {
                            Shape::Arc {
                                centre: m.apply(*centre),
                                radius: radius * k,
                                start: start + rot,
                                sweep: *sweep,
                            }
                        }
                    }
                    Shape::Circle { centre, radius } => {
                        Shape::Circle { centre: m.apply(*centre), radius: radius * k }
                    }
                    Shape::Text(t) => {
                        let mut t = t.clone();
                        t.position = m.apply(t.position);
                        t.rotation += rot;
                        Shape::Text(t)
                    }
                };
                Item { shape, ..it.clone() }
            })
            .collect();
        Drawing { units: self.units, scale: self.scale, items }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_mm_follows_scale() {
        let d = Drawing::model(100.0);
        assert!((d.paper_mm(2.5) - 0.25).abs() < 1e-12);
        assert!((Drawing::paper().paper_mm(2.5) - 2.5).abs() < 1e-12);
    }

    #[test]
    fn bounds_cover_items() {
        let mut d = Drawing::model(50.0);
        d.push(Item::line(pt(0.0, 0.0), pt(4.0, 1.0), Stroke::solid(weights::THIN), "test"));
        d.push(Item::text(Text::new(pt(-1.0, 3.0), "Α", 2.5), "text"));
        let b = d.bounds().unwrap();
        assert!((b.width() - 5.0).abs() < 1e-12 && (b.height() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn transform_maps_model_to_paper() {
        // 1:100 view placed at (100, 50) mm on a sheet: 1 m → 10 mm.
        let mut d = Drawing::model(100.0);
        d.push(Item::line(pt(0.0, 0.0), pt(1.0, 0.0), Stroke::solid(weights::THIN), "x"));
        let m = Affine::scale_translate(10.0, 100.0, 50.0);
        let p = d.transformed(&m);
        match &p.items[0].shape {
            Shape::Line { a, b } => {
                assert!(a.distance(pt(100.0, 50.0)) < 1e-12);
                assert!(b.distance(pt(110.0, 50.0)) < 1e-12);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn items_round_trip_through_json() {
        let item = Item::polygon(
            vec![vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0)]],
            Some(Fill::hatch(HatchPattern::Masonry, Some(Rgba::LIGHT_GREY))),
            Some(Stroke::solid(weights::EXTRA_THICK)),
            "walls.cut",
        )
        .with_source(7);
        let json = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(item, back);
    }
}

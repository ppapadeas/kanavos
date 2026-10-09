//! Planar primitives.

use std::borrow::Cow;
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

use kanavos_units::{Angle, Length, Tolerance};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A point in the plan, in metres. Serialises as `[x, y]`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point2 {
    /// X coordinate (east in plan).
    pub x: Length,
    /// Y coordinate (north in plan).
    pub y: Length,
}

impl Point2 {
    /// The origin.
    pub const ORIGIN: Point2 = Point2 { x: Length::ZERO, y: Length::ZERO };

    /// Creates a point from metres.
    pub const fn new(x: f64, y: f64) -> Self {
        Point2 { x: Length::m(x), y: Length::m(y) }
    }
    /// Creates a point from typed lengths.
    pub const fn from_lengths(x: Length, y: Length) -> Self {
        Point2 { x, y }
    }
    /// Coordinates in metres.
    pub fn xy(self) -> [f64; 2] {
        [self.x.as_m(), self.y.as_m()]
    }
    /// The position vector from the origin.
    pub fn to_vec(self) -> Vec2 {
        Vec2::new(self.x.as_m(), self.y.as_m())
    }
    /// Distance to another point.
    pub fn distance(self, other: Point2) -> Length {
        Length::m((other - self).length())
    }
    /// Whether two points coincide within tolerance.
    pub fn coincides(self, other: Point2, tol: &Tolerance) -> bool {
        tol.is_zero(self.distance(other))
    }
    /// Point halfway to another point.
    pub fn midpoint(self, other: Point2) -> Point2 {
        self.lerp(other, 0.5)
    }
    /// Linear interpolation: `t = 0` gives `self`, `t = 1` gives `other`.
    pub fn lerp(self, other: Point2, t: f64) -> Point2 {
        self + (other - self) * t
    }
    /// Whether both coordinates are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
    /// Rotates the point about a centre.
    pub fn rotated_about(self, centre: Point2, angle: Angle) -> Point2 {
        centre + (self - centre).rotated(angle)
    }
}

impl Serialize for Point2 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.xy().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Point2 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let [x, y] = <[f64; 2]>::deserialize(d)?;
        Ok(Point2::new(x, y))
    }
}

impl JsonSchema for Point2 {
    fn schema_name() -> Cow<'static, str> {
        "Point2".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A plan point [x, y] in metres (x east, y north).",
            "type": "array",
            "items": { "type": "number" },
            "minItems": 2,
            "maxItems": 2
        })
    }
}

/// A displacement in the plan, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Vec2 {
    /// X component, metres.
    pub x: f64,
    /// Y component, metres.
    pub y: f64,
}

impl Vec2 {
    /// The zero vector.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    /// Unit vector along +X.
    pub const X: Vec2 = Vec2 { x: 1.0, y: 0.0 };
    /// Unit vector along +Y.
    pub const Y: Vec2 = Vec2 { x: 0.0, y: 1.0 };

    /// Creates a vector from metres.
    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }
    /// Unit vector at an angle from +X.
    pub fn from_angle(a: Angle) -> Self {
        Vec2 { x: a.cos(), y: a.sin() }
    }
    /// Euclidean length in metres.
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    /// Squared length.
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }
    /// Dot product.
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    /// 2D cross product (z of the 3D cross product). Positive when `o` is counter-clockwise from `self`.
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    /// The vector scaled to unit length, or `None` if it is shorter than `1e-12` m.
    pub fn normalized(self) -> Option<Vec2> {
        let l = self.length();
        if l > 1e-12 && l.is_finite() { Some(Vec2 { x: self.x / l, y: self.y / l }) } else { None }
    }
    /// Rotated a quarter turn counter-clockwise (the left normal).
    pub fn perp(self) -> Vec2 {
        Vec2 { x: -self.y, y: self.x }
    }
    /// Rotated by an angle.
    pub fn rotated(self, a: Angle) -> Vec2 {
        let (s, c) = (a.sin(), a.cos());
        Vec2 { x: self.x * c - self.y * s, y: self.x * s + self.y * c }
    }
    /// Direction angle from +X.
    pub fn angle(self) -> Angle {
        Angle::rad(self.y.atan2(self.x))
    }
    /// Signed angle that rotates `self` onto `o`, in `(-π, π]`.
    pub fn angle_to(self, o: Vec2) -> Angle {
        Angle::rad(self.cross(o).atan2(self.dot(o)))
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x + o.x, y: self.y + o.y }
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x - o.x, y: self.y - o.y }
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2 { x: -self.x, y: -self.y }
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2 { x: self.x * k, y: self.y * k }
    }
}
impl Add<Vec2> for Point2 {
    type Output = Point2;
    fn add(self, v: Vec2) -> Point2 {
        Point2 { x: self.x + Length::m(v.x), y: self.y + Length::m(v.y) }
    }
}
impl Sub<Vec2> for Point2 {
    type Output = Point2;
    fn sub(self, v: Vec2) -> Point2 {
        Point2 { x: self.x - Length::m(v.x), y: self.y - Length::m(v.y) }
    }
}
impl Sub for Point2 {
    type Output = Vec2;
    fn sub(self, o: Point2) -> Vec2 {
        Vec2 { x: (self.x - o.x).as_m(), y: (self.y - o.y).as_m() }
    }
}

/// A straight segment between two points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Segment2 {
    /// Start point.
    pub start: Point2,
    /// End point.
    pub end: Point2,
}

impl Segment2 {
    /// Creates a segment.
    pub const fn new(start: Point2, end: Point2) -> Self {
        Segment2 { start, end }
    }
    /// Length of the segment.
    pub fn length(&self) -> Length {
        self.start.distance(self.end)
    }
    /// Vector from start to end.
    pub fn vector(&self) -> Vec2 {
        self.end - self.start
    }
    /// Unit direction, or `None` for a zero-length segment.
    pub fn direction(&self) -> Option<Vec2> {
        self.vector().normalized()
    }
    /// Midpoint.
    pub fn midpoint(&self) -> Point2 {
        self.start.midpoint(self.end)
    }
    /// Point at parameter `t` (0 at start, 1 at end).
    pub fn at(&self, t: f64) -> Point2 {
        self.start.lerp(self.end, t)
    }
    /// Parameter of the closest point on the infinite line through the segment (unclamped).
    pub fn project_param(&self, p: Point2) -> f64 {
        let v = self.vector();
        let l2 = v.length_squared();
        if l2 <= 0.0 { 0.0 } else { (p - self.start).dot(v) / l2 }
    }
    /// Closest point on the segment.
    pub fn closest_point(&self, p: Point2) -> Point2 {
        self.at(self.project_param(p).clamp(0.0, 1.0))
    }
    /// Distance from a point to the segment.
    pub fn distance_to(&self, p: Point2) -> Length {
        self.closest_point(p).distance(p)
    }
    /// Signed distance from the infinite line: positive on the left of start→end.
    pub fn signed_distance(&self, p: Point2) -> f64 {
        match self.direction() {
            Some(d) => d.cross(p - self.start),
            None => (p - self.start).length(),
        }
    }
    /// The segment reversed.
    pub fn reversed(&self) -> Segment2 {
        Segment2 { start: self.end, end: self.start }
    }
    /// The segment shifted sideways; positive distances move it to the left of start→end.
    pub fn offset(&self, distance: f64) -> Segment2 {
        let n = self.direction().map(Vec2::perp).unwrap_or(Vec2::ZERO) * distance;
        Segment2 { start: self.start + n, end: self.end + n }
    }
    /// Intersection point of two segments, including touching endpoints, with parameters on both.
    /// Parallel or collinear segments return `None`.
    pub fn intersection(&self, other: &Segment2, tol: &Tolerance) -> Option<(Point2, f64, f64)> {
        let (t, u) = Line2::from_segment(self).intersection_params(&Line2::from_segment(other))?;
        let eps_t = tol.length.as_m() / self.length().as_m().max(1e-12);
        let eps_u = tol.length.as_m() / other.length().as_m().max(1e-12);
        if t >= -eps_t && t <= 1.0 + eps_t && u >= -eps_u && u <= 1.0 + eps_u {
            Some((self.at(t), t, u))
        } else {
            None
        }
    }
}

/// An infinite line through a point with a direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line2 {
    /// A point on the line.
    pub origin: Point2,
    /// Direction (not necessarily unit length).
    pub direction: Vec2,
}

impl Line2 {
    /// The line through a segment.
    pub fn from_segment(s: &Segment2) -> Line2 {
        Line2 { origin: s.start, direction: s.vector() }
    }
    /// Parameters `(t, u)` such that `self.origin + t·self.direction == other.origin + u·other.direction`.
    /// Returns `None` for parallel lines.
    pub fn intersection_params(&self, other: &Line2) -> Option<(f64, f64)> {
        let d = self.direction.cross(other.direction);
        let scale = self.direction.length() * other.direction.length();
        if d.abs() <= 1e-12 * scale.max(1e-300) {
            return None;
        }
        let w = other.origin - self.origin;
        let t = w.cross(other.direction) / d;
        let u = w.cross(self.direction) / d;
        Some((t, u))
    }
    /// Intersection point of two lines, or `None` if parallel.
    pub fn intersection(&self, other: &Line2) -> Option<Point2> {
        let (t, _) = self.intersection_params(other)?;
        Some(self.origin + self.direction * t)
    }
}

/// A circular arc, counter-clockwise from `start` by `sweep` (negative sweep runs clockwise).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Arc2 {
    /// Centre.
    pub centre: Point2,
    /// Radius.
    pub radius: Length,
    /// Start angle.
    pub start: Angle,
    /// Signed sweep angle.
    pub sweep: Angle,
}

impl Arc2 {
    /// Point at parameter `t` (0 at start, 1 at end).
    pub fn at(&self, t: f64) -> Point2 {
        let a = self.start + self.sweep * t;
        self.centre + Vec2::from_angle(a) * self.radius.as_m()
    }
    /// Arc length.
    pub fn length(&self) -> Length {
        self.radius * self.sweep.as_rad().abs()
    }
    /// Arc through `start` and `end` with a DXF-style bulge (`tan(sweep/4)`, positive counter-clockwise).
    /// Returns `None` for a zero bulge (straight) or coincident ends.
    pub fn from_bulge(start: Point2, end: Point2, bulge: f64) -> Option<Arc2> {
        if bulge.abs() < 1e-9 || !bulge.is_finite() {
            return None;
        }
        let chord = end - start;
        let c = chord.length();
        if c < 1e-12 {
            return None;
        }
        let sweep = 4.0 * bulge.atan();
        let radius = c / (2.0 * (sweep / 2.0).sin()).abs();
        // Distance from the chord midpoint to the centre, towards the left of the chord for positive bulge.
        let sagitta_side = (radius * radius - (c / 2.0) * (c / 2.0)).max(0.0).sqrt();
        let n = chord.perp().normalized()?;
        let sign = if bulge.abs() < 1.0 { 1.0 } else { -1.0 } * bulge.signum();
        let centre = start.midpoint(end) + n * (sagitta_side * sign);
        let start_angle = (start - centre).angle();
        Some(Arc2 { centre, radius: Length::m(radius), start: start_angle, sweep: Angle::rad(sweep) })
    }
    /// Points along the arc so that the chord deviates from the arc by at most `max_deviation` metres.
    /// Always includes both ends; at least `min_segments` segments.
    pub fn tessellate(&self, max_deviation: f64, min_segments: usize) -> Vec<Point2> {
        let r = self.radius.as_m().abs();
        let sweep = self.sweep.as_rad().abs();
        let n_dev = if r > max_deviation && max_deviation > 0.0 {
            let max_step = 2.0 * (1.0 - max_deviation / r).acos();
            if max_step > 0.0 { (sweep / max_step).ceil() as usize } else { 64 }
        } else {
            1
        };
        let n = n_dev.clamp(min_segments.max(1), 1024);
        (0..=n).map(|i| self.at(i as f64 / n as f64)).collect()
    }
}

/// An axis-aligned rectangle in the plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bbox2 {
    /// Lower-left corner.
    pub min: Point2,
    /// Upper-right corner.
    pub max: Point2,
}

impl Bbox2 {
    /// Bounding box of points, or `None` when empty.
    pub fn from_points<I: IntoIterator<Item = Point2>>(points: I) -> Option<Bbox2> {
        let mut it = points.into_iter();
        let first = it.next()?;
        let mut b = Bbox2 { min: first, max: first };
        for p in it {
            b.include(p);
        }
        Some(b)
    }
    /// Grows the box to include a point.
    pub fn include(&mut self, p: Point2) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }
    /// Union of two boxes.
    pub fn union(&self, o: &Bbox2) -> Bbox2 {
        let mut b = *self;
        b.include(o.min);
        b.include(o.max);
        b
    }
    /// Width (X extent).
    pub fn width(&self) -> Length {
        self.max.x - self.min.x
    }
    /// Height (Y extent).
    pub fn height(&self) -> Length {
        self.max.y - self.min.y
    }
    /// Centre.
    pub fn centre(&self) -> Point2 {
        self.min.midpoint(self.max)
    }
    /// The box grown by a margin on every side.
    pub fn expanded(&self, margin: Length) -> Bbox2 {
        Bbox2 {
            min: Point2::from_lengths(self.min.x - margin, self.min.y - margin),
            max: Point2::from_lengths(self.max.x + margin, self.max.y + margin),
        }
    }
    /// Whether the box contains a point (boundary included).
    pub fn contains(&self, p: Point2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    /// Whether two boxes overlap (touching counts).
    pub fn intersects(&self, o: &Bbox2) -> bool {
        self.min.x <= o.max.x && o.min.x <= self.max.x && self.min.y <= o.max.y && o.min.y <= self.max.y
    }
}

/// A 2D affine transform `p' = M·p + t`, with `M = [[a, b], [c, d]]`.
///
/// Used for moving, rotating and mirroring elements and for placing symbols.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transform2 {
    /// Row 0, column 0.
    pub a: f64,
    /// Row 0, column 1.
    pub b: f64,
    /// Row 1, column 0.
    pub c: f64,
    /// Row 1, column 1.
    pub d: f64,
    /// Translation X, metres.
    pub tx: f64,
    /// Translation Y, metres.
    pub ty: f64,
}

impl Default for Transform2 {
    fn default() -> Self {
        Transform2::IDENTITY
    }
}

impl Transform2 {
    /// The identity.
    pub const IDENTITY: Transform2 = Transform2 { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    /// A translation.
    pub fn translation(v: Vec2) -> Self {
        Transform2 { tx: v.x, ty: v.y, ..Transform2::IDENTITY }
    }
    /// A rotation about the origin.
    pub fn rotation(angle: Angle) -> Self {
        let (s, c) = (angle.sin(), angle.cos());
        Transform2 { a: c, b: -s, c: s, d: c, tx: 0.0, ty: 0.0 }
    }
    /// A rotation about a centre.
    pub fn rotation_about(centre: Point2, angle: Angle) -> Self {
        let v = centre.to_vec();
        Transform2::translation(v).then(&Transform2::rotation(angle).then(&Transform2::translation(-v)))
    }
    /// A uniform scale about the origin.
    pub fn scale(k: f64) -> Self {
        Transform2 { a: k, b: 0.0, c: 0.0, d: k, tx: 0.0, ty: 0.0 }
    }
    /// A mirror about the infinite line through `a` and `b`. Returns the identity if `a == b`.
    pub fn mirror(a: Point2, b: Point2) -> Self {
        let Some(u) = (b - a).normalized() else { return Transform2::IDENTITY };
        // Reflection matrix about a line through the origin with direction u: [[2ux²-1, 2uxuy],[2uxuy, 2uy²-1]].
        let r = Transform2 {
            a: 2.0 * u.x * u.x - 1.0,
            b: 2.0 * u.x * u.y,
            c: 2.0 * u.x * u.y,
            d: 2.0 * u.y * u.y - 1.0,
            tx: 0.0,
            ty: 0.0,
        };
        let v = a.to_vec();
        Transform2::translation(v).then(&r.then(&Transform2::translation(-v)))
    }
    /// A placement: rotate about the origin, then move the origin to `at`.
    pub fn placement(at: Point2, rotation: Angle) -> Self {
        Transform2::translation(at.to_vec()).then(&Transform2::rotation(rotation))
    }
    /// Composition `self ∘ other`: applies `other` first, then `self`.
    pub fn then(&self, other: &Transform2) -> Transform2 {
        Transform2 {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            tx: self.a * other.tx + self.b * other.ty + self.tx,
            ty: self.c * other.tx + self.d * other.ty + self.ty,
        }
    }
    /// Applies the transform to a point.
    pub fn apply(&self, p: Point2) -> Point2 {
        let [x, y] = p.xy();
        Point2::new(self.a * x + self.b * y + self.tx, self.c * x + self.d * y + self.ty)
    }
    /// Applies the linear part to a vector.
    pub fn apply_vec(&self, v: Vec2) -> Vec2 {
        Vec2::new(self.a * v.x + self.b * v.y, self.c * v.x + self.d * v.y)
    }
    /// Applies the transform to raw coordinates.
    pub fn apply_xy(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        [self.a * x + self.b * y + self.tx, self.c * x + self.d * y + self.ty]
    }
    /// Determinant of the linear part; negative for mirroring transforms.
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }
    /// Whether the transform mirrors (reverses orientation).
    pub fn is_mirroring(&self) -> bool {
        self.determinant() < 0.0
    }
    /// Rotation angle of the linear part (meaningful for rigid transforms).
    pub fn rotation_angle(&self) -> Angle {
        Angle::rad(self.c.atan2(self.a))
    }
    /// The inverse, or `None` if singular.
    pub fn inverse(&self) -> Option<Transform2> {
        let det = self.determinant();
        if det.abs() < 1e-15 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Transform2 { a, b, c, d, tx: -(a * self.tx + b * self.ty), ty: -(c * self.tx + d * self.ty) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point2, b: Point2) -> bool {
        a.coincides(b, &Tolerance::default())
    }

    #[test]
    fn segment_intersection_and_offset() {
        let tol = Tolerance::default();
        let a = Segment2::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0));
        let b = Segment2::new(Point2::new(2.0, -1.0), Point2::new(2.0, 1.0));
        let (p, t, u) = a.intersection(&b, &tol).unwrap();
        assert!(close(p, Point2::new(2.0, 0.0)));
        assert!((t - 0.5).abs() < 1e-12 && (u - 0.5).abs() < 1e-12);
        let o = a.offset(0.5);
        assert!(close(o.start, Point2::new(0.0, 0.5)));
        let c = Segment2::new(Point2::new(5.0, -1.0), Point2::new(5.0, 1.0));
        assert!(a.intersection(&c, &tol).is_none());
    }

    #[test]
    fn transforms_compose_and_invert() {
        let r = Transform2::rotation_about(Point2::new(1.0, 1.0), Angle::deg(90.0));
        assert!(close(r.apply(Point2::new(2.0, 1.0)), Point2::new(1.0, 2.0)));
        let m = Transform2::mirror(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0));
        assert!(close(m.apply(Point2::new(3.0, 2.0)), Point2::new(-3.0, 2.0)));
        assert!(m.is_mirroring());
        let inv = r.inverse().unwrap();
        assert!(close(inv.apply(r.apply(Point2::new(5.0, -2.0))), Point2::new(5.0, -2.0)));
    }

    #[test]
    fn bulge_arc_passes_through_ends() {
        // A half circle from (0,0) to (2,0), counter-clockwise: bulge 1.
        let arc = Arc2::from_bulge(Point2::new(0.0, 0.0), Point2::new(2.0, 0.0), 1.0).unwrap();
        assert!((arc.radius.as_m() - 1.0).abs() < 1e-9);
        assert!(close(arc.at(0.0), Point2::new(0.0, 0.0)));
        assert!(close(arc.at(1.0), Point2::new(2.0, 0.0)));
        let pts = arc.tessellate(0.001, 4);
        assert!(pts.len() > 4);
    }

    #[test]
    fn point_serialises_as_pair() {
        let p = Point2::new(1.5, -2.0);
        assert_eq!(serde_json::to_string(&p).unwrap(), "[1.5,-2.0]");
        let q: Point2 = serde_json::from_str("[1.5,-2.0]").unwrap();
        assert!(close(p, q));
    }
}

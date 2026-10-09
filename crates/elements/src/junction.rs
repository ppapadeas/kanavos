//! Wall junctions: detection of corners, tees, crossings and multi-wall nodes, and the clipping rules that turn
//! them into clean wall outlines and layer bands.
//!
//! Rules (see `specs/elements/wall-junctions.md`):
//!
//! - Wall ends closer than [`JUNCTION_TOLERANCE`] form a node.
//! - **Corner (L)**, two ends: if the core materials have the same junction priority, every layer runs to the
//!   joint line through the two face intersections (a mitre for equal build-ups; faces extended or trimmed to
//!   meet for different thicknesses). If the priorities differ, the stronger wall runs to the far face of the
//!   other, which then abuts it like a tee.
//! - **Continuation**, two collinear ends: both end square at the node.
//! - **Tee**, an end inside another wall: the abutting wall is trimmed at the through wall's near face; each of
//!   its layers continues into the through wall across the through wall's layers of strictly lower priority, which
//!   are cut where it passes (higher-priority layers run through).
//! - **Node**, three or more ends: a collinear pair runs through and the others abut it; without a collinear
//!   pair, every wall ends at the joint lines shared with its two angular neighbours.
//! - **Cross (X)**, reference lines crossing inside both walls: the wall with the higher core priority (then the
//!   thicker, then the older) runs through; the other is interrupted except for layers that out-rank the through
//!   wall's layers, which continue across them.

use std::collections::{BTreeMap, HashMap};

use kanavos_geom::{Arc2, Bbox2, HalfPlane, Point2, Polygon2, Vec2, circle_circle, line_circle};
use kanavos_model::{Document, Element, ElementKind, Id, Wall};

use crate::region::{Clip, CutShape};
use crate::{GeometryError, base_elevation};

/// Wall ends closer than this (metres) meet; an end within this distance of another wall's band touches it.
pub const JUNCTION_TOLERANCE: f64 = 0.002;
/// Ends whose directions differ from opposite by less than this (radians, 2°) continue each other.
const COLLINEAR: f64 = 0.0349;
/// Ends whose directions differ by less than this (radians, 10°) overlap and are not joined.
const OVERLAP: f64 = 0.1745;
/// Corner points further than this multiple of the thickest wall from the node fall back to a bisector joint.
const MITRE_LIMIT: f64 = 6.0;
/// Chord deviation (metres) used to draw arc walls.
pub(crate) const ARC_DEVIATION: f64 = 0.002;

/// How walls meet at a junction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JunctionKind {
    /// Two walls meeting at an angle (L).
    Corner,
    /// Two walls continuing in a straight line.
    Continuation,
    /// A wall ending against another one (T).
    Tee,
    /// Two walls crossing (X), or two walls ending against a third from opposite sides.
    Cross,
    /// Three or more wall ends meeting without a through wall.
    Node,
}

/// A junction between walls on a storey.
#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    /// Kind.
    pub kind: JunctionKind,
    /// Where the walls meet (on their reference lines).
    pub at: Point2,
    /// The walls involved; for tees and crosses the through wall comes first.
    pub walls: Vec<Id>,
}

/// A junction seen from one wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallJoin {
    /// Kind of junction.
    pub kind: JunctionKind,
    /// The other wall.
    pub other: Id,
    /// Where they meet.
    pub at: Point2,
}

/// End of a wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum End {
    Start,
    End,
}

/// One layer of a wall, with the offsets of its faces from the reference line (left positive).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayerSpec {
    pub thickness: f64,
    pub material: Id,
    pub priority: u8,
    pub core: bool,
    pub d_left: f64,
    pub d_right: f64,
}

/// A face line or circle of a wall.
#[derive(Debug, Clone, Copy)]
enum Curve {
    Line(Point2, Vec2),
    Circle(Point2, f64),
}

/// Everything junction resolution needs to know about a wall.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WallInfo {
    pub id: Id,
    pub wall: Wall,
    pub layers: Vec<LayerSpec>,
    pub left: f64,
    pub right: f64,
    pub thickness: f64,
    pub base: f64,
    pub top: f64,
    pub arc: Option<Arc2>,
    pub len: f64,
    pub core_priority: u8,
    pub bbox: Bbox2,
    dir: Vec2,
    sign: f64,
}

impl WallInfo {
    /// Collects a wall's data. Failure modes: not a wall, missing build-up or storey, zero thickness.
    pub(crate) fn new(doc: &Document, e: &Element) -> Result<WallInfo, GeometryError> {
        let id = e.id;
        let ElementKind::Wall(w) = &e.kind else {
            return Err(GeometryError::WrongKind { id, expected: "wall" });
        };
        let layers = doc
            .structure_layers(&w.structure)
            .ok_or_else(|| GeometryError::MissingReference { id, what: "build-up".into() })?;
        let thickness: f64 = layers.iter().map(|l| l.thickness.as_m()).sum();
        if !(thickness.is_finite() && thickness > doc.tolerance().length.as_m()) {
            return Err(GeometryError::Invalid {
                id: Some(id),
                field: "structure",
                reason: "has no thickness".into(),
            });
        }
        if w.start.coincides(w.end, &doc.tolerance()) {
            return Err(GeometryError::Invalid {
                id: Some(id),
                field: "end",
                reason: "coincides with the start".into(),
            });
        }
        let (left, right) = w.face_offsets(kanavos_units::Length::m(thickness));
        let mut specs = Vec::with_capacity(layers.len());
        let mut d = left;
        for l in &layers {
            let next = d - l.thickness.as_m();
            specs.push(LayerSpec {
                thickness: l.thickness.as_m(),
                material: l.material,
                priority: doc.material(l.material).map(|m| m.priority).unwrap_or(0),
                core: l.core,
                d_left: d,
                d_right: next,
            });
            d = next;
        }
        let core_priority = doc
            .structure_core_material(&w.structure)
            .and_then(|m| doc.material(m))
            .map(|m| m.priority)
            .unwrap_or_else(|| specs.iter().map(|l| l.priority).max().unwrap_or(0));
        let base = base_elevation(doc, e)?.as_m();
        let arc = w.arc();
        let len = w.length().as_m();
        let dir = (w.end - w.start).normalized().unwrap_or(Vec2::X);
        let sign = arc.map(|a| if a.sweep.as_rad() > 0.0 { 1.0 } else { -1.0 }).unwrap_or(1.0);
        let mut info = WallInfo {
            id,
            wall: w.clone(),
            layers: specs,
            left,
            right,
            thickness,
            base,
            top: base + w.height.as_m(),
            arc,
            len,
            core_priority,
            bbox: Bbox2 { min: w.start, max: w.start },
            dir,
            sign,
        };
        let outline = info.band(left, right, 0.0, len);
        info.bbox = outline.bbox().unwrap_or(info.bbox);
        Ok(info)
    }

    pub(crate) fn is_arc(&self) -> bool {
        self.arc.is_some()
    }

    fn angle_at(&self, arc: &Arc2, u: f64) -> f64 {
        arc.start.as_rad() + self.sign * u / arc.radius.as_m().max(1e-9)
    }

    /// The point at arc length `u` from the start along the reference line, `d` to its left.
    pub(crate) fn point(&self, u: f64, d: f64) -> Point2 {
        match &self.arc {
            Some(a) => {
                let th = self.angle_at(a, u);
                let r = a.radius.as_m() - self.sign * d;
                a.centre + Vec2::new(th.cos(), th.sin()) * r
            }
            None => self.wall.start + self.dir * u + self.dir.perp() * d,
        }
    }

    /// Unit direction of travel at `u`.
    pub(crate) fn tangent(&self, u: f64) -> Vec2 {
        match &self.arc {
            Some(a) => {
                let th = self.angle_at(a, u);
                Vec2::new(-th.sin(), th.cos()) * self.sign
            }
            None => self.dir,
        }
    }

    /// Parameter along the reference line and signed offset (left positive) of a point.
    pub(crate) fn project(&self, p: Point2) -> (f64, f64) {
        match &self.arc {
            Some(a) => {
                let v = p - a.centre;
                let r = v.length();
                let th = v.y.atan2(v.x);
                let sweep = a.sweep.as_rad();
                let mid = a.start.as_rad() + sweep / 2.0;
                let delta = kanavos_units::Angle::rad(th - mid).normalized_signed().as_rad();
                let u = a.radius.as_m() * (sweep.abs() / 2.0 + self.sign * delta);
                (u, self.sign * (a.radius.as_m() - r))
            }
            None => {
                let v = p - self.wall.start;
                (v.dot(self.dir), self.dir.cross(v))
            }
        }
    }

    /// Whether a point lies in the wall's band (between its faces and within its length), with tolerance.
    pub(crate) fn contains(&self, p: Point2, tol: f64) -> bool {
        let (u, d) = self.project(p);
        u >= -tol && u <= self.len + tol && d <= self.left + tol && d >= self.right - tol
    }

    /// Mid offset of the thickness.
    pub(crate) fn mid(&self) -> f64 {
        (self.left + self.right) / 2.0
    }

    /// A point in the middle of the wall.
    pub(crate) fn body_point(&self) -> Point2 {
        self.point(self.len / 2.0, self.mid())
    }

    pub(crate) fn end_u(&self, e: End) -> f64 {
        match e {
            End::Start => 0.0,
            End::End => self.len,
        }
    }

    pub(crate) fn end_point(&self, e: End) -> Point2 {
        match e {
            End::Start => self.wall.start,
            End::End => self.wall.end,
        }
    }

    /// Direction from the end into the wall.
    pub(crate) fn outgoing(&self, e: End) -> Vec2 {
        match e {
            End::Start => self.tangent(0.0),
            End::End => -self.tangent(self.len),
        }
    }

    /// Offset of the face on the left of the outgoing direction at an end.
    fn face_left_of_out(&self, e: End) -> f64 {
        match e {
            End::Start => self.left,
            End::End => self.right,
        }
    }

    fn face_right_of_out(&self, e: End) -> f64 {
        match e {
            End::Start => self.right,
            End::End => self.left,
        }
    }

    fn curve(&self, d: f64) -> Curve {
        match &self.arc {
            Some(a) => Curve::Circle(a.centre, a.radius.as_m() - self.sign * d),
            None => Curve::Line(self.wall.start + self.dir.perp() * d, self.dir),
        }
    }

    /// Keeps the side of the curve at offset `d` with larger offsets (`keep_left`) or smaller ones.
    pub(crate) fn offset_clip(&self, d: f64, keep_left: bool) -> Option<Clip> {
        match &self.arc {
            Some(a) => Some(Clip::Disc {
                centre: a.centre,
                radius: a.radius.as_m() - self.sign * d,
                inside: keep_left == (self.sign > 0.0),
            }),
            None => {
                let n = self.dir.perp();
                HalfPlane::new(self.wall.start + n * d, if keep_left { n } else { -n }).map(Clip::Half)
            }
        }
    }

    /// Keeps the part of the wall beyond (`forward`) or before parameter `u`, cut square to the reference line.
    pub(crate) fn across_clip(&self, u: f64, forward: bool) -> Option<Clip> {
        let t = if forward { self.tangent(u) } else { -self.tangent(u) };
        // For arcs the boundary is the radial line through the centre and the point at `u`.
        let through = match &self.arc {
            Some(a) => a.centre,
            None => self.point(u, 0.0),
        };
        HalfPlane::new(through, t).map(Clip::Half)
    }

    /// The band between offsets `d_left > d_right` from `u0` to `u1`, counter-clockwise.
    pub(crate) fn band(&self, d_left: f64, d_right: f64, u0: f64, u1: f64) -> Polygon2 {
        match &self.arc {
            Some(a) => {
                let r = a.radius.as_m() + self.left.abs().max(self.right.abs());
                let span = (u1 - u0).abs() / a.radius.as_m().max(1e-9);
                let step = 2.0 * (1.0 - ARC_DEVIATION / r.max(ARC_DEVIATION * 2.0)).clamp(-1.0, 1.0).acos();
                let n = ((span / step.max(1e-4)).ceil() as usize).clamp(2, 2048);
                let mut pts = Vec::with_capacity(2 * n + 2);
                for i in 0..=n {
                    pts.push(self.point(u0 + (u1 - u0) * i as f64 / n as f64, d_right));
                }
                for i in (0..=n).rev() {
                    pts.push(self.point(u0 + (u1 - u0) * i as f64 / n as f64, d_left));
                }
                Polygon2::new(pts).to_ccw()
            }
            None => Polygon2::new(vec![
                self.point(u0, d_right),
                self.point(u1, d_right),
                self.point(u1, d_left),
                self.point(u0, d_left),
            ])
            .to_ccw(),
        }
    }

    /// Points of the curve at offset `d` from `u0` to `u1`.
    pub(crate) fn offset_points(&self, d: f64, u0: f64, u1: f64) -> Vec<Point2> {
        match &self.arc {
            Some(a) => {
                let r = (a.radius.as_m() - self.sign * d).abs().max(ARC_DEVIATION * 2.0);
                let span = (u1 - u0).abs() / a.radius.as_m().max(1e-9);
                let step = 2.0 * (1.0 - ARC_DEVIATION / r).clamp(-1.0, 1.0).acos();
                let n = ((span / step.max(1e-4)).ceil() as usize).clamp(1, 2048);
                (0..=n).map(|i| self.point(u0 + (u1 - u0) * i as f64 / n as f64, d)).collect()
            }
            None => vec![self.point(u0, d), self.point(u1, d)],
        }
    }

    /// Points where the wall's reference line meets another wall's reference line.
    fn reference_crossings(&self, other: &WallInfo) -> Vec<Point2> {
        match (&self.arc, &other.arc) {
            (None, None) => {
                let a = self.wall.segment();
                let b = other.wall.segment();
                let tol = kanavos_units::Tolerance::default();
                match kanavos_geom::segment_intersection(&a, &b, &tol) {
                    kanavos_geom::SegmentIntersection::Point { point, .. } => vec![point],
                    _ => Vec::new(),
                }
            }
            (Some(a), None) => line_circle(other.wall.start, other.dir, a.centre, a.radius.as_m())
                .into_iter()
                .map(|s| other.wall.start + other.dir * s)
                .collect(),
            (None, Some(b)) => line_circle(self.wall.start, self.dir, b.centre, b.radius.as_m())
                .into_iter()
                .map(|s| self.wall.start + self.dir * s)
                .collect(),
            (Some(a), Some(b)) => circle_circle(a.centre, a.radius.as_m(), b.centre, b.radius.as_m()),
        }
    }
}

fn intersect_curves(a: Curve, b: Curve, near: Point2) -> Option<Point2> {
    let pts: Vec<Point2> = match (a, b) {
        (Curve::Line(p, d), Curve::Line(q, e)) => {
            let den = d.cross(e);
            if den.abs() < 1e-9 {
                return None;
            }
            let t = (q - p).cross(e) / den;
            vec![p + d * t]
        }
        (Curve::Line(p, d), Curve::Circle(c, r)) | (Curve::Circle(c, r), Curve::Line(p, d)) => {
            line_circle(p, d, c, r).into_iter().map(|s| p + d * s).collect()
        }
        (Curve::Circle(c0, r0), Curve::Circle(c1, r1)) => circle_circle(c0, r0, c1, r1),
    };
    pts.into_iter().min_by(|x, y| x.distance(near).total_cmp(&y.distance(near)))
}

/// What happens at one end of a wall.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EndRule {
    /// How far the bands are extended beyond the end before clipping (metres along the reference line).
    pub extension: f64,
    /// Clips applied to the outline, the faces and every layer.
    pub all: Vec<Clip>,
    /// Abutting: the clip for each layer (to the depth it reaches into the other wall).
    pub per_layer: Vec<Option<Clip>>,
    /// Abutting: the clip for the outline and faces (the other wall's near face).
    pub outline: Option<Clip>,
}

/// All junction rules of one wall.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct WallRules {
    pub start: EndRule,
    pub end: EndRule,
    /// Crossing walls: per layer, the two sides kept.
    pub splits: Vec<(usize, [Clip; 2])>,
    /// Layers of other walls passing through: per layer of this wall, the region removed.
    pub cuts: Vec<(usize, CutShape)>,
    pub joins: Vec<WallJoin>,
}

impl WallRules {
    fn end_mut(&mut self, e: End) -> &mut EndRule {
        match e {
            End::Start => &mut self.start,
            End::End => &mut self.end,
        }
    }
}

/// Union-find over end indices.
struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, i: usize) -> usize {
        let mut r = i;
        while let Some(&p) = self.0.get(r) {
            if p == r {
                break;
            }
            r = p;
        }
        // Path compression.
        let mut k = i;
        while let Some(&p) = self.0.get(k) {
            if p == r {
                break;
            }
            if let Some(slot) = self.0.get_mut(k) {
                *slot = r;
            }
            k = p;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb
            && let Some(slot) = self.0.get_mut(ra.max(rb))
        {
            *slot = ra.min(rb);
        }
    }
}

/// Candidate pairs of walls whose (grown) boxes overlap, through a uniform grid.
fn candidate_pairs(walls: &[WallInfo], grow: f64) -> Vec<(usize, usize)> {
    let cell = 4.0;
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let key = |x: f64| (x / cell).floor() as i64;
    for (i, w) in walls.iter().enumerate() {
        let b = w.bbox.expanded(kanavos_units::Length::m(grow));
        for gx in key(b.min.x.as_m())..=key(b.max.x.as_m()) {
            for gy in key(b.min.y.as_m())..=key(b.max.y.as_m()) {
                grid.entry((gx, gy)).or_default().push(i);
            }
        }
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for list in grid.values() {
        for (k, &i) in list.iter().enumerate() {
            for &j in list.iter().skip(k + 1) {
                pairs.push((i.min(j), i.max(j)));
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs.retain(|&(i, j)| {
        let (Some(a), Some(b)) = (walls.get(i), walls.get(j)) else { return false };
        let g = kanavos_units::Length::m(grow);
        a.bbox.expanded(g).intersects(&b.bbox.expanded(g))
            && a.base < b.top - JUNCTION_TOLERANCE
            && b.base < a.top - JUNCTION_TOLERANCE
    });
    pairs
}

/// Ranking for which wall runs through: higher core priority, then thicker, then the older (lower id).
fn stronger(a: &WallInfo, b: &WallInfo) -> bool {
    (a.core_priority, ordered(a.thickness), std::cmp::Reverse(a.id))
        > (b.core_priority, ordered(b.thickness), std::cmp::Reverse(b.id))
}

/// Thickness rounded to 0.1 mm for comparisons.
fn ordered(t: f64) -> i64 {
    (t * 10_000.0).round() as i64
}

struct Resolver<'a> {
    walls: &'a [WallInfo],
    rules: Vec<WallRules>,
    junctions: Vec<Junction>,
}

impl<'a> Resolver<'a> {
    fn wall(&self, i: usize) -> Option<&'a WallInfo> {
        self.walls.get(i)
    }

    fn rules(&mut self, i: usize) -> Option<&mut WallRules> {
        self.rules.get_mut(i)
    }

    fn join(&mut self, a: usize, b: usize, kind: JunctionKind, at: Point2) {
        let (Some(wa), Some(wb)) = (self.wall(a), self.wall(b)) else { return };
        let (ia, ib) = (wa.id, wb.id);
        if let Some(r) = self.rules(a) {
            r.joins.push(WallJoin { kind, other: ib, at });
        }
        if let Some(r) = self.rules(b) {
            r.joins.push(WallJoin { kind, other: ia, at });
        }
    }

    /// The cut a layer of `a` makes in another wall, near `at` (parameter `u` on `a`).
    fn layer_cut(&self, a: &WallInfo, k: usize, u: f64, onto: &WallInfo) -> Option<CutShape> {
        let l = a.layers.get(k)?;
        if !a.is_arc() && !onto.is_arc() {
            let Some(Clip::Half(h1)) = a.offset_clip(l.d_left, false) else { return None };
            let Some(Clip::Half(h2)) = a.offset_clip(l.d_right, true) else { return None };
            return Some(CutShape::Strip(h1, h2));
        }
        let reach = onto.thickness * 2.0 + a.thickness + 0.5;
        Some(CutShape::Region(a.band(l.d_left, l.d_right, u - reach, u + reach)))
    }

    /// Layers of `b` from its face on side `s` (+1 left, −1 right), with their indices.
    fn layers_from(b: &WallInfo, s: f64) -> Vec<(usize, &LayerSpec)> {
        let mut v: Vec<(usize, &LayerSpec)> = b.layers.iter().enumerate().collect();
        if s < 0.0 {
            v.reverse();
        }
        v
    }

    /// Depth to which a layer of priority `p` runs into `b` from side `s`, and the layers it crosses.
    fn penetration(b: &WallInfo, s: f64, p: u8) -> (f64, Vec<usize>) {
        let mut depth = 0.0;
        let mut crossed = Vec::new();
        for (j, l) in Self::layers_from(b, s) {
            if l.priority < p {
                depth += l.thickness;
                crossed.push(j);
            } else {
                break;
            }
        }
        (depth, crossed)
    }

    /// End `e` of wall `a` abuts wall `b` (and the cuts also apply to `also`, the other half of a through pair).
    fn abut(&mut self, a: usize, e: End, b: usize, also: Option<usize>, at: Point2) {
        let (Some(wa), Some(wb)) = (self.wall(a), self.wall(b)) else { return };
        let (_, db) = wb.project(wa.body_point());
        let s = if db > wb.mid() { 1.0 } else { -1.0 };
        let near = if s > 0.0 { wb.left } else { wb.right };
        let u_end = wa.end_u(e);
        // Extension: enough to cross b entirely at the meeting angle.
        let (ub, _) = wb.project(at);
        let sin = wa.outgoing(e).cross(wb.tangent(ub)).abs().max(0.17);
        let extension = (wb.thickness + wa.thickness) / sin + 0.05;
        let mut per_layer = Vec::with_capacity(wa.layers.len());
        let mut cuts: Vec<(usize, CutShape)> = Vec::new();
        for (k, l) in wa.layers.iter().enumerate() {
            let (depth, crossed) = Self::penetration(wb, s, l.priority);
            per_layer.push(wb.offset_clip(near - s * depth, s > 0.0));
            if !crossed.is_empty()
                && let Some(shape) = self.layer_cut(wa, k, u_end, wb)
            {
                cuts.extend(crossed.into_iter().map(|j| (j, shape.clone())));
            }
        }
        let outline = wb.offset_clip(near, s > 0.0);
        if let Some(r) = self.rules(a) {
            let end = r.end_mut(e);
            end.extension = end.extension.max(extension);
            end.per_layer = per_layer;
            end.outline = outline;
        }
        let also_layers = also.and_then(|i| self.wall(i)).map(|w| w.layers.len());
        if let Some(r) = self.rules(b) {
            r.cuts.extend(cuts.iter().cloned());
        }
        if let (Some(i), Some(n)) = (also, also_layers)
            && let Some(r) = self.rules(i)
        {
            r.cuts.extend(cuts.into_iter().filter(|(j, _)| *j < n));
        }
    }

    /// Both ends end square at the node (a continuation, or the through pair of a node).
    fn square_ends(&mut self, ends: &[(usize, End)], node: Point2) {
        for &(i, e) in ends {
            let Some(w) = self.wall(i) else { continue };
            let (u, _) = w.project(node);
            let u = u.clamp(-JUNCTION_TOLERANCE * 10.0, w.len + JUNCTION_TOLERANCE * 10.0);
            let clip = w.across_clip(u, e == End::Start);
            if let Some(r) = self.rules(i) {
                let end = r.end_mut(e);
                end.extension = end.extension.max(JUNCTION_TOLERANCE * 10.0);
                end.all.extend(clip);
            }
        }
    }

    /// Clip keeping wall `i`'s body side of the line through `p` and `q`.
    fn line_clip(&self, i: usize, p: Point2, q: Point2) -> Option<Clip> {
        let w = self.wall(i)?;
        let hp = HalfPlane::left_of(p, q)?;
        let hp = if hp.signed_distance(w.body_point()) >= 0.0 { hp } else { hp.flipped() };
        Some(Clip::Half(hp))
    }

    fn corner(&mut self, a: (usize, End), b: (usize, End), node: Point2) {
        let (Some(wa), Some(wb)) = (self.wall(a.0), self.wall(b.0)) else { return };
        let (oa, ob) = (wa.outgoing(a.1), wb.outgoing(b.1));
        let cos = oa.dot(ob);
        if cos > OVERLAP.cos() {
            return; // Overlapping walls: leave them alone.
        }
        if cos < -COLLINEAR.cos() {
            self.square_ends(&[a, b], node);
            self.join(a.0, b.0, JunctionKind::Continuation, node);
            self.junctions.push(Junction {
                kind: JunctionKind::Continuation,
                at: node,
                walls: vec![wa.id, wb.id],
            });
            return;
        }
        if wa.core_priority != wb.core_priority {
            let (h, l) = if wa.core_priority > wb.core_priority { (a, b) } else { (b, a) };
            self.run_to_far_face(h, l.0, node);
            self.abut(l.0, l.1, h.0, None, node);
            self.join(a.0, b.0, JunctionKind::Corner, node);
            let ids = [h.0, l.0].iter().filter_map(|&i| self.wall(i).map(|w| w.id)).collect();
            self.junctions.push(Junction { kind: JunctionKind::Corner, at: node, walls: ids });
            return;
        }
        // Order counter-clockwise: x then y.
        let (x, y) = if oa.cross(ob) > 0.0 { (a, b) } else { (b, a) };
        let (Some(wx), Some(wy)) = (self.wall(x.0), self.wall(y.0)) else { return };
        let limit = MITRE_LIMIT * wx.thickness.max(wy.thickness);
        let p1 =
            intersect_curves(wx.curve(wx.face_left_of_out(x.1)), wy.curve(wy.face_right_of_out(y.1)), node);
        let p2 =
            intersect_curves(wy.curve(wy.face_left_of_out(y.1)), wx.curve(wx.face_right_of_out(x.1)), node);
        let (p, q) = match (p1, p2) {
            (Some(p), Some(q)) if p.distance(node).as_m() <= limit && q.distance(node).as_m() <= limit => {
                (p, q)
            }
            _ => {
                let bis =
                    (wx.outgoing(x.1) + wy.outgoing(y.1)).normalized().unwrap_or(wx.outgoing(x.1).perp());
                (node, node + bis)
            }
        };
        let reach = p.distance(node).as_m().max(q.distance(node).as_m()) + wx.thickness.max(wy.thickness);
        for end in [x, y] {
            let clip = self.line_clip(end.0, p, q);
            if let Some(r) = self.rules(end.0) {
                let er = r.end_mut(end.1);
                er.extension = er.extension.max(reach);
                er.all.extend(clip);
            }
        }
        self.join(a.0, b.0, JunctionKind::Corner, node);
        self.junctions.push(Junction { kind: JunctionKind::Corner, at: node, walls: vec![wa.id, wb.id] });
    }

    /// End `h` runs to the face of wall `l` away from `h`'s body.
    fn run_to_far_face(&mut self, h: (usize, End), l: usize, node: Point2) {
        let (Some(wh), Some(wl)) = (self.wall(h.0), self.wall(l)) else { return };
        let (_, dh) = wl.project(wh.body_point());
        let s = if dh > wl.mid() { 1.0 } else { -1.0 };
        let far = if s > 0.0 { wl.right } else { wl.left };
        let clip = wl.offset_clip(far, s > 0.0);
        let (ul, _) = wl.project(node);
        let sin = wh.outgoing(h.1).cross(wl.tangent(ul)).abs().max(0.17);
        let extension = (wl.thickness + wh.thickness) / sin + 0.05;
        if let Some(r) = self.rules(h.0) {
            let er = r.end_mut(h.1);
            er.extension = er.extension.max(extension);
            er.all.extend(clip);
        }
    }

    fn node(&mut self, ends: &[(usize, End)], node: Point2) {
        // The two strongest walls decide: if they continue each other they run through; if they are clearly
        // stronger than the rest and meet at an angle, they form a corner and the others abut them.
        let strength = |e: &(usize, End)| {
            self.wall(e.0)
                .map(|w| (w.core_priority, ordered(w.thickness), std::cmp::Reverse(w.id)))
                .unwrap_or((0, 0, std::cmp::Reverse(Id(u64::MAX))))
        };
        let mut ranked: Vec<(usize, End)> = ends.to_vec();
        ranked.sort_by_key(|e| std::cmp::Reverse(strength(e)));
        if let (Some(&a), Some(&b), Some(&c)) = (ranked.first(), ranked.get(1), ranked.get(2)) {
            let (Some(wa), Some(wb)) = (self.wall(a.0), self.wall(b.0)) else { return };
            let collinear = wa.outgoing(a.1).dot(wb.outgoing(b.1)) < -COLLINEAR.cos();
            let distinct = (strength(&b).0, strength(&b).1) > (strength(&c).0, strength(&c).1);
            if !collinear && distinct {
                self.corner(a, b, node);
                let mut ids = vec![wa.id, wb.id];
                for &(i, e) in ranked.iter().skip(2) {
                    let Some(w) = self.wall(i) else { continue };
                    let o = w.outgoing(e);
                    let across = |m: &WallInfo| {
                        let (u, _) = m.project(node);
                        o.cross(m.tangent(u)).abs()
                    };
                    let target = if across(wa) >= across(wb) { a.0 } else { b.0 };
                    self.abut(i, e, target, None, node);
                    self.join(i, target, JunctionKind::Tee, node);
                    ids.push(w.id);
                }
                self.junctions.push(Junction { kind: JunctionKind::Node, at: node, walls: ids });
                return;
            }
        }
        // Collinear pairs.
        let mut best: Option<(usize, usize)> = None;
        for (x, &ea) in ends.iter().enumerate() {
            for (y, &eb) in ends.iter().enumerate().skip(x + 1) {
                let (Some(wa), Some(wb)) = (self.wall(ea.0), self.wall(eb.0)) else { continue };
                if wa.outgoing(ea.1).dot(wb.outgoing(eb.1)) < -COLLINEAR.cos() {
                    let better = match best {
                        None => true,
                        Some((bx, by)) => {
                            let score = |i: usize, j: usize| {
                                let (Some(p), Some(q)) = (
                                    ends.get(i).and_then(|e| self.wall(e.0)),
                                    ends.get(j).and_then(|e| self.wall(e.0)),
                                ) else {
                                    return (0u8, 0i64, std::cmp::Reverse(Id(u64::MAX)));
                                };
                                (
                                    p.core_priority.max(q.core_priority),
                                    ordered(p.thickness.max(q.thickness)),
                                    std::cmp::Reverse(p.id.min(q.id)),
                                )
                            };
                            score(x, y) > score(bx, by)
                        }
                    };
                    if better {
                        best = Some((x, y));
                    }
                }
            }
        }
        if let Some((x, y)) = best {
            let (Some(&ex), Some(&ey)) = (ends.get(x), ends.get(y)) else { return };
            self.square_ends(&[ex, ey], node);
            self.join(ex.0, ey.0, JunctionKind::Continuation, node);
            let (Some(wx), Some(wy)) = (self.wall(ex.0), self.wall(ey.0)) else { return };
            let (primary, secondary) = if stronger(wy, wx) { (ey.0, ex.0) } else { (ex.0, ey.0) };
            let kind = if ends.len() == 3 { JunctionKind::Tee } else { JunctionKind::Cross };
            let mut ids = vec![wx.id, wy.id];
            for (k, &(i, e)) in ends.iter().enumerate() {
                if k == x || k == y {
                    continue;
                }
                self.abut(i, e, primary, Some(secondary), node);
                self.join(i, primary, kind, node);
                if let Some(w) = self.wall(i) {
                    ids.push(w.id);
                }
            }
            self.junctions.push(Junction { kind, at: node, walls: ids });
            return;
        }
        // Pinwheel: each wall between the joints with its angular neighbours.
        let mut order: Vec<(f64, usize, End)> = ends
            .iter()
            .filter_map(|&(i, e)| self.wall(i).map(|w| (w.outgoing(e).angle().as_rad(), i, e)))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n = order.len();
        let mut corners: Vec<Point2> = Vec::with_capacity(n);
        for k in 0..n {
            let (Some(&(_, i, ei)), Some(&(_, j, ej))) = (order.get(k), order.get((k + 1) % n)) else {
                continue;
            };
            let (Some(wi), Some(wj)) = (self.wall(i), self.wall(j)) else { continue };
            let limit = MITRE_LIMIT * wi.thickness.max(wj.thickness);
            let p =
                intersect_curves(wi.curve(wi.face_left_of_out(ei)), wj.curve(wj.face_right_of_out(ej)), node)
                    .filter(|p| p.distance(node).as_m() <= limit && p.distance(node).as_m() > 1e-9);
            let fallback = || {
                let bis = (wi.outgoing(ei) + wj.outgoing(ej)).normalized().unwrap_or(wi.outgoing(ei).perp());
                node + bis * wi.thickness.max(wj.thickness)
            };
            corners.push(p.unwrap_or_else(fallback));
        }
        let mut ids = Vec::with_capacity(n);
        for k in 0..n {
            let (Some(&(_, i, e)), Some(&p_next), Some(&p_prev)) =
                (order.get(k), corners.get(k), corners.get((k + n - 1) % n))
            else {
                continue;
            };
            let clips: Vec<Clip> = [self.line_clip(i, node, p_next), self.line_clip(i, node, p_prev)]
                .into_iter()
                .flatten()
                .collect();
            let reach = p_next.distance(node).as_m().max(p_prev.distance(node).as_m())
                + self.wall(i).map(|w| w.thickness).unwrap_or(0.0);
            if let Some(r) = self.rules(i) {
                let er = r.end_mut(e);
                er.extension = er.extension.max(reach);
                er.all.extend(clips);
            }
            if let Some(w) = self.wall(i) {
                ids.push(w.id);
            }
            for &(_, j, _) in order.iter().skip(k + 1) {
                self.join(i, j, JunctionKind::Node, node);
            }
        }
        self.junctions.push(Junction { kind: JunctionKind::Node, at: node, walls: ids });
    }

    /// Wall `a` crosses wall `b` at `x`, `b` running through.
    fn cross(&mut self, a: usize, b: usize, x: Point2) {
        let (Some(wa), Some(wb)) = (self.wall(a), self.wall(b)) else { return };
        let (ua, _) = wa.project(x);
        let mut splits = Vec::new();
        let mut cuts: Vec<(usize, CutShape)> = Vec::new();
        for (k, l) in wa.layers.iter().enumerate() {
            let (d1, c1) = Self::penetration(wb, 1.0, l.priority);
            let (d2, c2) = Self::penetration(wb, -1.0, l.priority);
            let shape = self.layer_cut(wa, k, ua, wb);
            if d1 + d2 >= wb.thickness - 1e-9 {
                if let Some(s) = shape {
                    cuts.extend((0..wb.layers.len()).map(|j| (j, s.clone())));
                }
                continue;
            }
            if let (Some(left), Some(right)) =
                (wb.offset_clip(wb.left - d1, true), wb.offset_clip(wb.right + d2, false))
            {
                splits.push((k, [left, right]));
            }
            if let Some(s) = shape {
                cuts.extend(c1.into_iter().chain(c2).map(|j| (j, s.clone())));
            }
        }
        if let Some(r) = self.rules(a) {
            r.splits.extend(splits);
        }
        if let Some(r) = self.rules(b) {
            r.cuts.extend(cuts);
        }
        self.join(b, a, JunctionKind::Cross, x);
        let ids = vec![wb.id, wa.id];
        self.junctions.push(Junction { kind: JunctionKind::Cross, at: x, walls: ids });
    }
}

/// Resolves every junction among `walls` (all on one storey).
pub(crate) fn resolve(walls: &[WallInfo]) -> (Vec<WallRules>, Vec<Junction>) {
    let tol = JUNCTION_TOLERANCE;
    let mut r = Resolver { walls, rules: vec![WallRules::default(); walls.len()], junctions: Vec::new() };
    let pairs = candidate_pairs(walls, tol);
    // Ends and their clusters.
    let ends: Vec<(usize, End, Point2)> = walls
        .iter()
        .enumerate()
        .flat_map(|(i, w)| [(i, End::Start, w.end_point(End::Start)), (i, End::End, w.end_point(End::End))])
        .collect();
    let mut dsu = Dsu((0..ends.len()).collect());
    for &(i, j) in &pairs {
        for ei in [2 * i, 2 * i + 1] {
            for ej in [2 * j, 2 * j + 1] {
                if let (Some(a), Some(b)) = (ends.get(ei), ends.get(ej))
                    && a.2.distance(b.2).as_m() <= tol
                {
                    dsu.union(ei, ej);
                }
            }
        }
    }
    let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for k in 0..ends.len() {
        let root = dsu.find(k);
        clusters.entry(root).or_default().push(k);
    }
    // Neighbours of each wall, for the through-wall test.
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); walls.len()];
    for &(i, j) in &pairs {
        if let Some(v) = neighbours.get_mut(i) {
            v.push(j);
        }
        if let Some(v) = neighbours.get_mut(j) {
            v.push(i);
        }
    }
    for members in clusters.values() {
        let list: Vec<(usize, End)> =
            members.iter().filter_map(|&k| ends.get(k).map(|e| (e.0, e.1))).collect();
        // A wall with both ends here is degenerate; skip it.
        let mut walls_here: Vec<usize> = list.iter().map(|e| e.0).collect();
        walls_here.sort_unstable();
        let unique = walls_here.windows(2).all(|w| matches!(w, [a, b] if a != b));
        if !unique {
            continue;
        }
        let node = {
            let pts: Vec<Point2> = members.iter().filter_map(|&k| ends.get(k).map(|e| e.2)).collect();
            let n = pts.len().max(1) as f64;
            let sum = pts.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec());
            Point2::new(sum.x / n, sum.y / n)
        };
        // Walls running through the node (the node is inside their band, not at one of their ends here).
        let mut through: Vec<usize> = Vec::new();
        for &(i, _) in &list {
            for &j in neighbours.get(i).map(Vec::as_slice).unwrap_or(&[]) {
                if walls_here.contains(&j) || through.contains(&j) {
                    continue;
                }
                if walls.get(j).is_some_and(|w| w.contains(node, tol)) {
                    through.push(j);
                }
            }
        }
        if let Some(&first) = through.first() {
            let b = through.iter().copied().fold(first, |best, j| match (walls.get(j), walls.get(best)) {
                (Some(wj), Some(wb)) if stronger(wj, wb) => j,
                _ => best,
            });
            let Some(wb) = walls.get(b) else { continue };
            let mut sides = Vec::new();
            let mut ids = vec![wb.id];
            for &(i, e) in &list {
                let Some(wa) = walls.get(i) else { continue };
                let (ub, _) = wb.project(node);
                if wa.outgoing(e).cross(wb.tangent(ub)).abs() < OVERLAP.sin() {
                    continue; // Running along the through wall: overlapping, not joined.
                }
                r.abut(i, e, b, None, node);
                let (_, d) = wb.project(wa.body_point());
                sides.push(d > wb.mid());
                ids.push(wa.id);
            }
            if ids.len() > 1 {
                let both = sides.iter().any(|s| *s) && sides.iter().any(|s| !*s);
                let kind = if both { JunctionKind::Cross } else { JunctionKind::Tee };
                for &(i, _) in &list {
                    r.join(i, b, kind, node);
                }
                r.junctions.push(Junction { kind, at: node, walls: ids });
            }
            continue;
        }
        match list.as_slice() {
            [_] => {}
            [a, b] => r.corner(*a, *b, node),
            many => {
                let v = many.to_vec();
                r.node(&v, node);
            }
        }
    }
    // Crossings of reference lines inside both walls.
    for &(i, j) in &pairs {
        let (Some(wi), Some(wj)) = (walls.get(i), walls.get(j)) else { continue };
        for x in wi.reference_crossings(wj) {
            let (ui, _) = wi.project(x);
            let (uj, _) = wj.project(x);
            let inside = |u: f64, len: f64| u > tol && u < len - tol;
            if !(inside(ui, wi.len) && inside(uj, wj.len)) {
                continue;
            }
            if stronger(wi, wj) {
                r.cross(j, i, x);
            } else {
                r.cross(i, j, x);
            }
        }
    }
    let mut rules = r.rules;
    for rule in &mut rules {
        rule.canonicalize();
    }
    (rules, r.junctions)
}

fn clip_key(c: &Clip) -> [f64; 4] {
    match c {
        Clip::Half(h) => [h.point.x.as_m(), h.point.y.as_m(), h.normal.x, h.normal.y],
        Clip::Disc { centre, radius, inside } => {
            [centre.x.as_m(), centre.y.as_m(), *radius, if *inside { 1.0 } else { 0.0 }]
        }
    }
}

fn cut_key(c: &CutShape) -> [f64; 4] {
    match c {
        CutShape::Strip(a, b) => [a.point.x.as_m(), a.point.y.as_m(), b.point.x.as_m(), b.point.y.as_m()],
        CutShape::Region(p) => {
            let first = p.vertices.first().map(|q| q.xy()).unwrap_or([0.0, 0.0]);
            [first[0], first[1], p.area().as_m2(), p.len() as f64]
        }
    }
}

fn cmp_keys(a: &[f64; 4], b: &[f64; 4]) -> std::cmp::Ordering {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x.total_cmp(y))
        .find(|o| o.is_ne())
        .unwrap_or(std::cmp::Ordering::Equal)
}

impl WallRules {
    /// Puts cuts, splits and joins in an order that depends only on their geometry, so a wall's layout is the
    /// same whatever order its junctions were found in.
    pub(crate) fn canonicalize(&mut self) {
        self.cuts.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| cmp_keys(&cut_key(&a.1), &cut_key(&b.1))));
        self.cuts.dedup();
        self.splits
            .sort_by(|a, b| a.0.cmp(&b.0).then_with(|| cmp_keys(&clip_key(&a.1[0]), &clip_key(&b.1[0]))));
        self.joins.sort_by(|a, b| {
            a.other
                .cmp(&b.other)
                .then_with(|| a.at.x.as_m().total_cmp(&b.at.x.as_m()))
                .then_with(|| a.at.y.as_m().total_cmp(&b.at.y.as_m()))
        });
        self.joins.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsu_groups() {
        let mut d = Dsu((0..5).collect());
        d.union(0, 3);
        d.union(3, 4);
        assert_eq!(d.find(4), d.find(0));
        assert_ne!(d.find(1), d.find(0));
    }
}

//! Snapping and inference (docs/ux.md §3, `specs/tools/snapping.md`).
//!
//! A [`Snapper`] collects the snap geometry of one storey (points, edges, wall faces, centrelines and layer
//! boundaries, grid lines and underlays) into a uniform-grid spatial index, so a query only looks at the few
//! cells around the pointer. Build it once per document version and storey ([`Snapper::cached`] does that and
//! shares the index between tools), then query it on every pointer move with [`Snapper::snap_query`].
//!
//! Every snap carries a named, localised label ("άκρο τοίχου 12", "κάθετη στον τοίχο 12", "προέκταση") and the
//! guide lines that explain it.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::rc::Rc;

use kanavos_drawing::Shape;
use kanavos_geom::{Angle, Arc2, Bbox2, Line2, Point2, PolygonWithHoles, Segment2, Vec2};
use kanavos_i18n::{t, tf};
use kanavos_model::{Category, Document, Element, ElementKind, Id, ReferenceLine, Wall};
use kanavos_units::{Length, Locale, Tolerance, format_decimal};

/// Snap settings.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapSettings {
    /// Master switch (Alt disables temporarily).
    pub enabled: bool,
    /// Snap to element points (ends, midpoints, corners, centres, intersections).
    pub points: bool,
    /// Snap to the nearest point on edges, wall faces, centrelines and layer boundaries.
    pub edges: bool,
    /// Grid spacing; `None` disables grid snapping.
    pub grid: Option<Length>,
    /// Angle step for direction locking from the previous point, degrees (0 disables).
    pub angle_step: f64,
    /// Snap radius in screen pixels.
    pub radius_px: f64,
    /// Inference lines: alignment with recently hovered points, and extensions, parallels and perpendiculars of
    /// recently hovered edges.
    pub inference: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        SnapSettings {
            enabled: true,
            points: true,
            edges: true,
            grid: Some(Length::cm(5.0)),
            angle_step: 15.0,
            radius_px: 10.0,
            inference: true,
        }
    }
}

/// Why the cursor snapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapHint {
    /// No snap.
    Free,
    /// An end point.
    Endpoint,
    /// A midpoint.
    Midpoint,
    /// A corner of an outline.
    Corner,
    /// A centre.
    Centre,
    /// The intersection of two edges.
    Intersection,
    /// The nearest point on an edge.
    OnEdge,
    /// Perpendicular from the previous point to an edge.
    Perpendicular,
    /// Along the extension of an edge.
    Extension,
    /// A locked angle step from the previous point.
    Angle,
    /// A grid point.
    Grid,
    /// Parallel to a recently hovered edge, through the previous point.
    Parallel,
    /// Horizontally or vertically aligned with a recently hovered point.
    Alignment,
    /// On a wall face.
    Face,
    /// On a wall's centreline.
    Centreline,
    /// On the boundary between two layers of a wall's build-up.
    LayerBoundary,
    /// The insertion point of an object, text, level or stair.
    Insertion,
    /// Along the direction locked with Shift.
    Locked,
}

impl SnapHint {
    /// Catalogue key of the on-screen label.
    pub fn key(self) -> &'static str {
        match self {
            SnapHint::Free => "tools.snap.free",
            SnapHint::Endpoint => "tools.snap.endpoint",
            SnapHint::Midpoint => "tools.snap.midpoint",
            SnapHint::Corner => "tools.snap.corner",
            SnapHint::Centre => "tools.snap.centre",
            SnapHint::Intersection => "tools.snap.intersection",
            SnapHint::OnEdge => "tools.snap.on_edge",
            SnapHint::Perpendicular => "tools.snap.perpendicular",
            SnapHint::Extension => "tools.snap.extension",
            SnapHint::Angle => "tools.snap.angle",
            SnapHint::Grid => "tools.snap.grid",
            SnapHint::Parallel => "tools.snap.parallel",
            SnapHint::Alignment => "tools.snap.alignment",
            SnapHint::Face => "tools.snap.face",
            SnapHint::Centreline => "tools.snap.centreline",
            SnapHint::LayerBoundary => "tools.snap.layer_boundary",
            SnapHint::Insertion => "tools.snap.insertion",
            SnapHint::Locked => "tools.snap.locked",
        }
    }

    /// Whether the hint names a point that can be tracked for alignment.
    pub fn is_point(self) -> bool {
        matches!(
            self,
            SnapHint::Endpoint
                | SnapHint::Midpoint
                | SnapHint::Corner
                | SnapHint::Centre
                | SnapHint::Intersection
                | SnapHint::Insertion
                | SnapHint::Perpendicular
        )
    }

    /// Whether the hint names a point on an edge.
    pub fn is_edge(self) -> bool {
        matches!(self, SnapHint::OnEdge | SnapHint::Face | SnapHint::Centreline | SnapHint::LayerBoundary)
    }

    /// Preference among point snaps at about the same distance (lower wins).
    fn rank(self) -> u8 {
        match self {
            SnapHint::Endpoint | SnapHint::Corner | SnapHint::Intersection => 0,
            SnapHint::Centre | SnapHint::Insertion => 1,
            _ => 2,
        }
    }

    /// Whether the label names the element with "to" ("κάθετη στον τοίχο 12") rather than "of".
    fn uses_to(self) -> bool {
        matches!(self, SnapHint::Perpendicular | SnapHint::Parallel)
    }
}

/// A snapped point.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    /// The point to use.
    pub point: Point2,
    /// Why.
    pub hint: SnapHint,
    /// On-screen label, e.g. "μέσο τοίχου 12" or "κάθετη στον τοίχο 12".
    pub label: String,
    /// Guide lines to draw (inference lines).
    pub guides: Vec<Segment2>,
    /// The element snapped to, if any.
    pub element: Option<Id>,
    /// The edge snapped to, for edge snaps (used to infer extensions and parallels later).
    pub edge: Option<Segment2>,
}

impl SnapResult {
    /// A free (unsnapped) point.
    pub fn free(point: Point2) -> SnapResult {
        SnapResult {
            point,
            hint: SnapHint::Free,
            label: String::new(),
            guides: Vec::new(),
            element: None,
            edge: None,
        }
    }
}

/// Context of one snap query.
#[derive(Debug, Clone, Copy)]
pub struct SnapQuery<'a> {
    /// The previous point of the operation (angle steps, perpendiculars and parallels start here).
    pub from: Option<Point2>,
    /// Settings.
    pub settings: &'a SnapSettings,
    /// Size of a screen pixel in metres.
    pub pixel: f64,
    /// Language of labels.
    pub locale: Locale,
    /// Recently hovered points, for alignment lines (most recent first).
    pub tracked: &'a [Point2],
    /// Recently hovered edges, for extension, parallel and perpendicular lines (most recent first).
    pub edges: &'a [(Segment2, Id)],
    /// Direction locked with Shift, from `from`.
    pub lock: Option<Vec2>,
    /// Elements to ignore (e.g. the ones being moved).
    pub exclude: &'a BTreeSet<Id>,
}

impl<'a> SnapQuery<'a> {
    /// A query with no tracking, lock or exclusions.
    pub fn simple(from: Option<Point2>, settings: &'a SnapSettings, pixel: f64, locale: Locale) -> Self {
        static NONE: BTreeSet<Id> = BTreeSet::new();
        SnapQuery { from, settings, pixel, locale, tracked: &[], edges: &[], lock: None, exclude: &NONE }
    }
}

#[derive(Debug, Clone, Copy)]
struct PointEntry {
    p: Point2,
    hint: SnapHint,
    element: Id,
}

#[derive(Debug, Clone, Copy)]
struct SegEntry {
    seg: Segment2,
    hint: SnapHint,
    element: Id,
}

#[derive(Debug, Clone, Default)]
struct Bucket {
    points: Vec<u32>,
    segments: Vec<u32>,
    boxes: Vec<u32>,
}

/// Uniform grid over the plan. Items that would cover too many cells go to the `large` lists, which every query
/// checks.
#[derive(Debug, Clone, Default)]
struct Grid {
    cell: f64,
    cells: HashMap<(i32, i32), Bucket>,
    large_segments: Vec<u32>,
    large_boxes: Vec<u32>,
}

const MAX_CELLS_PER_ITEM: usize = 2048;

impl Grid {
    fn key(&self, x: f64, y: f64) -> (i32, i32) {
        // `as` saturates for out-of-range values, which keeps far-away coordinates in edge cells.
        ((x / self.cell).floor() as i32, (y / self.cell).floor() as i32)
    }

    fn insert_point(&mut self, i: u32, p: Point2) {
        let [x, y] = p.xy();
        let k = self.key(x, y);
        self.cells.entry(k).or_default().points.push(i);
    }

    /// Inserts a segment into every cell it passes through (grid traversal).
    fn insert_segment(&mut self, i: u32, s: Segment2) {
        let [ax, ay] = s.start.xy();
        let [bx, by] = s.end.xy();
        let (mut cx, mut cy) = self.key(ax, ay);
        let (ex, ey) = self.key(bx, by);
        let span =
            (i64::from(ex) - i64::from(cx)).unsigned_abs() + (i64::from(ey) - i64::from(cy)).unsigned_abs();
        if span as usize > MAX_CELLS_PER_ITEM
            || !(ax.is_finite() && ay.is_finite() && bx.is_finite() && by.is_finite())
        {
            self.large_segments.push(i);
            return;
        }
        let (dx, dy) = (bx - ax, by - ay);
        let step_x = if dx > 0.0 { 1 } else { -1 };
        let step_y = if dy > 0.0 { 1 } else { -1 };
        let cell = self.cell;
        let t_delta_x = if dx.abs() > 1e-12 { cell / dx.abs() } else { f64::INFINITY };
        let t_delta_y = if dy.abs() > 1e-12 { cell / dy.abs() } else { f64::INFINITY };
        let bound_x = if dx > 0.0 { f64::from(cx + 1) * cell } else { f64::from(cx) * cell };
        let bound_y = if dy > 0.0 { f64::from(cy + 1) * cell } else { f64::from(cy) * cell };
        let mut t_max_x = if dx.abs() > 1e-12 { (bound_x - ax) / dx } else { f64::INFINITY };
        let mut t_max_y = if dy.abs() > 1e-12 { (bound_y - ay) / dy } else { f64::INFINITY };
        for _ in 0..=span + 1 {
            self.cells.entry((cx, cy)).or_default().segments.push(i);
            if cx == ex && cy == ey {
                break;
            }
            if t_max_x < t_max_y {
                t_max_x += t_delta_x;
                cx = cx.saturating_add(step_x);
            } else {
                t_max_y += t_delta_y;
                cy = cy.saturating_add(step_y);
            }
        }
    }

    fn insert_box(&mut self, i: u32, b: Bbox2) {
        let (x0, y0) = self.key(b.min.x.as_m(), b.min.y.as_m());
        let (x1, y1) = self.key(b.max.x.as_m(), b.max.y.as_m());
        let n = (i64::from(x1) - i64::from(x0) + 1) * (i64::from(y1) - i64::from(y0) + 1);
        if n <= 0 || n as usize > MAX_CELLS_PER_ITEM {
            self.large_boxes.push(i);
            return;
        }
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                self.cells.entry((cx, cy)).or_default().boxes.push(i);
            }
        }
    }

    /// Calls `f` for every bucket overlapping the rectangle `[min, max]`.
    fn visit(&self, min: [f64; 2], max: [f64; 2], mut f: impl FnMut(&Bucket)) {
        let eps = 1e-9;
        let (x0, y0) = self.key(min[0] - eps, min[1] - eps);
        let (x1, y1) = self.key(max[0] + eps, max[1] + eps);
        let n = (i64::from(x1) - i64::from(x0) + 1).max(0) * (i64::from(y1) - i64::from(y0) + 1).max(0);
        if n as usize > self.cells.len() {
            for ((cx, cy), b) in &self.cells {
                if (x0..=x1).contains(cx) && (y0..=y1).contains(cy) {
                    f(b);
                }
            }
        } else {
            for cx in x0..=x1 {
                for cy in y0..=y1 {
                    if let Some(b) = self.cells.get(&(cx, cy)) {
                        f(b);
                    }
                }
            }
        }
    }
}

/// How labels name an element: its category and number (tag, else id).
#[derive(Debug, Clone)]
struct ElementRef {
    category: Category,
    number: String,
}

/// Snap candidates of one storey with a spatial index; build once per document version and storey, query per
/// pointer move.
#[derive(Debug, Clone, Default)]
pub struct Snapper {
    points: Vec<PointEntry>,
    segments: Vec<SegEntry>,
    boxes: Vec<(Id, Bbox2)>,
    box_of: HashMap<Id, usize>,
    outlines: HashMap<Id, PolygonWithHoles>,
    refs: HashMap<Id, ElementRef>,
    ranges: HashMap<Id, (Range<usize>, Range<usize>)>,
    grid: Grid,
}

/// Plan scale of a storey (for the size of text and symbols), 50 when the storey has no plan view.
pub(crate) fn plan_scale(doc: &Document, storey: Id) -> f64 {
    doc.plan_view_of(storey).map(|v| f64::from(v.scale)).unwrap_or(50.0)
}

/// Whether an element shows on a storey's plan: placed on it, or a project-wide grid line.
pub(crate) fn on_storey(e: &Element, storey: Id) -> bool {
    e.storey == Some(storey)
        || (e.storey.is_none() && e.view.is_none() && matches!(e.kind, ElementKind::Grid(_)))
}

/// The number shown for an element in labels: its tag, else its id.
pub(crate) fn element_number(e: &Element) -> String {
    if e.tag.trim().is_empty() { e.id.0.to_string() } else { e.tag.clone() }
}

thread_local! {
    static CACHE: RefCell<Vec<CacheEntry>> = const { RefCell::new(Vec::new()) };
}

struct CacheEntry {
    doc: Document,
    storey: Id,
    fingerprint: u64,
    snapper: Rc<Snapper>,
}

const CACHE_SIZE: usize = 4;

/// What, besides the elements, changes snap geometry: build-up thicknesses and the plan scale.
fn fingerprint(doc: &Document, storey: Id) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x0100_0000_01b3);
    };
    for b in doc.build_ups() {
        mix(b.id.0);
        mix(b.thickness().as_m().to_bits());
    }
    mix(plan_scale(doc, storey).to_bits());
    mix(doc.storey(storey).map(|s| s.cut_height.as_m().to_bits()).unwrap_or(0));
    h
}

impl Snapper {
    /// The snapper of a storey, built on first use and shared until the document's elements or build-ups change.
    /// Tools call this on every event; only the first call after a change pays for building the index.
    pub fn cached(doc: &Document, storey: Id) -> Rc<Snapper> {
        let fp = fingerprint(doc, storey);
        let hit = CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let pos = c
                .iter()
                .position(|e| e.storey == storey && e.fingerprint == fp && e.doc.shares_elements_with(doc))?;
            let entry = c.remove(pos);
            let s = Rc::clone(&entry.snapper);
            c.insert(0, entry);
            Some(s)
        });
        if let Some(s) = hit {
            return s;
        }
        let s = Rc::new(Snapper::new(doc, storey));
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.insert(0, CacheEntry { doc: doc.clone(), storey, fingerprint: fp, snapper: Rc::clone(&s) });
            c.truncate(CACHE_SIZE);
        });
        s
    }

    /// Gathers snap geometry of the elements on a storey (and grid lines).
    pub fn new(doc: &Document, storey: Id) -> Snapper {
        let scale = plan_scale(doc, storey);
        let mut s = Snapper::default();
        for e in doc.elements() {
            if !on_storey(e, storey) {
                continue;
            }
            s.refs.insert(e.id, ElementRef { category: e.category(), number: element_number(e) });
            let (p0, s0) = (s.points.len(), s.segments.len());
            match &e.kind {
                ElementKind::Wall(w) => s.add_wall(doc, e.id, w),
                ElementKind::Opening(_) => s.add_opening(doc, e.id),
                ElementKind::Underlay(u) => s.add_underlay(e.id, &u.drawing),
                _ => s.add_generic(doc, e.id),
            }
            s.ranges.insert(e.id, (p0..s.points.len(), s0..s.segments.len()));
            let footprint = match &e.kind {
                ElementKind::Opening(_) | ElementKind::Underlay(_) => None,
                _ => kanavos_elements::footprint(doc, e.id).ok().flatten(),
            };
            let pts = s.points.get(p0..).unwrap_or(&[]);
            let segs = s.segments.get(s0..).unwrap_or(&[]);
            if let Some(b) = extent(e, scale, pts, segs, footprint.as_ref()) {
                s.box_of.insert(e.id, s.boxes.len());
                s.boxes.push((e.id, b));
            }
            if let Some(fp) = footprint {
                s.outlines.insert(e.id, fp);
            }
        }
        s.index();
        s
    }

    fn point(&mut self, p: Point2, hint: SnapHint, element: Id) {
        if p.is_finite() {
            self.points.push(PointEntry { p, hint, element });
        }
    }

    fn segment(&mut self, seg: Segment2, hint: SnapHint, element: Id) {
        if seg.start.is_finite() && seg.end.is_finite() && seg.length().as_m() > 1e-9 {
            self.segments.push(SegEntry { seg, hint, element });
        }
    }

    fn polyline(&mut self, pts: &[Point2], hint: SnapHint, element: Id) {
        for w in pts.windows(2) {
            if let [a, b] = w {
                self.segment(Segment2::new(*a, *b), hint, element);
            }
        }
    }

    fn add_wall(&mut self, doc: &Document, id: Id, w: &Wall) {
        self.point(w.start, SnapHint::Endpoint, id);
        self.point(w.end, SnapHint::Endpoint, id);
        let Ok(layout) = kanavos_elements::wall_layout(doc, id) else {
            self.segment(w.segment(), SnapHint::Centreline, id);
            self.point(w.start.midpoint(w.end), SnapHint::Midpoint, id);
            return;
        };
        let (left, right) = w.face_offsets(layout.thickness);
        // Centreline (the reference line itself for centred walls).
        let centre = offset_wall_line(w, (left + right) / 2.0);
        self.polyline(&centre, SnapHint::Centreline, id);
        if w.reference != ReferenceLine::Centre && w.bulge.abs() < 1e-9 {
            // The reference line lies on a face; its midpoint is still offered.
            self.point(w.start.midpoint(w.end), SnapHint::Midpoint, id);
        }
        match w.arc() {
            Some(arc) => self.point(arc.at(0.5), SnapHint::Midpoint, id),
            None => {
                if let (Some(a), Some(b)) = (centre.first(), centre.last()) {
                    self.point(a.midpoint(*b), SnapHint::Midpoint, id);
                }
            }
        }
        // Faces with their midpoints, and the end caps.
        for face in [&layout.left_face, &layout.right_face] {
            self.polyline(face, SnapHint::Face, id);
            if face.len() == 2
                && let (Some(a), Some(b)) = (face.first(), face.last())
            {
                self.point(a.midpoint(*b), SnapHint::Midpoint, id);
            }
        }
        if let (Some(l0), Some(r0), Some(l1), Some(r1)) = (
            layout.left_face.first(),
            layout.right_face.first(),
            layout.left_face.last(),
            layout.right_face.last(),
        ) {
            self.segment(Segment2::new(*l0, *r0), SnapHint::OnEdge, id);
            self.segment(Segment2::new(*l1, *r1), SnapHint::OnEdge, id);
        }
        for v in &layout.outline.vertices {
            self.point(*v, SnapHint::Corner, id);
        }
        // Layer boundaries between consecutive layers.
        if let Some(layers) = doc.structure_layers(&w.structure) {
            let mut d = left;
            let count = layers.len();
            for (i, l) in layers.iter().enumerate() {
                d -= l.thickness.as_m();
                if i + 1 < count {
                    let line = offset_wall_line(w, d);
                    self.polyline(&line, SnapHint::LayerBoundary, id);
                    if let (Some(a), Some(b)) = (line.first(), line.last()) {
                        self.point(*a, SnapHint::Endpoint, id);
                        self.point(*b, SnapHint::Endpoint, id);
                    }
                }
            }
        }
    }

    fn add_opening(&mut self, doc: &Document, id: Id) {
        let Ok(pl) = kanavos_elements::opening_placement(doc, id) else { return };
        let h = pl.width.as_m() / 2.0;
        self.point(pl.centre, SnapHint::Centre, id);
        let corners = [
            pl.centre - pl.along * h + pl.across * pl.left_face,
            pl.centre + pl.along * h + pl.across * pl.left_face,
            pl.centre + pl.along * h + pl.across * pl.right_face,
            pl.centre - pl.along * h + pl.across * pl.right_face,
        ];
        for c in corners {
            self.point(c, SnapHint::Corner, id);
        }
        let [a, b, c, d] = corners;
        self.segment(Segment2::new(a, d), SnapHint::OnEdge, id);
        self.segment(Segment2::new(b, c), SnapHint::OnEdge, id);
    }

    fn add_underlay(&mut self, id: Id, d: &kanavos_drawing::Drawing) {
        let p = |q: kanavos_drawing::Pt| Point2::new(q.x, q.y);
        for item in &d.items {
            match &item.shape {
                Shape::Line { a, b } => {
                    self.point(p(*a), SnapHint::Endpoint, id);
                    self.point(p(*b), SnapHint::Endpoint, id);
                    self.segment(Segment2::new(p(*a), p(*b)), SnapHint::OnEdge, id);
                }
                Shape::Polyline { points, closed } => {
                    let pts: Vec<Point2> = points.iter().map(|q| p(*q)).collect();
                    for q in &pts {
                        self.point(*q, SnapHint::Endpoint, id);
                    }
                    self.polyline(&pts, SnapHint::OnEdge, id);
                    if *closed && let (Some(a), Some(b)) = (pts.last(), pts.first()) {
                        self.segment(Segment2::new(*a, *b), SnapHint::OnEdge, id);
                    }
                }
                Shape::Polygon { rings } => {
                    for r in rings {
                        let pts: Vec<Point2> = r.iter().map(|q| p(*q)).collect();
                        for q in &pts {
                            self.point(*q, SnapHint::Corner, id);
                        }
                        self.polyline(&pts, SnapHint::OnEdge, id);
                        if let (Some(a), Some(b)) = (pts.last(), pts.first()) {
                            self.segment(Segment2::new(*a, *b), SnapHint::OnEdge, id);
                        }
                    }
                }
                Shape::Circle { centre, .. } => self.point(p(*centre), SnapHint::Centre, id),
                Shape::Arc { centre, radius, start, sweep } => {
                    let arc = Arc2 {
                        centre: p(*centre),
                        radius: Length::m(*radius),
                        start: Angle::rad(*start),
                        sweep: Angle::rad(*sweep),
                    };
                    self.point(arc.centre, SnapHint::Centre, id);
                    self.point(arc.at(0.0), SnapHint::Endpoint, id);
                    self.point(arc.at(1.0), SnapHint::Endpoint, id);
                    self.polyline(&arc.tessellate(0.005, 4), SnapHint::OnEdge, id);
                }
                Shape::Text(t) => self.point(p(t.position), SnapHint::Insertion, id),
            }
        }
    }

    fn add_generic(&mut self, doc: &Document, id: Id) {
        let g = kanavos_elements::snap_geometry(doc, id);
        let is_arc_or_circle = doc.element(id).is_some_and(|e| {
            matches!(
                &e.kind,
                ElementKind::Drafting(d)
                    if matches!(d.shape, kanavos_model::DraftShape::Arc { .. } | kanavos_model::DraftShape::Circle { .. })
            )
        });
        for (p, k) in g.points {
            let hint = match k {
                kanavos_elements::SnapKind::Endpoint => SnapHint::Endpoint,
                kanavos_elements::SnapKind::Midpoint => SnapHint::Midpoint,
                kanavos_elements::SnapKind::Centre => SnapHint::Centre,
                kanavos_elements::SnapKind::Corner => SnapHint::Corner,
                kanavos_elements::SnapKind::Insertion => SnapHint::Insertion,
            };
            self.point(p, hint, id);
        }
        for seg in g.segments {
            self.segment(seg, SnapHint::OnEdge, id);
        }
        // Arcs and circles: their curve as short chords, so nearest-point snapping works on them.
        if is_arc_or_circle && let Some(ElementKind::Drafting(d)) = doc.element(id).map(|e| &e.kind) {
            let arc = match &d.shape {
                kanavos_model::DraftShape::Arc { arc } => Some(*arc),
                kanavos_model::DraftShape::Circle { centre, radius } => Some(Arc2 {
                    centre: *centre,
                    radius: *radius,
                    start: Angle::ZERO,
                    sweep: Angle::rad(std::f64::consts::TAU),
                }),
                _ => None,
            };
            if let Some(a) = arc {
                self.polyline(&a.tessellate(0.005, 8), SnapHint::OnEdge, id);
                if matches!(d.shape, kanavos_model::DraftShape::Circle { .. }) {
                    for q in 0..4 {
                        self.point(a.at(f64::from(q) / 4.0), SnapHint::Endpoint, id);
                    }
                }
            }
        }
    }

    fn index(&mut self) {
        let mut all: Option<Bbox2> = None;
        let mut grow = |p: Point2| match &mut all {
            Some(b) => b.include(p),
            None => all = Some(Bbox2 { min: p, max: p }),
        };
        for p in &self.points {
            grow(p.p);
        }
        for s in &self.segments {
            grow(s.seg.start);
            grow(s.seg.end);
        }
        let n = (self.points.len() + self.segments.len()).max(1) as f64;
        let area = all.map(|b| (b.width().as_m() * b.height().as_m()).max(1.0)).unwrap_or(100.0);
        let cell = ((area / n).sqrt() * 2.0).clamp(0.5, 50.0);
        let mut grid = Grid { cell, ..Grid::default() };
        for (i, p) in self.points.iter().enumerate() {
            grid.insert_point(i as u32, p.p);
        }
        for (i, s) in self.segments.iter().enumerate() {
            grid.insert_segment(i as u32, s.seg);
        }
        for (i, (_, b)) in self.boxes.iter().enumerate() {
            grid.insert_box(i as u32, *b);
        }
        self.grid = grid;
    }

    /// Number of snap points and segments (for tests and diagnostics).
    pub fn size(&self) -> (usize, usize) {
        (self.points.len(), self.segments.len())
    }

    fn near(&self, at: Point2, r: f64, exclude: &BTreeSet<Id>) -> (Vec<usize>, Vec<usize>) {
        let [x, y] = at.xy();
        let mut pts = Vec::new();
        let mut segs = Vec::new();
        self.grid.visit([x - r, y - r], [x + r, y + r], |b| {
            pts.extend(b.points.iter().map(|i| *i as usize));
            segs.extend(b.segments.iter().map(|i| *i as usize));
        });
        segs.extend(self.grid.large_segments.iter().map(|i| *i as usize));
        pts.sort_unstable();
        pts.dedup();
        segs.sort_unstable();
        segs.dedup();
        pts.retain(|i| {
            self.points.get(*i).is_some_and(|p| !exclude.contains(&p.element) && p.p.distance(at).as_m() <= r)
        });
        segs.retain(|i| {
            self.segments
                .get(*i)
                .is_some_and(|s| !exclude.contains(&s.element) && s.seg.distance_to(at).as_m() <= r)
        });
        (pts, segs)
    }

    /// Elements whose extent, grown by `margin`, contains a point (candidates for exact hit tests).
    pub fn elements_near(&self, at: Point2, margin: f64) -> Vec<Id> {
        let [x, y] = at.xy();
        let mut idx: Vec<u32> = Vec::new();
        self.grid.visit([x - margin, y - margin], [x + margin, y + margin], |b| {
            idx.extend(b.boxes.iter().copied())
        });
        idx.extend(self.grid.large_boxes.iter().copied());
        idx.sort_unstable();
        idx.dedup();
        let m = Length::m(margin);
        idx.into_iter()
            .filter_map(|i| self.boxes.get(i as usize))
            .filter(|(_, b)| b.expanded(m).contains(at))
            .map(|(id, _)| *id)
            .collect()
    }

    /// The plan extent of an element, if it has one.
    pub fn element_box(&self, id: Id) -> Option<Bbox2> {
        self.box_of.get(&id).and_then(|i| self.boxes.get(*i)).map(|(_, b)| *b)
    }

    /// Elements selected by a rectangle: fully inside it (`crossing == false`, window selection) or touching it
    /// (`crossing == true`, crossing selection).
    pub fn elements_in_box(&self, b: Bbox2, crossing: bool) -> Vec<Id> {
        let mut idx: Vec<u32> = Vec::new();
        self.grid.visit(b.min.xy(), b.max.xy(), |bk| idx.extend(bk.boxes.iter().copied()));
        idx.extend(self.grid.large_boxes.iter().copied());
        idx.sort_unstable();
        idx.dedup();
        let mut out = Vec::new();
        for i in idx {
            let Some((id, eb)) = self.boxes.get(i as usize) else { continue };
            if !b.intersects(eb) {
                continue;
            }
            let inside = b.contains(eb.min) && b.contains(eb.max);
            if inside || (crossing && self.touches(*id, &b, eb)) {
                out.push(*id);
            }
        }
        out.sort_unstable();
        out
    }

    fn touches(&self, id: Id, b: &Bbox2, eb: &Bbox2) -> bool {
        let Some((pr, sr)) = self.ranges.get(&id) else { return true };
        let edges = box_edges(b);
        let tol = Tolerance::default();
        let pts = self.points.get(pr.clone()).unwrap_or(&[]);
        let segs = self.segments.get(sr.clone()).unwrap_or(&[]);
        if pts.is_empty() && segs.is_empty() {
            return true; // Text and other annotation: the extent decides.
        }
        if pts.iter().any(|p| b.contains(p.p)) {
            return true;
        }
        for s in segs {
            if b.contains(s.seg.start) || b.contains(s.seg.end) {
                return true;
            }
            if edges.iter().any(|e| s.seg.intersection(e, &tol).is_some()) {
                return true;
            }
        }
        // A box entirely inside a closed outline touches it.
        let _ = eb;
        self.outlines.get(&id).is_some_and(|o| o.contains(b.centre(), &tol))
    }

    /// The plan outline (footprint) of an element, if it has one: walls, slabs, roofs, spaces, columns, beams and
    /// stairs.
    pub fn outline(&self, id: Id) -> Option<&PolygonWithHoles> {
        self.outlines.get(&id)
    }

    /// Distance from a point to an element's snap geometry (points and edges), or `None` when it has none.
    pub fn distance_to(&self, id: Id, at: Point2) -> Option<f64> {
        let (pr, sr) = self.ranges.get(&id)?;
        let pts = self.points.get(pr.clone()).unwrap_or(&[]);
        let segs = self.segments.get(sr.clone()).unwrap_or(&[]);
        pts.iter()
            .map(|p| p.p.distance(at).as_m())
            .chain(segs.iter().map(|s| s.seg.distance_to(at).as_m()))
            .min_by(f64::total_cmp)
    }

    fn reference(&self, id: Option<Id>, to: bool, locale: Locale) -> Option<String> {
        let r = self.refs.get(&id?)?;
        let key = format!("tools.ref.{}.{}", r.category.key(), if to { "to" } else { "of" });
        Some(tf(locale, &key, &[("n", &r.number)]))
    }

    fn label(&self, hint: SnapHint, element: Option<Id>, locale: Locale) -> String {
        let base = t(locale, hint.key()).into_owned();
        match hint {
            SnapHint::Free | SnapHint::Grid | SnapHint::Alignment | SnapHint::Locked => base,
            _ => match self.reference(element, hint.uses_to(), locale) {
                Some(r) => format!("{base} {r}"),
                None => base,
            },
        }
    }

    /// Snaps a raw pointer position. `from` is the previous point of the operation, for angle locks and
    /// perpendiculars. `pixel` is the size of a screen pixel in metres.
    pub fn snap(
        &self,
        raw: Point2,
        from: Option<Point2>,
        settings: &SnapSettings,
        pixel: f64,
        locale: Locale,
    ) -> SnapResult {
        self.snap_query(raw, &SnapQuery::simple(from, settings, pixel, locale))
    }

    /// Snaps a raw pointer position with tracking, a locked direction and exclusions.
    ///
    /// Order: points and intersections of nearby edges; the perpendicular foot from the previous point;
    /// crossings of inference lines with each other or with edges; a single inference line; the nearest point on
    /// an edge; the grid. Tolerance: the snap radius (`radius_px` screen pixels).
    pub fn snap_query(&self, raw: Point2, q: &SnapQuery<'_>) -> SnapResult {
        let s = q.settings;
        let r = s.radius_px.max(1.0) * q.pixel;
        let tol = Tolerance::default();
        let locked_line = match (q.from, q.lock) {
            (Some(f), Some(d)) => Some(InferenceLine {
                origin: f,
                dir: d,
                ray: false,
                hint: SnapHint::Locked,
                element: None,
                label: t(q.locale, SnapHint::Locked.key()).into_owned(),
            }),
            _ => None,
        };
        if !s.enabled {
            if let Some(l) = locked_line {
                let p = l.project(raw);
                return self.on_line(&l, p, q, false);
            }
            return SnapResult::free(raw);
        }
        if let Some(l) = locked_line {
            return self.snap_locked(raw, &l, q, r, &tol);
        }
        let (pts, segs) = self.near(raw, r, q.exclude);
        let pts: Vec<usize> = if s.points { pts } else { Vec::new() };
        let segs: Vec<usize> = if s.edges || s.points { segs } else { Vec::new() };

        // 1. Points and intersections of nearby edges.
        let mut best: Option<(f64, SnapResult)> = None;
        let consider = |d: f64, rank: u8, res: SnapResult, best: &mut Option<(f64, SnapResult)>| {
            let score = d + f64::from(rank) * 0.15 * r;
            if best.as_ref().is_none_or(|(b, _)| score < *b) {
                *best = Some((score, res));
            }
        };
        for i in &pts {
            let Some(p) = self.points.get(*i) else { continue };
            let res = SnapResult {
                point: p.p,
                hint: p.hint,
                label: self.label(p.hint, Some(p.element), q.locale),
                guides: Vec::new(),
                element: Some(p.element),
                edge: None,
            };
            consider(p.p.distance(raw).as_m(), p.hint.rank(), res, &mut best);
        }
        if s.points {
            let near_segs: Vec<&SegEntry> =
                segs.iter().filter_map(|i| self.segments.get(*i)).take(32).collect();
            for (i, a) in near_segs.iter().enumerate() {
                for b in near_segs.iter().skip(i + 1) {
                    if a.element == b.element && a.hint == b.hint {
                        continue;
                    }
                    if let Some((x, _, _)) = a.seg.intersection(&b.seg, &tol) {
                        let d = x.distance(raw).as_m();
                        if d <= r {
                            let res = SnapResult {
                                point: x,
                                hint: SnapHint::Intersection,
                                label: self.label(SnapHint::Intersection, Some(a.element), q.locale),
                                guides: Vec::new(),
                                element: Some(a.element),
                                edge: None,
                            };
                            consider(d, SnapHint::Intersection.rank(), res, &mut best);
                        }
                    }
                }
            }
        }
        if let Some((_, res)) = best {
            return res;
        }

        // 2. Perpendicular foot from the previous point.
        if s.edges
            && let Some(f) = q.from
        {
            let mut best: Option<(f64, SnapResult)> = None;
            for i in &segs {
                let Some(e) = self.segments.get(*i) else { continue };
                let tpar = e.seg.project_param(f);
                if !(0.0..=1.0).contains(&tpar) {
                    continue;
                }
                let foot = e.seg.at(tpar);
                let d = foot.distance(raw).as_m();
                if d <= r
                    && foot.distance(f).as_m() > tol.length.as_m()
                    && best.as_ref().is_none_or(|(b, _)| d < *b)
                {
                    best = Some((
                        d,
                        SnapResult {
                            point: foot,
                            hint: SnapHint::Perpendicular,
                            label: self.label(SnapHint::Perpendicular, Some(e.element), q.locale),
                            guides: vec![Segment2::new(f, foot)],
                            element: Some(e.element),
                            edge: Some(e.seg),
                        },
                    ));
                }
            }
            if let Some((_, res)) = best {
                return res;
            }
        }

        // 3. Inference lines: crossings, then a single line.
        let lines = self.inference_lines(raw, q, r);
        let near_lines: Vec<&InferenceLine> = lines.iter().filter(|l| l.distance(raw) <= r).collect();
        let mut best: Option<(f64, SnapResult)> = None;
        for (i, a) in near_lines.iter().enumerate() {
            for b in near_lines.iter().skip(i + 1) {
                if let Some(x) = a.intersect(b)
                    && x.distance(raw).as_m() <= r
                {
                    let d = x.distance(raw).as_m();
                    if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                        let hint = if a.hint == SnapHint::Angle { b.hint } else { a.hint };
                        let element = a.element.or(b.element);
                        best = Some((
                            d,
                            SnapResult {
                                point: x,
                                hint,
                                label: format!("{} · {}", a.label, b.label),
                                guides: vec![a.guide(x), b.guide(x)],
                                element,
                                edge: None,
                            },
                        ));
                    }
                }
            }
            if s.edges {
                for j in &segs {
                    let Some(e) = self.segments.get(*j) else { continue };
                    let Some((u, v)) = Line2 { origin: a.origin, direction: a.dir }
                        .intersection_params(&Line2::from_segment(&e.seg))
                    else {
                        continue;
                    };
                    if !(-1e-9..=1.0 + 1e-9).contains(&v) || (a.ray && u < 0.0) {
                        continue;
                    }
                    let x = a.origin + a.dir * u;
                    let d = x.distance(raw).as_m();
                    if d <= r && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                        best = Some((
                            d,
                            SnapResult {
                                point: x,
                                hint: a.hint,
                                label: format!(
                                    "{} · {}",
                                    a.label,
                                    self.label(e.hint, Some(e.element), q.locale)
                                ),
                                guides: vec![a.guide(x)],
                                element: Some(e.element),
                                edge: Some(e.seg),
                            },
                        ));
                    }
                }
            }
        }
        if let Some((_, res)) = best {
            return res;
        }
        // An edge under the pointer beats a single inference line.
        let edge = if s.edges {
            segs.iter()
                .filter_map(|i| self.segments.get(*i))
                .map(|e| (e.seg.distance_to(raw).as_m(), e))
                .min_by(|a, b| a.0.total_cmp(&b.0))
        } else {
            None
        };
        if edge.is_none()
            && let Some(l) =
                near_lines.iter().min_by(|a, b| a.distance(raw).total_cmp(&b.distance(raw))).copied()
        {
            let p = l.project(raw);
            return self.on_line(l, p, q, true);
        }
        if let Some((_, e)) = edge {
            let p = e.seg.closest_point(raw);
            return SnapResult {
                point: p,
                hint: e.hint,
                label: self.label(e.hint, Some(e.element), q.locale),
                guides: Vec::new(),
                element: Some(e.element),
                edge: Some(e.seg),
            };
        }
        if let Some(g) = s.grid {
            let gs = g.as_m();
            if gs > 0.0 {
                let [x, y] = raw.xy();
                let mut res = SnapResult::free(Point2::new((x / gs).round() * gs, (y / gs).round() * gs));
                res.hint = SnapHint::Grid;
                return res;
            }
        }
        SnapResult::free(raw)
    }

    /// A point on an inference line, with the distance from its origin rounded to the grid step for rays from
    /// the previous point.
    fn on_line(&self, l: &InferenceLine, p: Point2, q: &SnapQuery<'_>, round: bool) -> SnapResult {
        let mut p = p;
        if round
            && matches!(
                l.hint,
                SnapHint::Angle | SnapHint::Locked | SnapHint::Parallel | SnapHint::Perpendicular
            )
            && let Some(g) = q.settings.grid
            && g.as_m() > 0.0
        {
            let gs = g.as_m();
            let along = (p - l.origin).dot(l.dir);
            p = l.origin + l.dir * ((along / gs).round() * gs);
        }
        SnapResult {
            point: p,
            hint: l.hint,
            label: l.label.clone(),
            guides: vec![l.guide(p)],
            element: l.element,
            edge: None,
        }
    }

    fn snap_locked(
        &self,
        raw: Point2,
        l: &InferenceLine,
        q: &SnapQuery<'_>,
        r: f64,
        tol: &Tolerance,
    ) -> SnapResult {
        let on = l.project(raw);
        let (pts, segs) = self.near(on, r, q.exclude);
        // Crossing with an edge near the projected pointer.
        let mut best: Option<(f64, SnapResult)> = None;
        if q.settings.edges || q.settings.points {
            for i in &segs {
                let Some(e) = self.segments.get(*i) else { continue };
                let Some((u, v)) = Line2 { origin: l.origin, direction: l.dir }
                    .intersection_params(&Line2::from_segment(&e.seg))
                else {
                    continue;
                };
                if !(-1e-9..=1.0 + 1e-9).contains(&v) {
                    continue;
                }
                let x = l.origin + l.dir * u;
                let d = x.distance(on).as_m();
                if d <= r && best.as_ref().is_none_or(|(b, _)| d < *b) {
                    best = Some((
                        d,
                        SnapResult {
                            point: x,
                            hint: SnapHint::Locked,
                            label: format!("{} · {}", l.label, self.label(e.hint, Some(e.element), q.locale)),
                            guides: vec![l.guide(x)],
                            element: Some(e.element),
                            edge: Some(e.seg),
                        },
                    ));
                }
            }
        }
        if let Some((_, res)) = best {
            return res;
        }
        // Alignment with a point near the pointer: its projection on the locked line.
        if q.settings.points {
            let (pts_raw, _) = self.near(raw, r, q.exclude);
            let pick = pts_raw
                .iter()
                .chain(pts.iter())
                .filter_map(|i| self.points.get(*i))
                .min_by(|a, b| a.p.distance(raw).total_cmp(&b.p.distance(raw)));
            if let Some(p) = pick {
                let x = l.project(p.p);
                if x.distance(l.origin).as_m() > tol.length.as_m() {
                    return SnapResult {
                        point: x,
                        hint: SnapHint::Locked,
                        label: format!("{} · {}", l.label, self.label(p.hint, Some(p.element), q.locale)),
                        guides: vec![l.guide(x), Segment2::new(p.p, x)],
                        element: Some(p.element),
                        edge: None,
                    };
                }
            }
        }
        self.on_line(l, on, q, true)
    }

    fn inference_lines(&self, raw: Point2, q: &SnapQuery<'_>, r: f64) -> Vec<InferenceLine> {
        let s = q.settings;
        let mut lines = Vec::new();
        if let Some(f) = q.from
            && s.angle_step > 0.0
        {
            let v = raw - f;
            if v.length() > 1e-9 {
                let step = s.angle_step.to_radians();
                let a = (v.angle().as_rad() / step).round() * step;
                let deg = a.to_degrees().rem_euclid(360.0);
                lines.push(InferenceLine {
                    origin: f,
                    dir: Vec2::from_angle(Angle::rad(a)),
                    ray: true,
                    hint: SnapHint::Angle,
                    element: None,
                    label: format!(
                        "{} {}°",
                        t(q.locale, SnapHint::Angle.key()),
                        format_decimal(deg, if (deg - deg.round()).abs() < 1e-6 { 0 } else { 1 }, q.locale)
                    ),
                });
            }
        }
        if !s.inference {
            return lines;
        }
        let tol = Tolerance::default();
        let align = t(q.locale, SnapHint::Alignment.key()).into_owned();
        for p in q.tracked.iter().take(3) {
            if q.from.is_some_and(|f| f.coincides(*p, &tol)) || p.distance(raw).as_m() <= r {
                continue;
            }
            for dir in [Vec2::X, Vec2::Y] {
                lines.push(InferenceLine {
                    origin: *p,
                    dir,
                    ray: false,
                    hint: SnapHint::Alignment,
                    element: None,
                    label: align.clone(),
                });
            }
        }
        for (seg, id) in q.edges.iter().take(2) {
            if q.exclude.contains(id) {
                continue;
            }
            let Some(dir) = seg.direction() else { continue };
            // Extension: only beyond the edge's ends (on the edge itself it is an edge snap).
            let t_raw = seg.project_param(raw);
            if !(0.0..=1.0).contains(&t_raw) {
                let origin = if t_raw > 1.0 { seg.end } else { seg.start };
                lines.push(InferenceLine {
                    origin,
                    dir,
                    ray: false,
                    hint: SnapHint::Extension,
                    element: Some(*id),
                    label: self.label(SnapHint::Extension, Some(*id), q.locale),
                });
            }
            if let Some(f) = q.from {
                lines.push(InferenceLine {
                    origin: f,
                    dir,
                    ray: false,
                    hint: SnapHint::Parallel,
                    element: Some(*id),
                    label: self.label(SnapHint::Parallel, Some(*id), q.locale),
                });
                lines.push(InferenceLine {
                    origin: f,
                    dir: dir.perp(),
                    ray: false,
                    hint: SnapHint::Perpendicular,
                    element: Some(*id),
                    label: self.label(SnapHint::Perpendicular, Some(*id), q.locale),
                });
            }
        }
        lines
    }
}

/// An infinite line (or a ray from `origin`) the pointer can lock onto.
#[derive(Debug, Clone)]
struct InferenceLine {
    origin: Point2,
    dir: Vec2,
    ray: bool,
    hint: SnapHint,
    element: Option<Id>,
    label: String,
}

impl InferenceLine {
    fn param(&self, p: Point2) -> f64 {
        let u = (p - self.origin).dot(self.dir);
        if self.ray { u.max(0.0) } else { u }
    }
    fn project(&self, p: Point2) -> Point2 {
        self.origin + self.dir * self.param(p)
    }
    fn distance(&self, p: Point2) -> f64 {
        self.project(p).distance(p).as_m()
    }
    fn intersect(&self, o: &InferenceLine) -> Option<Point2> {
        let (u, v) = Line2 { origin: self.origin, direction: self.dir }
            .intersection_params(&Line2 { origin: o.origin, direction: o.dir })?;
        if (self.ray && u < 0.0) || (o.ray && v < 0.0) {
            return None;
        }
        Some(self.origin + self.dir * u)
    }
    fn guide(&self, to: Point2) -> Segment2 {
        Segment2::new(self.origin, to)
    }
}

/// A wall's reference line offset sideways by `d` metres (positive to the left), tessellated for arcs.
pub(crate) fn offset_wall_line(w: &Wall, d: f64) -> Vec<Point2> {
    match w.arc() {
        Some(arc) => {
            let sign = if arc.sweep.as_rad() > 0.0 { -1.0 } else { 1.0 };
            let r = (arc.radius.as_m() + sign * d).max(1e-6);
            Arc2 { radius: Length::m(r), ..arc }.tessellate(0.005, 8)
        }
        None => {
            let n = (w.end - w.start).normalized().map(Vec2::perp).unwrap_or(Vec2::ZERO) * d;
            vec![w.start + n, w.end + n]
        }
    }
}

fn box_edges(b: &Bbox2) -> [Segment2; 4] {
    let (x0, y0, x1, y1) = (b.min.x, b.min.y, b.max.x, b.max.y);
    let p = Point2::from_lengths;
    [
        Segment2::new(p(x0, y0), p(x1, y0)),
        Segment2::new(p(x1, y0), p(x1, y1)),
        Segment2::new(p(x1, y1), p(x0, y1)),
        Segment2::new(p(x0, y1), p(x0, y0)),
    ]
}

/// Plan extent of an element for hit testing and box selection.
fn extent(
    e: &Element,
    scale: f64,
    pts: &[PointEntry],
    segs: &[SegEntry],
    footprint: Option<&PolygonWithHoles>,
) -> Option<Bbox2> {
    let mm = |v: f64| v * scale / 1000.0;
    let mut b: Option<Bbox2> = None;
    let mut grow = |p: Point2| {
        if !p.is_finite() {
            return;
        }
        match &mut b {
            Some(bb) => bb.include(p),
            None => b = Some(Bbox2 { min: p, max: p }),
        }
    };
    for p in pts {
        grow(p.p);
    }
    for s in segs {
        grow(s.seg.start);
        grow(s.seg.end);
    }
    if let Some(fp) = footprint {
        for v in &fp.outer.vertices {
            grow(*v);
        }
    }
    match &e.kind {
        ElementKind::Text(tn) => {
            let lines = tn.content.lines().count().max(1) as f64;
            let chars = tn.content.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
            let w = mm(chars * tn.size_mm * 0.6);
            let h = mm(tn.size_mm);
            let u = Vec2::from_angle(tn.rotation);
            let v = u.perp();
            let x0 = match tn.align {
                kanavos_model::TextAlign::Left => 0.0,
                kanavos_model::TextAlign::Centre => -w / 2.0,
                kanavos_model::TextAlign::Right => -w,
            };
            let down = -mm(tn.size_mm * 1.6) * (lines - 1.0);
            for (a, c) in [(x0, down), (x0 + w, down), (x0 + w, h), (x0, h)] {
                grow(tn.position + u * a + v * c);
            }
        }
        ElementKind::Level(l) => {
            let s = mm(2.0);
            grow(l.position - Vec2::new(s, s));
            grow(l.position + Vec2::new(s * 1.5 + mm(12.0), s));
        }
        ElementKind::Dimension(d) => {
            for p in &d.points {
                grow(*p);
            }
            grow(d.line_point);
            let u = Vec2::from_angle(d.direction);
            for p in &d.points {
                grow(d.line_point + u * (*p - d.line_point).dot(u));
            }
        }
        ElementKind::Object(o) => {
            let def = kanavos_objects::find(&o.definition);
            let w = kanavos_objects::param(def, &o.params, "width", def.map(|d| d.width).unwrap_or(1.0));
            let dd = kanavos_objects::param(def, &o.params, "depth", def.map(|d| d.depth).unwrap_or(1.0));
            for q in object_corners(o, w, dd) {
                grow(q);
            }
        }
        ElementKind::Grid(g) => {
            let r = mm(4.0);
            let dir = (g.end - g.start).normalized().unwrap_or(Vec2::X);
            for (p, sign) in [(g.start, -1.0), (g.end, 1.0)] {
                let c = p + dir * (2.0 * r * sign);
                grow(c - Vec2::new(r, r));
                grow(c + Vec2::new(r, r));
            }
        }
        ElementKind::Drafting(d) => match &d.shape {
            kanavos_model::DraftShape::Circle { centre, radius } => {
                let r = radius.as_m();
                grow(*centre - Vec2::new(r, r));
                grow(*centre + Vec2::new(r, r));
            }
            kanavos_model::DraftShape::Arc { arc } => {
                for p in arc.tessellate(0.01, 8) {
                    grow(p);
                }
            }
            _ => {}
        },
        ElementKind::Underlay(u) => {
            if let Some(r) = u.drawing.bounds() {
                grow(Point2::new(r.min.x, r.min.y));
                grow(Point2::new(r.max.x, r.max.y));
            }
        }
        _ => {}
    }
    b
}

/// Corners of a free object's plan rectangle in world coordinates.
pub(crate) fn object_corners(o: &kanavos_model::Object, w: f64, d: f64) -> [Point2; 4] {
    let u = Vec2::from_angle(o.rotation);
    let v = u.perp();
    let m = if o.mirrored { -1.0 } else { 1.0 };
    let place = |x: f64, y: f64| o.position + u * (x * m) + v * y;
    [place(-w / 2.0, 0.0), place(w / 2.0, 0.0), place(w / 2.0, d), place(-w / 2.0, d)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::{Structure, starter};

    type Ends = ((f64, f64), (f64, f64));

    fn walls(doc: &Document, storey: Id, list: &[Ends]) -> Document {
        let mut doc = doc.clone();
        for (a, b) in list {
            let (d, id) = doc.with_new_id();
            doc = d.with_element(Element::on_storey(
                id,
                storey,
                ElementKind::Wall(Wall {
                    start: Point2::new(a.0, a.1),
                    end: Point2::new(b.0, b.1),
                    bulge: 0.0,
                    reference: ReferenceLine::Centre,
                    structure: Structure::Layered { build_up: doc.defaults().exterior_wall.unwrap() },
                    height: Length::m(3.0),
                    base_offset: Length::ZERO,
                }),
            ));
        }
        doc
    }

    #[test]
    fn snaps_to_wall_ends_and_names_them() {
        let doc = starter();
        let st = doc.storeys()[0].id;
        let doc = walls(&doc, st, &[((0.0, 0.0), (5.0, 0.0))]);
        let s = Snapper::new(&doc, st);
        let settings = SnapSettings::default();
        let r = s.snap(Point2::new(5.03, 0.02), None, &settings, 0.01, Locale::El);
        assert_eq!(r.hint, SnapHint::Endpoint);
        assert!(r.point.coincides(Point2::new(5.0, 0.0), &Tolerance::default()));
        assert!(r.label.contains("τοίχου"), "{}", r.label);
    }

    #[test]
    fn snaps_to_faces_and_layer_boundaries() {
        let doc = starter();
        let st = doc.storeys()[0].id;
        let doc = walls(&doc, st, &[((0.0, 0.0), (5.0, 0.0))]);
        let s = Snapper::new(&doc, st);
        let settings = SnapSettings { grid: None, ..SnapSettings::default() };
        // Exterior build-up 31 cm centred: left face at y = 0.155.
        let r = s.snap(Point2::new(2.3, 0.16), None, &settings, 0.002, Locale::En);
        assert_eq!(r.hint, SnapHint::Face, "{r:?}");
        assert!((r.point.y.as_m() - 0.155).abs() < 1e-9);
        // Plaster 1 cm, then EPS 8 cm: boundary at 0.145 and 0.065.
        let r = s.snap(Point2::new(2.3, 0.066), None, &settings, 0.002, Locale::En);
        assert_eq!(r.hint, SnapHint::LayerBoundary, "{r:?}");
        assert!(r.label.starts_with("layer boundary"));
    }

    #[test]
    fn intersections_and_perpendiculars() {
        let doc = starter();
        let st = doc.storeys()[0].id;
        let doc = walls(&doc, st, &[((0.0, 0.0), (6.0, 0.0)), ((3.0, -3.0), (3.0, 3.0))]);
        let s = Snapper::new(&doc, st);
        let settings = SnapSettings { grid: None, ..SnapSettings::default() };
        // Away from points: the crossing of two centrelines is offered as an intersection or a corner.
        let r = s.snap(Point2::new(3.01, 0.01), None, &settings, 0.002, Locale::En);
        assert!(matches!(r.hint, SnapHint::Intersection | SnapHint::Corner), "{r:?}");
        // Perpendicular from a point above the first wall.
        let from = Point2::new(1.0, 2.0);
        let r = s.snap(Point2::new(1.01, 0.155), Some(from), &settings, 0.002, Locale::En);
        assert_eq!(r.hint, SnapHint::Perpendicular, "{r:?}");
        assert!(r.label.starts_with("perpendicular to wall"));
    }

    #[test]
    fn alignment_with_tracked_points() {
        let doc = starter();
        let st = doc.storeys()[0].id;
        let s = Snapper::new(&doc, st);
        let settings = SnapSettings { grid: None, angle_step: 0.0, ..SnapSettings::default() };
        let tracked = [Point2::new(4.0, 7.0), Point2::new(9.0, 2.0)];
        let ex = BTreeSet::new();
        let q = SnapQuery {
            from: None,
            settings: &settings,
            pixel: 0.01,
            locale: Locale::El,
            tracked: &tracked,
            edges: &[],
            lock: None,
            exclude: &ex,
        };
        // Near x = 4 and y = 2: the crossing of both alignment lines.
        let r = s.snap_query(Point2::new(4.03, 2.04), &q);
        assert_eq!(r.hint, SnapHint::Alignment);
        assert!(r.point.coincides(Point2::new(4.0, 2.0), &Tolerance::default()));
        assert_eq!(r.guides.len(), 2);
    }

    #[test]
    fn locked_direction_projects_the_pointer() {
        let doc = starter();
        let st = doc.storeys()[0].id;
        let s = Snapper::new(&doc, st);
        let settings = SnapSettings::default();
        let ex = BTreeSet::new();
        let q = SnapQuery {
            from: Some(Point2::new(0.0, 0.0)),
            settings: &settings,
            pixel: 0.01,
            locale: Locale::El,
            tracked: &[],
            edges: &[],
            lock: Some(Vec2::X),
            exclude: &ex,
        };
        let r = s.snap_query(Point2::new(3.02, 1.5), &q);
        assert_eq!(r.hint, SnapHint::Locked);
        assert!(r.point.coincides(Point2::new(3.0, 0.0), &Tolerance::default()));
    }

    #[test]
    fn box_selection_window_and_crossing() {
        let doc = starter();
        let st = doc.storeys()[0].id;
        let doc = walls(&doc, st, &[((0.0, 0.0), (4.0, 0.0)), ((10.0, 0.0), (10.0, 4.0))]);
        let s = Snapper::new(&doc, st);
        let b = Bbox2::from_points([Point2::new(-1.0, -1.0), Point2::new(5.0, 1.0)]).unwrap();
        assert_eq!(s.elements_in_box(b, false).len(), 1);
        let b = Bbox2::from_points([Point2::new(3.0, -1.0), Point2::new(11.0, 1.0)]).unwrap();
        assert_eq!(s.elements_in_box(b, false).len(), 0);
        assert_eq!(s.elements_in_box(b, true).len(), 2);
    }
}

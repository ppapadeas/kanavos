//! Outline tools: slab, roof, space, fill and plot (`specs/tools/outlines.md`).
//!
//! All five get a closed outline in one of three ways:
//! - **Polygon**: click the vertices; click the first vertex again, double-click or press Enter to close.
//! - **Rectangle**: two opposite corners.
//! - **Pick inside**: click inside an area enclosed by walls. Spaces take the walls' inside faces; slabs and roofs
//!   take the outside faces. When the walls do not enclose the point (or detection is not available), the tool
//!   says so and the click starts a polygon instead.
//!
//! Roofs grow the outline by the overhang and slope the edges the roof kind asks for: one edge for a mono-pitch
//! roof (the edge nearest the first click; the flip key moves it to the next edge), the two long opposite edges
//! for a gable roof (the flip key swaps to the other pair), every edge for a hip roof. Flat roofs get the
//! parapet from the settings.

use kanavos_engine::AnyCommand;
use kanavos_engine::commands::{CreateElement, DetectSpace, SetSite};
use kanavos_geom::{Line2, Point2, Polygon2, PolygonWithHoles, Segment2};
use kanavos_i18n::{t, tf};
use kanavos_model::{DraftShape, Drafting, ElementKind, Roof, RoofShape, Site, Slab, Space};
use kanavos_units::AreaDisplay;

use crate::common::{
    Field, Fields, Pointer, base_output, length_text, near, next_space_number, segment_live, tr, typed_target,
};
use crate::preview::{self, BLUE};
use crate::settings::{OutlineMode, RoofKind, storey_height};
use crate::{Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput};

/// A cached wall-enclosure detection.
#[derive(Debug, Clone)]
struct Detection {
    doc: kanavos_model::Document,
    at: Point2,
    result: Result<PolygonWithHoles, String>,
}

/// Slab, roof, space, fill and plot tool.
#[derive(Debug, Clone)]
pub struct OutlineTool {
    id: ToolId,
    pointer: Pointer,
    fields: Fields,
    points: Vec<Point2>,
    /// Pick-inside failed for this operation: draw a polygon instead.
    fallback: bool,
    /// Why pick-inside failed.
    notice: Option<String>,
    /// Flip presses: moves the sloped edge of mono-pitch and gable roofs.
    shift: usize,
    detection: Option<Detection>,
}

impl OutlineTool {
    /// An outline tool for slabs, roofs, spaces, fills or the plot.
    pub fn new(id: ToolId) -> OutlineTool {
        OutlineTool {
            id,
            pointer: Pointer::default(),
            fields: Fields::default(),
            points: Vec::new(),
            fallback: false,
            notice: None,
            shift: 0,
            detection: None,
        }
    }

    fn setting_mode(&self, ctx: &ToolContext<'_>) -> OutlineMode {
        match self.id {
            ToolId::Slab => ctx.settings.slab.mode,
            ToolId::Roof => ctx.settings.roof.mode,
            ToolId::Space => ctx.settings.space.mode,
            ToolId::Fill => ctx.settings.fill_mode,
            _ => OutlineMode::Polygon,
        }
    }

    fn mode(&self, ctx: &ToolContext<'_>) -> OutlineMode {
        match self.setting_mode(ctx) {
            OutlineMode::PickInside if self.fallback || !self.points.is_empty() => OutlineMode::Polygon,
            m => m,
        }
    }

    fn reset(&mut self) {
        self.points.clear();
        self.fallback = false;
        self.notice = None;
        self.fields.clear();
        self.pointer.unlock();
    }

    fn detect(&mut self, ctx: &ToolContext<'_>, at: Point2) -> Result<PolygonWithHoles, String> {
        if let Some(d) = &self.detection
            && d.doc.shares_elements_with(ctx.doc)
            && d.at.distance(at).as_m() <= 3.0 * ctx.pixel_size
        {
            return d.result.clone();
        }
        let result = match kanavos_elements::detect_space(ctx.doc, ctx.storey, at) {
            Ok(r) => Ok(r),
            Err(kanavos_elements::GeometryError::Unsupported(_)) => {
                Err(tr(ctx, "tools.outline.detect_unavailable"))
            }
            Err(e) => Err(tf(
                ctx.locale,
                "tools.outline.detect_failed",
                &[("reason", &preview::explain(&e, ctx.locale))],
            )),
        };
        self.detection = Some(Detection { doc: ctx.doc.clone(), at, result: result.clone() });
        result
    }

    /// The element kind for an outline (`None` for the plot, which is not an element).
    fn kind(
        &self,
        ctx: &ToolContext<'_>,
        region: &PolygonWithHoles,
        from_walls: bool,
        click: Option<Point2>,
    ) -> Option<ElementKind> {
        let tol = ctx.doc.tolerance();
        let region = PolygonWithHoles {
            outer: region.outer.simplified(&tol).to_ccw(),
            holes: region.holes.iter().map(|h| h.simplified(&tol).to_cw()).collect(),
        };
        match self.id {
            ToolId::Slab => {
                let outer =
                    if from_walls { expand_to_walls(ctx, &region.outer) } else { region.outer.clone() };
                Some(ElementKind::Slab(Slab {
                    outline: PolygonWithHoles { outer, holes: region.holes },
                    structure: ctx.settings.slab_structure(ctx.doc, ctx.storey)?,
                    top_offset: ctx.settings.slab.top_offset,
                }))
            }
            ToolId::Roof => {
                let s = &ctx.settings.roof;
                let walls =
                    if from_walls { expand_to_walls(ctx, &region.outer) } else { region.outer.clone() };
                let d = vec![s.overhang.as_m(); walls.len()];
                let outline = offset_ring(&walls, &d);
                let shape = self.roof_shape(ctx, &outline, click);
                Some(ElementKind::Roof(Roof {
                    base: s.base.unwrap_or_else(|| storey_height(ctx.doc, ctx.storey)),
                    structure: ctx.settings.roof_structure(ctx.doc)?,
                    shape,
                    outline,
                }))
            }
            ToolId::Space => {
                let s = &ctx.settings.space;
                Some(ElementKind::Space(Space {
                    boundary: region,
                    name: s.name.clone(),
                    number: if s.auto_number {
                        next_space_number(ctx.doc, ctx.storey)
                    } else {
                        String::new()
                    },
                    category: s.category,
                    height: None,
                    stamp: click.filter(|_| from_walls),
                    finish: String::new(),
                }))
            }
            ToolId::Fill => Some(ElementKind::Drafting(Drafting {
                shape: DraftShape::Fill { region },
                style: ctx.settings.draft.clone(),
            })),
            _ => None,
        }
    }

    fn roof_shape(&self, ctx: &ToolContext<'_>, outline: &Polygon2, click: Option<Point2>) -> RoofShape {
        let s = &ctx.settings.roof;
        let n = outline.len();
        match s.kind {
            RoofKind::Flat => RoofShape::Flat { parapet: s.parapet },
            RoofKind::Hip => RoofShape::Pitched { slopes: vec![Some(s.pitch); n] },
            RoofKind::MonoPitch => {
                let i = (primary_edge(outline, click) + self.shift) % n.max(1);
                RoofShape::Pitched { slopes: (0..n).map(|j| (j == i).then_some(s.pitch)).collect() }
            }
            RoofKind::Gable => {
                let (a, b) = gable_edges(outline, self.shift);
                RoofShape::Pitched { slopes: (0..n).map(|j| (j == a || j == b).then_some(s.pitch)).collect() }
            }
        }
    }

    fn commands(
        &self,
        ctx: &ToolContext<'_>,
        region: &PolygonWithHoles,
        from_walls: bool,
        click: Option<Point2>,
    ) -> Vec<AnyCommand> {
        if self.id == ToolId::Plot {
            let site = Site {
                plot: Some(region.outer.simplified(&ctx.doc.tolerance()).to_ccw()),
                ..ctx.doc.site().clone()
            };
            return vec![SetSite { site }.into()];
        }
        if self.id == ToolId::Space
            && from_walls
            && let Some(at) = click
        {
            let s = &ctx.settings.space;
            return vec![
                DetectSpace {
                    storey: ctx.storey,
                    point: at,
                    name: s.name.clone(),
                    number: if s.auto_number {
                        next_space_number(ctx.doc, ctx.storey)
                    } else {
                        String::new()
                    },
                    category: s.category,
                }
                .into(),
            ];
        }
        match self.kind(ctx, region, from_walls, click) {
            Some(kind) => vec![CreateElement::on(ctx.storey, kind).into()],
            None => Vec::new(),
        }
    }

    /// Commits an outline if it is valid; returns whether it did.
    fn commit(
        &mut self,
        ctx: &ToolContext<'_>,
        out: &mut ToolOutput,
        region: PolygonWithHoles,
        from_walls: bool,
        click: Option<Point2>,
    ) -> bool {
        if !region.outer.is_valid(&ctx.doc.tolerance()) {
            out.warning = Some(tr(ctx, "tools.outline.too_small"));
            return false;
        }
        if let Some(kind) = self.kind(ctx, &region, from_walls, click) {
            let e = preview::new_element(ctx, 0, Some(ctx.storey), kind);
            if let Err(err) = kanavos_elements::validate(ctx.doc, &e) {
                out.warning = Some(preview::explain(&err, ctx.locale));
                return false;
            }
        }
        out.commands = self.commands(ctx, &region, from_walls, click);
        out.finished = true;
        self.reset();
        true
    }

    fn polygon_with(&self, cursor: Option<Point2>) -> Vec<Point2> {
        let mut v = self.points.clone();
        if let Some(c) = cursor
            && v.last().is_none_or(|l| l.distance(c).as_m() > 1e-9)
        {
            v.push(c);
        }
        v
    }

    fn rect_corner(&self, a: Point2, cursor: Point2) -> Point2 {
        let (dx, dy) = (self.fields.length(Field::Dx), self.fields.length(Field::Dy));
        let sx = if cursor.x >= a.x { 1.0 } else { -1.0 };
        let sy = if cursor.y >= a.y { 1.0 } else { -1.0 };
        let x = dx.map(|v| a.x.as_m() + sx * v.as_m()).unwrap_or(cursor.x.as_m());
        let y = dy.map(|v| a.y.as_m() + sy * v.as_m()).unwrap_or(cursor.y.as_m());
        Point2::new(x, y)
    }

    fn target(&self, ctx: &ToolContext<'_>) -> Option<Point2> {
        let cursor = self.pointer.point();
        if self.mode(ctx) == OutlineMode::Rectangle
            && let Some(a) = self.points.first().copied()
        {
            if let Some(p) = self.fields.point_form() {
                return p.to_point(Some(a), &ctx.doc.site().georef);
            }
            return cursor.map(|c| self.rect_corner(a, c));
        }
        typed_target(&self.fields, self.points.last().copied(), cursor, ctx.doc).or(cursor)
    }

    fn click(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, p: Point2, raw: Point2) {
        let tol = ctx.doc.tolerance();
        match self.mode(ctx) {
            OutlineMode::PickInside => match self.detect(ctx, raw) {
                Ok(region) => {
                    self.commit(ctx, out, region, true, Some(raw));
                }
                Err(why) => {
                    self.notice = Some(why);
                    self.fallback = true;
                    self.points.push(p);
                }
            },
            OutlineMode::Rectangle => match self.points.first().copied() {
                None => self.points.push(p),
                Some(a) => {
                    let r = Polygon2::rectangle(a, p);
                    self.commit(ctx, out, r.into(), false, Some(a));
                }
            },
            OutlineMode::Polygon => {
                if self.points.len() >= 3
                    && self.points.first().is_some_and(|f| near(ctx, *f, p) || f.coincides(p, &tol))
                {
                    self.close(ctx, out);
                } else if self.points.last().is_none_or(|l| !l.coincides(p, &tol)) {
                    if self.points.is_empty() {
                        self.pointer.acquire(p);
                    }
                    self.points.push(p);
                }
            }
        }
    }

    fn close(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput) {
        if self.points.len() >= 3 {
            let click = self.points.first().copied();
            let poly = Polygon2::new(self.points.clone());
            self.commit(ctx, out, poly.into(), false, click);
        }
    }

    fn preview(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput) {
        let mut d = preview::drawing(ctx);
        let mode = self.mode(ctx);
        let cursor = self.target(ctx);
        let mut live = Vec::new();
        let mut region: Option<(PolygonWithHoles, bool, Option<Point2>)> = None;
        match mode {
            OutlineMode::PickInside => {
                if let Some(raw) = self.pointer.raw {
                    match self.detect(ctx, raw) {
                        Ok(r) => region = Some((r, true, Some(raw))),
                        Err(why) => out.prompt = why,
                    }
                }
            }
            OutlineMode::Rectangle => {
                if let (Some(a), Some(c)) = (self.points.first().copied(), cursor) {
                    region = Some((Polygon2::rectangle(a, c).into(), false, Some(a)));
                    live = vec![
                        length_text((c.x - a.x).abs(), ctx.locale),
                        length_text((c.y - a.y).abs(), ctx.locale),
                    ];
                }
            }
            OutlineMode::Polygon => {
                let pts = self.polygon_with(cursor);
                if let (Some(last), Some(c)) = (self.points.last().copied(), cursor) {
                    live = segment_live(last, c, ctx.locale);
                    preview::length_label(&mut d, last, c, ctx.locale);
                }
                preview::polyline(&mut d, &pts, false, BLUE);
                if pts.len() >= 3 {
                    if let (Some(a), Some(b)) = (pts.last(), pts.first()) {
                        preview::rubber(&mut d, *a, *b);
                    }
                    region = Some((Polygon2::new(pts).into(), false, self.points.first().copied()));
                }
                if self.points.len() >= 3
                    && let (Some(f), Some(c)) = (self.points.first(), cursor)
                    && near(ctx, *f, c)
                {
                    out.prompt = tr(ctx, "tools.outline.close");
                }
            }
        }
        if let Some((r, from_walls, click)) = region
            && r.outer.is_valid(&ctx.doc.tolerance())
        {
            match self.kind(ctx, &r, from_walls, click) {
                Some(kind) => {
                    let e = preview::new_element(ctx, 0, Some(ctx.storey), kind);
                    let g = preview::ghost(ctx, &[e], &[], &[]);
                    d.extend(g.drawing);
                    out.warning = out.warning.take().or(g.problem);
                }
                None => preview::area(&mut d, &r.outer.vertices, BLUE),
            }
            if self.id != ToolId::Space
                && let Some(c) = r.outer.centroid()
            {
                preview::label(
                    &mut d,
                    c,
                    AreaDisplay(r.area(), ctx.locale).to_string(),
                    kanavos_drawing::Anchor::Centre,
                );
            }
        }
        out.preview = d;
        out.fields = self.fields.view(&live, ctx.locale);
    }

    fn prompt(&self, ctx: &ToolContext<'_>) -> String {
        let mode = self.mode(ctx);
        let key = match (mode, self.points.len()) {
            (OutlineMode::PickInside, _) => "tools.outline.pick",
            (OutlineMode::Rectangle, 0) => "tools.outline.rect_start",
            (OutlineMode::Rectangle, _) => "tools.outline.rect_end",
            (OutlineMode::Polygon, 0) => "tools.outline.start",
            (OutlineMode::Polygon, 1 | 2) => "tools.outline.next",
            (OutlineMode::Polygon, _) => "tools.outline.next_or_close",
        };
        let what = t(ctx.locale, &format!("tools.{}", self.id.key())).into_owned();
        let mut s = tf(ctx.locale, key, &[("what", &what)]);
        if let Some(n) = &self.notice {
            s = format!("{n} — {s}");
        }
        if self.id == ToolId::Roof && matches!(ctx.settings.roof.kind, RoofKind::MonoPitch | RoofKind::Gable)
        {
            s = format!("{s} · {}", tr(ctx, "tools.roof.flip_hint"));
        }
        s
    }
}

/// The edge nearest a click (ties go to the longer edge); the longest edge when there is no click.
fn primary_edge(outline: &Polygon2, click: Option<Point2>) -> usize {
    let mut best: Option<(f64, f64, usize)> = None;
    for (i, e) in outline.edges().enumerate() {
        let d = click.map(|c| e.distance_to(c).as_m()).unwrap_or(0.0);
        let len = e.length().as_m();
        let better = match best {
            None => true,
            Some((bd, bl, _)) => d < bd - 1e-6 || ((d - bd).abs() <= 1e-6 && len > bl + 1e-9),
        };
        if better {
            best = Some((d, len, i));
        }
    }
    best.map(|b| b.2).unwrap_or(0)
}

/// The two edges a gable roof slopes: for four edges, the opposite pair with the larger total length (`shift`
/// odd picks the other pair); otherwise the two longest edges.
fn gable_edges(outline: &Polygon2, shift: usize) -> (usize, usize) {
    let lens: Vec<f64> = outline.edges().map(|e| e.length().as_m()).collect();
    if lens.len() == 4 {
        let l = |i: usize| lens.get(i).copied().unwrap_or(0.0);
        let pair_a = l(0) + l(2);
        let pair_b = l(1) + l(3);
        let long_first = pair_a >= pair_b;
        return if long_first == shift.is_multiple_of(2) { (0, 2) } else { (1, 3) };
    }
    let mut idx: Vec<usize> = (0..lens.len()).collect();
    idx.sort_by(|a, b| {
        lens.get(*b).copied().unwrap_or(0.0).total_cmp(&lens.get(*a).copied().unwrap_or(0.0)).then(a.cmp(b))
    });
    let n = idx.len().max(1);
    let a = idx.get(shift % n).copied().unwrap_or(0);
    let b = idx.get((shift + 1) % n).copied().unwrap_or(0);
    (a, b)
}

/// Offsets each edge of a ring outwards by its own distance and joins the edges with mitres. The ring is made
/// counter-clockwise first; `d[i]` belongs to edge `i` of that ring. Parallel neighbours keep the offset vertex.
pub(crate) fn offset_ring(ring: &Polygon2, d: &[f64]) -> Polygon2 {
    if ring.signed_area() >= 0.0 {
        return offset_ccw(ring, d);
    }
    // Edge j of the reversed ring is edge n-2-j of the original (mod n).
    let n = ring.len().max(1);
    let dd: Vec<f64> = (0..n).map(|j| d.get((2 * n - 2 - j) % n).copied().unwrap_or(0.0)).collect();
    offset_ccw(&ring.reversed(), &dd)
}

fn offset_ccw(ring: &Polygon2, d: &[f64]) -> Polygon2 {
    let n = ring.len();
    let line = |i: usize| -> Option<Line2> {
        let e = ring.edge(i)?;
        let dir = e.direction()?;
        let out = -dir.perp();
        Some(Line2 { origin: e.start + out * d.get(i).copied().unwrap_or(0.0), direction: e.vector() })
    };
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let (Some(l0), Some(l1)) = (line(prev), line(i)) else {
            if let Some(p) = ring.vertices.get(i) {
                v.push(*p);
            }
            continue;
        };
        v.push(l0.intersection(&l1).unwrap_or(l1.origin));
    }
    Polygon2::new(v)
}

/// The outline grown from the walls' inside faces to their outside faces: each edge that lies on a wall face
/// moves out by that wall's thickness.
fn expand_to_walls(ctx: &ToolContext<'_>, ring: &Polygon2) -> Polygon2 {
    let ring = ring.to_ccw();
    let faces: Vec<(Segment2, f64)> = ctx
        .doc
        .walls_on(ctx.storey)
        .filter_map(|(id, _)| kanavos_elements::wall_layout(ctx.doc, id).ok())
        .flat_map(|l| {
            let t = l.thickness.as_m();
            let mut v = Vec::new();
            for face in [&l.left_face, &l.right_face] {
                for w in face.windows(2) {
                    if let [a, b] = w {
                        v.push((Segment2::new(*a, *b), t));
                    }
                }
            }
            v
        })
        .collect();
    let d: Vec<f64> = ring
        .edges()
        .map(|e| {
            let m = e.midpoint();
            faces
                .iter()
                .filter(|(f, _)| {
                    let (Some(a), Some(b)) = (f.direction(), e.direction()) else { return false };
                    a.cross(b).abs() < 1e-3 && f.distance_to(m).as_m() < 2e-3
                })
                .map(|(_, t)| *t)
                .fold(0.0, f64::max)
        })
        .collect();
    offset_ring(&ring, &d)
}

impl Tool for OutlineTool {
    fn id(&self) -> ToolId {
        self.id
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let mode = self.mode(ctx);
        let kinds: &[Field] = match (mode, self.points.is_empty()) {
            (OutlineMode::PickInside, _) => &[],
            (_, true) => &[Field::Point],
            (OutlineMode::Rectangle, false) => &[Field::Dx, Field::Dy],
            (OutlineMode::Polygon, false) => &[Field::Length, Field::Angle, Field::Offset],
        };
        self.fields.set(kinds);
        let mut out = base_output(&self.pointer);
        match event {
            ToolEvent::Move { at, mods } => {
                let from = self.points.last().copied();
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let from = self.points.last().copied();
                self.pointer.update(ctx, *at, *mods, from);
                if let Some(p) = self.target(ctx) {
                    self.click(ctx, &mut out, p, *at);
                }
                self.fields.clear();
            }
            ToolEvent::DoubleClick { .. } => {
                if self.mode(ctx) == OutlineMode::Polygon {
                    self.close(ctx, &mut out);
                }
            }
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Tab => self.fields.tab(),
                Key::Flip => self.shift += 1,
                Key::Backspace => {
                    if !self.fields.backspace() {
                        self.points.pop();
                        if self.points.is_empty() {
                            self.fallback = false;
                            self.notice = None;
                        }
                    }
                }
                Key::Enter => {
                    if !self.fields.is_empty() {
                        if let Some(p) = self
                            .target(ctx)
                            .filter(|_| self.fields.point_form().is_some() || !self.points.is_empty())
                        {
                            let raw = self.pointer.raw.unwrap_or(p);
                            self.click(ctx, &mut out, p, raw);
                        }
                        self.fields.clear();
                    } else if self.mode(ctx) == OutlineMode::Polygon && self.points.len() >= 3 {
                        self.close(ctx, &mut out);
                    }
                }
                Key::Escape => {
                    if self.fields.is_empty() {
                        self.reset();
                        out.finished = true;
                    } else {
                        self.fields.clear();
                    }
                }
                _ => {}
            },
            _ => {}
        }
        let mode = self.mode(ctx);
        let kinds: &[Field] = match (mode, self.points.is_empty()) {
            (OutlineMode::PickInside, _) => &[],
            (_, true) => &[Field::Point],
            (OutlineMode::Rectangle, false) => &[Field::Dx, Field::Dy],
            (OutlineMode::Polygon, false) => &[Field::Length, Field::Angle, Field::Offset],
        };
        self.fields.set(kinds);
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        out.prompt = self.prompt(ctx);
        let warning = out.warning.take();
        self.preview(ctx, &mut out);
        if warning.is_some() {
            out.warning = warning;
        }
        if self.fields.has_error() {
            out.warning = Some(tr(ctx, "tools.input.unreadable"));
        }
        out.typing = self.fields.is_shown();
        out
    }

    fn cancel(&mut self) {
        self.reset();
    }

    fn is_idle(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_units::Tolerance;

    #[test]
    fn offsets_rings_outwards_per_edge() {
        let tol = Tolerance::default();
        let sq = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 3.0)]);
        let o = offset_ring(&sq, &[0.5; 4]);
        assert!((o.area().as_m2() - 5.0 * 4.0).abs() < 1e-9);
        assert!(o.vertices[0].coincides(Point2::new(-0.5, -0.5), &tol));
        // Clockwise input gives the same result.
        let o2 = offset_ring(&sq.reversed(), &[0.5; 4]);
        assert!((o2.area().as_m2() - 20.0).abs() < 1e-9);
        // Only the south edge moves.
        let o3 = offset_ring(&sq, &[1.0, 0.0, 0.0, 0.0]);
        assert!((o3.area().as_m2() - 16.0).abs() < 1e-9);
    }

    #[test]
    fn gable_and_mono_edges() {
        let r = Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]);
        assert_eq!(gable_edges(&r, 0), (0, 2));
        assert_eq!(gable_edges(&r, 1), (1, 3));
        assert_eq!(primary_edge(&r, Some(Point2::new(9.0, 5.5))), 2);
        assert_eq!(primary_edge(&r, Some(Point2::new(0.0, 0.0))), 0);
    }
}

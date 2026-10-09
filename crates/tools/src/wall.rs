//! The wall tool (`specs/tools/wall.md`).
//!
//! - **Chain**: click a start point, then each click adds a wall from the previous point. Clicking the chain's
//!   first point again closes it; Enter, a double click or Escape ends it.
//! - **Rectangle**: two opposite corners make four walls in one undo step, running clockwise so that the
//!   build-up's first layer (its left face) is on the outside.
//! - **Arc**: start, end, then a point the arc passes through (or a typed rise); the chain continues from the end.
//!
//! Every wall is created as soon as it is drawn, so it joins its neighbours and shows in other views at once.
//! Backspace (with nothing typed) removes the chain's last wall with an undoable delete and steps back to its
//! start point. The flip key swaps the reference line side (left ↔ right face) for the walls that follow.

use kanavos_engine::commands::{CreateElement, DeleteElements};
use kanavos_geom::{Arc2, Point2, Polygon2};
use kanavos_i18n::t;
use kanavos_model::{ElementKind, ReferenceLine, Wall};

use crate::common::{
    Field, Fields, Pointer, base_output, bulge_from_rise, bulge_through, find_wall, length_text, near,
    one_step, segment_live, tr, typed_target,
};
use crate::preview::{self, Ghost};
use crate::settings::{WallMode, storey_height};
use crate::{Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput};

/// Wall tool.
#[derive(Debug, Clone, Default)]
pub struct WallTool {
    pointer: Pointer,
    fields: Fields,
    /// Points of the current chain.
    chain: Vec<Point2>,
    /// Walls created in the current chain (start, end, bulge), oldest first.
    created: Vec<(Point2, Point2, f64)>,
    /// Arc mode: the end point, waiting for a point on the arc.
    arc_end: Option<Point2>,
    /// Rectangle mode: the first corner.
    corner: Option<Point2>,
    /// Reference line side swapped.
    flip: bool,
    /// Mode of the operation in progress.
    mode: WallMode,
}

impl WallTool {
    fn wall(&self, ctx: &ToolContext<'_>, start: Point2, end: Point2, bulge: f64) -> Option<ElementKind> {
        let s = &ctx.settings.wall;
        let structure = ctx.settings.wall_structure(ctx.doc)?;
        let height = s.height.unwrap_or_else(|| storey_height(ctx.doc, ctx.storey));
        let reference = match (s.reference, self.flip) {
            (ReferenceLine::Left, true) => ReferenceLine::Right,
            (ReferenceLine::Right, true) => ReferenceLine::Left,
            (r, _) => r,
        };
        Some(ElementKind::Wall(Wall {
            start,
            end,
            bulge,
            reference,
            structure,
            height,
            base_offset: s.base_offset,
        }))
    }

    fn create(
        &mut self,
        ctx: &ToolContext<'_>,
        out: &mut ToolOutput,
        start: Point2,
        end: Point2,
        bulge: f64,
    ) -> bool {
        if start.coincides(end, &ctx.doc.tolerance()) {
            return false;
        }
        let Some(kind) = self.wall(ctx, start, end, bulge) else {
            out.warning = Some(tr(ctx, "tools.wall.no_structure"));
            return false;
        };
        out.commands.push(CreateElement::on(ctx.storey, kind).into());
        self.created.push((start, end, bulge));
        true
    }

    fn rectangle(&self, a: Point2, b: Point2) -> Vec<(Point2, Point2)> {
        let r = Polygon2::rectangle(a, b);
        // Counter-clockwise from the lower-left corner; walk it backwards to go clockwise.
        let v = &r.vertices;
        let (Some(p0), Some(p1), Some(p2), Some(p3)) = (v.first(), v.get(1), v.get(2), v.get(3)) else {
            return Vec::new();
        };
        vec![(*p0, *p3), (*p3, *p2), (*p2, *p1), (*p1, *p0)]
    }

    /// The point the next click would use: typed input, else the snapped pointer.
    fn target(&self, ctx: &ToolContext<'_>) -> Option<Point2> {
        if self.mode == WallMode::Rectangle
            && let Some(a) = self.corner
        {
            if let Some(p) = self.fields.point_form() {
                return p.to_point(Some(a), &ctx.doc.site().georef);
            }
            return self.pointer.point().map(|c| self.rect_corner(a, c));
        }
        let from = self.from();
        typed_target(&self.fields, from, self.pointer.point(), ctx.doc).or_else(|| self.pointer.point())
    }

    /// The opposite rectangle corner: typed X/Y extents towards the pointer, else the pointer.
    fn rect_corner(&self, a: Point2, cursor: Point2) -> Point2 {
        let (dx, dy) = (self.fields.length(Field::Dx), self.fields.length(Field::Dy));
        let sx = if cursor.x >= a.x { 1.0 } else { -1.0 };
        let sy = if cursor.y >= a.y { 1.0 } else { -1.0 };
        let x = dx.map(|v| a.x.as_m() + sx * v.as_m()).unwrap_or(cursor.x.as_m());
        let y = dy.map(|v| a.y.as_m() + sy * v.as_m()).unwrap_or(cursor.y.as_m());
        Point2::new(x, y)
    }

    fn from(&self) -> Option<Point2> {
        match self.mode {
            WallMode::Rectangle => self.corner,
            _ => self.arc_end.or(self.chain.last().copied()),
        }
    }

    fn fields_for_state(&mut self) {
        let kinds: &[Field] = match (self.mode, self.from()) {
            (WallMode::Rectangle, Some(_)) => &[Field::Dx, Field::Dy],
            (WallMode::Arc, Some(_)) if self.arc_end.is_some() => &[Field::Rise],
            (_, Some(_)) => &[Field::Length, Field::Angle, Field::Offset],
            (_, None) => &[Field::Point],
        };
        self.fields.set(kinds);
    }

    /// Handles a click (or Enter with typed input) at `p`.
    fn click(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, p: Point2) {
        let tol = ctx.doc.tolerance();
        match self.mode {
            WallMode::Chain => {
                let Some(last) = self.chain.last().copied() else {
                    self.chain.push(p);
                    self.pointer.acquire(p);
                    return;
                };
                if p.coincides(last, &tol) {
                    return;
                }
                // Within the snap radius of the chain's first point, the chain closes exactly on it.
                let first = self
                    .chain
                    .first()
                    .copied()
                    .filter(|f| self.chain.len() >= 2 && (near(ctx, *f, p) || f.coincides(p, &tol)));
                let closing = first.is_some();
                let p = first.unwrap_or(p);
                if self.create(ctx, out, last, p, 0.0) {
                    if closing {
                        self.finish(out);
                    } else {
                        self.chain.push(p);
                    }
                }
            }
            WallMode::Arc => {
                let Some(start) = self.chain.last().copied() else {
                    self.chain.push(p);
                    return;
                };
                match self.arc_end {
                    None => {
                        if !p.coincides(start, &tol) {
                            self.arc_end = Some(p);
                        }
                    }
                    Some(end) => {
                        let bulge = self.arc_bulge(ctx, start, end, p);
                        if bulge.abs() > 1.0 + 1e-9 {
                            out.warning = Some(tr(ctx, "tools.wall.arc_too_deep"));
                            return;
                        }
                        if self.create(ctx, out, start, end, bulge) {
                            self.chain.push(end);
                            self.arc_end = None;
                        }
                    }
                }
            }
            WallMode::Rectangle => match self.corner {
                None => self.corner = Some(p),
                Some(a) => {
                    let sides = self.rectangle(a, p);
                    let degenerate = (a.x - p.x).abs().as_m() <= tol.length.as_m()
                        || (a.y - p.y).abs().as_m() <= tol.length.as_m();
                    if degenerate {
                        return;
                    }
                    let mut cmds = Vec::new();
                    for (s, e) in sides {
                        if let Some(kind) = self.wall(ctx, s, e, 0.0) {
                            cmds.push(CreateElement::on(ctx.storey, kind).into());
                        }
                    }
                    out.commands.extend(one_step(cmds, tr(ctx, "tools.history.wall_rectangle")));
                    self.corner = None;
                    out.finished = true;
                }
            },
        }
    }

    fn arc_bulge(&self, ctx: &ToolContext<'_>, start: Point2, end: Point2, through: Point2) -> f64 {
        match self.fields.length(Field::Rise) {
            Some(rise) => bulge_from_rise(start, end, rise.as_m(), self.pointer.point().unwrap_or(through)),
            None => {
                let _ = ctx;
                bulge_through(start, through, end)
            }
        }
    }

    fn finish(&mut self, out: &mut ToolOutput) {
        self.reset();
        out.finished = true;
    }

    fn reset(&mut self) {
        self.chain.clear();
        self.created.clear();
        self.arc_end = None;
        self.corner = None;
        self.fields.clear();
        self.pointer.unlock();
    }

    /// Backspace with nothing typed: undo the last step of the operation.
    fn step_back(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput) {
        if self.corner.take().is_some() {
            return;
        }
        if self.arc_end.take().is_some() {
            return;
        }
        match self.created.pop() {
            Some((s, e, b)) => {
                if let Some(id) = find_wall(ctx.doc, ctx.storey, s, e, b) {
                    out.commands.push(DeleteElements { ids: vec![id] }.into());
                }
                self.chain.pop();
                if self.chain.is_empty() {
                    self.chain.push(s);
                }
            }
            None => {
                self.chain.clear();
            }
        }
    }

    fn preview(&self, ctx: &ToolContext<'_>, out: &mut ToolOutput) {
        let mut d = preview::drawing(ctx);
        let Some(cursor) = self.target(ctx) else { return };
        let mut walls: Vec<(Point2, Point2, f64)> = Vec::new();
        let mut live = Vec::new();
        match self.mode {
            WallMode::Chain => {
                if let Some(last) = self.chain.last().copied() {
                    walls.push((last, cursor, 0.0));
                    preview::length_label(&mut d, last, cursor, ctx.locale);
                    live = segment_live(last, cursor, ctx.locale);
                }
            }
            WallMode::Arc => match (self.chain.last().copied(), self.arc_end) {
                (Some(start), None) => {
                    preview::rubber(&mut d, start, cursor);
                    preview::length_label(&mut d, start, cursor, ctx.locale);
                    live = segment_live(start, cursor, ctx.locale);
                }
                (Some(start), Some(end)) => {
                    let through = self.pointer.point().unwrap_or(cursor);
                    let b = self.arc_bulge(ctx, start, end, through);
                    walls.push((start, end, b.clamp(-1.0, 1.0)));
                    let rise = Arc2::from_bulge(start, end, b)
                        .map(|a| a.at(0.5).distance(start.midpoint(end)))
                        .unwrap_or_default();
                    live = vec![length_text(rise, ctx.locale)];
                    if b.abs() > 1.0 + 1e-9 {
                        out.warning = Some(tr(ctx, "tools.wall.arc_too_deep"));
                    }
                }
                _ => {}
            },
            WallMode::Rectangle => {
                if let Some(a) = self.corner {
                    let p = cursor;
                    walls.extend(self.rectangle(a, p).into_iter().map(|(s, e)| (s, e, 0.0)));
                    live = vec![
                        length_text((p.x - a.x).abs(), ctx.locale),
                        length_text((p.y - a.y).abs(), ctx.locale),
                    ];
                }
            }
        }
        let kinds: Vec<_> = walls
            .iter()
            .filter(|(s, e, _)| !s.coincides(*e, &ctx.doc.tolerance()))
            .filter_map(|(s, e, b)| self.wall(ctx, *s, *e, *b))
            .enumerate()
            .map(|(i, k)| preview::new_element(ctx, i as u64, Some(ctx.storey), k))
            .collect();
        if !kinds.is_empty() {
            let context: Vec<_> = nearby_walls(ctx, &walls);
            let Ghost { drawing, problem } = preview::ghost(ctx, &kinds, &context, &[]);
            d.extend(drawing);
            if out.warning.is_none() {
                out.warning = problem;
            }
        }
        if self.mode != WallMode::Rectangle
            && self.chain.len() >= 2
            && let Some(first) = self.chain.first()
            && near(ctx, *first, cursor)
        {
            out.prompt = tr(ctx, "tools.wall.close");
        }
        out.preview = d;
        out.fields = self.fields.view(&live, ctx.locale);
    }
}

/// Walls near the ends of the previewed walls, so the ghost joins them as the real wall will.
fn nearby_walls(ctx: &ToolContext<'_>, walls: &[(Point2, Point2, f64)]) -> Vec<kanavos_model::Id> {
    let m = 1.0;
    let mut out = Vec::new();
    for (id, w) in ctx.doc.walls_on(ctx.storey) {
        let close = walls.iter().any(|(s, e, _)| {
            [*s, *e].iter().any(|p| [w.start, w.end].iter().any(|q| q.distance(*p).as_m() <= m))
        });
        if close {
            out.push(id);
            if out.len() >= 16 {
                break;
            }
        }
    }
    out
}

impl Tool for WallTool {
    fn id(&self) -> ToolId {
        ToolId::Wall
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let mode = ctx.settings.wall.mode;
        if mode != self.mode {
            self.reset();
            self.mode = mode;
        }
        self.fields_for_state();
        let mut out = base_output(&self.pointer);
        match event {
            ToolEvent::Move { at, mods } => {
                let from = self.from();
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let from = self.from();
                self.pointer.update(ctx, *at, *mods, from);
                if let Some(p) = self.target(ctx) {
                    self.click(ctx, &mut out, p);
                }
                self.fields.clear();
            }
            ToolEvent::DoubleClick { .. } => {
                if self.mode != WallMode::Rectangle {
                    self.finish(&mut out);
                }
            }
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Tab => self.fields.tab(),
                Key::Flip => self.flip = !self.flip,
                Key::Backspace => {
                    if !self.fields.backspace() {
                        self.step_back(ctx, &mut out);
                    }
                }
                Key::Enter => {
                    if !self.fields.is_empty() {
                        let usable = self.fields.point_form().is_some() || self.from().is_some();
                        if usable && let Some(p) = self.target(ctx) {
                            self.click(ctx, &mut out, p);
                        }
                        self.fields.clear();
                    } else {
                        self.finish(&mut out);
                    }
                }
                Key::Escape => {
                    if self.fields.is_empty() {
                        self.finish(&mut out);
                    } else {
                        self.fields.clear();
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.fields_for_state();
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        out.prompt = t(
            ctx.locale,
            match (self.mode, self.from(), self.arc_end) {
                (WallMode::Rectangle, None, _) => "tools.wall.rect_start",
                (WallMode::Rectangle, Some(_), _) => "tools.wall.rect_end",
                (WallMode::Arc, Some(_), None) => "tools.wall.arc_end",
                (WallMode::Arc, Some(_), Some(_)) => "tools.wall.arc_through",
                (_, None, _) => "tools.wall.start",
                _ => "tools.wall.next",
            },
        )
        .into_owned();
        if self.fields.has_error() {
            out.warning = Some(tr(ctx, "tools.input.unreadable"));
        }
        self.preview(ctx, &mut out);
        out.typing = self.fields.is_shown();
        out
    }

    fn cancel(&mut self) {
        self.reset();
    }

    fn is_idle(&self) -> bool {
        self.chain.is_empty() && self.corner.is_none() && self.arc_end.is_none()
    }
}

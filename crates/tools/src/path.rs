//! Tools that draw along points: beams and lines (chained segments), polylines and railings (one element along a
//! path), grid lines (two points with automatic labels), and arcs and circles (`specs/tools/placement.md`,
//! `specs/tools/annotation.md`).

use kanavos_engine::commands::{CreateElement, DeleteElements};
use kanavos_geom::{Arc2, Point2};
use kanavos_i18n::tf;
use kanavos_model::{Beam, DraftShape, Drafting, ElementKind, GridLine, Railing};
use kanavos_units::Length;

use crate::common::{
    Field, Fields, Pointer, arc_through, base_output, bulge_from_rise, find_element, length_text, near,
    segment_live, tr, typed_target,
};
use crate::preview;
use crate::settings::{railing_material, storey_height};
use crate::{Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    /// Each click after the first adds one element (beams, lines).
    Segments,
    /// One element along all points (polylines, railings).
    Path,
    /// One element per two clicks (grid lines).
    Two,
}

/// Beam, line, polyline, railing and grid-line tool.
#[derive(Debug, Clone)]
pub struct PathTool {
    id: ToolId,
    pointer: Pointer,
    fields: Fields,
    points: Vec<Point2>,
    /// Segments created in this chain, oldest first (for Backspace).
    created: Vec<(Point2, Point2)>,
}

impl PathTool {
    /// A tool for beams, lines, polylines, railings or grid lines.
    pub fn new(id: ToolId) -> PathTool {
        PathTool {
            id,
            pointer: Pointer::default(),
            fields: Fields::default(),
            points: Vec::new(),
            created: Vec::new(),
        }
    }

    fn style(&self) -> Style {
        match self.id {
            ToolId::Beam | ToolId::Line => Style::Segments,
            ToolId::Grid => Style::Two,
            _ => Style::Path,
        }
    }

    fn segment_kind(&self, ctx: &ToolContext<'_>, a: Point2, b: Point2) -> Option<ElementKind> {
        match self.id {
            ToolId::Beam => {
                let s = &ctx.settings.beam;
                Some(ElementKind::Beam(Beam {
                    start: a,
                    end: b,
                    profile: s.profile,
                    material: ctx.settings.beam_material(ctx.doc)?,
                    top_offset: s.top_offset.unwrap_or_else(|| storey_height(ctx.doc, ctx.storey)),
                }))
            }
            ToolId::Grid => {
                Some(ElementKind::Grid(GridLine { label: grid_label(ctx, a, b), start: a, end: b }))
            }
            _ => Some(ElementKind::Drafting(Drafting {
                shape: DraftShape::Line { a, b },
                style: ctx.settings.draft.clone(),
            })),
        }
    }

    fn path_kind(&self, ctx: &ToolContext<'_>, pts: &[Point2], closed: bool) -> Option<ElementKind> {
        match self.id {
            ToolId::Railing => {
                let s = &ctx.settings.railing;
                let mut path = pts.to_vec();
                if closed && let Some(f) = pts.first() {
                    path.push(*f);
                }
                Some(ElementKind::Railing(Railing {
                    path,
                    height: s.height,
                    base_offset: Length::ZERO,
                    post_spacing: s.post_spacing,
                    infill: s.infill,
                    material: railing_material(ctx.doc)?,
                }))
            }
            _ => Some(ElementKind::Drafting(Drafting {
                shape: DraftShape::Polyline { points: pts.to_vec(), closed },
                style: ctx.settings.draft.clone(),
            })),
        }
    }

    fn storey(&self, ctx: &ToolContext<'_>) -> Option<kanavos_model::Id> {
        if self.id == ToolId::Grid { None } else { Some(ctx.storey) }
    }

    fn create(&self, ctx: &ToolContext<'_>, out: &mut ToolOutput, kind: Option<ElementKind>) -> bool {
        let Some(kind) = kind else {
            out.warning = Some(tr(ctx, "tools.place.no_material"));
            return false;
        };
        let e = preview::new_element(ctx, 0, self.storey(ctx), kind.clone());
        if let Err(err) = kanavos_elements::validate(ctx.doc, &e) {
            out.warning = Some(preview::explain(&err, ctx.locale));
            return false;
        }
        let mut c = CreateElement::on(ctx.storey, kind);
        c.storey = self.storey(ctx);
        out.commands.push(c.into());
        true
    }

    fn click(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, p: Point2) {
        let tol = ctx.doc.tolerance();
        let Some(last) = self.points.last().copied() else {
            self.points.push(p);
            self.pointer.acquire(p);
            return;
        };
        if last.coincides(p, &tol) {
            return;
        }
        match self.style() {
            Style::Segments => {
                if self.create(ctx, out, self.segment_kind(ctx, last, p)) {
                    self.created.push((last, p));
                    self.points.push(p);
                }
            }
            Style::Two => {
                if self.create(ctx, out, self.segment_kind(ctx, last, p)) {
                    self.points.clear();
                    out.finished = true;
                }
            }
            Style::Path => {
                let closing = self.points.len() >= 3 && self.points.first().is_some_and(|f| near(ctx, *f, p));
                if closing {
                    self.commit_path(ctx, out, true);
                } else {
                    self.points.push(p);
                }
            }
        }
    }

    fn commit_path(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, closed: bool) {
        if self.points.len() < 2 {
            return;
        }
        let pts = self.points.clone();
        if self.create(ctx, out, self.path_kind(ctx, &pts, closed)) {
            self.reset();
            out.finished = true;
        }
    }

    fn finish(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput) {
        match self.style() {
            Style::Path => self.commit_path(ctx, out, false),
            _ => {
                self.reset();
                out.finished = true;
            }
        }
    }

    fn reset(&mut self) {
        self.points.clear();
        self.created.clear();
        self.fields.clear();
        self.pointer.unlock();
    }

    fn step_back(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput) {
        if self.style() == Style::Segments
            && let Some((a, b)) = self.created.pop()
        {
            let found = find_element(ctx.doc, Some(ctx.storey), |k| match k {
                ElementKind::Beam(x) => {
                    x.start.coincides(a, &ctx.doc.tolerance()) && x.end.coincides(b, &ctx.doc.tolerance())
                }
                ElementKind::Drafting(Drafting { shape: DraftShape::Line { a: x, b: y }, .. }) => {
                    x.coincides(a, &ctx.doc.tolerance()) && y.coincides(b, &ctx.doc.tolerance())
                }
                _ => false,
            });
            if let Some(id) = found {
                out.commands.push(DeleteElements { ids: vec![id] }.into());
            }
            self.points.pop();
            return;
        }
        self.points.pop();
    }
}

impl Tool for PathTool {
    fn id(&self) -> ToolId {
        self.id
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let kinds: &[Field] = if self.points.is_empty() {
            &[Field::Point]
        } else {
            &[Field::Length, Field::Angle, Field::Offset]
        };
        self.fields.set(kinds);
        let mut out = base_output(&self.pointer);
        let from = self.points.last().copied();
        match event {
            ToolEvent::Move { at, mods } => {
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                self.pointer.update(ctx, *at, *mods, from);
                if let Some(p) =
                    typed_target(&self.fields, from, self.pointer.point(), ctx.doc).or(self.pointer.point())
                {
                    self.click(ctx, &mut out, p);
                }
                self.fields.clear();
            }
            ToolEvent::DoubleClick { .. } => self.finish(ctx, &mut out),
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Tab => self.fields.tab(),
                Key::Backspace => {
                    if !self.fields.backspace() {
                        self.step_back(ctx, &mut out);
                    }
                }
                Key::Enter => {
                    if self.fields.is_empty() {
                        self.finish(ctx, &mut out);
                    } else {
                        if let Some(p) = typed_target(&self.fields, from, self.pointer.point(), ctx.doc) {
                            self.click(ctx, &mut out, p);
                        }
                        self.fields.clear();
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
        let kinds: &[Field] = if self.points.is_empty() {
            &[Field::Point]
        } else {
            &[Field::Length, Field::Angle, Field::Offset]
        };
        self.fields.set(kinds);
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        let what = tr(ctx, &format!("tools.{}", self.id.key()));
        out.prompt = match (self.style(), self.points.len()) {
            (_, 0) => tf(ctx.locale, "tools.path.start", &[("what", &what)]),
            (Style::Two, _) => tf(ctx.locale, "tools.path.end", &[("what", &what)]),
            (Style::Segments, _) => tf(ctx.locale, "tools.path.next", &[("what", &what)]),
            (Style::Path, _) => tf(ctx.locale, "tools.path.next_or_finish", &[("what", &what)]),
        };
        let from = self.points.last().copied();
        let cursor = typed_target(&self.fields, from, self.pointer.point(), ctx.doc).or(self.pointer.point());
        let mut d = preview::drawing(ctx);
        let mut live = Vec::new();
        if let (Some(last), Some(c)) = (from, cursor)
            && !last.coincides(c, &ctx.doc.tolerance())
        {
            live = segment_live(last, c, ctx.locale);
            preview::length_label(&mut d, last, c, ctx.locale);
            let kind = match self.style() {
                Style::Path => {
                    let mut pts = self.points.clone();
                    pts.push(c);
                    self.path_kind(ctx, &pts, false)
                }
                _ => self.segment_kind(ctx, last, c),
            };
            if let Some(kind) = kind {
                let e = preview::new_element(ctx, 0, self.storey(ctx), kind);
                let g = preview::ghost(ctx, &[e], &[], &[]);
                d.extend(g.drawing);
                out.warning = out.warning.take().or(g.problem);
            }
            if self.style() == Style::Path
                && self.points.len() >= 3
                && let Some(f) = self.points.first()
                && near(ctx, *f, c)
            {
                out.prompt = tr(ctx, "tools.outline.close");
            }
        }
        out.preview = d;
        out.fields = self.fields.view(&live, ctx.locale);
        if self.fields.has_error() {
            out.warning = Some(tr(ctx, "tools.input.unreadable"));
        }
        out.typing = true;
        out
    }

    fn cancel(&mut self) {
        self.reset();
    }

    fn is_idle(&self) -> bool {
        self.points.is_empty()
    }
}

/// Bijective base-26 letters: 0 → A, 25 → Z, 26 → AA.
fn letters(mut i: u32) -> String {
    let mut s = Vec::new();
    loop {
        s.push(char::from(b'A' + (i % 26) as u8));
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.iter().rev().collect()
}

/// Inverse of [`letters`] for upper-case Latin labels.
fn letter_index(s: &str) -> Option<u32> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let mut v: u32 = 0;
    for c in s.chars() {
        v = v.checked_mul(26)?.checked_add(u32::from(c as u8 - b'A') + 1)?;
    }
    Some(v - 1)
}

/// The label of a new grid line: the setting when given; otherwise the next letter (A, B, C…) for lines running
/// mostly east–west and the next number (1, 2, 3…) for lines running mostly north–south, after the highest
/// label of that family in the project.
fn grid_label(ctx: &ToolContext<'_>, a: Point2, b: Point2) -> String {
    if let Some(l) = ctx.settings.grid.label.as_ref().filter(|l| !l.trim().is_empty()) {
        return l.clone();
    }
    let v = b - a;
    let horizontal = v.x.abs() > v.y.abs();
    let labels = ctx.doc.elements().filter_map(|e| match &e.kind {
        ElementKind::Grid(g) => Some(g.label.trim().to_owned()),
        _ => None,
    });
    if horizontal {
        let next = labels.filter_map(|l| letter_index(&l)).max().map(|i| i + 1).unwrap_or(0);
        letters(next)
    } else {
        let next = labels.filter_map(|l| l.parse::<u32>().ok()).max().unwrap_or(0) + 1;
        next.to_string()
    }
}

/// Arc (start, end, a point on the arc) and circle (centre, radius) tool.
#[derive(Debug, Clone)]
pub struct CurveTool {
    id: ToolId,
    pointer: Pointer,
    fields: Fields,
    points: Vec<Point2>,
}

impl CurveTool {
    /// An arc or circle tool.
    pub fn new(id: ToolId) -> CurveTool {
        CurveTool { id, pointer: Pointer::default(), fields: Fields::default(), points: Vec::new() }
    }

    fn kinds(&self) -> &'static [Field] {
        match (self.id, self.points.len()) {
            (_, 0) => &[Field::Point],
            (ToolId::Circle, _) => &[Field::Radius],
            (_, 1) => &[Field::Length, Field::Angle, Field::Offset],
            _ => &[Field::Rise],
        }
    }

    /// The shape for the points so far and the pointer at `c`.
    fn shape(&self, c: Point2) -> Option<DraftShape> {
        match (self.id, self.points.as_slice()) {
            (ToolId::Circle, [centre]) => {
                let r = self.fields.length(Field::Radius).unwrap_or_else(|| centre.distance(c));
                (r.as_m() > 1e-6).then_some(DraftShape::Circle { centre: *centre, radius: r })
            }
            (ToolId::Arc, [s]) => Some(DraftShape::Line { a: *s, b: c }),
            (ToolId::Arc, [s, e]) => {
                let arc = match self.fields.length(Field::Rise) {
                    Some(rise) => Arc2::from_bulge(*s, *e, bulge_from_rise(*s, *e, rise.as_m(), c)),
                    None => arc_through(*s, c, *e),
                };
                arc.map(|arc| DraftShape::Arc { arc })
            }
            _ => None,
        }
    }

    fn target(&self, ctx: &ToolContext<'_>) -> Option<Point2> {
        let from = self.points.last().copied();
        let typed = if self.id == ToolId::Arc && self.points.len() == 1 {
            typed_target(&self.fields, from, self.pointer.point(), ctx.doc)
        } else if self.points.is_empty() {
            self.fields.point_form().and_then(|p| p.to_point(None, &ctx.doc.site().georef))
        } else {
            None
        };
        typed.or(self.pointer.point())
    }

    fn click(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, p: Point2) {
        let complete = matches!((self.id, self.points.len()), (ToolId::Circle, 1) | (ToolId::Arc, 2));
        if !complete {
            if self.points.last().is_none_or(|l| !l.coincides(p, &ctx.doc.tolerance())) {
                self.points.push(p);
            }
            return;
        }
        match self.shape(p) {
            Some(shape) => {
                let kind = ElementKind::Drafting(Drafting { shape, style: ctx.settings.draft.clone() });
                out.commands.push(CreateElement::on(ctx.storey, kind).into());
                out.finished = true;
                self.points.clear();
            }
            None => out.warning = Some(tr(ctx, "tools.curve.degenerate")),
        }
    }
}

impl Tool for CurveTool {
    fn id(&self) -> ToolId {
        self.id
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        self.fields.set(self.kinds());
        let mut out = base_output(&self.pointer);
        let from = self.points.last().copied();
        match event {
            ToolEvent::Move { at, mods } => {
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                self.pointer.update(ctx, *at, *mods, from);
                if let Some(p) = self.target(ctx) {
                    self.click(ctx, &mut out, p);
                }
                self.fields.clear();
            }
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Tab => self.fields.tab(),
                Key::Backspace => {
                    if !self.fields.backspace() {
                        self.points.pop();
                    }
                }
                Key::Enter => {
                    if !self.fields.is_empty()
                        && let Some(p) = self.target(ctx)
                    {
                        self.click(ctx, &mut out, p);
                    }
                    self.fields.clear();
                }
                Key::Escape => {
                    if !self.fields.is_empty() {
                        self.fields.clear();
                    } else {
                        self.points.clear();
                        out.finished = true;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.fields.set(self.kinds());
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        out.prompt = tr(
            ctx,
            match (self.id, self.points.len()) {
                (ToolId::Circle, 0) => "tools.circle.centre",
                (ToolId::Circle, _) => "tools.circle.radius",
                (_, 0) => "tools.arc.start",
                (_, 1) => "tools.arc.end",
                _ => "tools.arc.through",
            },
        );
        let mut d = preview::drawing(ctx);
        let mut live = Vec::new();
        if let Some(c) = self.target(ctx)
            && let Some(shape) = self.shape(c)
        {
            {
                match &shape {
                    DraftShape::Circle { centre, radius } => {
                        live = vec![length_text(*radius, ctx.locale)];
                        preview::rubber(&mut d, *centre, c);
                    }
                    DraftShape::Line { a, b } => {
                        live = segment_live(*a, *b, ctx.locale);
                        preview::length_label(&mut d, *a, *b, ctx.locale);
                    }
                    DraftShape::Arc { arc } => {
                        if let [s, e] = self.points.as_slice() {
                            live = vec![length_text(arc.at(0.5).distance(s.midpoint(*e)), ctx.locale)];
                        }
                    }
                    _ => {}
                }
                let kind = ElementKind::Drafting(Drafting { shape, style: ctx.settings.draft.clone() });
                let e = preview::new_element(ctx, 0, Some(ctx.storey), kind);
                d.extend(preview::ghost(ctx, &[e], &[], &[]).drawing);
            }
        }
        out.preview = d;
        out.fields = self.fields.view(&live, ctx.locale);
        out.typing = true;
        out
    }

    fn cancel(&mut self) {
        self.points.clear();
        self.fields.clear();
    }

    fn is_idle(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_labels_round_trip() {
        assert_eq!(letters(0), "A");
        assert_eq!(letters(25), "Z");
        assert_eq!(letters(26), "AA");
        assert_eq!(letters(27), "AB");
        for i in [0, 5, 25, 26, 51, 52, 700] {
            assert_eq!(letter_index(&letters(i)), Some(i));
        }
        assert_eq!(letter_index("a"), None);
    }
}

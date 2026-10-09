//! The measure tool: click points to read distances along a chain; close it on its first point (or press Enter)
//! to read the enclosed area. It creates nothing (`specs/tools/outlines.md`).

use kanavos_geom::{Point2, Polygon2};
use kanavos_i18n::{t, tf};
use kanavos_units::{AreaDisplay, Length};

use crate::common::{Pointer, angle_text, base_output, length_text, near};
use crate::preview::{self, BLUE};
use crate::{InputField, Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput};

/// Measure tool.
#[derive(Debug, Clone, Default)]
pub struct MeasureTool {
    pointer: Pointer,
    points: Vec<Point2>,
    closed: bool,
}

fn chain_length(pts: &[Point2]) -> Length {
    pts.windows(2).map(|w| if let [a, b] = w { a.distance(*b) } else { Length::ZERO }).sum()
}

impl Tool for MeasureTool {
    fn id(&self) -> ToolId {
        ToolId::Measure
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let mut out = base_output(&self.pointer);
        let from = if self.closed { None } else { self.points.last().copied() };
        match event {
            ToolEvent::Move { at, mods } => {
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let s = self.pointer.update(ctx, *at, *mods, from);
                if self.closed {
                    self.points.clear();
                    self.closed = false;
                }
                if self.points.len() >= 3 && self.points.first().is_some_and(|f| near(ctx, *f, s.point)) {
                    self.closed = true;
                } else if self.points.last().is_none_or(|l| !l.coincides(s.point, &ctx.doc.tolerance())) {
                    self.points.push(s.point);
                }
            }
            ToolEvent::DoubleClick { .. } => {
                if self.points.len() >= 3 {
                    self.closed = true;
                }
            }
            ToolEvent::Key { key, .. } => match key {
                Key::Enter => {
                    if self.points.len() >= 3 {
                        self.closed = true;
                    }
                }
                Key::Backspace => {
                    self.closed = false;
                    self.points.pop();
                }
                Key::Escape => {
                    if self.points.is_empty() {
                        out.finished = true;
                    }
                    self.points.clear();
                    self.closed = false;
                }
                _ => {}
            },
            _ => {}
        }
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        let mut pts = self.points.clone();
        let cursor = self.pointer.point();
        if !self.closed
            && let Some(c) = cursor
            && pts.last().is_none_or(|l| !l.coincides(c, &ctx.doc.tolerance()))
            && !pts.is_empty()
        {
            pts.push(c);
        }
        let mut d = preview::drawing(ctx);
        preview::polyline(&mut d, &pts, self.closed, BLUE);
        for p in &self.points {
            preview::mark(&mut d, *p);
        }
        let field = |key: &str, value: String| InputField {
            label: t(ctx.locale, key).into_owned(),
            value,
            active: false,
            locked: false,
        };
        let mut fields = Vec::new();
        if let [.., a, b] = pts.as_slice() {
            preview::length_label(&mut d, *a, *b, ctx.locale);
            fields.push(field("tools.measure.segment", length_text(a.distance(*b), ctx.locale)));
            fields.push(field("tools.field.angle", angle_text((*b - *a).angle().as_deg(), ctx.locale)));
        }
        let mut total = chain_length(&pts);
        if self.closed
            && let (Some(a), Some(b)) = (pts.last(), pts.first())
        {
            total += a.distance(*b);
        }
        if pts.len() >= 2 {
            fields.push(field("tools.measure.total", length_text(total, ctx.locale)));
        }
        let mut prompt =
            t(ctx.locale, if self.points.is_empty() { "tools.measure.start" } else { "tools.measure.next" })
                .into_owned();
        if pts.len() >= 3 {
            let poly = Polygon2::new(pts.clone());
            let area = AreaDisplay(poly.area(), ctx.locale).to_string();
            fields.push(field("tools.measure.area", area.clone()));
            if self.closed {
                preview::area(&mut d, &pts, BLUE);
                prompt = tf(
                    ctx.locale,
                    "tools.measure.result",
                    &[("area", &area), ("perimeter", &length_text(total, ctx.locale))],
                );
            }
        }
        out.preview = d;
        out.fields = fields;
        out.prompt = prompt;
        out
    }

    fn cancel(&mut self) {
        self.points.clear();
        self.closed = false;
    }

    fn is_idle(&self) -> bool {
        self.points.is_empty()
    }
}

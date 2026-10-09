//! The selection tool: click, shift/ctrl-click, box selection, drag to move, Delete to remove.

use std::collections::BTreeSet;

use kanavos_drawing::{Drawing, Item, LinePattern, Rgba, Stroke, pt, weights};
use kanavos_engine::commands::{DeleteElements, TransformElements, TransformSpec};
use kanavos_geom::{Bbox2, Point2};
use kanavos_i18n::t;

use crate::{CursorHint, Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput, hit_test};

#[derive(Debug, Clone, Default)]
enum State {
    #[default]
    Idle,
    /// Button down on empty space: box selection.
    Box { from: Point2, to: Point2 },
    /// Button down on a selected element: moving.
    Drag { from: Point2, to: Point2 },
}

/// Selection tool.
#[derive(Debug, Clone, Default)]
pub struct SelectTool {
    state: State,
}

impl Tool for SelectTool {
    fn id(&self) -> ToolId {
        ToolId::Select
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let mut out = ToolOutput {
            prompt: t(ctx.locale, "tools.select.prompt").into_owned(),
            cursor: CursorHint::Arrow,
            ..ToolOutput::default()
        };
        let radius = 6.0 * ctx.pixel_size;
        match event {
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let hits = hit_test(ctx.doc, ctx.storey, *at, radius);
                match hits.first() {
                    Some(id) if ctx.selection.contains(id) && !mods.shift && !mods.ctrl => {
                        self.state = State::Drag { from: *at, to: *at };
                    }
                    Some(id) => {
                        let mut sel =
                            if mods.shift || mods.ctrl { ctx.selection.clone() } else { BTreeSet::new() };
                        if mods.ctrl && sel.contains(id) {
                            sel.remove(id);
                        } else {
                            sel.insert(*id);
                        }
                        out.selection = Some(sel);
                        self.state = State::Drag { from: *at, to: *at };
                    }
                    None => {
                        if !mods.shift && !mods.ctrl {
                            out.selection = Some(BTreeSet::new());
                        }
                        self.state = State::Box { from: *at, to: *at };
                    }
                }
            }
            ToolEvent::Move { at, .. } => match &mut self.state {
                State::Box { from, to } => {
                    *to = *at;
                    out.preview = box_preview(*from, *to);
                }
                State::Drag { from, to } => {
                    *to = *at;
                    out.cursor = CursorHint::Move;
                    let v = *to - *from;
                    out.prompt = format!("Δx {:.2}  Δy {:.2}", v.x, v.y);
                }
                State::Idle => {
                    if !hit_test(ctx.doc, ctx.storey, *at, radius).is_empty() {
                        out.cursor = CursorHint::Grab;
                    }
                }
            },
            ToolEvent::Release { at, button: MouseButton::Primary, mods } => {
                match std::mem::take(&mut self.state) {
                    State::Box { from, .. } => {
                        if let Some(b) = Bbox2::from_points([from, *at])
                            && b.width().as_m() > radius
                        {
                            let mut sel =
                                if mods.shift || mods.ctrl { ctx.selection.clone() } else { BTreeSet::new() };
                            for e in ctx.doc.elements_on(ctx.storey) {
                                if let Ok(Some(fp)) = kanavos_elements::footprint(ctx.doc, e.id)
                                    && fp.outer.vertices.iter().all(|q| b.contains(*q))
                                {
                                    sel.insert(e.id);
                                }
                            }
                            out.selection = Some(sel);
                        }
                    }
                    State::Drag { from, .. } => {
                        let v = *at - from;
                        if v.length() > radius && !ctx.selection.is_empty() {
                            out.commands.push(
                                TransformElements {
                                    ids: ctx.selection.iter().copied().collect(),
                                    transform: TransformSpec::Move { dx: v.x, dy: v.y },
                                    copy: mods.ctrl,
                                    count: 1,
                                }
                                .into(),
                            );
                        }
                    }
                    State::Idle => {}
                }
            }
            ToolEvent::Key { key: Key::Delete, .. } if !ctx.selection.is_empty() => {
                out.commands.push(DeleteElements { ids: ctx.selection.iter().copied().collect() }.into());
                out.selection = Some(BTreeSet::new());
            }
            ToolEvent::Key { key: Key::Escape, .. } => {
                self.state = State::Idle;
                out.selection = Some(BTreeSet::new());
            }
            _ => {}
        }
        out
    }

    fn cancel(&mut self) {
        self.state = State::Idle;
    }

    fn is_idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }
}

fn box_preview(a: Point2, b: Point2) -> Drawing {
    let mut d = Drawing::model(1.0);
    let [x0, y0] = a.xy();
    let [x1, y1] = b.xy();
    d.push(Item::polyline(
        vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)],
        true,
        Stroke::patterned(weights::FINE, LinePattern::Dashed).with_color(Rgba::rgb(40, 110, 220)),
        "tool.box",
    ));
    d
}

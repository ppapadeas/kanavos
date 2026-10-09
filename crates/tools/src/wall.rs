//! The wall tool: click a start point, then each next point adds a wall; Enter, Escape or a double click ends
//! the chain. Typing a number fixes the next wall's length along the pointer direction.

use kanavos_drawing::{Drawing, Item, Rgba, Stroke, pt, weights};
use kanavos_engine::commands::CreateElement;
use kanavos_geom::{Point2, Vec2};
use kanavos_i18n::t;
use kanavos_model::{ElementKind, Wall};
use kanavos_units::{LengthDisplay, Locale};

use crate::input::{Typed, parse};
use crate::snap::Snapper;
use crate::{CursorHint, InputField, Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput};

/// Wall tool.
#[derive(Debug, Clone, Default)]
pub struct WallTool {
    last: Option<Point2>,
    cursor: Option<Point2>,
    typed: String,
}

impl WallTool {
    fn wall_to(&self, ctx: &ToolContext<'_>, start: Point2, end: Point2) -> Option<CreateElement> {
        let structure = ctx.settings.wall_structure(ctx.doc)?;
        let height = ctx.settings.wall.height.or_else(|| ctx.doc.storey(ctx.storey).map(|s| s.height))?;
        Some(CreateElement::on(
            ctx.storey,
            ElementKind::Wall(Wall {
                start,
                end,
                bulge: 0.0,
                reference: ctx.settings.wall.reference,
                structure,
                height,
                base_offset: ctx.settings.wall.base_offset,
            }),
        ))
    }

    fn typed_target(&self, from: Point2, toward: Option<Point2>) -> Option<Point2> {
        match parse(&self.typed).ok()? {
            Typed::Length(l) => {
                let dir = toward.and_then(|c| (c - from).normalized()).unwrap_or(Vec2::X);
                Some(from + dir * l.as_m())
            }
            Typed::Polar { length, degrees } => {
                Some(from + Vec2::from_angle(kanavos_geom::Angle::deg(degrees)) * length.as_m())
            }
            Typed::Relative { dx, dy } => Some(from + Vec2::new(dx.as_m(), dy.as_m())),
            Typed::Absolute { x, y } => Some(Point2::from_lengths(x, y)),
            Typed::Grid { .. } => None,
        }
    }

    fn preview(&self, ctx: &ToolContext<'_>, end: Point2, locale: Locale) -> (Drawing, Vec<InputField>) {
        let mut d = Drawing::model(1.0);
        let mut fields = Vec::new();
        if let Some(start) = self.last {
            let thickness = ctx
                .settings
                .wall_structure(ctx.doc)
                .and_then(|s| ctx.doc.structure_thickness(&s))
                .map(|t| t.as_m())
                .unwrap_or(0.25);
            if let Some(dir) = (end - start).normalized() {
                let w = Wall {
                    start,
                    end,
                    bulge: 0.0,
                    reference: ctx.settings.wall.reference,
                    structure: kanavos_model::Structure::Basic {
                        material: kanavos_model::Id(0),
                        thickness: kanavos_units::Length::m(thickness),
                    },
                    height: kanavos_units::Length::m(3.0),
                    base_offset: kanavos_units::Length::ZERO,
                };
                let (l, r) = w.face_offsets(kanavos_units::Length::m(thickness));
                let n = dir.perp();
                let corners = [start + n * l, end + n * l, end + n * r, start + n * r];
                d.push(Item::polygon(
                    vec![
                        corners
                            .iter()
                            .map(|q| {
                                let [x, y] = q.xy();
                                pt(x, y)
                            })
                            .collect(),
                    ],
                    Some(kanavos_drawing::Fill::Solid(Rgba::new(40, 110, 220, 60))),
                    Some(Stroke::solid(weights::THIN).with_color(Rgba::rgb(40, 110, 220))),
                    "tool.preview",
                ));
            }
            let len = start.distance(end);
            fields.push(InputField {
                label: t(locale, "tools.field.length").into_owned(),
                value: if self.typed.is_empty() {
                    LengthDisplay(len, locale).to_string()
                } else {
                    self.typed.clone()
                },
                active: true,
                locked: !self.typed.is_empty(),
            });
            fields.push(InputField {
                label: t(locale, "tools.field.angle").into_owned(),
                value: format!("{:.1}°", (end - start).angle().as_deg()),
                active: false,
                locked: false,
            });
        }
        (d, fields)
    }
}

impl Tool for WallTool {
    fn id(&self) -> ToolId {
        ToolId::Wall
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let locale = ctx.locale;
        let mut out = ToolOutput { cursor: CursorHint::Crosshair, ..ToolOutput::default() };
        let snapper = Snapper::new(ctx.doc, ctx.storey);
        let snap = |raw: Point2, alt: bool| {
            let mut s = ctx.snap.clone();
            if alt {
                s.enabled = false;
            }
            snapper.snap(raw, self.last, &s, ctx.pixel_size, locale)
        };
        match event {
            ToolEvent::Move { at, mods } => {
                let s = snap(*at, mods.alt);
                self.cursor = Some(s.point);
                out.snap = Some(s);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let s = snap(*at, mods.alt);
                let target = match self.last {
                    Some(start) if !self.typed.is_empty() => {
                        self.typed_target(start, Some(s.point)).unwrap_or(s.point)
                    }
                    _ => s.point,
                };
                if let Some(start) = self.last
                    && !start.coincides(target, &ctx.doc.tolerance())
                    && let Some(cmd) = self.wall_to(ctx, start, target)
                {
                    out.commands.push(cmd.into());
                }
                self.last = Some(target);
                self.typed.clear();
                out.snap = Some(s);
            }
            ToolEvent::Text(s) => {
                self.typed.push_str(s);
            }
            ToolEvent::Key { key: Key::Backspace, .. } => {
                self.typed.pop();
            }
            ToolEvent::Key { key: Key::Enter, .. } => {
                if let Some(start) = self.last
                    && !self.typed.is_empty()
                {
                    if let Some(target) = self.typed_target(start, self.cursor)
                        && let Some(cmd) = self.wall_to(ctx, start, target)
                    {
                        out.commands.push(cmd.into());
                        self.last = Some(target);
                    }
                    self.typed.clear();
                } else {
                    self.cancel();
                    out.finished = true;
                }
            }
            ToolEvent::Key { key: Key::Escape, .. } | ToolEvent::DoubleClick { .. } => {
                self.cancel();
                out.finished = true;
            }
            _ => {}
        }
        let end = match (self.last, self.typed.is_empty()) {
            (Some(start), false) => self.typed_target(start, self.cursor).or(self.cursor),
            _ => self.cursor,
        };
        if let Some(end) = end {
            let (preview, fields) = self.preview(ctx, end, locale);
            out.preview = preview;
            out.fields = fields;
        }
        out.prompt =
            t(locale, if self.last.is_some() { "tools.wall.next" } else { "tools.wall.start" }).into_owned();
        out
    }

    fn cancel(&mut self) {
        self.last = None;
        self.typed.clear();
    }

    fn is_idle(&self) -> bool {
        self.last.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Modifiers, SnapSettings, ToolSettings};
    use std::collections::BTreeSet;

    #[test]
    fn two_clicks_make_a_wall_and_typing_sets_the_length() {
        let doc = kanavos_model::starter();
        let storey = doc.storeys()[0].id;
        let sel = BTreeSet::new();
        let snap = SnapSettings { enabled: false, ..SnapSettings::default() };
        let settings = ToolSettings::default();
        let ctx = ToolContext {
            doc: &doc,
            storey,
            selection: &sel,
            pixel_size: 0.01,
            snap: &snap,
            settings: &settings,
            locale: Locale::El,
        };
        let mut tool = WallTool::default();
        let m = Modifiers::default();
        tool.handle(
            &ctx,
            &ToolEvent::Press { at: Point2::new(0.0, 0.0), button: MouseButton::Primary, mods: m },
        );
        tool.handle(&ctx, &ToolEvent::Move { at: Point2::new(1.0, 0.0), mods: m });
        tool.handle(&ctx, &ToolEvent::Text("4.5".into()));
        let out = tool.handle(&ctx, &ToolEvent::Key { key: Key::Enter, mods: m });
        assert_eq!(out.commands.len(), 1);
        let kanavos_engine::AnyCommand::CreateElement(c) = &out.commands[0] else {
            panic!("expected element.create")
        };
        let ElementKind::Wall(w) = &c.kind else { panic!("expected a wall") };
        assert!(doc.tolerance().equal(w.length(), kanavos_units::Length::m(4.5)));
    }
}

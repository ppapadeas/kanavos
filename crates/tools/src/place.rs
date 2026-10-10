//! Point-placed elements: columns and objects, and stairs (`specs/tools/placement.md`).
//!
//! Columns and objects: a click places the element with the rotation from the settings (the rotate key turns it
//! by 90° per press, the flip key mirrors objects). Pressing and dragging sets the rotation from the drag
//! direction; with "pick rotation" set, a second click sets it. A typed rotation (degrees) overrides all of these.
//!
//! Stairs: click the midpoint of the first riser, then a point in the walking direction (or type the direction in
//! degrees). Shape, width and going come from the settings; the stair rises one storey height, with the fewest
//! risers that keep each riser at or below the maximum.

use kanavos_engine::commands::CreateElement;
use kanavos_geom::{Angle, Point2};
use kanavos_i18n::tf;
use kanavos_model::{Column, ElementKind, Object, Stair, StairShape};
use kanavos_units::{Length, format_decimal};

use crate::common::{Field, Fields, Pointer, angle_text, base_output, length_text, tr};
use crate::preview;
use crate::settings::{storey_height, structural_material};
use crate::{Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput};

/// Column and object tool.
#[derive(Debug, Clone)]
pub struct PlaceTool {
    id: ToolId,
    pointer: Pointer,
    fields: Fields,
    /// Where the primary button went down (a drag from here sets the rotation).
    press: Option<Point2>,
    /// Placed position waiting for the rotation click.
    pick: Option<Point2>,
    /// Quarter turns from the rotate key.
    steps: u32,
    /// Mirror toggled with the flip key (objects).
    mirror: bool,
}

impl PlaceTool {
    /// A column or object tool.
    pub fn new(id: ToolId) -> PlaceTool {
        PlaceTool {
            id,
            pointer: Pointer::default(),
            fields: Fields::default(),
            press: None,
            pick: None,
            steps: 0,
            mirror: false,
        }
    }

    fn base_rotation(&self, ctx: &ToolContext<'_>) -> Angle {
        let base = match self.id {
            ToolId::Column => ctx.settings.column.rotation,
            _ => ctx.settings.object_placement.rotation,
        };
        base + Angle::deg(90.0 * f64::from(self.steps % 4))
    }

    fn pick_rotation(&self, ctx: &ToolContext<'_>) -> bool {
        match self.id {
            ToolId::Column => ctx.settings.column.pick_rotation,
            _ => ctx.settings.object_placement.pick_rotation,
        }
    }

    fn rotation(&self, ctx: &ToolContext<'_>, at: Point2, towards: Option<Point2>) -> Angle {
        if let Some(deg) = self.fields.angle(Field::Rotation) {
            return Angle::deg(deg);
        }
        match towards.and_then(|p| (p - at).normalized()) {
            Some(d) => d.angle(),
            None => self.base_rotation(ctx),
        }
    }

    fn kind(&self, ctx: &ToolContext<'_>, at: Point2, rotation: Angle) -> Option<ElementKind> {
        match self.id {
            ToolId::Column => {
                let s = &ctx.settings.column;
                Some(ElementKind::Column(Column {
                    position: at,
                    rotation,
                    profile: s.profile,
                    material: ctx.settings.column_material(ctx.doc)?,
                    height: s.height.unwrap_or_else(|| storey_height(ctx.doc, ctx.storey)),
                    base_offset: Length::ZERO,
                }))
            }
            _ => Some(ElementKind::Object(Object {
                definition: ctx.settings.object.clone(),
                position: at,
                rotation,
                mirrored: ctx.settings.object_placement.mirrored != self.mirror,
                elevation: Length::ZERO,
                params: Default::default(),
            })),
        }
    }

    fn place(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, at: Point2, rotation: Angle) {
        let Some(kind) = self.kind(ctx, at, rotation) else {
            out.warning = Some(tr(ctx, "tools.place.no_material"));
            return;
        };
        let e = preview::new_element(ctx, 0, Some(ctx.storey), kind.clone());
        if let Err(err) = kanavos_elements::validate(ctx.doc, &e) {
            out.warning = Some(preview::explain(&err, ctx.locale));
            return;
        }
        out.commands.push(CreateElement::on(ctx.storey, kind).into());
        out.finished = true;
        self.pick = None;
        self.press = None;
        self.fields.clear();
    }

    fn drag_threshold(ctx: &ToolContext<'_>) -> f64 {
        6.0 * ctx.pixel_size
    }
}

impl Tool for PlaceTool {
    fn id(&self) -> ToolId {
        self.id
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        self.fields.set(&[Field::Rotation]);
        let mut out = base_output(&self.pointer);
        match event {
            ToolEvent::Move { at, mods } => {
                let from = self.pick.or(self.press);
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let from = self.pick;
                let s = self.pointer.update(ctx, *at, *mods, from);
                match self.pick {
                    Some(anchor) => {
                        let r = self.rotation(ctx, anchor, Some(s.point));
                        self.place(ctx, &mut out, anchor, r);
                    }
                    None => self.press = Some(s.point),
                }
            }
            ToolEvent::Release { at, button: MouseButton::Primary, mods } => {
                if let Some(anchor) = self.press.take() {
                    let s = self.pointer.update(ctx, *at, *mods, Some(anchor));
                    if anchor.distance(s.point).as_m() > Self::drag_threshold(ctx) {
                        let r = self.rotation(ctx, anchor, Some(s.point));
                        self.place(ctx, &mut out, anchor, r);
                    } else if self.pick_rotation(ctx) && self.fields.is_empty() {
                        self.pick = Some(anchor);
                    } else {
                        let r = self.rotation(ctx, anchor, None);
                        self.place(ctx, &mut out, anchor, r);
                    }
                }
            }
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Rotate => self.steps = self.steps.wrapping_add(1),
                Key::Flip => {
                    if self.id == ToolId::Object {
                        self.mirror = !self.mirror;
                    } else {
                        self.steps = self.steps.wrapping_add(1);
                    }
                }
                Key::Tab => self.fields.tab(),
                Key::Backspace => {
                    if !self.fields.backspace() {
                        self.pick = None;
                    }
                }
                Key::Enter => {
                    let at = self.pick.or(self.pointer.point());
                    if let Some(at) = at {
                        let r = self.rotation(ctx, at, None);
                        self.place(ctx, &mut out, at, r);
                    }
                }
                Key::Escape => {
                    if !self.fields.is_empty() {
                        self.fields.clear();
                    } else if self.pick.take().is_none() {
                        out.finished = true;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        let what = tr(ctx, &format!("tools.{}", self.id.key()));
        out.prompt = match self.pick {
            Some(_) => tr(ctx, "tools.place.rotation"),
            None => tf(ctx.locale, "tools.place.position", &[("what", &what)]),
        };
        // Preview at the pick point (rotating towards the pointer) or at the pointer.
        let cursor = self.pointer.point();
        let (at, towards) = match (self.pick, self.press) {
            (Some(p), _) => (Some(p), cursor),
            (None, Some(p)) if cursor.is_some_and(|c| c.distance(p).as_m() > Self::drag_threshold(ctx)) => {
                (Some(p), cursor)
            }
            _ => (cursor, None),
        };
        if let Some(at) = at {
            let r = self.rotation(ctx, at, towards);
            if let Some(kind) = self.kind(ctx, at, r) {
                let e = preview::new_element(ctx, 0, Some(ctx.storey), kind);
                let g = preview::ghost(ctx, &[e], &[], &[]);
                out.preview = g.drawing;
                out.warning = out.warning.take().or(g.problem);
                if let Some(t) = towards {
                    preview::rubber(&mut out.preview, at, t);
                }
            }
            out.fields = self.fields.view(&[angle_text(r.as_deg(), ctx.locale)], ctx.locale);
        }
        out.typing = true;
        out
    }

    fn cancel(&mut self) {
        self.pick = None;
        self.press = None;
        self.fields.clear();
    }

    fn is_idle(&self) -> bool {
        self.pick.is_none() && self.press.is_none()
    }
}

/// Stair tool.
#[derive(Debug, Clone, Default)]
pub struct StairTool {
    pointer: Pointer,
    fields: Fields,
    origin: Option<Point2>,
}

impl StairTool {
    fn stair(ctx: &ToolContext<'_>, origin: Point2, direction: Angle) -> Option<Stair> {
        let s = &ctx.settings.stair;
        let rise = storey_height(ctx.doc, ctx.storey);
        let max = s.max_riser.as_m().max(0.01);
        let risers = ((rise.as_m() / max) - 1e-9).ceil().max(2.0) as u32;
        Some(Stair {
            origin,
            direction,
            shape: s.shape,
            width: s.width,
            rise,
            risers,
            going: s.going,
            first_flight_risers: None,
            gap: s.gap,
            thickness: s.thickness,
            material: structural_material(ctx.doc)?,
            usage: s.usage,
            base_offset: Length::ZERO,
        })
    }

    fn direction(&self, origin: Point2) -> Angle {
        if let Some(d) = self.fields.angle(Field::Angle) {
            return Angle::deg(d);
        }
        self.pointer
            .point()
            .and_then(|c| (c - origin).normalized())
            .map(|v| v.angle())
            .unwrap_or(Angle::deg(90.0))
    }

    fn summary(ctx: &ToolContext<'_>, s: &Stair) -> String {
        let run = match s.shape {
            StairShape::Straight => s.going * f64::from(s.risers.saturating_sub(1)),
            _ => s.going * f64::from(s.risers.saturating_sub(2)) + s.width,
        };
        tf(
            ctx.locale,
            "tools.stair.summary",
            &[
                ("risers", &s.risers.to_string()),
                ("riser", &format_decimal(s.riser_height().as_cm(), 1, ctx.locale)),
                ("going", &format_decimal(s.going.as_cm(), 1, ctx.locale)),
                ("run", &length_text(run, ctx.locale)),
            ],
        )
    }
}

impl Tool for StairTool {
    fn id(&self) -> ToolId {
        ToolId::Stair
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        self.fields.set(if self.origin.is_some() { &[Field::Angle] } else { &[Field::Point] });
        let mut out = base_output(&self.pointer);
        let mut commit = None;
        match event {
            ToolEvent::Move { at, mods } => {
                self.pointer.update(ctx, *at, *mods, self.origin);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let s = self.pointer.update(ctx, *at, *mods, self.origin);
                match self.origin {
                    None => {
                        self.origin = self
                            .fields
                            .point_form()
                            .and_then(|p| p.to_point(None, &ctx.doc.site().georef))
                            .or(Some(s.point));
                        self.fields.clear();
                    }
                    Some(o)
                        if !o.coincides(s.point, &ctx.doc.tolerance())
                            || self.fields.angle(Field::Angle).is_some() =>
                    {
                        commit = Some((o, self.direction(o)));
                    }
                    Some(_) => {}
                }
            }
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Tab => self.fields.tab(),
                Key::Backspace => {
                    if !self.fields.backspace() {
                        self.origin = None;
                    }
                }
                Key::Enter => match self.origin {
                    Some(o) => commit = Some((o, self.direction(o))),
                    None => {
                        self.origin =
                            self.fields.point_form().and_then(|p| p.to_point(None, &ctx.doc.site().georef));
                        self.fields.clear();
                    }
                },
                Key::Escape => {
                    if !self.fields.is_empty() {
                        self.fields.clear();
                    } else if self.origin.take().is_none() {
                        out.finished = true;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if let Some((o, dir)) = commit {
            match Self::stair(ctx, o, dir) {
                Some(s) => {
                    let e = preview::new_element(ctx, 0, Some(ctx.storey), ElementKind::Stair(s.clone()));
                    match kanavos_elements::validate(ctx.doc, &e) {
                        Ok(()) => {
                            out.commands.push(CreateElement::on(ctx.storey, ElementKind::Stair(s)).into());
                            out.finished = true;
                            self.origin = None;
                            self.fields.clear();
                        }
                        Err(err) => out.warning = Some(preview::explain(&err, ctx.locale)),
                    }
                }
                None => out.warning = Some(tr(ctx, "tools.place.no_material")),
            }
        }
        self.fields.set(if self.origin.is_some() { &[Field::Angle] } else { &[Field::Point] });
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        let origin = self.origin.or(self.pointer.point());
        let mut prompt =
            tr(ctx, if self.origin.is_some() { "tools.stair.direction" } else { "tools.stair.origin" });
        if let Some(o) = origin {
            let dir = if self.origin.is_some() { self.direction(o) } else { Angle::deg(90.0) };
            if let Some(s) = Self::stair(ctx, o, dir) {
                prompt = format!("{prompt} · {}", Self::summary(ctx, &s));
                let e = preview::new_element(ctx, 0, Some(ctx.storey), ElementKind::Stair(s));
                let g = preview::ghost(ctx, &[e], &[], &[]);
                out.preview = g.drawing;
                out.warning = out.warning.take().or(g.problem);
            }
            if self.origin.is_some()
                && let Some(c) = self.pointer.point()
            {
                preview::rubber(&mut out.preview, o, c);
            }
            let live =
                if self.origin.is_some() { vec![angle_text(dir.as_deg(), ctx.locale)] } else { Vec::new() };
            out.fields = self.fields.view(&live, ctx.locale);
        }
        out.prompt = prompt;
        out.typing = true;
        out
    }

    fn cancel(&mut self) {
        self.origin = None;
        self.fields.clear();
    }

    fn is_idle(&self) -> bool {
        self.origin.is_none()
    }
}

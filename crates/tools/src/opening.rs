//! Doors, windows and voids in walls (`specs/tools/openings.md`).
//!
//! Hover a wall: the opening previews in it, centred on the pointer. Its position snaps, in this order, to a
//! typed distance, the wall's centre, flush with the nearest wall end, and otherwise to steps (5 cm by default)
//! of the distance from the nearest wall end to the opening's near jamb.
//!
//! The side of the wall the pointer is on is where the opening "faces": for a door, the side its leaves open
//! towards (click inside the room the door opens into); for a window, its outer face (click outside the building).
//! In the model a door opens towards the wall's right face and a window's outer face is the wall's left face unless
//! `flip_side` is set (`specs/objects/orientation.md`), so a door is flipped when the pointer is on the wall's left
//! side and a window when it is on the right side. For an exterior wall drawn clockwise the usual clicks (inside
//! for a door, outside for a window) therefore give unflipped openings. The flip key swaps the hinge jamb.
//! Typing a number sets the distance from the nearest wall end to the near jamb; Tab moves to the width.

use kanavos_engine::commands::CreateElement;
use kanavos_geom::{Point2, Vec2};
use kanavos_i18n::{t, tf};
use kanavos_model::{ElementKind, Id, Opening, OpeningKind, Wall};
use kanavos_units::{Length, format_decimal};

use crate::common::{Field, Fields, Pointer, base_output, length_text, tr};
use crate::preview;
use crate::snap::{SnapHint, SnapResult, Snapper};
use crate::{Key, MouseButton, Tool, ToolContext, ToolEvent, ToolId, ToolOutput};

/// Where the opening would go.
#[derive(Debug, Clone)]
struct Spot {
    wall: Id,
    /// Centre offset along the wall's reference line.
    offset: f64,
    width: f64,
    /// The pointer is on the wall's left side (looking from start to end).
    left: bool,
    /// The jamb distance is measured from the wall's start (else from its end).
    from_start: bool,
    /// Distance from that end to the near jamb.
    jamb: f64,
    snap: SnapResult,
}

/// Door, window and void tool.
#[derive(Debug, Clone)]
pub struct OpeningTool {
    kind: OpeningKind,
    pointer: Pointer,
    fields: Fields,
    flip_hand: bool,
    /// The end the typed distance is measured from (fixed when typing starts): wall and "from start".
    anchor: Option<(Id, bool)>,
    spot: Option<Spot>,
}

impl OpeningTool {
    /// A tool for doors, windows or voids.
    pub fn new(kind: OpeningKind) -> OpeningTool {
        OpeningTool {
            kind,
            pointer: Pointer::default(),
            fields: Fields::default(),
            flip_hand: false,
            anchor: None,
            spot: None,
        }
    }

    fn size(&self, ctx: &ToolContext<'_>) -> (Length, Length) {
        let s = &ctx.settings.opening;
        let (w, h) = match self.kind {
            OpeningKind::Door => s.door_size,
            OpeningKind::Window => s.window_size,
            OpeningKind::Void => s.void_size,
        };
        (self.fields.length(Field::Width).unwrap_or(w), h)
    }

    fn opening(&self, ctx: &ToolContext<'_>, wall: &Wall, spot: &Spot) -> Opening {
        let s = &ctx.settings.opening;
        let (width, height) = self.size(ctx);
        let (style, sill) = match self.kind {
            OpeningKind::Door => (s.door_style.clone(), wall.base_offset),
            OpeningKind::Window => (s.window_style.clone(), s.window_sill),
            OpeningKind::Void => {
                (kanavos_objects::default_style(OpeningKind::Void).to_owned(), wall.base_offset)
            }
        };
        Opening {
            host: spot.wall,
            kind: self.kind,
            style,
            offset: Length::m(spot.offset),
            width,
            height,
            sill,
            flip_side: flip_side_for(self.kind, spot.left),
            flip_hand: self.flip_hand,
            params: Default::default(),
        }
    }

    /// The wall under the pointer: the nearest wall whose body contains it or whose reference line is within
    /// half its thickness plus the snap radius.
    fn wall_under(ctx: &ToolContext<'_>, raw: Point2) -> Option<(Id, Wall)> {
        let r = ctx.snap.radius_px.max(1.0) * ctx.pixel_size;
        let index = Snapper::cached(ctx.doc, ctx.storey);
        let tol = ctx.doc.tolerance();
        let mut best: Option<(f64, Id, Wall)> = None;
        for id in index.elements_near(raw, r) {
            let Some(w) = ctx.doc.element(id).and_then(|e| e.as_wall()) else { continue };
            let inside = index.outline(id).is_some_and(|o| o.contains(raw, &tol));
            let t = ctx.doc.structure_thickness(&w.structure).map(|t| t.as_m()).unwrap_or(0.25);
            let d = reference_distance(w, raw);
            if (inside || d <= t / 2.0 + r) && best.as_ref().is_none_or(|(bd, ..)| d < *bd) {
                best = Some((d, id, w.clone()));
            }
        }
        best.map(|(_, id, w)| (id, w))
    }

    fn spot(&mut self, ctx: &ToolContext<'_>, raw: Point2, snapping: bool) -> Option<Spot> {
        let (id, w) = Self::wall_under(ctx, raw)?;
        let len = w.length().as_m();
        let (along, side) = wall_param(&w, raw);
        let width = self.size(ctx).0.as_m();
        let half = width / 2.0;
        let r = ctx.snap.radius_px.max(1.0) * ctx.pixel_size;
        let raw_off = if len >= width { along.clamp(half, len - half) } else { len / 2.0 };
        let near_start = raw_off <= len / 2.0;
        let mut from_start = near_start;
        let label_end = |from_start: bool| {
            tr(ctx, if from_start { "tools.opening.from_start" } else { "tools.opening.from_end" })
        };
        let (offset, hint, label) = if let Some(d) = self.fields.length(Field::Distance) {
            let fs = match self.anchor {
                Some((wid, fs)) if wid == id => fs,
                _ => near_start,
            };
            self.anchor = Some((id, fs));
            from_start = fs;
            let off = if fs { d.as_m() + half } else { len - d.as_m() - half };
            (off, SnapHint::Locked, label_end(fs))
        } else if !snapping {
            (raw_off, SnapHint::Free, String::new())
        } else if (raw_off - len / 2.0).abs() <= r && len >= width {
            (len / 2.0, SnapHint::Midpoint, tr(ctx, "tools.opening.centre"))
        } else {
            let jamb = if near_start { raw_off - half } else { len - raw_off - half };
            let step = ctx.settings.opening.step.as_m();
            if jamb <= r {
                (
                    if near_start { half } else { len - half },
                    SnapHint::Endpoint,
                    tr(ctx, "tools.opening.flush"),
                )
            } else if step > 1e-6 {
                let j = (jamb / step).round() * step;
                let off = if near_start { j + half } else { len - j - half };
                let label = tf(
                    ctx.locale,
                    "tools.opening.step",
                    &[("step", &format_decimal(step * 100.0, 0, ctx.locale))],
                );
                (off, SnapHint::Grid, label)
            } else {
                (raw_off, SnapHint::Free, String::new())
            }
        };
        let jamb = if from_start { offset - half } else { len - offset - half };
        let centre = point_at(&w, offset);
        let label = if label.is_empty() {
            String::new()
        } else {
            format!(
                "{label} · {}",
                tf(ctx.locale, "tools.opening.distance", &[("d", &length_text(Length::m(jamb), ctx.locale))])
            )
        };
        Some(Spot {
            wall: id,
            offset,
            width,
            left: side >= 0.0,
            from_start,
            jamb,
            snap: SnapResult {
                point: centre,
                hint,
                label,
                guides: Vec::new(),
                element: Some(id),
                edge: None,
            },
        })
    }

    /// Why the opening cannot go at the spot, if it cannot.
    fn problem(&self, ctx: &ToolContext<'_>, w: &Wall, spot: &Spot) -> Option<String> {
        let o = self.opening(ctx, w, spot);
        let e = preview::new_element(ctx, 0, Some(ctx.storey), ElementKind::Opening(o));
        if let Err(err) = kanavos_elements::validate(ctx.doc, &e) {
            return Some(preview::explain(&err, ctx.locale));
        }
        let (a, b) = (spot.offset - spot.width / 2.0, spot.offset + spot.width / 2.0);
        let tol = ctx.doc.tolerance().length.as_m();
        let overlaps = ctx.doc.openings_in(spot.wall).any(|(_, other)| {
            let h = other.width.as_m() / 2.0;
            let (c, d) = (other.offset.as_m() - h, other.offset.as_m() + h);
            a < d - tol && c < b - tol
        });
        overlaps.then(|| tr(ctx, "tools.opening.overlap"))
    }
}

/// The `flip_side` that makes an opening face the side of the wall the pointer is on (`pointer_left`: the
/// pointer is on the wall's left side, looking from start to end).
///
/// Unflipped, a window's outer face is the wall's left face and a door opens towards its right face. The pointer
/// marks a window's outer face and the side a door opens to, so a window is flipped when the pointer is on the
/// right and a door when it is on the left. Voids look the same from both sides and are never flipped.
fn flip_side_for(kind: OpeningKind, pointer_left: bool) -> bool {
    match kind {
        OpeningKind::Window => !pointer_left,
        OpeningKind::Door => pointer_left,
        OpeningKind::Void => false,
    }
}

/// Distance from a point to a wall's reference line (arc or segment).
fn reference_distance(w: &Wall, p: Point2) -> f64 {
    match w.arc() {
        Some(arc) => {
            let (along, _) = wall_param(w, p);
            let len = w.length().as_m().max(1e-9);
            if (0.0..=len).contains(&along) {
                ((p - arc.centre).length() - arc.radius.as_m()).abs()
            } else {
                w.start.distance(p).as_m().min(w.end.distance(p).as_m())
            }
        }
        None => w.segment().distance_to(p).as_m(),
    }
}

/// Position of a point along a wall's reference line (metres from the start, unclamped) and its signed distance
/// to the left of the line.
pub(crate) fn wall_param(w: &Wall, p: Point2) -> (f64, f64) {
    match w.arc() {
        Some(arc) => {
            let tau = std::f64::consts::TAU;
            let sweep = arc.sweep.as_rad();
            let a = (p - arc.centre).angle().as_rad() - arc.start.as_rad();
            let mut da = if sweep >= 0.0 { a.rem_euclid(tau) } else { -(-a).rem_euclid(tau) };
            // Points past the start (outside the arc's span) count as negative.
            if da.abs() > sweep.abs() + (tau - sweep.abs()) / 2.0 {
                da -= tau * da.signum();
            }
            let along = da / sweep * w.length().as_m();
            let radial = (p - arc.centre).length() - arc.radius.as_m();
            let side = if sweep > 0.0 { -radial } else { radial };
            (along, side)
        }
        None => {
            let d = w.direction().unwrap_or(Vec2::X);
            let v = p - w.start;
            (v.dot(d), d.cross(v))
        }
    }
}

/// The point on a wall's reference line at a distance from its start.
fn point_at(w: &Wall, offset: f64) -> Point2 {
    match w.arc() {
        Some(arc) => arc.at((offset / w.length().as_m().max(1e-9)).clamp(0.0, 1.0)),
        None => w.start + w.direction().unwrap_or(Vec2::X) * offset,
    }
}

impl Tool for OpeningTool {
    fn id(&self) -> ToolId {
        match self.kind {
            OpeningKind::Door => ToolId::Door,
            OpeningKind::Window => ToolId::Window,
            OpeningKind::Void => ToolId::Void,
        }
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        self.fields.set(&[Field::Distance, Field::Width]);
        let mut out = base_output(&self.pointer);
        let mut commit = false;
        let mut alt = false;
        match event {
            ToolEvent::Move { at, mods } => {
                self.pointer.update(ctx, *at, *mods, None);
                alt = mods.alt;
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                self.pointer.update(ctx, *at, *mods, None);
                alt = mods.alt;
                commit = true;
            }
            ToolEvent::Text(s) => {
                if self.fields.is_empty() {
                    self.anchor = None;
                }
                self.fields.type_text(s);
            }
            ToolEvent::Key { key, .. } => match key {
                Key::Tab => self.fields.tab(),
                Key::Flip => self.flip_hand = !self.flip_hand,
                Key::Backspace => {
                    self.fields.backspace();
                }
                Key::Enter => commit = true,
                Key::Escape => {
                    if self.fields.is_empty() {
                        out.finished = true;
                    } else {
                        self.fields.clear();
                        self.anchor = None;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        let what = tr(ctx, &format!("tools.{}", ToolId::key(self.id())));
        out.prompt = tf(ctx.locale, "tools.opening.hover", &[("what", &what)]);
        out.tracked = Vec::new();
        self.spot = self.pointer.raw.and_then(|raw| self.spot(ctx, raw, !alt && ctx.snap.enabled));
        let Some(spot) = self.spot.clone() else {
            out.snap = None;
            out.fields =
                self.fields.view(&[String::new(), length_text(self.size(ctx).0, ctx.locale)], ctx.locale);
            out.typing = true;
            return out;
        };
        let Some(wall) = ctx.doc.element(spot.wall).and_then(|e| e.as_wall()).cloned() else { return out };
        out.highlight = vec![spot.wall];
        let problem = self.problem(ctx, &wall, &spot);
        if commit {
            match &problem {
                None => {
                    let o = self.opening(ctx, &wall, &spot);
                    out.commands.push(CreateElement::on(ctx.storey, ElementKind::Opening(o)).into());
                    out.finished = true;
                    self.fields.clear();
                    self.anchor = None;
                }
                Some(p) => out.warning = Some(p.clone()),
            }
        }
        // Preview: the opening symbol in its wall, and the distance from the measured end to the near jamb.
        let o = self.opening(ctx, &wall, &spot);
        let e = preview::new_element(ctx, 0, Some(ctx.storey), ElementKind::Opening(o));
        let g = preview::ghost(ctx, &[e], &[spot.wall], &[]);
        let mut d = g.drawing;
        let end = if spot.from_start { 0.0 } else { wall.length().as_m() };
        let jamb =
            if spot.from_start { spot.offset - spot.width / 2.0 } else { spot.offset + spot.width / 2.0 };
        if spot.jamb > 1e-6 {
            preview::rubber(&mut d, point_at(&wall, end), point_at(&wall, jamb));
            preview::length_label(&mut d, point_at(&wall, end), point_at(&wall, jamb), ctx.locale);
        }
        out.preview = d;
        out.warning = out.warning.take().or(problem).or(g.problem);
        out.snap = Some(spot.snap.clone());
        out.prompt = format!(
            "{} · {}",
            out.prompt,
            t(
                ctx.locale,
                if self.kind == OpeningKind::Door {
                    "tools.opening.side_door"
                } else {
                    "tools.opening.side_window"
                }
            )
        );
        out.fields = self.fields.view(
            &[length_text(Length::m(spot.jamb), ctx.locale), length_text(Length::m(spot.width), ctx.locale)],
            ctx.locale,
        );
        out.typing = true;
        out
    }

    fn cancel(&mut self) {
        self.fields.clear();
        self.anchor = None;
    }

    fn is_idle(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::{ReferenceLine, Structure};

    fn wall(bulge: f64) -> Wall {
        Wall {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(4.0, 0.0),
            bulge,
            reference: ReferenceLine::Centre,
            structure: Structure::Basic { material: Id(1), thickness: Length::cm(25.0) },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        }
    }

    #[test]
    fn parameters_along_straight_and_curved_walls() {
        let (a, s) = wall_param(&wall(0.0), Point2::new(1.5, 0.3));
        assert!((a - 1.5).abs() < 1e-12 && s > 0.0);
        // Half circle bending to the right (below the chord), centre (2, 0), radius 2.
        let w = wall(1.0);
        let len = w.length().as_m();
        let (a, _) = wall_param(&w, Point2::new(2.0, -2.0));
        assert!((a - len / 2.0).abs() < 1e-9, "{a} vs {}", len / 2.0);
        let mid = point_at(&w, len / 2.0);
        assert!(mid.coincides(Point2::new(2.0, -2.0), &kanavos_units::Tolerance::default()));
        assert!(reference_distance(&w, Point2::new(2.0, -2.5)) - 0.5 < 1e-9);
    }
}

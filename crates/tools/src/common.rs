//! Parts shared by the drawing tools: snapping with tracking and direction lock, typed-input fields, and small
//! helpers for prompts, ids and outputs.

use std::collections::BTreeSet;

use kanavos_engine::AnyCommand;
use kanavos_engine::commands::Batch;
use kanavos_geom::{Angle, Arc2, Point2, Segment2, Vec2};
use kanavos_i18n::t;
use kanavos_model::{Document, Id};
use kanavos_units::{Length, LengthDisplay, Locale, format_decimal};

use crate::input::{Typed, eval_angle, eval_length, parse};
use crate::snap::{SnapQuery, SnapResult, Snapper};
use crate::{CursorHint, InputField, Modifiers, ToolContext, ToolOutput};

/// No exclusions.
pub(crate) fn no_exclusions() -> &'static BTreeSet<Id> {
    static NONE: BTreeSet<Id> = BTreeSet::new();
    &NONE
}

/// The pointer of a drawing tool: the last raw and snapped positions, the points and edges it recently passed
/// over (for alignment, extension and parallel inference) and the Shift direction lock.
#[derive(Debug, Clone, Default)]
pub(crate) struct Pointer {
    /// Last raw position.
    pub raw: Option<Point2>,
    /// Last snap.
    pub snap: Option<SnapResult>,
    tracked: Vec<Point2>,
    edges: Vec<(Segment2, Id)>,
    lock: Option<Vec2>,
}

impl Pointer {
    /// The last snapped position.
    pub fn point(&self) -> Option<Point2> {
        self.snap.as_ref().map(|s| s.point)
    }

    /// Snaps a raw position. `from` is the previous point of the operation.
    pub fn update(
        &mut self,
        ctx: &ToolContext<'_>,
        raw: Point2,
        mods: Modifiers,
        from: Option<Point2>,
    ) -> SnapResult {
        self.update_excluding(ctx, raw, mods, from, no_exclusions())
    }

    /// Snaps a raw position, ignoring some elements (e.g. the ones being moved).
    pub fn update_excluding(
        &mut self,
        ctx: &ToolContext<'_>,
        raw: Point2,
        mods: Modifiers,
        from: Option<Point2>,
        exclude: &BTreeSet<Id>,
    ) -> SnapResult {
        let snapper = Snapper::cached(ctx.doc, ctx.storey);
        match from {
            Some(f) if mods.shift => {
                if self.lock.is_none() {
                    let towards = self.point().unwrap_or(raw);
                    self.lock = (towards - f).normalized().or_else(|| (raw - f).normalized());
                }
            }
            _ => self.lock = None,
        }
        let mut settings = ctx.snap.clone();
        if mods.alt {
            settings.enabled = false;
        }
        let q = SnapQuery {
            from,
            settings: &settings,
            pixel: ctx.pixel_size,
            locale: ctx.locale,
            tracked: &self.tracked,
            edges: &self.edges,
            lock: self.lock,
            exclude,
        };
        let s = snapper.snap_query(raw, &q);
        self.track(&s, from, ctx.doc);
        self.raw = Some(raw);
        self.snap = Some(s.clone());
        s
    }

    fn track(&mut self, s: &SnapResult, from: Option<Point2>, doc: &Document) {
        let tol = doc.tolerance();
        if s.hint.is_point()
            && !from.is_some_and(|f| f.coincides(s.point, &tol))
            && !self.tracked.iter().any(|p| p.coincides(s.point, &tol))
        {
            self.tracked.insert(0, s.point);
            self.tracked.truncate(3);
        }
        if s.hint.is_edge()
            && let (Some(seg), Some(id)) = (s.edge, s.element)
            && !self
                .edges
                .iter()
                .any(|(e, _)| e.start.coincides(seg.start, &tol) && e.end.coincides(seg.end, &tol))
        {
            self.edges.insert(0, (seg, id));
            self.edges.truncate(2);
        }
    }

    /// Tracks a point for alignment (e.g. the first point of a chain, so the chain can close square).
    pub fn acquire(&mut self, p: Point2) {
        self.tracked.retain(|q| q.distance(p).as_m() > 1e-9);
        self.tracked.insert(0, p);
        self.tracked.truncate(3);
    }

    /// Points currently tracked for alignment.
    pub fn tracked(&self) -> Vec<Point2> {
        self.tracked.clone()
    }

    /// Forgets the direction lock.
    pub fn unlock(&mut self) {
        self.lock = None;
    }
}

/// A typed-input field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    /// A point: absolute or ΕΓΣΑ87 coordinates, or relative/polar from the previous point.
    Point,
    /// Length along the direction.
    Length,
    /// Direction in degrees.
    Angle,
    /// Sideways offset to the left of the direction.
    Offset,
    /// X extent.
    Dx,
    /// Y extent.
    Dy,
    /// Rise of an arc above its chord.
    Rise,
}

impl Field {
    fn key(self) -> &'static str {
        match self {
            Field::Point => "tools.field.point",
            Field::Length => "tools.field.length",
            Field::Angle => "tools.field.angle",
            Field::Offset => "tools.field.offset",
            Field::Dx => "tools.field.dx",
            Field::Dy => "tools.field.dy",
            Field::Rise => "tools.field.rise",
        }
    }

    fn is_angle(self) -> bool {
        self == Field::Angle
    }
}

/// Typed input of a tool: one text per field and the active field (Tab cycles).
#[derive(Debug, Clone, Default)]
pub(crate) struct Fields {
    kinds: Vec<Field>,
    texts: Vec<String>,
    active: usize,
}

impl Fields {
    /// Sets the fields shown; typed text is kept when the set does not change.
    pub fn set(&mut self, kinds: &[Field]) {
        if self.kinds != kinds {
            self.kinds = kinds.to_vec();
            self.texts = vec![String::new(); kinds.len()];
            self.active = 0;
        }
    }

    /// Forgets everything typed.
    pub fn clear(&mut self) {
        for t in &mut self.texts {
            t.clear();
        }
        self.active = 0;
    }

    /// Whether nothing is typed.
    pub fn is_empty(&self) -> bool {
        self.texts.iter().all(String::is_empty)
    }

    /// Whether any field is shown.
    pub fn is_shown(&self) -> bool {
        !self.kinds.is_empty()
    }

    /// Adds typed characters to the active field. Only characters that can appear in typed input are kept.
    pub fn type_text(&mut self, s: &str) {
        let keep: String =
            s.chars().filter(|c| c.is_ascii_digit() || ".,+-*/()@<#;° mMcC".contains(*c)).collect();
        if keep.is_empty() {
            return;
        }
        if let Some(t) = self.texts.get_mut(self.active) {
            t.push_str(&keep);
        }
    }

    /// Deletes the last typed character of the active field (or of the last non-empty field). Returns whether
    /// something was deleted.
    pub fn backspace(&mut self) -> bool {
        if let Some(t) = self.texts.get_mut(self.active)
            && t.pop().is_some()
        {
            return true;
        }
        if let Some(t) = self.texts.iter_mut().rev().find(|t| !t.is_empty()) {
            t.pop();
            return true;
        }
        false
    }

    /// Activates the next field.
    pub fn tab(&mut self) {
        if !self.kinds.is_empty() {
            self.active = (self.active + 1) % self.kinds.len();
        }
    }

    fn text(&self, f: Field) -> Option<&str> {
        let i = self.kinds.iter().position(|k| *k == f)?;
        self.texts.get(i).map(String::as_str).filter(|s| !s.trim().is_empty())
    }

    /// A typed length in a field.
    pub fn length(&self, f: Field) -> Option<Length> {
        let s = self.text(f)?;
        if f == Field::Length && self.point_form().is_some() {
            return None;
        }
        eval_length(s).ok()
    }

    /// A typed angle (degrees) in a field.
    pub fn angle(&self, f: Field) -> Option<f64> {
        eval_angle(self.text(f)?).ok()
    }

    /// The first field's text when it names a point by itself (`@…`, `#…`, coordinate pairs).
    pub fn point_form(&self) -> Option<Typed> {
        let s = self.texts.first()?;
        if s.trim().is_empty() {
            return None;
        }
        parse(s).ok().filter(Typed::is_point)
    }

    /// Whether the typed text of any field cannot be read.
    pub fn has_error(&self) -> bool {
        self.kinds.iter().zip(&self.texts).any(|(k, s)| {
            if s.trim().is_empty() {
                return false;
            }
            if k.is_angle() { eval_angle(s).is_err() } else { parse(s).is_err() }
        })
    }

    /// The fields for the output: typed text where typed, else the live value from `live` (same order as the
    /// kinds; missing values show empty).
    pub fn view(&self, live: &[String], locale: Locale) -> Vec<InputField> {
        self.kinds
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let typed = self.texts.get(i).filter(|s| !s.is_empty());
                InputField {
                    label: t(locale, k.key()).into_owned(),
                    value: typed.cloned().or_else(|| live.get(i).cloned()).unwrap_or_default(),
                    active: i == self.active,
                    locked: typed.is_some(),
                }
            })
            .collect()
    }
}

/// The point named by typed input: a point form in the first field, else length/angle/offset fields from
/// `from` in the direction of `cursor`. `None` when nothing usable is typed.
pub(crate) fn typed_target(
    fields: &Fields,
    from: Option<Point2>,
    cursor: Option<Point2>,
    doc: &Document,
) -> Option<Point2> {
    if let Some(p) = fields.point_form() {
        return p.to_point(from, &doc.site().georef);
    }
    let f = from?;
    let l = fields.length(Field::Length);
    let a = fields.angle(Field::Angle);
    let o = fields.length(Field::Offset);
    if l.is_none() && a.is_none() && o.is_none() {
        return None;
    }
    let dir = a
        .map(|d| Vec2::from_angle(Angle::deg(d)))
        .or_else(|| cursor.and_then(|c| (c - f).normalized()))
        .unwrap_or(Vec2::X);
    let len = l.map(|l| l.as_m()).unwrap_or_else(|| cursor.map(|c| (c - f).dot(dir)).unwrap_or(0.0));
    let off = o.map(|o| o.as_m()).unwrap_or(0.0);
    Some(f + dir * len + dir.perp() * off)
}

/// Live values of the length, angle and offset fields for a segment.
pub(crate) fn segment_live(a: Point2, b: Point2, locale: Locale) -> Vec<String> {
    vec![
        length_text(a.distance(b), locale),
        angle_text((b - a).angle().as_deg(), locale),
        length_text(Length::ZERO, locale),
    ]
}

/// A length for display, e.g. "3,25 m".
pub(crate) fn length_text(l: Length, locale: Locale) -> String {
    LengthDisplay(l, locale).to_string()
}

/// An angle for display, e.g. "45,0°", normalised to [0, 360).
pub(crate) fn angle_text(deg: f64, locale: Locale) -> String {
    format!("{}°", format_decimal(deg.rem_euclid(360.0), 1, locale))
}

/// The output every drawing tool starts from: crosshair, snap and tracked points.
pub(crate) fn base_output(pointer: &Pointer) -> ToolOutput {
    ToolOutput {
        cursor: CursorHint::Crosshair,
        snap: pointer.snap.clone(),
        tracked: pointer.tracked(),
        ..ToolOutput::default()
    }
}

/// Wraps commands into one undo step with a label.
pub(crate) fn one_step(commands: Vec<AnyCommand>, label: String) -> Vec<AnyCommand> {
    if commands.len() <= 1 { commands } else { vec![Batch { label, commands }.into()] }
}

/// The id of a wall on a storey with these ends and bulge (the newest when several match).
pub(crate) fn find_wall(doc: &Document, storey: Id, start: Point2, end: Point2, bulge: f64) -> Option<Id> {
    let tol = doc.tolerance();
    doc.walls_on(storey)
        .filter(|(_, w)| {
            w.start.coincides(start, &tol) && w.end.coincides(end, &tol) && (w.bulge - bulge).abs() < 1e-9
        })
        .map(|(id, _)| id)
        .max()
}

/// The arc from `s` to `e` through `m`, or `None` when the points are collinear or coincide.
pub(crate) fn arc_through(s: Point2, m: Point2, e: Point2) -> Option<Arc2> {
    let b = m - s;
    let c = e - s;
    let d = 2.0 * b.cross(c);
    let scale = b.length().max(c.length()).max(1e-9);
    if d.abs() < 1e-9 * scale * scale {
        return None;
    }
    let (b2, c2) = (b.length_squared(), c.length_squared());
    let ux = (c.y * b2 - b.y * c2) / d;
    let uy = (b.x * c2 - c.x * b2) / d;
    let centre = s + Vec2::new(ux, uy);
    let radius = Vec2::new(ux, uy).length();
    let a_s = (s - centre).angle().as_rad();
    let a_m = (m - centre).angle().as_rad();
    let a_e = (e - centre).angle().as_rad();
    let tau = std::f64::consts::TAU;
    let ccw = (a_e - a_s).rem_euclid(tau);
    let to_m = (a_m - a_s).rem_euclid(tau);
    let sweep = if to_m < ccw { ccw } else { ccw - tau };
    Some(Arc2 { centre, radius: Length::m(radius), start: Angle::rad(a_s), sweep: Angle::rad(sweep) })
}

/// Bulge (`tan(sweep/4)`) of the arc from `s` to `e` through `m`; 0 for collinear points.
pub(crate) fn bulge_through(s: Point2, m: Point2, e: Point2) -> f64 {
    arc_through(s, m, e).map(|a| (a.sweep.as_rad() / 4.0).tan()).unwrap_or(0.0)
}

/// Bulge of an arc from `s` to `e` that rises `rise` metres above its chord, on the side of `side`.
pub(crate) fn bulge_from_rise(s: Point2, e: Point2, rise: f64, side: Point2) -> f64 {
    let chord = e - s;
    let c = chord.length();
    if c < 1e-9 {
        return 0.0;
    }
    // Positive bulge bends the arc to the right of start → end.
    let right = chord.cross(side - s) < 0.0;
    let mag = 2.0 * rise.abs() / c;
    if right { mag } else { -mag }
}

/// Whether the pointer is within the snap radius of a point.
pub(crate) fn near(ctx: &ToolContext<'_>, a: Point2, b: Point2) -> bool {
    a.distance(b).as_m() <= ctx.snap.radius_px.max(1.0) * ctx.pixel_size
}

/// Storey index used in space numbers: 0 for the first storey above ground, negative below.
pub(crate) fn storey_index(doc: &Document, storey: Id) -> i64 {
    let all = doc.storeys();
    let ground = all.iter().position(|s| !s.below_ground).unwrap_or(0) as i64;
    let me = all.iter().position(|s| s.id == storey).unwrap_or(0) as i64;
    me - ground
}

/// The next space number on a storey: `<storey>.<nn>` continuing from the highest number with the same prefix.
pub(crate) fn next_space_number(doc: &Document, storey: Id) -> String {
    let prefix = storey_index(doc, storey).to_string();
    let highest = doc
        .elements_on(storey)
        .filter_map(|e| e.as_space())
        .filter_map(|s| s.number.split_once('.'))
        .filter(|(p, _)| *p == prefix)
        .filter_map(|(_, n)| n.trim().parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{prefix}.{:02}", highest + 1)
}

/// Text in the language of the context.
pub(crate) fn tr(ctx: &ToolContext<'_>, key: &str) -> String {
    t(ctx.locale, key).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_units::Tolerance;

    #[test]
    fn arcs_through_three_points() {
        let tol = Tolerance::default();
        let (s, m, e) = (Point2::new(0.0, 0.0), Point2::new(1.0, -1.0), Point2::new(2.0, 0.0));
        let a = arc_through(s, m, e).unwrap();
        assert!(a.at(0.0).coincides(s, &tol) && a.at(1.0).coincides(e, &tol) && a.at(0.5).coincides(m, &tol));
        let b = bulge_through(s, m, e);
        assert!((b - 1.0).abs() < 1e-9);
        let arc = Arc2::from_bulge(s, e, b).unwrap();
        assert!(arc.at(0.5).coincides(m, &tol));
        // A shallow arc on the left side.
        let m2 = Point2::new(1.0, 0.5);
        let b2 = bulge_through(s, m2, e);
        assert!(b2 < 0.0);
        assert!(Arc2::from_bulge(s, e, b2).unwrap().at(0.5).coincides(m2, &tol));
        assert!((bulge_from_rise(s, e, 0.5, m2) - b2).abs() < 1e-9);
        assert!(arc_through(s, Point2::new(1.0, 0.0), e).is_none());
    }

    #[test]
    fn fields_take_typed_values() {
        let mut f = Fields::default();
        f.set(&[Field::Length, Field::Angle, Field::Offset]);
        f.type_text("3,5");
        f.tab();
        f.type_text("90");
        let doc = kanavos_model::starter();
        let p = typed_target(&f, Some(Point2::new(1.0, 1.0)), None, &doc).unwrap();
        assert!(p.coincides(Point2::new(1.0, 4.5), &Tolerance::default()));
        assert!(f.backspace());
        assert!(f.angle(Field::Angle).is_some());
        f.clear();
        f.type_text("@2;0");
        let p = typed_target(&f, Some(Point2::new(1.0, 1.0)), None, &doc).unwrap();
        assert!(p.coincides(Point2::new(3.0, 1.0), &Tolerance::default()));
    }
}

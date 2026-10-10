//! Previews: ghost geometry of what a tool will create or change, drawn exactly as the plan will draw it, plus
//! rubber bands and live dimensions.
//!
//! Ghosts are drawn by building a small document that holds the document's attributes and storeys, the new or
//! changed elements and the few elements they depend on (host walls, hosted openings), and deriving its plan with
//! `kanavos_views::plan`. The cost depends on what is previewed, not on the size of the document.

use std::collections::BTreeSet;

use kanavos_drawing::{Anchor, Drawing, Fill, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt, weights};
use kanavos_elements::GeometryError;
use kanavos_geom::{Point2, Vec2};
use kanavos_i18n::tf;
use kanavos_model::{Document, Element, ElementKind, Id, Phase, View};
use kanavos_units::{Length, Locale};
use kanavos_views::PlanOptions;

use crate::ToolContext;
use crate::common::length_text;
use crate::snap::plan_scale;

/// Colour of previews.
pub(crate) const BLUE: Rgba = Rgba::rgb(40, 110, 220);
/// Colour of previews that cannot be built.
pub(crate) const RED: Rgba = Rgba::rgb(205, 45, 35);

/// A drawn preview and, when it cannot be built, why.
#[derive(Debug, Clone, Default)]
pub(crate) struct Ghost {
    /// The drawing.
    pub drawing: Drawing,
    /// Why the previewed elements are not valid, in the user's language.
    pub problem: Option<String>,
}

/// An empty preview drawing at the plan's scale.
pub(crate) fn drawing(ctx: &ToolContext<'_>) -> Drawing {
    Drawing::model(plan_scale(ctx.doc, ctx.storey))
}

/// A new element on the active storey with the `i`-th id not yet used by the document.
pub(crate) fn new_element(ctx: &ToolContext<'_>, i: u64, storey: Option<Id>, kind: ElementKind) -> Element {
    Element {
        id: Id(ctx.doc.peek_next_id().0 + i),
        storey,
        view: None,
        phase: Phase::New,
        tag: String::new(),
        properties: Default::default(),
        external_id: None,
        kind,
    }
}

/// A document with the attributes and storeys of `src` and no elements, views or sheets.
fn base(src: &Document) -> Document {
    let mut d = Document::new().with_tolerance(src.tolerance()).with_defaults(src.defaults().clone());
    for s in src.storeys() {
        d = d.with_storey(s.clone());
    }
    for m in src.materials() {
        d = d.with_material(m.clone());
    }
    for b in src.build_ups() {
        d = d.with_build_up(b.clone());
    }
    d
}

/// Explains a geometry error in the user's language.
pub(crate) fn explain(err: &GeometryError, locale: Locale) -> String {
    match err {
        GeometryError::Invalid { field, reason, .. } => {
            tf(locale, "engine.error.invalid", &[("field", field), ("reason", reason)])
        }
        other => tf(locale, "engine.error.geometry", &[("reason", &other.to_string())]),
    }
}

/// Ghost of new elements (validated in order) next to unchanged context elements of the document.
///
/// `new` elements must carry ids not used by the document (see [`new_element`]). Only `new` elements and `views`
/// are drawn; context elements (host walls) shape them but are not drawn.
pub(crate) fn ghost(ctx: &ToolContext<'_>, new: &[Element], context: &[Id], views: &[View]) -> Ghost {
    let mut doc = base(ctx.doc);
    for id in context {
        if let Some(e) = ctx.doc.element(*id) {
            doc = doc.with_element(e.clone());
        }
    }
    let mut problem = None;
    let mut show = BTreeSet::new();
    for e in new {
        if let Err(err) = kanavos_elements::validate(&doc, e)
            && problem.is_none()
        {
            problem = Some(explain(&err, ctx.locale));
        }
        doc = doc.with_element(e.clone());
        show.insert(e.id.0);
    }
    for v in views {
        doc = doc.with_view(v.clone());
        show.insert(v.id.0);
    }
    let color = if problem.is_some() { RED } else { BLUE };
    Ghost { drawing: render(&doc, ctx.storey, &show, ctx, color), problem }
}

/// Ghost of elements of another document (e.g. the document after a preview command), drawn with the elements
/// they depend on.
pub(crate) fn ghost_of(ctx: &ToolContext<'_>, src: &Document, ids: &BTreeSet<Id>, color: Rgba) -> Drawing {
    let mut doc = base(src);
    let add = |doc: Document, id: Id| match src.element(id) {
        Some(e) => doc.with_element(e.clone()),
        None => doc,
    };
    for id in ids {
        doc = add(doc, *id);
        if let Some(ElementKind::Opening(o)) = src.element(*id).map(|e| &e.kind) {
            doc = add(doc, o.host);
        }
        let hosted: Vec<Id> = src.openings_in(*id).map(|(o, _)| o).collect();
        for o in hosted {
            doc = add(doc, o);
        }
    }
    let show: BTreeSet<u64> = ids.iter().map(|i| i.0).collect();
    render(&doc, ctx.storey, &show, ctx, color)
}

fn render(doc: &Document, storey: Id, show: &BTreeSet<u64>, ctx: &ToolContext<'_>, color: Rgba) -> Drawing {
    let scale = plan_scale(ctx.doc, storey);
    let opts = PlanOptions { scale, locale: ctx.locale, space_fill: true, ..PlanOptions::default() };
    let mut out = Drawing::model(scale);
    let Ok(d) = kanavos_views::plan(doc, storey, &opts) else { return out };
    for mut item in d.items {
        if !item.source.is_some_and(|s| show.contains(&s)) {
            continue;
        }
        tint(&mut item, color);
        item.layer = format!("tool.ghost.{}", item.layer);
        item.source = None;
        out.push(item);
    }
    out
}

/// Recolours an item for a preview: strokes and text in the colour, fills as a light tint.
pub(crate) fn tint(item: &mut Item, c: Rgba) {
    if let Some(s) = &mut item.stroke {
        s.color = c;
    }
    if item.fill.is_some() {
        item.fill = Some(Fill::Solid(Rgba::new(c.r, c.g, c.b, 45)));
    }
    if let Shape::Text(t) = &mut item.shape {
        t.color = c;
    }
}

fn p(q: Point2) -> Pt {
    let [x, y] = q.xy();
    pt(x, y)
}

/// A rubber-band line.
pub(crate) fn rubber(d: &mut Drawing, a: Point2, b: Point2) {
    d.push(Item::line(
        p(a),
        p(b),
        Stroke::patterned(weights::FINE, LinePattern::Dashed).with_color(BLUE),
        "tool.rubber",
    ));
}

/// A solid preview line.
pub(crate) fn line(d: &mut Drawing, a: Point2, b: Point2, color: Rgba) {
    d.push(Item::line(p(a), p(b), Stroke::solid(weights::THIN).with_color(color), "tool.line"));
}

/// A preview polyline.
pub(crate) fn polyline(d: &mut Drawing, pts: &[Point2], closed: bool, color: Rgba) {
    if pts.len() >= 2 {
        d.push(Item::polyline(
            pts.iter().map(|q| p(*q)).collect(),
            closed,
            Stroke::solid(weights::THIN).with_color(color),
            "tool.line",
        ));
    }
}

/// A translucent preview area.
pub(crate) fn area(d: &mut Drawing, pts: &[Point2], color: Rgba) {
    if pts.len() >= 3 {
        d.push(Item::polygon(
            vec![pts.iter().map(|q| p(*q)).collect()],
            Some(Fill::Solid(Rgba::new(color.r, color.g, color.b, 40))),
            Some(Stroke::solid(weights::THIN).with_color(color)),
            "tool.area",
        ));
    }
}

/// A small cross marking a point.
pub(crate) fn mark(d: &mut Drawing, at: Point2) {
    let s = d.paper_mm(1.2);
    let c = p(at);
    let stroke = Stroke::solid(weights::FINE).with_color(BLUE);
    d.push(Item::line(c.offset(-s, -s), c.offset(s, s), stroke.clone(), "tool.mark"));
    d.push(Item::line(c.offset(-s, s), c.offset(s, -s), stroke, "tool.mark"));
}

/// Text next to a point.
pub(crate) fn label(d: &mut Drawing, at: Point2, text: impl Into<String>, anchor: Anchor) {
    let mut t = Text::new(p(at), text, 2.0).anchored(anchor);
    t.color = BLUE;
    d.push(Item::text(t, "tool.label"));
}

/// The length of the segment being drawn, written beside it.
pub(crate) fn length_label(d: &mut Drawing, a: Point2, b: Point2, locale: Locale) {
    let v = b - a;
    let Some(u) = v.normalized() else { return };
    let mut angle = u.angle().as_rad();
    if angle > std::f64::consts::FRAC_PI_2 + 1e-9 || angle <= -std::f64::consts::FRAC_PI_2 + 1e-9 {
        angle += std::f64::consts::PI;
    }
    let off = Vec2::from_angle(kanavos_geom::Angle::rad(angle)).perp() * d.paper_mm(2.0);
    let mut t = Text::new(p(a.midpoint(b) + off), length_text(Length::m(v.length()), locale), 2.0)
        .anchored(Anchor::BottomCentre)
        .rotated(angle);
    t.color = BLUE;
    d.push(Item::text(t, "tool.dimension"));
}

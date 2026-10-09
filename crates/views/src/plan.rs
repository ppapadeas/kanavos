//! Floor plans.

use std::collections::BTreeSet;

use kanavos_drawing::{
    Affine, Anchor, Drawing, Fill, HatchPattern, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt,
    weights,
};
use kanavos_elements::{OpeningPlacement, opening_placement, plan_parts, stair_layout};
use kanavos_geom::{Point2, Polygon2, PolygonWithHoles, Segment2, Vec2};
use kanavos_model::{
    Dimension, Document, DraftShape, Element, ElementKind, GridLine, Id, LevelMark, TextAlign, TextNote,
    ViewKind,
};
use kanavos_objects::{OpeningContext, object_symbol, opening_symbol};
use kanavos_units::{AreaDisplay, Locale, format_decimal, format_level};

use crate::ViewError;

/// Options for drawing a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanOptions {
    /// Scale denominator (sizes of text and symbols follow it).
    pub scale: f64,
    /// Language of generated text.
    pub locale: Locale,
    /// Hidden layer prefixes.
    pub hidden_layers: BTreeSet<String>,
    /// Fill spaces with a light tint.
    pub space_fill: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions { scale: 50.0, locale: Locale::El, hidden_layers: BTreeSet::new(), space_fill: true }
    }
}

pub(crate) fn p(q: Point2) -> Pt {
    let [x, y] = q.xy();
    pt(x, y)
}

fn ring(poly: &Polygon2) -> Vec<Pt> {
    poly.vertices.iter().map(|q| p(*q)).collect()
}

pub(crate) fn rings(poly: &PolygonWithHoles) -> Vec<Vec<Pt>> {
    poly.rings().map(ring).collect()
}

fn seg_item(s: &Segment2, stroke: Stroke, layer: &str, source: Id) -> Item {
    Item::line(p(s.start), p(s.end), stroke, layer).with_source(source.0)
}

/// Material fill for a cut area.
pub(crate) fn material_fill(doc: &Document, material: Option<Id>) -> Fill {
    match material.and_then(|m| doc.material(m)) {
        Some(m) => Fill::Hatch {
            pattern: m.cut_pattern,
            color: Rgba::BLACK,
            background: m.cut_background,
            scale: 1.0,
            angle: 0.0,
        },
        None => Fill::hatch(HatchPattern::None, Some(Rgba::LIGHT_GREY)),
    }
}

fn opening_affine(pl: &OpeningPlacement) -> Affine {
    let [cx, cy] = pl.centre.xy();
    Affine { a: pl.along.x, b: pl.across.x, c: pl.along.y, d: pl.across.y, tx: cx, ty: cy }
}

/// Draws the plan of a storey.
pub fn plan(doc: &Document, storey: Id, opts: &PlanOptions) -> Result<Drawing, ViewError> {
    let st = doc.storey(storey).ok_or(ViewError::NotFound(storey))?;
    let cut = st.cut_elevation();
    let mut d = Drawing::model(opts.scale);
    let mm = |v: f64| v * opts.scale / 1000.0;

    // Underlays first, so everything draws over them.
    for e in doc.elements_on(storey) {
        if let ElementKind::Underlay(u) = &e.kind {
            for item in &u.drawing.items {
                let mut item = item.clone();
                item.layer = format!("underlay.{}", item.layer);
                item.source = Some(e.id.0);
                d.push(item);
            }
        }
    }

    // Spaces: tint and stamp.
    for e in doc.elements_on(storey) {
        if let ElementKind::Space(s) = &e.kind {
            if opts.space_fill {
                d.push(
                    Item::polygon(
                        rings(&s.boundary),
                        Some(Fill::Solid(Rgba::new(250, 244, 225, 255))),
                        None,
                        "space.fill",
                    )
                    .with_source(e.id.0),
                );
            }
            let at = s.stamp.or_else(|| s.boundary.outer.centroid()).unwrap_or(Point2::ORIGIN);
            let lines = [
                (s.name.clone(), 2.5, true),
                (s.number.clone(), 2.0, false),
                (AreaDisplay(s.boundary.area(), opts.locale).to_string(), 2.0, false),
            ];
            let mut y = p(at).y + mm(3.5);
            for (text, size, bold) in lines {
                if text.is_empty() {
                    continue;
                }
                let mut t = Text::new(pt(p(at).x, y), text, size).anchored(Anchor::Centre);
                t.bold = bold;
                d.push(Item::text(t, "space.stamp").with_source(e.id.0));
                y -= mm(size * 1.6);
            }
        }
    }

    // Below and above the cut plane.
    for e in doc.elements_on(storey) {
        if !e.category().is_building_part() {
            continue;
        }
        let Ok(parts) = plan_parts(doc, e.id, cut) else { continue };
        let key = e.category().key();
        for s in &parts.below {
            d.push(seg_item(s, Stroke::solid(weights::FINE), &format!("{key}.below"), e.id));
        }
        for s in &parts.above {
            d.push(seg_item(
                s,
                Stroke::patterned(weights::FINE, LinePattern::Dashed),
                &format!("{key}.above"),
                e.id,
            ));
        }
    }
    // Elements of the storey above that overhang (roofs, slabs): dashed.
    if let Some(above) = doc.storey_above(storey) {
        for e in doc.elements_on(above.id) {
            if let ElementKind::Roof(r) = &e.kind {
                for s in r.outline.edges() {
                    d.push(seg_item(
                        &s,
                        Stroke::patterned(weights::FINE, LinePattern::Dashed),
                        "roof.above",
                        e.id,
                    ));
                }
            }
        }
    }

    // Stair walking line and arrow.
    for e in doc.elements_on(storey) {
        if let ElementKind::Stair(_) = &e.kind
            && let Ok(l) = stair_layout(doc, e.id)
        {
            let pts: Vec<Pt> = l.walking_line.iter().map(|q| p(*q)).collect();
            if let (Some(a), Some(b)) = (pts.get(pts.len().saturating_sub(2)).copied(), pts.last().copied()) {
                let len = a.distance(b).max(1e-9);
                let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
                let s = mm(2.5);
                d.push(
                    Item::polygon(
                        vec![vec![
                            b,
                            pt(b.x - ux * s - uy * s * 0.5, b.y - uy * s + ux * s * 0.5),
                            pt(b.x - ux * s + uy * s * 0.5, b.y - uy * s - ux * s * 0.5),
                        ]],
                        Some(Fill::Solid(Rgba::BLACK)),
                        None,
                        "stair.arrow",
                    )
                    .with_source(e.id.0),
                );
            }
            d.push(
                Item::polyline(pts, false, Stroke::solid(weights::FINE), "stair.line").with_source(e.id.0),
            );
        }
    }

    // Cut elements: material hatch, then outline.
    for e in doc.elements_on(storey) {
        if !e.category().is_building_part() {
            continue;
        }
        let Ok(parts) = plan_parts(doc, e.id, cut) else { continue };
        let key = e.category().key();
        for c in &parts.cut {
            d.push(
                Item::polygon(
                    rings(&c.region),
                    Some(material_fill(doc, c.material)),
                    Some(Stroke::solid(weights::FINE)),
                    format!("{key}.layer"),
                )
                .with_source(e.id.0),
            );
        }
        for c in &parts.cut {
            for r in c.region.rings() {
                d.push(
                    Item::polyline(ring(r), true, Stroke::solid(weights::THICK), format!("{key}.cut"))
                        .with_source(e.id.0),
                );
            }
        }
    }

    // Openings.
    for e in doc.elements_on(storey) {
        if let ElementKind::Opening(o) = &e.kind
            && let Ok(pl) = opening_placement(doc, e.id)
        {
            let sill = pl.sill_elevation;
            if sill > cut.as_m() {
                continue; // High windows are not cut; a later version draws them dashed.
            }
            let ctx = OpeningContext { opening: o, left_face: pl.left_face, right_face: pl.right_face };
            let m = opening_affine(&pl);
            let mut local = Drawing::model(opts.scale);
            local.items = opening_symbol(&ctx);
            for mut item in local.transformed(&m).items {
                item.source = Some(e.id.0);
                d.push(item);
            }
        }
    }

    // Free objects.
    for e in doc.elements_on(storey) {
        if let ElementKind::Object(o) = &e.kind {
            let [x, y] = o.position.xy();
            let mut m = Affine::rotation(o.rotation.as_rad());
            if o.mirrored {
                m = m.then(&Affine { a: -1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 });
            }
            m.tx = x;
            m.ty = y;
            let mut local = Drawing::model(opts.scale);
            local.items = object_symbol(o);
            for mut item in local.transformed(&m).items {
                item.source = Some(e.id.0);
                d.push(item);
            }
        }
    }

    // Annotation and drafting on this storey, and grid lines everywhere.
    for e in doc.elements() {
        let on_storey = e.storey == Some(storey);
        match &e.kind {
            ElementKind::Grid(g) if e.storey.is_none() || on_storey => draw_grid(&mut d, e, g, opts),
            _ if !on_storey => {}
            ElementKind::Dimension(dim) => draw_dimension(&mut d, e, dim, opts),
            ElementKind::Text(t) => draw_text(&mut d, e, t, opts),
            ElementKind::Level(l) => draw_level(&mut d, e, l, st.elevation, opts),
            ElementKind::Drafting(df) => draw_drafting(&mut d, e, df),
            _ => {}
        }
    }

    // Section and elevation markers.
    for v in doc.views() {
        if let ViewKind::Section { start, end, marker, flip, .. }
        | ViewKind::Elevation { start, end, marker, flip, .. } = &v.kind
        {
            let is_section = matches!(v.kind, ViewKind::Section { .. });
            draw_marker(&mut d, *start, *end, marker, *flip, is_section, opts, v.id);
        }
    }

    crate::filter_layers(&mut d, &opts.hidden_layers);
    Ok(d)
}

fn draw_grid(d: &mut Drawing, e: &Element, g: &GridLine, opts: &PlanOptions) {
    let mm = |v: f64| v * opts.scale / 1000.0;
    d.push(
        Item::line(p(g.start), p(g.end), Stroke::patterned(weights::FINE, LinePattern::DashDot), "grid.line")
            .with_source(e.id.0),
    );
    let r = mm(4.0);
    let dir = (g.end - g.start).normalized().unwrap_or(Vec2::X);
    for (end, sign) in [(g.start, -1.0), (g.end, 1.0)] {
        let c = end + dir * (r * sign);
        d.push(Item {
            shape: Shape::Circle { centre: p(c), radius: r },
            stroke: Some(Stroke::solid(weights::THIN)),
            fill: Some(Fill::Solid(Rgba::WHITE)),
            layer: "grid.bubble".into(),
            source: Some(e.id.0),
        });
        d.push(
            Item::text(Text::new(p(c), g.label.clone(), 3.0).anchored(Anchor::Centre), "grid.label")
                .with_source(e.id.0),
        );
    }
}

fn draw_dimension(d: &mut Drawing, e: &Element, dim: &Dimension, opts: &PlanOptions) {
    let mm = |v: f64| v * opts.scale / 1000.0;
    let u = Vec2::from_angle(dim.direction);
    let n = u.perp();
    // Project each measured point onto the dimension line.
    let base = dim.line_point;
    let proj = |q: Point2| -> (f64, Point2) {
        let t = (q - base).dot(u);
        (t, base + u * t)
    };
    let mut pts: Vec<(f64, Point2, Point2)> = dim
        .points
        .iter()
        .map(|q| {
            let (t, on) = proj(*q);
            (t, on, *q)
        })
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let stroke = Stroke::solid(weights::FINE);
    if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
        let ext = mm(2.0);
        d.push(
            Item::line(p(first.1 - u * ext), p(last.1 + u * ext), stroke.clone(), "dimension.line")
                .with_source(e.id.0),
        );
    }
    for (_, on, q) in &pts {
        // Extension line from near the measured point to just past the dimension line.
        let side = (*on - *q).normalized().unwrap_or(n);
        let gap = mm(1.5);
        d.push(
            Item::line(p(*q + side * gap), p(*on + side * mm(2.0)), stroke.clone(), "dimension.extension")
                .with_source(e.id.0),
        );
        // 45° tick.
        let t = (u + n).normalized().unwrap_or(u) * mm(1.4);
        d.push(
            Item::line(p(*on - t), p(*on + t), Stroke::solid(weights::THIN), "dimension.tick")
                .with_source(e.id.0),
        );
    }
    for w in pts.windows(2) {
        if let [a, b] = w {
            let len = b.0 - a.0;
            let mid = a.1.midpoint(b.1) + n * mm(1.0);
            let text = format_decimal(len, 2, opts.locale);
            let mut angle = dim.direction.as_rad();
            // Keep text readable from the bottom or the right.
            if angle > std::f64::consts::FRAC_PI_2 + 1e-9 || angle <= -std::f64::consts::FRAC_PI_2 + 1e-9 {
                angle += std::f64::consts::PI;
            }
            d.push(
                Item::text(
                    Text::new(p(mid), text, 2.0).anchored(Anchor::BottomCentre).rotated(angle),
                    "dimension.text",
                )
                .with_source(e.id.0),
            );
        }
    }
}

fn draw_text(d: &mut Drawing, e: &Element, t: &TextNote, opts: &PlanOptions) {
    let mm = |v: f64| v * opts.scale / 1000.0;
    let anchor = match t.align {
        TextAlign::Left => Anchor::BottomLeft,
        TextAlign::Centre => Anchor::BottomCentre,
        TextAlign::Right => Anchor::BottomRight,
    };
    let down = Vec2::from_angle(t.rotation).perp() * (-mm(t.size_mm * 1.6));
    for (i, line) in t.content.lines().enumerate() {
        let at = t.position + down * i as f64;
        let mut text = Text::new(p(at), line, t.size_mm).anchored(anchor).rotated(t.rotation.as_rad());
        text.bold = t.bold;
        d.push(Item::text(text, "text").with_source(e.id.0));
    }
}

fn draw_level(d: &mut Drawing, e: &Element, l: &LevelMark, floor: kanavos_units::Length, opts: &PlanOptions) {
    let mm = |v: f64| v * opts.scale / 1000.0;
    let value = l.value.unwrap_or(floor);
    let c = p(l.position);
    let s = mm(2.0);
    // Plan level symbol: a small square with a cross, then the value.
    d.push(
        Item::polyline(
            vec![c.offset(-s, -s), c.offset(s, -s), c.offset(s, s), c.offset(-s, s)],
            true,
            Stroke::solid(weights::FINE),
            "level.symbol",
        )
        .with_source(e.id.0),
    );
    d.push(
        Item::line(c.offset(-s, -s), c.offset(s, s), Stroke::solid(weights::FINE), "level.symbol")
            .with_source(e.id.0),
    );
    let mut label = format_level(value, opts.locale);
    if !l.label.is_empty() {
        label = format!("{} {label}", l.label);
    }
    d.push(
        Item::text(Text::new(c.offset(s * 1.5, 0.0), label, 2.0).anchored(Anchor::MiddleLeft), "level.text")
            .with_source(e.id.0),
    );
}

fn draw_drafting(d: &mut Drawing, e: &Element, df: &kanavos_model::Drafting) {
    let stroke =
        Stroke { weight_mm: df.style.weight_mm, color: df.style.color, pattern: df.style.pattern.clone() };
    let item = match &df.shape {
        DraftShape::Line { a, b } => Item::line(p(*a), p(*b), stroke, "drafting"),
        DraftShape::Polyline { points, closed } => {
            Item::polyline(points.iter().map(|q| p(*q)).collect(), *closed, stroke, "drafting")
        }
        DraftShape::Arc { arc } => Item {
            shape: Shape::Arc {
                centre: p(arc.centre),
                radius: arc.radius.as_m(),
                start: arc.start.as_rad(),
                sweep: arc.sweep.as_rad(),
            },
            stroke: Some(stroke),
            fill: None,
            layer: "drafting".into(),
            source: None,
        },
        DraftShape::Circle { centre, radius } => Item {
            shape: Shape::Circle { centre: p(*centre), radius: radius.as_m() },
            stroke: Some(stroke),
            fill: None,
            layer: "drafting".into(),
            source: None,
        },
        DraftShape::Fill { region } => Item::polygon(
            rings(region),
            Some(Fill::Hatch {
                pattern: df.style.hatch,
                color: df.style.color,
                background: df.style.background,
                scale: 1.0,
                angle: 0.0,
            }),
            Some(stroke),
            "drafting.fill",
        ),
    };
    d.push(item.with_source(e.id.0));
}

#[allow(clippy::too_many_arguments)]
fn draw_marker(
    d: &mut Drawing,
    start: Point2,
    end: Point2,
    marker: &str,
    flip: bool,
    is_section: bool,
    opts: &PlanOptions,
    view: Id,
) {
    let mm = |v: f64| v * opts.scale / 1000.0;
    let Some(u) = (end - start).normalized() else { return };
    let look = if flip { -u.perp() } else { u.perp() };
    let layer = if is_section { "section.marker" } else { "elevation.marker" };
    if is_section {
        d.push(
            Item::line(p(start), p(end), Stroke::patterned(weights::THIN, LinePattern::DashDot), layer)
                .with_source(view.0),
        );
    }
    for q in [start, end] {
        // Arrow showing the viewing direction, with the marker letter.
        let tip = q + look * mm(6.0);
        let side = u * mm(2.5);
        d.push(
            Item::polygon(
                vec![vec![p(q - side), p(q + side), p(tip)]],
                Some(Fill::Solid(Rgba::BLACK)),
                None,
                layer,
            )
            .with_source(view.0),
        );
        d.push(
            Item::text(Text::new(p(q + look * mm(9.0)), marker, 3.5).anchored(Anchor::Centre), layer)
                .with_source(view.0),
        );
        if !is_section {
            break;
        }
    }
}
